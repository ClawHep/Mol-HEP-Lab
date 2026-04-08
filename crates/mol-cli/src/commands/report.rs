//! `mol report` — Generate a human-readable run report.
//!
//! Ports `cmd_report` from `backend/agent/mol/cli.py`.

use anyhow::{bail, Result};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ReportArgs {
    /// Path to run artifacts directory
    #[arg(long, required = true)]
    pub run_dir: PathBuf,

    /// Write report to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

/// Scan a run directory and produce a markdown report.
fn generate_report(run_dir: &PathBuf) -> Result<String> {
    if !run_dir.exists() {
        bail!("run directory not found: {}", run_dir.display());
    }
    if !run_dir.is_dir() {
        bail!("not a directory: {}", run_dir.display());
    }

    let dir_name = run_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("# Mol-HEP-Lab Run Report: `{dir_name}`\n"));

    // Collect artifact files
    let mut artifacts: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(run_dir)
        .max_depth(4)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            artifacts.push(entry.into_path());
        }
    }

    if artifacts.is_empty() {
        lines.push("_No artifacts found._\n".to_string());
    } else {
        lines.push(format!("## Artifacts ({} files)\n", artifacts.len()));
        for path in &artifacts {
            let rel = path.strip_prefix(run_dir).unwrap_or(path);
            let size = std::fs::metadata(path)
                .map(|m| format!("{} B", m.len()))
                .unwrap_or_else(|_| "?".to_string());
            lines.push(format!("- `{}` ({})", rel.display(), size));
        }
        lines.push(String::new());
    }

    // Check for a pipeline summary file
    let summary_candidates = ["pipeline_summary.json", "summary.json", "decision.md"];
    for candidate in &summary_candidates {
        let candidate_path = run_dir.join(candidate);
        if candidate_path.exists() {
            let content = std::fs::read_to_string(&candidate_path)
                .unwrap_or_else(|_| String::from("(unreadable)"));
            lines.push(format!("## {candidate}\n"));
            lines.push(format!("```\n{content}\n```\n"));
        }
    }

    Ok(lines.join("\n"))
}

pub async fn execute(args: ReportArgs) -> Result<()> {
    let report = generate_report(&args.run_dir)?;
    println!("{report}");

    if let Some(output_path) = args.output {
        std::fs::write(&output_path, &report)?;
        println!("\nReport written to {}", output_path.display());
    }

    Ok(())
}
