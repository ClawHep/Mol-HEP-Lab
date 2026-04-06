//! Result-analysis agent runtime.
//!
//! Ports `backend/agent/researchclaw/pipeline/result_analysis/runtime.py`.
//!
//! Responsibilities:
//! - Prepare an analysis workspace (copies runs/, experiment_final/, result
//!   files, and prior analysis into the workspace)
//! - List data files for the agent to analyse
//! - Build system and user prompts for the result-analysis LLM agent
//! - Detect success (presence of experiment_summary.json + analysis.md)
//! - Copy summary, analysis, and charts back to the stage directory

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Maximum file size to include in the analysis workspace (5 MB)
// ---------------------------------------------------------------------------

const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Data file extensions
// ---------------------------------------------------------------------------

const DATA_EXTENSIONS: &[&str] = &[
    "json", "csv", "tsv", "txt", "yaml", "yml", "log", "md",
];

// ---------------------------------------------------------------------------
// Workspace preparation
// ---------------------------------------------------------------------------

/// Prepare an analysis workspace under `stage_dir/workspace/`.
///
/// Copies into the workspace (if they exist in `run_dir`):
/// - `runs/` directory
/// - `experiment_final/` directory
/// - `results.json`, `refinement_log.json`, `sanity_report.json`
/// - `exp_plan.yaml` / `EXPERIMENT_PLAN.yaml`
/// - `prior_analysis.md`
pub fn prepare_workspace(
    stage_dir: &Path,
    run_dir: &Path,
    _config: &crate::executor::MolConfig,
) -> Result<PathBuf> {
    let workspace = stage_dir.join("workspace");
    fs::create_dir_all(&workspace).context("create workspace dir")?;

    // Copy directories
    for dir_name in &["runs", "experiment_final"] {
        let src = run_dir.join(dir_name);
        if src.exists() && src.is_dir() {
            copy_dir_contents(&src, &workspace.join(dir_name))?;
        }
    }

    // Also search stage dirs for experiment_final and result files
    let stage_dirs = collect_stage_dirs(run_dir);
    for stage_dir_path in &stage_dirs {
        let exp_final = stage_dir_path.join("experiment_final");
        if exp_final.exists() {
            copy_dir_contents(&exp_final, &workspace.join("experiment_final"))?;
            break; // Use the most recent one (already sorted descending)
        }
    }

    // Copy individual result files
    let result_files = [
        "results.json",
        "refinement_log.json",
        "sanity_report.json",
        "prior_analysis.md",
    ];
    for file_name in &result_files {
        // Try run-level first
        let src = run_dir.join(file_name);
        if src.exists() {
            fs::copy(&src, workspace.join(file_name))
                .with_context(|| format!("copy {file_name}"))?;
            continue;
        }
        // Try stage dirs
        for stage_dir_path in &stage_dirs {
            let src = stage_dir_path.join(file_name);
            if src.exists() {
                fs::copy(&src, workspace.join(file_name))
                    .with_context(|| format!("copy {file_name} from stage"))?;
                break;
            }
        }
    }

    // Copy experiment plan
    for plan_name in &["exp_plan.yaml", "EXPERIMENT_PLAN.yaml"] {
        let src = run_dir.join(plan_name);
        if src.exists() {
            fs::copy(&src, workspace.join("exp_plan.yaml"))
                .context("copy exp_plan.yaml")?;
            break;
        }
        for stage_dir_path in &stage_dirs {
            let src = stage_dir_path.join(plan_name);
            if src.exists() {
                fs::copy(&src, workspace.join("exp_plan.yaml"))
                    .context("copy exp_plan.yaml from stage")?;
                break;
            }
        }
    }

    Ok(workspace)
}

// ---------------------------------------------------------------------------
// File listing
// ---------------------------------------------------------------------------

/// List data files in `workspace` for agent analysis.
///
/// Included extensions: `.json`, `.csv`, `.tsv`, `.txt`, `.yaml`, `.yml`,
/// `.log`, `.md`.
///
/// Excluded directories: `__pycache__`, `.git`, `codebases`, `datasets`,
/// `checkpoints`.
///
/// Files larger than 5 MB are skipped.
pub fn list_data_files(workspace: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_data_files(workspace, workspace, &mut files);
    files.sort();
    files
}

fn collect_data_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip excluded directories
        if path.is_dir() {
            if name_str.starts_with('.')
                || matches!(
                    name_str.as_ref(),
                    "__pycache__" | "codebases" | "datasets" | "checkpoints"
                )
            {
                continue;
            }
            if !path.is_symlink() {
                collect_data_files(root, &path, out);
            }
            continue;
        }

        if !path.is_file() {
            continue;
        }

        // Check extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !DATA_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        // Check file size
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > MAX_FILE_SIZE {
                continue;
            }
        }

        if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt building
// ---------------------------------------------------------------------------

/// Build the system prompt for the result-analysis agent.
pub fn build_system_prompt(python_path: &str, workspace: &Path) -> String {
    format!(
        r#"You are an expert scientific analyst reviewing machine learning experiment results.

Workspace: {workspace}
Python interpreter: {python}

## Your Objectives

1. Thoroughly analyze all experimental results in the workspace
2. Identify key findings, trends, and statistical patterns
3. Compare results against baselines and research hypotheses
4. Generate informative visualizations (charts, plots) when helpful
5. Write a comprehensive analysis report

## Output Files Required

You must produce:
- `experiment_summary.json` — structured summary with key metrics and findings
- `analysis.md` — detailed Markdown analysis report (methods, results, interpretation)
- `charts/` directory — any generated plots/figures (PNG format preferred)

## analysis.md Format

The report should include:
1. Executive Summary
2. Experimental Setup
3. Results Overview (with key metrics)
4. Detailed Analysis
5. Comparison to Baseline/Prior Work
6. Limitations and Failure Modes
7. Conclusions and Recommendations

## experiment_summary.json Format

```json
{{
  "best_metric_value": value,
  "metric_key": "...",
  "metric_direction": "maximize|minimize",
  "key_findings": ["...", "..."],
  "recommendation": "proceed|pivot|refine",
  "confidence": "high|medium|low"
}}
```
"#,
        workspace = workspace.display(),
        python = python_path,
    )
}

/// Build the user message for the result-analysis agent.
pub fn build_user_message(
    workspace: &Path,
    data_files: &[String],
    metric_key: &str,
    direction: &str,
    topic: &str,
) -> String {
    let file_list = if data_files.is_empty() {
        "  (no data files found)".to_owned()
    } else {
        data_files
            .iter()
            .map(|f| format!("  - {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"Please analyze the experimental results in:
  {workspace}

## Research Topic
{topic}

## Primary Metric
- Key: `{metric_key}`
- Direction: {direction}

## Available Data Files
{file_list}

## Analysis Steps

1. Read `results.json` and `refinement_log.json` for quantitative results
2. Review `exp_plan.yaml` to understand the research goals
3. Analyze trends, improvements, and final performance
4. Generate visualizations for key results
5. Write `analysis.md` with full findings
6. Write `experiment_summary.json` with structured summary

Begin by reading the results files.
"#,
        workspace = workspace.display(),
        topic = topic,
        metric_key = metric_key,
        direction = direction,
        file_list = file_list,
    )
}

// ---------------------------------------------------------------------------
// Success detection
// ---------------------------------------------------------------------------

/// Returns `true` when both `experiment_summary.json` and `analysis.md`
/// exist in `workspace`.
pub fn check_success(workspace: &Path) -> bool {
    workspace.join("experiment_summary.json").exists()
        && workspace.join("analysis.md").exists()
}

// ---------------------------------------------------------------------------
// Copy results to stage
// ---------------------------------------------------------------------------

/// Copy analysis outputs from workspace to `stage_dir`.
///
/// Copies:
/// - `experiment_summary.json`
/// - `analysis.md`
/// - `charts/` directory
///
/// Returns the list of artifact file names (relative to `stage_dir`).
pub fn copy_results_to_stage(workspace: &Path, stage_dir: &Path) -> Result<Vec<String>> {
    let mut artifacts = Vec::new();

    for name in &["experiment_summary.json", "analysis.md"] {
        let src = workspace.join(name);
        if src.exists() {
            let dst = stage_dir.join(name);
            fs::copy(&src, &dst)
                .with_context(|| format!("copy {name} to stage_dir"))?;
            artifacts.push((*name).to_owned());
        }
    }

    // Copy charts/ directory
    let charts_src = workspace.join("charts");
    if charts_src.exists() {
        let charts_dst = stage_dir.join("charts");
        copy_dir_contents(&charts_src, &charts_dst)?;
        // List individual chart files as artifacts
        if let Ok(entries) = fs::read_dir(&charts_dst) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_file() {
                    let name = format!("charts/{}", entry.file_name().to_string_lossy());
                    artifacts.push(name);
                }
            }
        }
    }

    Ok(artifacts)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn collect_stage_dirs(run_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(run_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("stage-"))
                    .unwrap_or(false)
        })
        .collect();
    dirs.sort_by(|a, b| b.cmp(a)); // descending
    dirs
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    let Ok(entries) = fs::read_dir(src) else {
        return Ok(());
    };
    fs::create_dir_all(dst).ok();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') || name_str == "__pycache__" {
            continue;
        }

        let dest = dst.join(&name);
        if path.is_symlink() {
            continue;
        } else if path.is_dir() {
            copy_dir_contents(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)
                .with_context(|| format!("copy {} -> {}", path.display(), dest.display()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn check_success_both_present() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("experiment_summary.json"),
            r#"{"best_metric_value": 0.9}"#,
        )
        .unwrap();
        fs::write(tmp.path().join("analysis.md"), "# Analysis").unwrap();
        assert!(check_success(tmp.path()));
    }

    #[test]
    fn check_success_missing_summary() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("analysis.md"), "# Analysis").unwrap();
        assert!(!check_success(tmp.path()));
    }

    #[test]
    fn check_success_missing_analysis() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("experiment_summary.json"),
            r#"{"best_metric_value": 0.9}"#,
        )
        .unwrap();
        assert!(!check_success(tmp.path()));
    }

    #[test]
    fn check_success_both_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(!check_success(tmp.path()));
    }

    #[test]
    fn list_data_files_basic() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        fs::write(ws.join("results.json"), "{}").unwrap();
        fs::write(ws.join("notes.md"), "# notes").unwrap();
        fs::write(ws.join("script.py"), "").unwrap(); // should be excluded

        let files = list_data_files(ws);
        assert!(files.contains(&"results.json".to_owned()));
        assert!(files.contains(&"notes.md".to_owned()));
        assert!(!files.iter().any(|f| f.ends_with(".py")));
    }

    #[test]
    fn list_data_files_excludes_blocked_dirs() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        fs::create_dir_all(ws.join("datasets")).unwrap();
        fs::write(ws.join("datasets").join("data.csv"), "a,b").unwrap();
        fs::write(ws.join("results.json"), "{}").unwrap();

        let files = list_data_files(ws);
        assert!(!files.iter().any(|f| f.contains("datasets")));
        assert!(files.contains(&"results.json".to_owned()));
    }

    #[test]
    fn list_data_files_excludes_large_files() {
        let tmp = TempDir::new().unwrap();
        // Write a file just over 5 MB
        let big: Vec<u8> = vec![0u8; (MAX_FILE_SIZE + 1) as usize];
        fs::write(tmp.path().join("big.json"), &big).unwrap();
        fs::write(tmp.path().join("small.json"), "{}").unwrap();

        let files = list_data_files(tmp.path());
        assert!(!files.iter().any(|f| f == "big.json"));
        assert!(files.contains(&"small.json".to_owned()));
    }

    #[test]
    fn prepare_workspace_creates_dir() {
        let tmp = TempDir::new().unwrap();
        let run_dir = tmp.path().join("run");
        let stage_dir = run_dir.join("stage-16");
        fs::create_dir_all(&stage_dir).unwrap();
        fs::write(run_dir.join("results.json"), r#"{"loss": 0.2}"#).unwrap();

        let config = crate::executor::MolConfig::default();
        let ws = prepare_workspace(&stage_dir, &run_dir, &config).unwrap();
        assert!(ws.exists());
        assert!(ws.join("results.json").exists());
    }

    #[test]
    fn copy_results_to_stage_basic() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let stage = tmp.path().join("stage");
        fs::create_dir_all(&ws).unwrap();
        fs::create_dir_all(&stage).unwrap();

        fs::write(ws.join("experiment_summary.json"), r#"{"best": 0.9}"#).unwrap();
        fs::write(ws.join("analysis.md"), "# Analysis\nGreat results.").unwrap();

        let artifacts = copy_results_to_stage(&ws, &stage).unwrap();
        assert!(artifacts.contains(&"experiment_summary.json".to_owned()));
        assert!(artifacts.contains(&"analysis.md".to_owned()));
        assert!(stage.join("experiment_summary.json").exists());
        assert!(stage.join("analysis.md").exists());
    }

    #[test]
    fn copy_results_to_stage_includes_charts() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let stage = tmp.path().join("stage");
        let charts = ws.join("charts");
        fs::create_dir_all(&charts).unwrap();
        fs::create_dir_all(&stage).unwrap();
        fs::write(ws.join("experiment_summary.json"), "{}").unwrap();
        fs::write(ws.join("analysis.md"), "# ok").unwrap();
        fs::write(charts.join("loss_curve.png"), b"PNG").unwrap();

        let artifacts = copy_results_to_stage(&ws, &stage).unwrap();
        assert!(artifacts.iter().any(|a| a.contains("loss_curve.png")));
    }
}
