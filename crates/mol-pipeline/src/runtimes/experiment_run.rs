//! Experiment-run agent runtime.
//!
//! Ports `backend/agent/researchclaw/pipeline/experiment_run/runtime.py`.
//!
//! Responsibilities:
//! - Select a free GPU via nvidia-smi
//! - Ensure dependencies are installed
//! - Prepare an isolated workspace
//! - Build prompts for the experiment-run LLM agent
//! - Detect success (presence of results.json) and copy outputs back

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Safe package allow-list
// ---------------------------------------------------------------------------

const SAFE_PACKAGES: &[&str] = &[
    "torch",
    "torchvision",
    "torchmetrics",
    "transformers",
    "diffusers",
    "accelerate",
    "peft",
    "safetensors",
    "einops",
    "PIL",
    "cv2",
    "numpy",
    "scipy",
    "pandas",
    "sklearn",
    "tqdm",
    "matplotlib",
];

// ---------------------------------------------------------------------------
// GPU selection
// ---------------------------------------------------------------------------

/// Find the GPU with the lowest combined (50 % mem + 50 % util) score.
///
/// Calls `nvidia-smi --query-gpu=index,memory.used,memory.total,utilization.gpu
/// --format=csv,noheader,nounits` and parses the CSV.  Falls back to `"0"` on
/// any error.
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
// Dependency check
// ---------------------------------------------------------------------------

/// Scan `.py` files in `experiment_dir` for import statements and verify that
/// any imports from the safe-package set are installed.
///
/// Returns the list of packages that were successfully verified.
pub fn ensure_deps(experiment_dir: &Path, python_path: &str) -> Vec<String> {
    let imports = collect_imports(experiment_dir);
    let safe: HashSet<&str> = SAFE_PACKAGES.iter().copied().collect();
    let to_check: Vec<&str> = imports
        .iter()
        .map(|s| s.as_str())
        .filter(|s| safe.contains(s))
        .collect();

    let mut installed = Vec::new();
    for pkg in to_check {
        let ok = Command::new(python_path)
            .args(["-c", &format!("import {pkg}")])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            installed.push(pkg.to_owned());
        }
    }
    installed
}

fn collect_imports(dir: &Path) -> Vec<String> {
    let mut imports = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return imports;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "py").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("import ") {
                        // `import foo, bar` — take first token
                        if let Some(name) = rest.split_whitespace().next() {
                            imports.push(name.trim_end_matches(',').to_owned());
                        }
                    } else if let Some(rest) = line.strip_prefix("from ") {
                        if let Some(name) = rest.split_whitespace().next() {
                            imports.push(name.trim_end_matches('.').to_owned());
                        }
                    }
                }
            }
        }
    }
    imports.sort();
    imports.dedup();
    imports
}

// ---------------------------------------------------------------------------
// Workspace preparation
// ---------------------------------------------------------------------------

/// Prepare an isolated workspace for the experiment run under `stage_dir/workspace/`.
pub fn prepare_workspace(
    stage_dir: &Path,
    experiment_dir: &Path,
    _config: &crate::executor::MolConfig,
) -> Result<PathBuf> {
    let workspace = stage_dir.join("workspace");
    fs::create_dir_all(&workspace).context("create workspace dir")?;

    copy_dir_contents(experiment_dir, &workspace)?;

    // Symlink shared resource dirs from the parent of experiment_dir
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

/// List `.py` files in `workspace`, excluding `__pycache__` and hidden dirs.
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

/// Build the system prompt for the experiment-run agent.
pub fn build_system_prompt(
    python_path: &str,
    workspace: &Path,
    time_budget: u32,
    gpu_id: &str,
) -> String {
    format!(
        r#"You are an expert ML researcher and engineer running a scientific experiment.

Your workspace: {workspace}
Python interpreter: {python}
Available GPU: cuda:{gpu}
Time budget: {time_budget} minutes

## Your Objectives

1. Review the experiment code and understand the research goal
2. Set up the environment (verify dependencies, data paths, etc.)
3. Run the full experiment — do NOT use toy/smoke-test settings
4. Monitor progress and handle any runtime errors
5. Save results to `results.json` in the workspace when complete

## Working Guidelines

- Use GPU cuda:{gpu} for all training (set CUDA_VISIBLE_DEVICES={gpu})
- The experiment should run to completion within the time budget
- Do not modify the core algorithm — only fix engineering issues
- Write intermediate checkpoints in case of interruptions
- After completion, write a `results.json` with all evaluation metrics

## Output Format for results.json

```json
{{
  "metric_name": value,
  "additional_metrics": {{...}},
  "training_time_seconds": value,
  "notes": "..."
}}
```
"#,
        workspace = workspace.display(),
        python = python_path,
        gpu = gpu_id,
        time_budget = time_budget,
    )
}

/// Build the user message for the experiment-run agent.
pub fn build_user_message(
    workspace: &Path,
    files: &[String],
    time_budget: u32,
    metric_key: &str,
    metric_direction: &str,
    prior_results: &str,
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

    let prior_section = if prior_results.is_empty() {
        "  (no prior results)".to_owned()
    } else {
        prior_results.to_owned()
    };

    format!(
        r#"Please run the experiment in:
  {workspace}

## Optimization Target
- Metric: `{metric_key}`
- Direction: {direction} (lower is better = minimize, higher is better = maximize)

## Time Budget
{time_budget} minutes total

## Prior Results (if any)
{prior_section}

## Experiment Files
{file_list}

## Steps to Follow

1. Read main.py and understand the experiment setup
2. Verify all data paths and dependencies
3. Run the full experiment with `CUDA_VISIBLE_DEVICES` set appropriately
4. Monitor for errors and resolve them
5. Save final metrics to `results.json`

Start by reading the main experiment file.
"#,
        workspace = workspace.display(),
        metric_key = metric_key,
        direction = metric_direction,
        time_budget = time_budget,
        prior_section = prior_section,
        file_list = file_list,
    )
}

// ---------------------------------------------------------------------------
// Success detection
// ---------------------------------------------------------------------------

/// Returns `true` when `workspace/results.json` exists and contains valid JSON.
pub fn check_success(workspace: &Path) -> bool {
    let results_path = workspace.join("results.json");
    if !results_path.exists() {
        return false;
    }
    match fs::read_to_string(&results_path) {
        Ok(content) => serde_json::from_str::<serde_json::Value>(&content).is_ok(),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/// Copy `results.json`, modified `.py` files, and the `outputs/` directory from
/// workspace back to `experiment_dir` and `stage_dir`.
pub fn copy_results_back(
    workspace: &Path,
    experiment_dir: &Path,
    stage_dir: &Path,
) -> Result<()> {
    // results.json → stage_dir and experiment_dir
    let results_src = workspace.join("results.json");
    if results_src.exists() {
        fs::copy(&results_src, stage_dir.join("results.json"))
            .context("copy results.json to stage_dir")?;
        fs::copy(&results_src, experiment_dir.join("results.json"))
            .context("copy results.json to experiment_dir")?;
    }

    // .py files → experiment_dir
    copy_py_files_back(workspace, workspace, experiment_dir)?;

    // outputs/ directory → stage_dir/outputs/
    let outputs_src = workspace.join("outputs");
    if outputs_src.exists() {
        let outputs_dst = stage_dir.join("outputs");
        copy_dir_contents(&outputs_src, &outputs_dst)?;
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

/// Load the sanity-check report from a prior stage directory.
///
/// Searches stage directories (descending) for `sanity_report.json`.
pub fn load_sanity_results(run_dir: &Path) -> String {
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
        let p = stage_dir.join("sanity_report.json");
        if let Ok(content) = fs::read_to_string(&p) {
            return content;
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

    #[test]
    fn check_success_missing_results() {
        let tmp = TempDir::new().unwrap();
        assert!(!check_success(tmp.path()));
    }

    #[test]
    fn check_success_valid_json() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("results.json"), r#"{"accuracy": 0.95}"#).unwrap();
        assert!(check_success(tmp.path()));
    }

    #[test]
    fn check_success_invalid_json() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("results.json"), "not json").unwrap();
        assert!(!check_success(tmp.path()));
    }

    #[test]
    fn list_experiment_files_basic() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("train.py"), "").unwrap();
        fs::write(tmp.path().join("model.py"), "").unwrap();
        fs::write(tmp.path().join("README.md"), "").unwrap();

        let files = list_experiment_files(tmp.path());
        assert!(files.contains(&"train.py".to_owned()));
        assert!(files.contains(&"model.py".to_owned()));
        assert!(!files.iter().any(|f| f.ends_with(".md")));
    }

    #[test]
    fn prepare_workspace_copies_files() {
        let tmp = TempDir::new().unwrap();
        let exp = tmp.path().join("experiment");
        let stage = tmp.path().join("stage-14");
        fs::create_dir_all(&exp).unwrap();
        fs::create_dir_all(&stage).unwrap();
        fs::write(exp.join("main.py"), "# main").unwrap();

        let config = crate::executor::MolConfig::default();
        let ws = prepare_workspace(&stage, &exp, &config).unwrap();
        assert!(ws.join("main.py").exists());
    }

    #[test]
    fn copy_results_back_copies_json_and_py() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let exp = tmp.path().join("experiment");
        let stage = tmp.path().join("stage");
        fs::create_dir_all(&ws).unwrap();
        fs::create_dir_all(&exp).unwrap();
        fs::create_dir_all(&stage).unwrap();

        fs::write(ws.join("results.json"), r#"{"loss": 0.1}"#).unwrap();
        fs::write(ws.join("model.py"), "# model").unwrap();

        copy_results_back(&ws, &exp, &stage).unwrap();
        assert!(stage.join("results.json").exists());
        assert!(exp.join("results.json").exists());
        assert!(exp.join("model.py").exists());
    }

    #[test]
    fn collect_imports_finds_torch() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("train.py"),
            "import torch\nfrom numpy import array\n",
        )
        .unwrap();
        let imports = collect_imports(tmp.path());
        assert!(imports.contains(&"torch".to_owned()));
        assert!(imports.contains(&"numpy".to_owned()));
    }
}
