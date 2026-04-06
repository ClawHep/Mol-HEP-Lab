//! Iterative-refinement agent runtime.
//!
//! Ports `backend/agent/researchclaw/pipeline/iterative_refine/runtime.py`.
//!
//! Responsibilities:
//! - Locate the experiment directory (also checks `experiment_final/main.py`)
//! - Load baseline results and compare metric improvements
//! - Prepare an isolated workspace
//! - Build prompts for the iterative-refinement LLM agent
//! - Write the best-found experiment to `experiment_final/`
//! - Copy results back to experiment dir and stage dir

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Experiment directory search
// ---------------------------------------------------------------------------

/// Scan stage directories backward for `experiment/main.py` **or**
/// `experiment_final/main.py`.  Returns the first match (highest stage number).
pub fn find_experiment_dir(run_dir: &Path) -> Option<PathBuf> {
    let mut stage_dirs: Vec<PathBuf> = fs::read_dir(run_dir)
        .ok()?
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
    stage_dirs.sort_by(|a, b| b.cmp(a));

    for stage_dir in &stage_dirs {
        for sub in &["experiment_final", "experiment"] {
            let candidate = stage_dir.join(sub).join("main.py");
            if candidate.exists() {
                return Some(stage_dir.join(sub));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Baseline results loading
// ---------------------------------------------------------------------------

/// Search for a `results.json` in experiment and runs directories.
///
/// Checks (in order):
/// 1. `run_dir/runs/*/results.json` (most recent)
/// 2. Stage experiment dirs descending
pub fn load_baseline_results(run_dir: &Path) -> String {
    // Check runs/ subdirectory
    let runs_dir = run_dir.join("runs");
    if runs_dir.exists() {
        let mut run_subdirs: Vec<PathBuf> = fs::read_dir(&runs_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        run_subdirs.sort_by(|a, b| b.cmp(a));

        for sub in run_subdirs {
            let p = sub.join("results.json");
            if let Ok(content) = fs::read_to_string(&p) {
                return content;
            }
        }
    }

    // Check stage experiment dirs
    let mut stage_dirs: Vec<PathBuf> = fs::read_dir(run_dir)
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
    stage_dirs.sort_by(|a, b| b.cmp(a));

    for stage_dir in stage_dirs {
        for sub in &["experiment_final", "experiment", "."] {
            let p = stage_dir.join(sub).join("results.json");
            if let Ok(content) = fs::read_to_string(&p) {
                return content;
            }
        }
    }

    String::new()
}

// ---------------------------------------------------------------------------
// Metric extraction and comparison
// ---------------------------------------------------------------------------

/// Extract a single numeric metric value from a JSON string.
///
/// The `metric_key` may use dot-notation (e.g. `"metrics.accuracy"`).
pub fn extract_metric(results_str: &str, metric_key: &str) -> Option<f64> {
    let value: serde_json::Value = serde_json::from_str(results_str).ok()?;
    let keys: Vec<&str> = metric_key.split('.').collect();
    let mut current = &value;
    for key in &keys {
        current = current.get(key)?;
    }
    current.as_f64()
}

/// Return `true` when the final result improves upon the baseline.
///
/// `direction` is `"minimize"` or `"maximize"` (case-insensitive).
pub fn check_improvement(
    baseline_str: &str,
    final_str: &str,
    metric_key: &str,
    direction: &str,
) -> bool {
    let baseline = match extract_metric(baseline_str, metric_key) {
        Some(v) => v,
        None => return false,
    };
    let final_val = match extract_metric(final_str, metric_key) {
        Some(v) => v,
        None => return false,
    };

    if direction.to_lowercase() == "minimize" {
        final_val < baseline
    } else {
        final_val > baseline
    }
}

// ---------------------------------------------------------------------------
// Workspace preparation
// ---------------------------------------------------------------------------

/// Prepare an isolated workspace under `stage_dir/workspace/`.
pub fn prepare_workspace(
    stage_dir: &Path,
    experiment_dir: &Path,
    _config: &crate::executor::MolConfig,
) -> Result<PathBuf> {
    let workspace = stage_dir.join("workspace");
    fs::create_dir_all(&workspace).context("create workspace dir")?;

    copy_dir_contents(experiment_dir, &workspace)?;

    // Symlink shared resource dirs
    if let Some(run_dir) = experiment_dir.parent().and_then(|p| p.parent()) {
        for dir_name in &["datasets", "checkpoints", "codebases"] {
            let src = run_dir.join(dir_name);
            if src.exists() {
                let link = workspace.join(dir_name);
                if !link.exists() {
                    symlink_dir(&src, &link).ok();
                }
            }
        }
    }

    Ok(workspace)
}

// ---------------------------------------------------------------------------
// File listing
// ---------------------------------------------------------------------------

/// List `.py` files in workspace excluding `__pycache__` and hidden dirs.
pub fn list_experiment_files(workspace: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_py_files(workspace, workspace, &mut files);
    files.sort();
    files
}

fn collect_py_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') || name_str == "__pycache__" {
            continue;
        }

        if path.is_dir() && !path.is_symlink() {
            collect_py_files(root, &path, out);
        } else if path.is_file() && path.extension().map(|e| e == "py").unwrap_or(false) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt building
// ---------------------------------------------------------------------------

/// Build the system prompt for the iterative-refinement agent.
pub fn build_system_prompt(
    python_path: &str,
    workspace: &Path,
    time_budget: u32,
    metric_key: &str,
    direction: &str,
    max_iterations: u32,
) -> String {
    format!(
        r#"You are an expert ML researcher performing iterative refinement of an experiment.

Workspace: {workspace}
Python interpreter: {python}
Time budget: {time_budget} minutes
Max refinement iterations: {max_iterations}

## Optimization Goal

Metric: `{metric}`
Direction: {direction} (lower = minimize, higher = maximize)

## Your Objectives

1. Understand the current experiment and its baseline performance
2. Identify the most promising improvements (algorithmic, hyperparameter, data)
3. Implement improvements iteratively — test each change
4. Track all results and keep the best configuration
5. Write the final best configuration to disk

## Guidelines

- Make targeted, principled changes — do not rewrite from scratch
- Run experiments and measure the target metric after each change
- Keep a refinement log of what you tried and the results
- Stop when time budget is exhausted or no further improvement is possible
- Save final results to `results.json` and a log to `refinement_log.json`
"#,
        workspace = workspace.display(),
        python = python_path,
        time_budget = time_budget,
        metric = metric_key,
        direction = direction,
        max_iterations = max_iterations,
    )
}

/// Build the user message for the iterative-refinement agent.
pub fn build_user_message(
    workspace: &Path,
    files: &[String],
    baseline_results: &str,
    metric_key: &str,
    direction: &str,
    max_iterations: u32,
    plan_summary: &str,
) -> String {
    let file_list = if files.is_empty() {
        "  (no Python files found)".to_owned()
    } else {
        files
            .iter()
            .map(|f| format!("  - {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let baseline_section = if baseline_results.is_empty() {
        "  (no baseline results available)".to_owned()
    } else {
        baseline_results.to_owned()
    };

    format!(
        r#"Please iteratively refine the experiment in:
  {workspace}

## Research Plan
{plan_summary}

## Baseline Results
{baseline_section}

## Optimization Target
- Metric: `{metric_key}`
- Direction: {direction}
- Max iterations: {max_iterations}

## Experiment Files
{file_list}

## Process

1. Review the current code and baseline performance
2. Propose and implement the most impactful improvement
3. Run the experiment and record the result
4. Repeat until time/iteration budget is exhausted
5. Save the best configuration and write `results.json` + `refinement_log.json`

Begin with a review of the experiment code and baseline results.
"#,
        workspace = workspace.display(),
        plan_summary = plan_summary,
        baseline_section = baseline_section,
        metric_key = metric_key,
        direction = direction,
        max_iterations = max_iterations,
        file_list = file_list,
    )
}

// ---------------------------------------------------------------------------
// Writing final experiment
// ---------------------------------------------------------------------------

/// Copy non-symlink, non-hidden files from `workspace` to `final_dir`.
///
/// Returns the number of files copied.
pub fn write_final_experiment(workspace: &Path, final_dir: &Path) -> Result<u32> {
    fs::create_dir_all(final_dir).context("create experiment_final dir")?;
    let mut count = 0u32;
    write_final_inner(workspace, workspace, final_dir, &mut count)?;
    Ok(count)
}

fn write_final_inner(
    root: &Path,
    src_dir: &Path,
    dest_root: &Path,
    count: &mut u32,
) -> Result<()> {
    let Ok(entries) = fs::read_dir(src_dir) else {
        return Ok(());
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden and symlinks
        if name_str.starts_with('.') || path.is_symlink() {
            continue;
        }

        let rel = path.strip_prefix(root)?;
        let dest = dest_root.join(rel);

        if path.is_dir() {
            fs::create_dir_all(&dest).ok();
            write_final_inner(root, &path, dest_root, count)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::copy(&path, &dest)
                .with_context(|| format!("copy {} -> {}", path.display(), dest.display()))?;
            *count += 1;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Copy results back
// ---------------------------------------------------------------------------

/// Copy `results.json`, `.py` files, and `outputs/` back to `experiment_dir`
/// and `stage_dir`.
pub fn copy_results_back(
    workspace: &Path,
    experiment_dir: &Path,
    stage_dir: &Path,
) -> Result<()> {
    for name in &["results.json", "refinement_log.json"] {
        let src = workspace.join(name);
        if src.exists() {
            fs::copy(&src, stage_dir.join(name))
                .with_context(|| format!("copy {name} to stage_dir"))?;
            fs::copy(&src, experiment_dir.join(name))
                .with_context(|| format!("copy {name} to experiment_dir"))?;
        }
    }

    copy_py_files_back(workspace, workspace, experiment_dir)?;

    let outputs_src = workspace.join("outputs");
    if outputs_src.exists() {
        copy_dir_contents(&outputs_src, &stage_dir.join("outputs"))?;
    }

    Ok(())
}

fn copy_py_files_back(root: &Path, src_dir: &Path, dest_root: &Path) -> Result<()> {
    let Ok(entries) = fs::read_dir(src_dir) else {
        return Ok(());
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') || name_str == "__pycache__" {
            continue;
        }

        if path.is_dir() && !path.is_symlink() {
            let rel = path.strip_prefix(root)?;
            let dest_dir = dest_root.join(rel);
            fs::create_dir_all(&dest_dir).ok();
            copy_py_files_back(root, &path, dest_root)?;
        } else if path.is_file() && path.extension().map(|e| e == "py").unwrap_or(false) {
            let rel = path.strip_prefix(root)?;
            let dest = dest_root.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::copy(&path, &dest)
                .with_context(|| format!("copy {} -> {}", path.display(), dest.display()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GPU selection
// ---------------------------------------------------------------------------

/// Find GPU with lowest (50% mem + 50% util) score.  Falls back to `"0"`.
pub fn find_free_gpu() -> String {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,memory.used,memory.total,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return "0".to_owned(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut best_idx = "0".to_owned();
    let mut best_score = f64::MAX;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 4 {
            continue;
        }
        let idx = parts[0];
        let mem_used: f64 = parts[1].parse().unwrap_or(f64::MAX);
        let mem_total: f64 = parts[2].parse().unwrap_or(1.0);
        let util: f64 = parts[3].parse().unwrap_or(100.0);

        let mem_pct = if mem_total > 0.0 {
            mem_used / mem_total * 100.0
        } else {
            100.0
        };
        let score = 0.5 * mem_pct + 0.5 * util;

        if score < best_score {
            best_score = score;
            best_idx = idx.to_owned();
        }
    }

    best_idx
}

// ---------------------------------------------------------------------------
// Plan summary
// ---------------------------------------------------------------------------

/// Load experiment plan summary from run-level or stage directories.
pub fn load_plan_summary(run_dir: &Path) -> String {
    for name in &["exp_plan.yaml", "EXPERIMENT_PLAN.yaml"] {
        if let Ok(c) = fs::read_to_string(run_dir.join(name)) {
            return c;
        }
    }

    let mut stage_dirs: Vec<PathBuf> = fs::read_dir(run_dir)
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
    stage_dirs.sort_by(|a, b| b.cmp(a));

    for stage_dir in stage_dirs {
        for name in &["exp_plan.yaml", "EXPERIMENT_PLAN.yaml"] {
            if let Ok(c) = fs::read_to_string(stage_dir.join(name)) {
                return c;
            }
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn symlink_dir(src: &Path, dst: &Path) -> Result<()> {
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("symlink {} -> {}", dst.display(), src.display()))
}

#[cfg(not(unix))]
fn symlink_dir(src: &Path, dst: &Path) -> Result<()> {
    copy_dir_contents(src, dst)
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

    fn make_stage_experiment(run_dir: &Path, stage_num: u32, sub: &str) {
        let dir = run_dir
            .join(format!("stage-{stage_num:02}"))
            .join(sub);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.py"), "# main").unwrap();
    }

    #[test]
    fn find_experiment_dir_finds_experiment_final() {
        let tmp = TempDir::new().unwrap();
        make_stage_experiment(tmp.path(), 3, "experiment");
        make_stage_experiment(tmp.path(), 5, "experiment_final");

        let found = find_experiment_dir(tmp.path()).unwrap();
        assert!(found.to_string_lossy().contains("experiment_final"));
    }

    #[test]
    fn find_experiment_dir_falls_back_to_experiment() {
        let tmp = TempDir::new().unwrap();
        make_stage_experiment(tmp.path(), 4, "experiment");

        let found = find_experiment_dir(tmp.path()).unwrap();
        assert!(found.to_string_lossy().contains("experiment"));
    }

    #[test]
    fn find_experiment_dir_none() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("stage-01")).unwrap();
        assert!(find_experiment_dir(tmp.path()).is_none());
    }

    #[test]
    fn extract_metric_flat() {
        let json = r#"{"accuracy": 0.95, "loss": 0.05}"#;
        assert_eq!(extract_metric(json, "accuracy"), Some(0.95));
        assert_eq!(extract_metric(json, "loss"), Some(0.05));
    }

    #[test]
    fn extract_metric_nested() {
        let json = r#"{"metrics": {"f1": 0.88}}"#;
        assert_eq!(extract_metric(json, "metrics.f1"), Some(0.88));
    }

    #[test]
    fn extract_metric_missing() {
        let json = r#"{"accuracy": 0.9}"#;
        assert_eq!(extract_metric(json, "loss"), None);
    }

    #[test]
    fn check_improvement_maximize() {
        let baseline = r#"{"accuracy": 0.80}"#;
        let final_r = r#"{"accuracy": 0.90}"#;
        assert!(check_improvement(baseline, final_r, "accuracy", "maximize"));
    }

    #[test]
    fn check_improvement_minimize() {
        let baseline = r#"{"loss": 0.50}"#;
        let final_r = r#"{"loss": 0.30}"#;
        assert!(check_improvement(baseline, final_r, "loss", "minimize"));
    }

    #[test]
    fn check_improvement_no_change() {
        let baseline = r#"{"loss": 0.50}"#;
        let final_r = r#"{"loss": 0.50}"#;
        assert!(!check_improvement(baseline, final_r, "loss", "minimize"));
    }

    #[test]
    fn check_improvement_missing_metric() {
        let baseline = r#"{"accuracy": 0.8}"#;
        let final_r = r#"{"accuracy": 0.9}"#;
        assert!(!check_improvement(baseline, final_r, "loss", "minimize"));
    }

    #[test]
    fn write_final_experiment_copies_files() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let final_dir = tmp.path().join("experiment_final");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("main.py"), "# main").unwrap();
        fs::write(ws.join("model.py"), "# model").unwrap();

        let count = write_final_experiment(&ws, &final_dir).unwrap();
        assert_eq!(count, 2);
        assert!(final_dir.join("main.py").exists());
        assert!(final_dir.join("model.py").exists());
    }

    #[test]
    fn write_final_experiment_skips_hidden() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let final_dir = tmp.path().join("experiment_final");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("main.py"), "").unwrap();
        fs::write(ws.join(".hidden"), "secret").unwrap();

        write_final_experiment(&ws, &final_dir).unwrap();
        assert!(final_dir.join("main.py").exists());
        assert!(!final_dir.join(".hidden").exists());
    }

    #[test]
    fn load_baseline_results_from_stage() {
        let tmp = TempDir::new().unwrap();
        let stage = tmp.path().join("stage-05").join("experiment");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("results.json"), r#"{"loss": 0.4}"#).unwrap();

        let content = load_baseline_results(tmp.path());
        assert!(content.contains("loss"));
    }
}
