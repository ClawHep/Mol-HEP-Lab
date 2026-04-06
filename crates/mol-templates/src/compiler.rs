//! LaTeX compilation and quality-check utilities for Mol-HEP-Lab.
//!
//! Provides [`compile_latex`] which runs the full pdflatex → bibtex →
//! pdflatex → pdflatex cycle, and [`check_quality`] which inspects the
//! compiled artefacts for common problems.

use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

/// Outcome of a LaTeX compilation run.
#[derive(Debug, Clone)]
pub struct CompileResult {
    /// Whether the final pdflatex pass succeeded (exit code 0).
    pub success: bool,
    /// Absolute path to the produced PDF, if `success` is `true`.
    pub pdf_path: Option<PathBuf>,
    /// Last 2 000 characters of the combined stdout/stderr log.
    pub log: String,
    /// LaTeX warnings extracted from the log.
    pub warnings: Vec<String>,
    /// LaTeX errors extracted from the log.
    pub errors: Vec<String>,
}

/// Results of post-compilation quality checks.
#[derive(Debug, Clone, Default)]
pub struct QualityCheckResult {
    /// Page count extracted from the `.log` or `.aux` file (0 if unknown).
    pub page_count: u32,
    /// `true` if an `abstract` environment was found in the `.tex` source.
    pub has_abstract: bool,
    /// `true` if a `\bibliography{}` or `\begin{thebibliography}` command
    /// was found in the source.
    pub has_references: bool,
    /// Number of `\begin{figure}` environments found in the source.
    pub figure_count: u32,
    /// Number of `\begin{table}` environments found in the source.
    pub table_count: u32,
    /// Human-readable warning strings summarising any issues found.
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// compile_latex
// ---------------------------------------------------------------------------

/// Compile a `.tex` file using the full pdflatex → bibtex → pdflatex →
/// pdflatex cycle.
///
/// # Arguments
/// * `tex_path`   — Path to the `.tex` source file.
/// * `output_dir` — Directory where output artefacts (`.pdf`, `.log`, …)
///                  will be written.  Should normally be the same directory
///                  as `tex_path`.
///
/// # Errors
/// Returns an error only for I/O failures (e.g. the file does not exist).
/// Compilation failures are reported through [`CompileResult::success`].
pub async fn compile_latex(tex_path: &Path, output_dir: &Path) -> Result<CompileResult> {
    let tex_name = tex_path
        .file_name()
        .context("tex_path has no file name")?
        .to_string_lossy()
        .into_owned();

    let stem = tex_path
        .file_stem()
        .context("tex_path has no stem")?
        .to_string_lossy()
        .into_owned();

    // --- First pdflatex pass ---
    let first = run_pdflatex(&tex_name, output_dir).await;
    let log_text = first.log.clone();
    let (errors, warnings) = parse_log(&log_text);

    if !first.success {
        return Ok(CompileResult {
            success: false,
            pdf_path: None,
            log: truncate_log(&log_text),
            warnings,
            errors,
        });
    }

    info!("mol-templates: first pdflatex pass succeeded for {tex_name}");

    // --- bibtex pass (best-effort; ignored if bibtex not installed) ---
    run_bibtex(&stem, output_dir).await;

    // --- Second pdflatex pass ---
    run_pdflatex(&tex_name, output_dir).await;

    // --- Third (final) pdflatex pass ---
    let final_pass = run_pdflatex(&tex_name, output_dir).await;
    let final_log = final_pass.log.clone();
    let (final_errors, final_warnings) = parse_log(&final_log);

    let pdf_path = if final_pass.success {
        let p = output_dir.join(format!("{stem}.pdf"));
        if p.exists() { Some(p) } else { None }
    } else {
        None
    };

    let success = final_pass.success && pdf_path.is_some();

    if success {
        info!("mol-templates: full compilation cycle completed for {tex_name}");
    } else {
        warn!(
            "mol-templates: compilation failed for {tex_name} — {} error(s)",
            final_errors.len()
        );
    }

    Ok(CompileResult {
        success,
        pdf_path,
        log: truncate_log(&final_log),
        warnings: final_warnings,
        errors: final_errors,
    })
}

// ---------------------------------------------------------------------------
// check_quality
// ---------------------------------------------------------------------------

/// Inspect a compiled PDF and its `.tex` source for common quality issues.
///
/// `pdf_path` is used to locate the companion `.log` and `.aux` files.
/// `tex_content` is the raw source text used for structural checks.
pub async fn check_quality(pdf_path: &Path, tex_content: &str) -> Result<QualityCheckResult> {
    let mut result = QualityCheckResult::default();

    // --- Structural checks on the .tex source ---
    result.has_abstract = tex_content.contains(r"\begin{abstract}");
    result.has_references = tex_content.contains(r"\bibliography{")
        || tex_content.contains(r"\begin{thebibliography}");

    let fig_re = Regex::new(r"\\begin\{figure")?;
    result.figure_count = fig_re.find_iter(tex_content).count() as u32;

    let tbl_re = Regex::new(r"\\begin\{table")?;
    result.table_count = tbl_re.find_iter(tex_content).count() as u32;

    // --- Page count from .aux or .log ---
    let stem = pdf_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let dir = pdf_path.parent().unwrap_or(Path::new("."));

    // Try .aux first (hyperref / lastpage provides \newlabel{LastPage})
    let aux_path = dir.join(format!("{stem}.aux"));
    if aux_path.exists() {
        let aux_text = tokio::fs::read_to_string(&aux_path).await.unwrap_or_default();
        let lastpage_re = Regex::new(r"\\newlabel\{LastPage\}\{\{(\d+)\}")?;
        if let Some(caps) = lastpage_re.captures(&aux_text) {
            result.page_count = caps[1].parse().unwrap_or(0);
        }
    }

    // Fall back to "Output written on … (N pages)" in the .log
    if result.page_count == 0 {
        let log_path = dir.join(format!("{stem}.log"));
        if log_path.exists() {
            let log_text = tokio::fs::read_to_string(&log_path)
                .await
                .unwrap_or_default();
            let pages_re = Regex::new(r"Output written on .* \((\d+) page")?;
            if let Some(caps) = pages_re.captures(&log_text) {
                result.page_count = caps[1].parse().unwrap_or(0);
            }

            // Collect warnings from the log
            let (_, log_warnings) = parse_log(&log_text);
            result.warnings.extend(log_warnings);
        }
    }

    // --- Structural warnings ---
    if !result.has_abstract {
        result.warnings.push("No \\begin{abstract} found".to_owned());
    }
    if !result.has_references {
        result
            .warnings
            .push("No bibliography command found".to_owned());
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

struct PassResult {
    success: bool,
    log: String,
}

async fn run_pdflatex(tex_name: &str, work_dir: &Path) -> PassResult {
    let output = Command::new("pdflatex")
        .args(["-interaction=nonstopmode", "-halt-on-error", tex_name])
        .current_dir(work_dir)
        .output()
        .await;

    match output {
        Ok(out) => {
            let log = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            PassResult {
                success: out.status.success(),
                log,
            }
        }
        Err(e) => PassResult {
            success: false,
            log: format!("Failed to spawn pdflatex: {e}"),
        },
    }
}

async fn run_bibtex(stem: &str, work_dir: &Path) {
    let result = Command::new("bibtex")
        .arg(stem)
        .current_dir(work_dir)
        .output()
        .await;

    match result {
        Ok(out) if out.status.success() => {
            info!("mol-templates: bibtex pass succeeded for {stem}");
        }
        Ok(_) => {
            warn!("mol-templates: bibtex pass failed for {stem} (ignored)");
        }
        Err(e) => {
            warn!("mol-templates: bibtex not available: {e}");
        }
    }
}

fn parse_log(log_text: &str) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for line in log_text.lines() {
        let stripped = line.trim();
        if stripped.starts_with('!') {
            errors.push(stripped.to_owned());
        } else if stripped.contains("LaTeX Warning:") {
            warnings.push(stripped.to_owned());
        } else if stripped.contains("Undefined control sequence") {
            errors.push(stripped.to_owned());
        } else if stripped.contains("Missing") && stripped.contains("inserted") {
            errors.push(stripped.to_owned());
        } else if stripped.contains("File") && stripped.contains("not found") {
            errors.push(stripped.to_owned());
        }
    }

    (errors, warnings)
}

fn truncate_log(log: &str) -> String {
    if log.len() > 2_000 {
        log[log.len() - 2_000..].to_owned()
    } else {
        log.to_owned()
    }
}
