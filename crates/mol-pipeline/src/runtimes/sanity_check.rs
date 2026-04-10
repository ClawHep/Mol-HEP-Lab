//! Sanity-check agent runtime.
//!
//! Ports `backend/agent/researchclaw/pipeline/sanity_check/runtime.py`.
//!
//! Responsibilities:
//! - Prepare an isolated workspace (copy experiment files + symlink shared dirs)
//! - Build the system and user prompts for the sanity-check LLM agent
//! - Detect success / failure: structured verdict file > output existence > phrase matching
//! - Copy fixes back to the authoritative experiment directory

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Pass / fail phrase lists (ported from Python)
// ---------------------------------------------------------------------------

const PASS_PHRASES: &[&str] = &[
    "smoke test pass",
    "sanity check pass",
    "test passed",
    "completed successfully",
    "all checks pass",
    "exit code 0",
    "smoke_test passed",
    "tests pass",
    "all tests pass",
    "successfully completed",
    "0 failures",
    "0 errors",
    "no errors",
    "no failures",
    "ran successfully",
    "execution successful",
    "passed all",
    "checks passed",
    "validation passed",
    "sanity passed",
];

const FAIL_PHRASES: &[&str] = &[
    "traceback",
    "not pass",
    "does not pass",
    "exit code 1",
    "unable to fix",
    "syntax error",
    "import error",
    "module not found",
    "file not found",
    "segmentation fault",
    "killed",
    "out of memory",
    "assertion error",
];

// ---------------------------------------------------------------------------
// Workspace preparation
// ---------------------------------------------------------------------------

/// Scan stage directories backward (stage-XX/) looking for `experiment/main.py`.
///
/// Returns the first matching directory, or `None` if not found.
pub fn find_experiment_dir(run_dir: &Path) -> Option<PathBuf> {
    // Collect and sort stage dirs in descending order
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

    stage_dirs.sort_by(|a, b| b.cmp(a)); // descending

    for stage_dir in stage_dirs {
        let candidate = stage_dir.join("experiment").join("main.py");
        if candidate.exists() {
            return Some(stage_dir.join("experiment"));
        }
    }
    None
}

/// Copy experiment files into an isolated workspace under `stage_dir/workspace/`.
///
/// Symlinks are created for `datasets`, `checkpoints`, and `codebases`
/// directories found in `run_dir`.  The `EXPERIMENT_PLAN.yaml` is copied from
/// `run_dir` when present.
pub fn prepare_workspace(
    stage_dir: &Path,
    experiment_dir: &Path,
    run_dir: &Path,
    _config: &crate::executor::MolConfig,
) -> Result<PathBuf> {
    let workspace = stage_dir.join("workspace");
    fs::create_dir_all(&workspace).context("create workspace dir")?;

    // Copy experiment files (non-symlink regular files only)
    copy_dir_contents(experiment_dir, &workspace)?;

    // Symlink shared resource directories
    for dir_name in &["datasets", "checkpoints", "codebases"] {
        let src = run_dir.join(dir_name);
        if src.exists() {
            let link = workspace.join(dir_name);
            if !link.exists() {
                symlink_dir(&src, &link)?;
            }
        }
    }

    // Copy EXPERIMENT_PLAN.yaml if present
    for name in &["EXPERIMENT_PLAN.yaml", "exp_plan.yaml"] {
        let plan = run_dir.join(name);
        if plan.exists() {
            fs::copy(&plan, workspace.join("EXPERIMENT_PLAN.yaml"))
                .context("copy EXPERIMENT_PLAN.yaml")?;
            break;
        }
    }

    Ok(workspace)
}

// ---------------------------------------------------------------------------
// File listing
// ---------------------------------------------------------------------------

/// List `.py` files in `workspace`, excluding `__pycache__`, `.git`, and
/// hidden directories.  Returns relative paths as strings.
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

        // Skip hidden dirs, __pycache__, .git
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

/// Build the system prompt for the sanity-check agent.
pub fn build_system_prompt(python_path: &str, workspace_path: &Path) -> String {
    format!(
        r#"You are a Python debugging expert helping to verify and fix a research experiment.

Your task is to perform a sanity check on the experiment code located at: {workspace}

Python interpreter: {python}

## Your Objectives

1. Read and understand the experiment code structure
2. Run the experiment with a minimal/smoke-test configuration (small dataset, few epochs)
3. Identify and fix any import errors, syntax errors, or runtime errors
4. Ensure the experiment can complete at least one training iteration
5. Verify outputs are written to the expected locations

## Working Guidelines

- Work in the directory: {workspace}
- Use `{python}` to run Python scripts
- Make targeted, minimal fixes — do not refactor the architecture
- After fixes, verify the smoke test passes
- Report the final status clearly

## Success Criteria

The sanity check passes when the experiment runs to completion without errors
and produces output files.

## REQUIRED: Final Verdict

After completing your sanity check, you MUST write a file `sanity_verdict.json` to
the workspace directory with your structured assessment:

```json
{{
  "verdict": "pass" or "fail",
  "summary": "One-sentence explanation of the result",
  "errors_found": ["list of errors encountered, empty if none"],
  "fixes_applied": ["list of fixes you made, empty if none"],
  "outputs_verified": ["list of output files confirmed to exist"]
}}
```

Rules:
- `"verdict": "pass"` means the code runs end-to-end and produces expected outputs.
- `"verdict": "fail"` means there are unresolved errors preventing successful execution.
- Always write this file, even if the experiment fails — the pipeline reads it for decisions.
"#,
        workspace = workspace_path.display(),
        python = python_path,
    )
}

/// Build the user message for the sanity-check agent.
pub fn build_user_message(workspace: &Path, files: &[String], plan_summary: &str) -> String {
    let file_list = if files.is_empty() {
        "  (no Python files found)".to_owned()
    } else {
        files
            .iter()
            .map(|f| format!("  - {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"Please perform a sanity check on the experiment in:
  {workspace}

## Experiment Plan Summary
{plan_summary}

## Python Files to Review
{file_list}

## Steps to Follow

1. Review the main entry point (main.py or similar)
2. Check all imports are resolvable
3. Run the experiment with a minimal configuration (e.g., 1 epoch, small batch)
4. Fix any errors encountered
5. Confirm the smoke test passes

Begin by reading the main experiment file and checking for obvious issues.
"#,
        workspace = workspace.display(),
        plan_summary = plan_summary,
        file_list = file_list,
    )
}

// ---------------------------------------------------------------------------
// Success detection
// ---------------------------------------------------------------------------

/// Check for a structured `sanity_verdict.json` file in the workspace.
///
/// **Primary method**: The agent is prompted to write this file with a structured
/// verdict (`"pass"` or `"fail"`) plus reasoning. This avoids brittle phrase
/// matching and gives the agent semantic control over the decision.
///
/// Returns `Some(true)` for pass, `Some(false)` for fail, `None` if no verdict file.
pub fn check_verdict_file(workspace: &std::path::Path) -> Option<bool> {
    let path = workspace.join("sanity_verdict.json");
    let content = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let verdict = v.get("verdict")?.as_str()?.to_lowercase();
    let summary = v.get("summary").and_then(|s| s.as_str()).unwrap_or("");
    tracing::info!(
        verdict = %verdict,
        summary = %summary,
        "Sanity verdict from structured JSON"
    );
    Some(verdict == "pass" || verdict == "passed")
}

/// Determine whether the agent run succeeded.
///
/// **Priority order:**
/// 1. Structured verdict file (`sanity_verdict.json`) — agent's semantic decision
/// 2. Output file existence check — structural evidence of success
/// 3. Phrase matching in LLM response — legacy fallback
///
/// The structured verdict is preferred because it lets the agent reason about
/// success/failure holistically rather than relying on brittle keyword matching.
pub fn check_success(
    final_text: &str,
    errors: &[String],
    iterations: u32,
    max_iterations: u32,
) -> bool {
    // Errors from the runtime are hard failures regardless of verdict
    if !errors.is_empty() {
        return false;
    }

    // Hit the iteration ceiling — treat as failure
    if iterations >= max_iterations {
        return false;
    }

    // Legacy phrase-based detection (fallback only — verdict file is checked separately)
    let lower = final_text.to_lowercase();

    // Explicit pass phrase
    if PASS_PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }

    // Explicit fail phrase
    if FAIL_PHRASES.iter().any(|p| lower.contains(p)) {
        return false;
    }

    // No strong signal after 2+ iterations with no errors — accept
    if iterations >= 2 {
        return true;
    }

    false
}

/// Check success by looking for expected output files on disk.
///
/// This is a structural check complementing the verdict-based approach.
/// If the experiment produced figures or results files, it likely succeeded.
pub fn check_outputs_exist(workspace: &std::path::Path) -> bool {
    let indicators = ["figures", "results", "output"];
    for name in &indicators {
        let dir = workspace.join(name);
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                if entries.count() > 0 {
                    return true;
                }
            }
        }
    }
    // Check for common output files
    let file_indicators = ["results.json", "output.json", "fit_results.json", "summary.json",
                           "run_report.json", "sanity_verdict.json"];
    for name in &file_indicators {
        if workspace.join(name).exists() {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// File I/O helpers
// ---------------------------------------------------------------------------

/// Copy modified `.py` files from `workspace` back to `experiment_dir`.
///
/// Returns the number of files copied.
pub fn copy_fixes_back(workspace: &Path, experiment_dir: &Path) -> Result<u32> {
    let mut count = 0u32;
    copy_py_files(workspace, workspace, experiment_dir, &mut count)?;
    Ok(count)
}

fn copy_py_files(
    workspace_root: &Path,
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

        if name_str.starts_with('.') || name_str == "__pycache__" {
            continue;
        }

        if path.is_dir() && !path.is_symlink() {
            let rel = path.strip_prefix(workspace_root)?;
            let dest_dir = dest_root.join(rel);
            fs::create_dir_all(&dest_dir).ok();
            copy_py_files(workspace_root, &path, dest_root, count)?;
        } else if path.is_file() && path.extension().map(|e| e == "py").unwrap_or(false) {
            let rel = path.strip_prefix(workspace_root)?;
            let dest = dest_root.join(rel);
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

/// Load the experiment plan summary from prior stages.
///
/// Looks for `exp_plan.yaml` or `EXPERIMENT_PLAN.yaml` in `run_dir`, then
/// in each `stage-XX/` directory (descending). Falls back to an empty string.
pub fn load_plan_summary(run_dir: &Path) -> String {
    // Try run-level first
    for name in &["exp_plan.yaml", "EXPERIMENT_PLAN.yaml"] {
        let p = run_dir.join(name);
        if let Ok(content) = fs::read_to_string(&p) {
            return content;
        }
    }

    // Search stage dirs descending
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
            let p = stage_dir.join(name);
            if let Ok(content) = fs::read_to_string(&p) {
                return content;
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
    // On non-Unix (Windows) fall back to a junction/copy stub
    // Real junction support would need `junction` crate; for now copy.
    copy_dir_contents(src, dst)
}

/// Recursively copy directory contents (non-symlink files only).
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
            // Skip symlinks when copying
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
    use std::fs;
    use tempfile::TempDir;

    fn make_experiment(run_dir: &Path, stage_num: u32) {
        let stage_dir = run_dir.join(format!("stage-{stage_num:02}"));
        let exp_dir = stage_dir.join("experiment");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::write(exp_dir.join("main.py"), "# main").unwrap();
    }

    #[test]
    fn find_experiment_dir_finds_latest() {
        let tmp = TempDir::new().unwrap();
        make_experiment(tmp.path(), 3);
        make_experiment(tmp.path(), 7);

        let found = find_experiment_dir(tmp.path()).unwrap();
        // Should return the stage-07 one (highest)
        assert!(found.to_string_lossy().contains("stage-07"));
    }

    #[test]
    fn find_experiment_dir_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("stage-01")).unwrap();
        assert!(find_experiment_dir(tmp.path()).is_none());
    }

    #[test]
    fn list_experiment_files_excludes_pycache() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        fs::write(ws.join("main.py"), "").unwrap();
        fs::write(ws.join("utils.py"), "").unwrap();
        let cache = ws.join("__pycache__");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("main.cpython-311.pyc"), "").unwrap();

        let files = list_experiment_files(ws);
        assert!(files.contains(&"main.py".to_owned()));
        assert!(files.contains(&"utils.py".to_owned()));
        assert!(!files.iter().any(|f| f.contains("__pycache__")));
    }

    #[test]
    fn check_success_pass_phrase() {
        assert!(check_success("smoke test pass", &[], 1, 10));
        assert!(check_success("Sanity Check Pass — done", &[], 1, 10));
    }

    #[test]
    fn check_success_fail_phrase() {
        assert!(!check_success("there was an error", &[], 1, 10));
    }

    #[test]
    fn check_success_with_errors() {
        assert!(!check_success("", &["some error".to_owned()], 2, 10));
    }

    #[test]
    fn check_success_max_iterations() {
        assert!(!check_success("", &[], 10, 10));
    }

    #[test]
    fn check_success_two_iterations_no_errors() {
        assert!(check_success("", &[], 2, 10));
    }

    #[test]
    fn check_success_one_iteration_no_errors_no_phrase() {
        assert!(!check_success("looks good", &[], 1, 10));
    }

    #[test]
    fn prepare_workspace_creates_dir() {
        let tmp = TempDir::new().unwrap();
        let run_dir = tmp.path().join("run");
        let stage_dir = run_dir.join("stage-12");
        let exp_dir = run_dir.join("experiment");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::write(exp_dir.join("main.py"), "print('hi')").unwrap();
        fs::create_dir_all(&stage_dir).unwrap();

        let config = crate::executor::MolConfig::default();
        let ws = prepare_workspace(&stage_dir, &exp_dir, &run_dir, &config).unwrap();
        assert!(ws.exists());
        assert!(ws.join("main.py").exists());
    }

    #[test]
    fn load_plan_summary_returns_content() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("exp_plan.yaml"), "topic: test").unwrap();
        let summary = load_plan_summary(tmp.path());
        assert!(summary.contains("topic: test"));
    }

    #[test]
    fn copy_fixes_back_copies_py_files() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let exp = tmp.path().join("experiment");
        fs::create_dir_all(&ws).unwrap();
        fs::create_dir_all(&exp).unwrap();
        fs::write(ws.join("main.py"), "# fixed").unwrap();
        fs::write(ws.join("notes.txt"), "notes").unwrap();

        let count = copy_fixes_back(&ws, &exp).unwrap();
        assert_eq!(count, 1);
        assert!(exp.join("main.py").exists());
        assert!(!exp.join("notes.txt").exists());
    }
}
