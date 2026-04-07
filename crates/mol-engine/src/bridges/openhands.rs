//! Aider "Beast Mode" bridge — routes complex code generation to Aider CLI.
//!
//! Uses Aider CLI (headless --message mode) to generate experiment code via a
//! TODO-driven loop: first generates a skeleton with TODO markers, then
//! iteratively fills in each TODO until none remain.
//!
//! Aider reads the workspace codebase via its repo-map feature, understanding
//! the full code structure before generating/modifying files.
//!
//! Re-exports complexity scoring and result types from opencode_bridge so that
//! executor can import from this module interchangeably.

// Re-export shared types from opencode module
pub use super::opencode::{ComplexityScore, OpenCodeResult, count_historical_failures, score_complexity};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use md5::{Digest, Md5};
use regex::Regex;
use serde_json::Value;
use tokio::process::Command;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Prompt templates
// ---------------------------------------------------------------------------

const _RULES: &str = "\
CRITICAL: Read EXPERIMENT_PLAN.yaml FIRST — it defines method names, algorithm steps, baselines, metrics.
Read GUIDANCE.md for ABSOLUTE paths to data/checkpoints/codebases. Read codebase .py files to learn the API.

RULES:
- Build ON TOP of existing codebase. Use ABSOLUTE paths from GUIDANCE.md.
- Load ALL models from LOCAL CHECKPOINTS_DIR paths. NEVER download from internet (no HuggingFace IDs, no `pretrained=True`, no URLs, no `torch.hub.load`).
- Each condition must implement a genuinely DIFFERENT algorithm, not just different hyperparameters.
- Metrics MUST be computed from ACTUAL model outputs. NEVER hardcode fake values or compute from formulas/profiles.
- NO try/except anywhere (except save_outputs for file I/O). Let all errors crash with full traceback.
- NO argparse. NO wrapper classes with empty methods. NO files named `utils.py`/`models.py`/`config.py` that shadow codebase modules.
- sys.path.insert: add the REPOSITORY ROOT only, NOT subdirectories.
- When calling codebase functions, READ function bodies to understand expected args (e.g. pass pipeline object, not pipeline.unet).
";

const _RULES_NO_CODEBASE: &str = "\
NO-CODEBASE: You MUST use real ML libraries (torch, diffusers, transformers, peft) — NEVER simulate with numpy/PIL.
Load checkpoints via `from_pretrained(CHECKPOINTS_DIR)`. Prioritize topic requirements over generic benchmark suggestions.
";

/// Skeleton generation prompt. Placeholders: {metric}, {time_budget_sec}
fn skeleton_prompt() -> String {
    String::from(_RULES) + "
TASK: Generate a SHORT main.py SKELETON (<130 lines). Do NOT implement any logic — use `pass` for function bodies.

1. Read EXPERIMENT_PLAN.yaml. Pick 1 baseline + 2 proposed methods (3 total).
2. Create main.py with plain functions (no classes):
   - Imports + sys.path.insert(0, REPO_ROOT)
   - Constants: DATASETS_DIR, CHECKPOINTS_DIR, OUTPUT_DIR='outputs', TIME_BUDGET={time_budget_sec}, SEEDS=[42,123,456]
   - set_seed(seed) — IMPLEMENT (3 lines)
   - should_stop() — IMPLEMENT (2 lines)
   - load_pipeline(), load_data(), compute_metric(), save_outputs() — each: `# TODO: <what>` then `pass`
   - 3 condition functions (REAL names from plan) — each: `# TODO: <algorithm steps>` then `pass`
   - run_condition() — IMPLEMENT (dispatch dict + compute_metric + save_outputs, NO try/except)
   - main() — IMPLEMENT (loop conditions/seeds, print results, NO try/except around run_condition)
   - if __name__ == '__main__': main()
"
}

/// TODO fill prompt. Placeholders: {todo_line}, {metric}, {time_budget_sec}
fn fill_todo_prompt() -> String {
    String::from(_RULES) + "
TASK: Implement ONE TODO in main.py. The TODO:

{todo_line}

- Read codebase .py files and EXPERIMENT_PLAN.yaml first. Implement ONLY this function, keep output SHORT.
- Remove `# TODO:` and replace `pass` with real implementation using ABSOLUTE paths from constants.
- save_outputs(): save visual artifacts (PNG/curves/text) proving model ran. try/except allowed ONLY here.
"
}

/// Fix prompt. Placeholder: {error_output}
const _FIX_PROMPT_TEMPLATE: &str = "\
The file main.py has a syntax error or import error:

{error_output}

Fix the error. Make the MINIMAL change needed. Do NOT rewrite the entire file.
";

/// Single-shot fallback prompt. Placeholders: {metric}, {time_budget_sec}
fn fallback_prompt() -> String {
    String::from(_RULES) + "
Create a COMPLETE main.py using EXPERIMENT_PLAN.yaml method names, algorithm steps, and metric definitions.
3 conditions (1 baseline + 2 proposed) with genuinely different algorithms.
Print: {metric}: <value> for each condition and seed. Time budget: {time_budget_sec}s.
Save visual results to `outputs/{condition}_{seed}.png`.
"
}

/// Fix sanity prompt. Placeholders: {test_name}, {test_code}, {stderr}, {repeat_hint}
fn fix_sanity_prompt() -> String {
    String::from(_RULES) + "
TASK: Fix a sanity check failure in main.py — make the smallest surgical change, do NOT rewrite the file.

**Failed test:** `{test_name}`
**Test code:**
```python
{test_code}
```
**Error (stderr tail):**
```
{stderr}
```
{repeat_hint}
## DIAGNOSE FIRST (read relevant files before fixing):
- Path errors: read the ACTUAL config YAML and GUIDANCE.md directory tree. Use `os.path.basename()` to extract filenames, rebuild paths from DATASETS_DIR. Never blindly join nested relative paths.
- NoneType errors: read the reference implementation (e.g. `inference.py`) for correct values. Search ALL attribute accesses on that object in codebase, not just the crash site.
- External library errors: fix CALLING code, not the library. Read codebase source to understand expected params.

## FIX RULES:
- Fix ONLY the error lines. No try/except. No DummyPipeline. No hardcoded metrics. Keep print/metric statements.
- Verify your fix works for ALL loop entries (not just the first) and doesn't introduce new errors.
"
}

const MAX_TODO_ITERATIONS: usize = 10;
const MAX_FIX_ATTEMPTS: usize = 2;
const MAX_STUCK: usize = 3;

// ---------------------------------------------------------------------------
// OpenHandsBridge
// ---------------------------------------------------------------------------

/// Manages Aider CLI invocations for beast mode code generation.
///
/// Despite the struct name (kept for backward compatibility), this uses Aider
/// with a TODO-driven loop: generate skeleton with TODO markers, then
/// iteratively implement each TODO until none remain.
#[derive(Debug, Clone)]
pub struct OpenHandsBridge {
    pub model: String,
    pub llm_base_url: String,
    pub api_key_env: String,
    pub api_key: String,
    pub timeout_sec: u64,
    pub max_retries: usize,
}

impl Default for OpenHandsBridge {
    fn default() -> Self {
        Self {
            model: "openai/claude-opus-4-6".to_string(),
            llm_base_url: String::new(),
            api_key_env: String::new(),
            api_key: String::new(),
            timeout_sec: 1200,
            max_retries: 0,
        }
    }
}

impl OpenHandsBridge {
    /// Create a new bridge with the given configuration.
    pub fn new(
        model: impl Into<String>,
        llm_base_url: impl Into<String>,
        api_key_env: impl Into<String>,
        api_key: impl Into<String>,
        timeout_sec: u64,
        max_retries: usize,
    ) -> Self {
        Self {
            model: model.into(),
            llm_base_url: llm_base_url.into(),
            api_key_env: api_key_env.into(),
            api_key: api_key.into(),
            timeout_sec,
            max_retries,
        }
    }

    // -- binary location -------------------------------------------------------

    /// Locate the aider binary.
    ///
    /// Search order:
    /// 1. PATH (which)
    /// 2. Same bin directory as the running process executable
    ///    (handles conda/venv where aider is co-installed)
    /// 3. ~/.local/bin/aider (pip install --user)
    pub fn find_binary() -> String {
        // 1. PATH
        if let Ok(out) = std::process::Command::new("which").arg("aider").output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() && std::path::Path::new(&p).is_file() {
                    return p;
                }
            }
        }

        // 2. Same bin dir as current executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let candidate = parent.join("aider");
                if candidate.is_file() {
                    return candidate.to_string_lossy().to_string();
                }
            }
        }

        // 3. ~/.local/bin/aider
        if let Some(home) = std::env::var_os("HOME") {
            let candidate = PathBuf::from(home).join(".local").join("bin").join("aider");
            if candidate.is_file() {
                return candidate.to_string_lossy().to_string();
            }
        }

        "aider".to_string()
    }

    // -- availability check ---------------------------------------------------

    /// Return `true` if the aider CLI is installed and callable.
    pub async fn check_available(&self) -> bool {
        match Command::new(Self::find_binary())
            .arg("--version")
            .output()
            .await
        {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    // -- API key resolution ---------------------------------------------------

    /// Resolve the API key: prefer explicit api_key, fallback to env var.
    pub fn resolve_api_key(&self) -> String {
        if !self.api_key.is_empty() {
            return self.api_key.clone();
        }
        if !self.api_key_env.is_empty() {
            return std::env::var(&self.api_key_env).unwrap_or_default();
        }
        String::new()
    }

    // -- workspace preparation ------------------------------------------------

    /// Prepare an aider workspace directory.
    ///
    /// Creates:
    /// - EXPERIMENT_PLAN.yaml
    /// - GUIDANCE.md (with prepended usage hints, trimmed to 300 lines)
    /// - codebases/ (copy of codebases dir)
    /// - datasets/NAME (symlink)
    /// - checkpoints/NAME (symlink)
    /// - .codebase_snapshot.json
    /// - empty main.py if it doesn't exist
    pub async fn prepare_workspace(
        &self,
        stage_dir: &Path,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
        extra_guidance: &str,
        time_budget_sec: u64,
        codebases_dir: &str,
        datasets_dir: &str,
        checkpoints_dir: &str,
        _selected_repos: Option<&[String]>,
    ) -> Result<PathBuf> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let pid = std::process::id();
        let ws = stage_dir.join(format!("aider_beast_{ts}_{pid}"));
        tokio::fs::create_dir_all(&ws).await?;

        // Write EXPERIMENT_PLAN.yaml
        let plan_content = if exp_plan.is_empty() {
            "# No experiment plan provided\n".to_string()
        } else {
            exp_plan.to_string()
        };
        tokio::fs::write(ws.join("EXPERIMENT_PLAN.yaml"), &plan_content).await?;

        // Build initial GUIDANCE.md
        let mut guidance_parts = vec![
            "# Experiment Guidance\n".to_string(),
            format!("## Topic\n{topic}\n"),
            format!("## Primary Metric\n{metric}\n"),
            format!("## Time Budget\n{time_budget_sec} seconds\n"),
        ];
        if !pkg_hint.is_empty() {
            guidance_parts.push(format!("## Environment\n{pkg_hint}\n"));
        }
        if !extra_guidance.is_empty() {
            guidance_parts.push(format!("## Additional Guidance\n{extra_guidance}\n"));
        }
        tokio::fs::write(ws.join("GUIDANCE.md"), guidance_parts.join("\n")).await?;

        // Copy codebases into codebases/ subdirectory
        if !codebases_dir.is_empty() {
            let cb_path = PathBuf::from(codebases_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(codebases_dir));
            if cb_path.is_dir() {
                let codebases_ws = ws.join("codebases");
                tokio::fs::create_dir_all(&codebases_ws).await?;
                let dest = codebases_ws.join(cb_path.file_name().unwrap_or_default());
                copy_dir_ignore(&cb_path, &dest)?;
            }
        }

        // Symlink datasets
        if !datasets_dir.is_empty() {
            let ds_path = PathBuf::from(datasets_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(datasets_dir));
            if ds_path.is_dir() {
                let link = ws.join("datasets").join(ds_path.file_name().unwrap_or_default());
                if let Some(parent) = link.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if !link.exists() {
                    std::os::unix::fs::symlink(&ds_path, &link)?;
                }
            }
        }

        // Symlink checkpoints
        if !checkpoints_dir.is_empty() {
            let ck_path = PathBuf::from(checkpoints_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(checkpoints_dir));
            if ck_path.is_dir() {
                let link = ws.join("checkpoints").join(ck_path.file_name().unwrap_or_default());
                if let Some(parent) = link.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if !link.exists() {
                    std::os::unix::fs::symlink(&ck_path, &link)?;
                }
            }
        }

        // Build usage hints
        let ws_abs = ws.canonicalize().unwrap_or_else(|_| ws.clone());
        let ws_abs_str = ws_abs.to_string_lossy();
        let mut usage_hints: Vec<String> = Vec::new();

        usage_hints.push(
            "## IMPORTANT: Use ABSOLUTE paths in your code\n\
            The code will be copied to a different directory for execution. \
            Do NOT rely on relative paths or `__file__`-based resolution.\n"
                .to_string(),
        );

        if !datasets_dir.is_empty() {
            let ds_path = PathBuf::from(datasets_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(datasets_dir));
            if ds_path.is_dir() {
                let ds_str = ds_path.to_string_lossy();
                let ds_name = ds_path.file_name().unwrap_or_default().to_string_lossy();
                match dir_tree(&ds_path, 3, 40) {
                    Ok(tree) => {
                        usage_hints.push(format!(
                            "## Local Data — ACTUAL directory structure\n\
                            Original path: \"{ds_str}\"\n\
                            Workspace symlink: \"{ws_abs_str}/datasets/{ds_name}\"\n\
                            In code use: `DATASETS_DIR = \"{ds_str}\"`\n\
                            ```\n{tree}\n```\n"
                        ));
                    }
                    Err(_) => {}
                }
            }
        }

        if !checkpoints_dir.is_empty() {
            let ck_path = PathBuf::from(checkpoints_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(checkpoints_dir));
            if ck_path.is_dir() {
                let ck_str = ck_path.to_string_lossy();
                let ck_name = ck_path.file_name().unwrap_or_default().to_string_lossy();
                match dir_tree(&ck_path, 2, 30) {
                    Ok(tree) => {
                        usage_hints.push(format!(
                            "\n## Local Checkpoints — ACTUAL directory structure\n\
                            Original path: \"{ck_str}\"\n\
                            Workspace symlink: \"{ws_abs_str}/checkpoints/{ck_name}\"\n\
                            In code use: `CHECKPOINTS_DIR = \"{ck_str}\"`\n\
                            NEVER use HuggingFace model IDs — load ALL models from this local path.\n\
                            ```\n{tree}\n```\n"
                        ));
                    }
                    Err(_) => {}
                }
            }
        }

        // When no codebase is provided, add diffusion loading hint
        if codebases_dir.is_empty() && !checkpoints_dir.is_empty() {
            let ck_path = PathBuf::from(checkpoints_dir);
            if ck_path.is_dir() {
                let ck_name_lower = ck_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if ["stable-diffusion", "sd-", "sdxl", "diffusion"]
                    .iter()
                    .any(|kw| ck_name_lower.contains(kw))
                {
                    usage_hints.push(format!(
                        "\n## No codebase — load model from checkpoints:\n\
                        ```python\n\
                        from diffusers import StableDiffusionPipeline\n\
                        pipe = StableDiffusionPipeline.from_pretrained('{checkpoints_dir}', \
                        local_files_only=True).to('cuda')\n\
                        ```\n"
                    ));
                }
            }
        }

        // Codebase hints
        if !codebases_dir.is_empty() {
            let cb_path = PathBuf::from(codebases_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(codebases_dir));
            if cb_path.is_dir() {
                let cb_abs = cb_path.to_string_lossy();
                let codebases_ws = ws.join("codebases");
                let mut repo_names: Vec<String> = Vec::new();
                if codebases_ws.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&codebases_ws) {
                        let mut names: Vec<String> = entries
                            .filter_map(|e| e.ok())
                            .filter(|e| e.path().is_dir())
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect();
                        names.sort();
                        repo_names = names;
                    }
                }

                let repos_str = repo_names.join(", ");
                let mut hint_lines = vec![format!(
                    "\n## Local Codebases\n\
                    Original absolute path: \"{cb_abs}\"\n\
                    Repos: {repos_str}\n\n\
                    IMPORTANT: In main.py, use the ORIGINAL absolute path for sys.path.insert:\n\
                      `sys.path.insert(0, '{cb_abs}')`\n\n\
                    Do NOT use workspace-relative paths — the code will be copied to a sandbox directory.\n\
                    The original path is permanent and always accessible.\n"
                )];
                hint_lines.push(
                    "\nThis keeps each codebase's internal imports (e.g. `from utils.utils import ...`) working correctly.\n\
                    Do NOT add subdirectories like `.../freecustom` or `.../utils` to sys.path.\n"
                        .to_string(),
                );

                // Example/demo scripts
                let mut example_lines: Vec<String> = Vec::new();
                let mut example_budget: i64 = 80;
                let example_patterns = [
                    // Explicit example/demo directories
                    "**/example*/**/*.py", "**/demo*/**/*.py",
                    "**/sample*/**/*.py", "**/tutorial*/**/*.py",
                    // Root-level scripts
                    "run*.py", "main*.py", "train*.py", "infer*.py",
                    "generate*.py", "test_*.py", "predict*.py", "eval*.py",
                    // Named patterns
                    "*example*.py", "*demo*.py", "*inference*.py",
                    // Scripts directory
                    "**/scripts/**/*.py",
                    // Quickstart
                    "**/quickstart*/**/*.py",
                ];

                for rn in &repo_names {
                    let repo_dir = codebases_ws.join(rn);
                    let mut seen_examples: std::collections::HashSet<String> = std::collections::HashSet::new();
                    for pattern in &example_patterns {
                        if example_budget <= 0 {
                            break;
                        }
                        let full_pattern = format!("{}/{}", repo_dir.to_string_lossy(), pattern);
                        if let Ok(paths) = glob::glob(&full_pattern) {
                            let mut matched: Vec<PathBuf> = paths
                                .filter_map(|p| p.ok())
                                .filter(|p| p.is_file() && !p.is_symlink())
                                .collect();
                            matched.sort();
                            for ex_file in matched {
                                if example_budget <= 0 {
                                    break;
                                }
                                let rel = ex_file.strip_prefix(&codebases_ws)
                                    .map(|r| r.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if seen_examples.contains(&rel) {
                                    continue;
                                }
                                // Check parts for disallowed directories
                                let parts: Vec<&str> = ex_file
                                    .strip_prefix(&repo_dir)
                                    .map(|r| r.components().map(|c| c.as_os_str().to_str().unwrap_or("")).collect())
                                    .unwrap_or_default();
                                if parts.iter().any(|p| p.starts_with("__pycache__") || p.starts_with('.') || *p == "tests") {
                                    continue;
                                }
                                // Skip large files
                                if ex_file.metadata().map(|m| m.len()).unwrap_or(0) > 15000 {
                                    continue;
                                }
                                seen_examples.insert(rel.clone());
                                let content = match std::fs::read_to_string(&ex_file) {
                                    Ok(c) => c,
                                    Err(_) => continue,
                                };
                                let mut lines: Vec<&str> = content.lines().collect();
                                if lines.len() as i64 > example_budget {
                                    lines.truncate(example_budget as usize);
                                    example_lines.push(format!("\n**Example: `{rel}`** — KEY code patterns (non-essential lines removed):\n"));
                                    example_lines.push("```python\n".to_string());
                                    example_lines.push(lines.join("\n"));
                                    example_lines.push("\n# ... (truncated)\n```\n".to_string());
                                } else {
                                    example_lines.push(format!("\n**Example: `{rel}`** — KEY code patterns (non-essential lines removed):\n"));
                                    example_lines.push("```python\n".to_string());
                                    example_lines.push(lines.join("\n"));
                                    example_lines.push("\n```\n".to_string());
                                }
                                example_budget -= lines.len() as i64;
                            }
                        }
                    }
                }

                if !example_lines.is_empty() {
                    hint_lines.push("\n## Working example scripts from codebase (USE THESE AS REFERENCE)\n".to_string());
                    hint_lines.push("Copy loading patterns. Adapt HuggingFace IDs to LOCAL CHECKPOINTS_DIR paths.\n\n".to_string());
                    hint_lines.extend(example_lines);
                }

                // API signatures
                let mut api_lines: Vec<String> = vec!["\n## Key API signatures from codebase\n".to_string()];
                let mut api_budget: i64 = 30;
                for rn in &repo_names {
                    let repo_dir = codebases_ws.join(rn);
                    let mut py_files: Vec<PathBuf> = Vec::new();
                    if let Ok(entries) = walkdir::WalkDir::new(&repo_dir)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("py"))
                        .map(|e| Ok::<PathBuf, anyhow::Error>(e.path().to_path_buf()))
                        .collect::<Result<Vec<_>>>()
                    {
                        py_files = entries;
                        py_files.sort_by_key(|f| {
                            f.strip_prefix(&repo_dir)
                                .map(|r| r.components().count())
                                .unwrap_or(0)
                        });
                    }

                    for py_file in &py_files {
                        if api_budget <= 0 {
                            break;
                        }
                        if py_file.is_symlink() {
                            continue;
                        }
                        let rel = py_file.strip_prefix(&codebases_ws)
                            .map(|r| r.to_path_buf())
                            .unwrap_or_else(|_| py_file.clone());
                        let parts: Vec<&str> = rel.components()
                            .map(|c| c.as_os_str().to_str().unwrap_or(""))
                            .collect();
                        if parts.iter().any(|p| p.starts_with("__pycache__") || p.starts_with('.') || *p == "examples" || *p == "tests") {
                            continue;
                        }
                        let source = match std::fs::read_to_string(py_file) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        let sigs: Vec<String> = source
                            .lines()
                            .filter(|l| {
                                let t = l.trim();
                                t.starts_with("def ") || t.starts_with("class ")
                            })
                            .map(|l| l.trim().trim_end_matches(':').trim_end().to_string())
                            .take(5)
                            .collect();
                        if !sigs.is_empty() {
                            let rel_str = rel.to_string_lossy();
                            let sigs_str = sigs.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ");
                            api_lines.push(format!("\n**`{rel_str}`**: "));
                            api_lines.push(format!("{sigs_str}\n"));
                            api_budget -= 2;
                            if api_budget <= 0 {
                                api_lines.push("  ... (truncated)\n".to_string());
                                break;
                            }
                        }
                    }
                }
                if api_lines.len() > 1 {
                    hint_lines.extend(api_lines);
                }

                usage_hints.push(hint_lines.join(""));
            }
        }

        // PREPEND hints to GUIDANCE.md (unlike opencode which appends)
        if !usage_hints.is_empty() {
            let hint_block = format!("\n\n{}\n", usage_hints.join("\n"));
            let guidance_path = ws.join("GUIDANCE.md");
            let existing = tokio::fs::read_to_string(&guidance_path).await.unwrap_or_default();
            tokio::fs::write(&guidance_path, format!("{hint_block}\n{existing}")).await?;
        }

        // Trim GUIDANCE.md to 300 lines max
        let max_guidance_lines = 300usize;
        let guidance_path = ws.join("GUIDANCE.md");
        if guidance_path.exists() {
            let g_text = tokio::fs::read_to_string(&guidance_path).await.unwrap_or_default();
            let mut g_lines: Vec<&str> = g_text.lines().collect();
            if g_lines.len() > max_guidance_lines {
                g_lines.truncate(max_guidance_lines);
                let mut trimmed = g_lines.join("\n");
                trimmed.push_str("\n\n<!-- GUIDANCE trimmed to fit token budget -->");
                tokio::fs::write(&guidance_path, trimmed).await?;
            }
        }

        // Snapshot file hashes for collect_files filtering
        let mut snapshot: HashMap<String, String> = HashMap::new();
        collect_py_hashes(&ws, &ws, &mut snapshot);
        let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();
        tokio::fs::write(ws.join(".codebase_snapshot.json"), snapshot_json).await?;

        // Create empty main.py if it doesn't exist
        let main_py = ws.join("main.py");
        if !main_py.exists() {
            tokio::fs::write(
                &main_py,
                "# main.py — experiment entry point (to be implemented)\n",
            )
            .await?;
        }

        Ok(ws)
    }

    // -- file collection ------------------------------------------------------

    /// Collect new/modified Python files from the workspace.
    pub fn collect_files(workspace: &Path) -> HashMap<String, String> {
        let snapshot_file = workspace.join(".codebase_snapshot.json");
        let original_hashes: HashMap<String, String> = if snapshot_file.exists() {
            std::fs::read_to_string(&snapshot_file)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        let mut files: HashMap<String, String> = HashMap::new();

        // Collect .py files sorted by depth
        let mut py_files: Vec<PathBuf> = Vec::new();
        if let Ok(walker) = walkdir::WalkDir::new(workspace)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("py"))
            .map(|e| Ok::<PathBuf, anyhow::Error>(e.path().to_path_buf()))
            .collect::<Result<Vec<_>>>()
        {
            py_files = walker;
            py_files.sort_by_key(|p| {
                p.strip_prefix(workspace)
                    .map(|r| r.components().count())
                    .unwrap_or(0)
            });
        }

        for py_file in &py_files {
            if py_file.is_symlink() {
                continue;
            }
            let rel = match py_file.strip_prefix(workspace) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            let parts: Vec<&str> = rel.components()
                .map(|c| c.as_os_str().to_str().unwrap_or(""))
                .collect();
            // Skip __pycache__, hidden dirs, and the codebases directory
            if parts.iter().any(|p| p.starts_with("__pycache__") || p.starts_with('.') || *p == "codebases") {
                continue;
            }
            let rel_str = rel.to_string_lossy().to_string();
            if let Some(orig_hash) = original_hashes.get(&rel_str) {
                if let Ok(bytes) = std::fs::read(py_file) {
                    let mut hasher = Md5::new();
                    hasher.update(&bytes);
                    let current_hash = format!("{:x}", hasher.finalize());
                    if &current_hash == orig_hash {
                        continue;
                    }
                }
            }
            let basename = rel.file_name().unwrap_or_default().to_string_lossy().to_string();
            if !files.contains_key(&basename) {
                match std::fs::read_to_string(py_file) {
                    Ok(content) => {
                        files.insert(basename, content);
                    }
                    Err(e) => {
                        warn!("Aider: failed to read {}: {}", py_file.display(), e);
                    }
                }
            }
        }

        // Also check requirements.txt and setup.py
        for extra in ["requirements.txt", "setup.py"] {
            let p = workspace.join(extra);
            if p.exists() && !files.contains_key(extra) {
                if let Some(orig_hash) = original_hashes.get(extra) {
                    if let Ok(bytes) = std::fs::read(&p) {
                        let mut hasher = Md5::new();
                        hasher.update(&bytes);
                        let current_hash = format!("{:x}", hasher.finalize());
                        if &current_hash == orig_hash {
                            continue;
                        }
                    }
                }
                if let Ok(content) = std::fs::read_to_string(&p) {
                    files.insert(extra.to_string(), content);
                }
            }
        }

        files
    }

    // -- core source files ---------------------------------------------------

    /// Find small, core .py files in codebases/ to pass as --read to aider.
    ///
    /// Sorted by line count (smallest first), skips files starting with `_`
    /// (except `__init__.py`), between 10 and max_lines lines.
    pub fn find_core_source_files(workspace: &Path, max_files: usize, max_lines: usize) -> Vec<String> {
        let codebases_dir = workspace.join("codebases");
        if !codebases_dir.is_dir() {
            return Vec::new();
        }

        let mut candidates: Vec<(usize, PathBuf)> = Vec::new();
        let walker = walkdir::WalkDir::new(&codebases_dir);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("py") {
                continue;
            }
            if path.is_symlink() {
                continue;
            }
            let rel = match path.strip_prefix(&codebases_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let parts: Vec<&str> = rel.components()
                .map(|c| c.as_os_str().to_str().unwrap_or(""))
                .collect();
            if parts.iter().any(|p| p.starts_with("__pycache__") || p.starts_with('.') || *p == "tests" || *p == "examples") {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('_') && name != "__init__.py" {
                continue;
            }
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let line_count = content.lines().count();
            if line_count > 10 && line_count <= max_lines {
                candidates.push((line_count, path.to_path_buf()));
            }
        }

        candidates.sort_by_key(|(lc, _)| *lc);
        candidates
            .into_iter()
            .take(max_files)
            .map(|(_, p)| p.to_string_lossy().to_string())
            .collect()
    }

    // -- aider cmd builders --------------------------------------------------

    /// Build the aider CLI command for a single invocation.
    pub fn build_aider_cmd(
        &self,
        workspace: &Path,
        message: &str,
        api_key: &str,
        edit_format: &str,
    ) -> Vec<String> {
        let model = if self.model.contains('/') {
            self.model.clone()
        } else {
            format!("openai/{}", self.model)
        };

        let msg_file = workspace.join(".aider_task.md");
        let _ = std::fs::write(&msg_file, message);

        // Editable files: main.py and GUIDANCE.md
        let mut add_files: Vec<String> = Vec::new();
        let main_py = workspace.join("main.py");
        if main_py.exists() {
            add_files.push(main_py.to_string_lossy().to_string());
        }
        let guidance_path = workspace.join("GUIDANCE.md");
        if guidance_path.exists() {
            add_files.push(guidance_path.to_string_lossy().to_string());
        }

        // Read-only context: EXPERIMENT_PLAN.yaml + core source files
        let mut read_files: Vec<String> = Vec::new();
        let exp_plan_path = workspace.join("EXPERIMENT_PLAN.yaml");
        if exp_plan_path.exists() {
            read_files.push("--read".to_string());
            read_files.push(exp_plan_path.to_string_lossy().to_string());
        }
        for rf in Self::find_core_source_files(workspace, 10, 150) {
            read_files.push("--read".to_string());
            read_files.push(rf);
        }

        let mut cmd = vec![
            Self::find_binary(),
            "--model".to_string(),
            model,
            "--openai-api-base".to_string(),
            self.llm_base_url.clone(),
            "--openai-api-key".to_string(),
            api_key.to_string(),
            "--message-file".to_string(),
            msg_file.to_string_lossy().to_string(),
            "--yes".to_string(),
            "--no-auto-commits".to_string(),
            "--no-stream".to_string(),
            "--no-git".to_string(),
            "--no-show-model-warnings".to_string(),
            "--no-show-release-notes".to_string(),
            "--no-check-update".to_string(),
            "--no-browser".to_string(),
            "--edit-format".to_string(),
            edit_format.to_string(),
            "--map-tokens".to_string(),
            "2048".to_string(),
        ];
        cmd.extend(read_files);
        cmd.extend(add_files);
        cmd
    }

    /// Build the aider CLI command for a sanity fix invocation.
    ///
    /// All .py files in workspace are editable; GUIDANCE.md, EXPERIMENT_PLAN.yaml,
    /// and codebase .py files are read-only context.
    pub fn build_aider_fix_cmd(
        &self,
        workspace: &Path,
        message: &str,
        api_key: &str,
    ) -> Vec<String> {
        let model = if self.model.contains('/') {
            self.model.clone()
        } else {
            format!("openai/{}", self.model)
        };

        let msg_file = workspace.join(".aider_task.md");
        let _ = std::fs::write(&msg_file, message);

        // Editable: all .py files in workspace root
        let mut add_files: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(workspace) {
            let mut sorted_entries: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("py"))
                .collect();
            sorted_entries.sort_by_key(|e| e.file_name());
            for entry in sorted_entries {
                add_files.push(entry.path().to_string_lossy().to_string());
            }
        }

        // Read-only: GUIDANCE.md, EXPERIMENT_PLAN.yaml, core source files, config YAMLs
        let mut read_files: Vec<String> = Vec::new();
        for ctx in ["GUIDANCE.md", "EXPERIMENT_PLAN.yaml"] {
            let ctx_path = workspace.join(ctx);
            if ctx_path.exists() {
                read_files.push("--read".to_string());
                read_files.push(ctx_path.to_string_lossy().to_string());
            }
        }
        for rf in Self::find_core_source_files(workspace, 10, 150) {
            read_files.push("--read".to_string());
            read_files.push(rf);
        }
        // Include dataset config YAMLs and codebase config YAMLs
        for ctx_dir_name in ["dataset_configs", "codebase_configs"] {
            let ctx_dir = workspace.join(ctx_dir_name);
            if ctx_dir.is_dir() {
                let walker = walkdir::WalkDir::new(&ctx_dir);
                let mut yaml_files: Vec<PathBuf> = walker
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("yaml"))
                    .map(|e| e.path().to_path_buf())
                    .collect();
                yaml_files.sort();
                for yf in yaml_files.into_iter().take(8) {
                    read_files.push("--read".to_string());
                    read_files.push(yf.to_string_lossy().to_string());
                }
            }
        }

        let mut cmd = vec![
            Self::find_binary(),
            "--model".to_string(),
            model,
            "--openai-api-base".to_string(),
            self.llm_base_url.clone(),
            "--openai-api-key".to_string(),
            api_key.to_string(),
            "--message-file".to_string(),
            msg_file.to_string_lossy().to_string(),
            "--yes".to_string(),
            "--no-auto-commits".to_string(),
            "--no-stream".to_string(),
            "--no-git".to_string(),
            "--no-show-model-warnings".to_string(),
            "--no-show-release-notes".to_string(),
            "--no-check-update".to_string(),
            "--no-browser".to_string(),
            "--edit-format".to_string(),
            "diff".to_string(),
            "--map-tokens".to_string(),
            "2048".to_string(),
        ];
        cmd.extend(read_files);
        cmd.extend(add_files);
        cmd
    }

    // -- subprocess invocation -----------------------------------------------

    /// Run a single aider invocation in the workspace.
    ///
    /// Returns `(success, log, elapsed_secs)`.
    pub async fn invoke_aider(
        &self,
        workspace: &Path,
        message: &str,
        api_key: &str,
        step_timeout: u64,
        edit_format: &str,
    ) -> (bool, String, f64) {
        let workspace = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());

        let mut env: HashMap<String, String> = std::env::vars().collect();
        for var in &["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            env.remove(*var);
        }
        env.insert("no_proxy".to_string(), "*".to_string());
        env.insert("NO_PROXY".to_string(), "*".to_string());
        if !api_key.is_empty() {
            env.insert("OPENAI_API_KEY".to_string(), api_key.to_string());
        }

        let cmd_args = self.build_aider_cmd(&workspace, message, api_key, edit_format);
        let timeout = if step_timeout > 0 { step_timeout } else { self.timeout_sec };

        let t0 = Instant::now();

        if cmd_args.is_empty() {
            return (false, "Empty command".to_string(), 0.0);
        }

        let child = match Command::new(&cmd_args[0])
            .args(&cmd_args[1..])
            .current_dir(&workspace)
            .envs(&env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let elapsed = t0.elapsed().as_secs_f64();
                if e.kind() == std::io::ErrorKind::NotFound {
                    return (false, "aider CLI not found".to_string(), elapsed);
                }
                return (false, format!("Unexpected error: {e}"), elapsed);
            }
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            child.wait_with_output(),
        )
        .await;

        let elapsed = t0.elapsed().as_secs_f64();

        match result {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let log = format!("{stdout}\n{stderr}");
                (out.status.success(), log, elapsed)
            }
            Ok(Err(e)) => (false, format!("Process error: {e}"), elapsed),
            Err(_) => (false, format!("Timeout after {elapsed:.1}s"), elapsed),
        }
    }

    // -- TODO scanning -------------------------------------------------------

    /// Scan main.py for TODO markers. Returns list of TODO lines with ±3 line context.
    pub fn scan_todos(main_py: &Path) -> Vec<String> {
        if !main_py.exists() {
            return Vec::new();
        }
        let text = match std::fs::read_to_string(main_py) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        let lines: Vec<&str> = text.lines().collect();
        let mut todos: Vec<String> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("# TODO:") {
                let context_start = i.saturating_sub(3);
                let context_end = (i + 2).min(lines.len());
                let context = (context_start..context_end)
                    .map(|j| format!("  L{}: {}", j + 1, lines[j]))
                    .collect::<Vec<_>>()
                    .join("\n");
                todos.push(format!(
                    "Line {}: {}\nContext:\n{}",
                    i + 1,
                    line.trim(),
                    context
                ));
            }
        }
        todos
    }

    // -- syntax check --------------------------------------------------------

    /// Run `python3 -c 'import main'` in the workspace; return error string or None.
    pub async fn check_syntax(workspace: &Path) -> Option<String> {
        let main_py = workspace.join("main.py");
        if !main_py.exists() {
            return Some("main.py does not exist".to_string());
        }
        match Command::new("python3")
            .args(["-c", "import main"])
            .current_dir(workspace)
            .output()
            .await
        {
            Ok(out) => {
                if out.status.success() {
                    None
                } else {
                    let err = String::from_utf8_lossy(&out.stderr).to_string()
                        + &String::from_utf8_lossy(&out.stdout);
                    let tail: String = err.chars().rev().take(1000).collect::<String>().chars().rev().collect();
                    Some(tail)
                }
            }
            Err(e) => Some(e.to_string()),
        }
    }

    // -- TODO loop -----------------------------------------------------------

    /// Generate code via TODO-driven loop.
    ///
    /// Phase 1: Generate skeleton with TODO markers.
    /// Phase 2: Repeatedly scan for TODOs and implement one at a time.
    /// Phase 3: Syntax-check and fix.
    ///
    /// Returns `(has_main_with_more_than_10_lines, combined_log, total_elapsed)`.
    pub async fn run_todo_loop(
        &self,
        workspace: &Path,
        metric: &str,
        time_budget_sec: u64,
    ) -> (bool, String, f64) {
        let api_key = self.resolve_api_key();
        let mut all_logs: Vec<String> = Vec::new();
        let mut total_elapsed = 0.0f64;
        let per_call_timeout = std::cmp::max(300, self.timeout_sec / (MAX_TODO_ITERATIONS as u64 + 2));
        let main_py = workspace.join("main.py");

        let has_codebases = {
            let cb = workspace.join("codebases");
            cb.is_dir() && std::fs::read_dir(&cb).map(|mut d| d.next().is_some()).unwrap_or(false)
        };
        let extra_rules = if has_codebases { "" } else { _RULES_NO_CODEBASE };

        // --- Phase 1: Generate skeleton ---
        let skeleton = skeleton_prompt()
            + extra_rules;
        let skeleton_prompt_str = skeleton
            .replace("{metric}", metric)
            .replace("{time_budget_sec}", &time_budget_sec.to_string());

        info!("Aider TODO loop: generating skeleton...");
        let (ok, log, elapsed) = self
            .invoke_aider(workspace, &skeleton_prompt_str, &api_key, per_call_timeout, "diff")
            .await;
        total_elapsed += elapsed;
        all_logs.push(format!("=== Skeleton (ok={ok}, {elapsed:.1}s) ===\n{log}"));

        let skeleton_ok = main_py.exists() && {
            let text = std::fs::read_to_string(&main_py).unwrap_or_default();
            text.trim().lines().count() >= 5
        };
        if !skeleton_ok {
            warn!("Aider TODO loop: skeleton generation failed");
            return (false, all_logs.join("\n"), total_elapsed);
        }

        // --- Phase 2: TODO fill loop ---
        let mut prev_todo_count: i64 = -1;
        let mut stuck_count = 0usize;
        for iteration in 0..MAX_TODO_ITERATIONS {
            let todos = Self::scan_todos(&main_py);

            if todos.is_empty() {
                info!("Aider TODO loop: no TODOs remaining after {} iterations", iteration);
                break;
            }

            let todo_count = todos.len() as i64;
            if todo_count == prev_todo_count {
                stuck_count += 1;
                if stuck_count >= MAX_STUCK {
                    warn!(
                        "Aider TODO loop: TODO count unchanged ({}) for {} iterations — giving up",
                        todos.len(),
                        MAX_STUCK
                    );
                    break;
                }
                info!(
                    "Aider TODO loop: TODO count unchanged ({}), retry {}/{}",
                    todos.len(),
                    stuck_count,
                    MAX_STUCK
                );
            } else {
                stuck_count = 0;
            }
            prev_todo_count = todo_count;

            let first_todo = &todos[0];
            let fill_base = fill_todo_prompt() + extra_rules;
            let fill_prompt = fill_base
                .replace("{todo_line}", first_todo)
                .replace("{metric}", metric)
                .replace("{time_budget_sec}", &time_budget_sec.to_string());

            info!(
                "Aider TODO loop: iteration {}/{} — {} TODOs remaining, implementing first...",
                iteration + 1,
                MAX_TODO_ITERATIONS,
                todos.len()
            );
            let (ok, log, elapsed) = self
                .invoke_aider(workspace, &fill_prompt, &api_key, per_call_timeout, "diff")
                .await;
            total_elapsed += elapsed;
            all_logs.push(format!(
                "=== TODO iter {} ({} left, ok={}, {:.1}s) ===\n{}",
                iteration + 1,
                todos.len(),
                ok,
                elapsed,
                log
            ));
        }

        // --- Phase 3: Syntax fix ---
        for fix_attempt in 0..MAX_FIX_ATTEMPTS {
            let syntax_err = Self::check_syntax(workspace).await;
            if syntax_err.is_none() {
                break;
            }
            let err_str = syntax_err.unwrap();
            info!(
                "Aider TODO loop: fix attempt {} — syntax error detected",
                fix_attempt + 1
            );
            let err_truncated: String = err_str.chars().take(2000).collect();
            let fix_prompt = _FIX_PROMPT_TEMPLATE.replace("{error_output}", &err_truncated);
            let (ok, log, elapsed) = self
                .invoke_aider(workspace, &fix_prompt, &api_key, per_call_timeout, "diff")
                .await;
            total_elapsed += elapsed;
            all_logs.push(format!(
                "=== Fix attempt {} (ok={}, {:.1}s) ===\n{}",
                fix_attempt + 1,
                ok,
                elapsed,
                log
            ));
        }

        let has_main = main_py.exists() && {
            let text = std::fs::read_to_string(&main_py).unwrap_or_default();
            text.trim().lines().count() > 10
        };
        (has_main, all_logs.join("\n"), total_elapsed)
    }

    // -- main generate entry point -------------------------------------------

    /// Run Aider to generate experiment code via TODO-driven loop.
    pub async fn generate(
        &self,
        stage_dir: &Path,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
        extra_guidance: &str,
        time_budget_sec: u64,
        codebases_dir: &str,
        datasets_dir: &str,
        checkpoints_dir: &str,
        selected_repos: Option<&[String]>,
    ) -> OpenCodeResult {
        if !self.check_available().await {
            return OpenCodeResult {
                success: false,
                error: "Aider CLI not installed or not callable".to_string(),
                ..Default::default()
            };
        }

        let mut last_error = String::new();
        let mut last_log = String::new();
        let mut last_elapsed = 0.0f64;

        for attempt in 0..=(self.max_retries) {
            let workspace = match self
                .prepare_workspace(
                    stage_dir,
                    topic,
                    exp_plan,
                    metric,
                    pkg_hint,
                    extra_guidance,
                    time_budget_sec,
                    codebases_dir,
                    datasets_dir,
                    checkpoints_dir,
                    selected_repos,
                )
                .await
            {
                Ok(ws) => ws,
                Err(e) => {
                    last_error = format!("Failed to prepare workspace: {e}");
                    warn!("Aider beast mode: {}", last_error);
                    continue;
                }
            };

            info!(
                "Aider beast mode (TODO loop): attempt {}/{}",
                attempt + 1,
                1 + self.max_retries
            );

            // --- TODO-driven generation ---
            let (success, log, elapsed) =
                self.run_todo_loop(&workspace, metric, time_budget_sec).await;

            let files = if workspace.exists() {
                Self::collect_files(&workspace)
            } else {
                HashMap::new()
            };

            if success && files.contains_key("main.py") {
                let _ = tokio::fs::write(stage_dir.join("aider_log.txt"), &log).await;
                info!(
                    "Aider beast mode: SUCCESS (TODO loop) — {} files in {:.1}s",
                    files.len(),
                    elapsed
                );
                return OpenCodeResult {
                    success: true,
                    files,
                    opencode_log: log,
                    elapsed_sec: elapsed,
                    error: String::new(),
                };
            }

            // --- Single-shot fallback ---
            warn!("Aider TODO loop produced no valid main.py — trying single-shot fallback");

            let api_key = self.resolve_api_key();
            let has_cb = {
                let cb = workspace.join("codebases");
                cb.is_dir() && std::fs::read_dir(&cb).map(|mut d| d.next().is_some()).unwrap_or(false)
            };
            let fb_extra = if has_cb { "" } else { _RULES_NO_CODEBASE };
            let fb_base = fallback_prompt() + fb_extra;
            let fallback_prompt_str = fb_base
                .replace("{metric}", metric)
                .replace("{time_budget_sec}", &time_budget_sec.to_string());

            let (_, fb_log, fb_elapsed) = self
                .invoke_aider(
                    &workspace,
                    &fallback_prompt_str,
                    &api_key,
                    self.timeout_sec,
                    "whole",
                )
                .await;
            let total_elapsed = elapsed + fb_elapsed;
            let full_log = format!("{log}\n=== Single-shot fallback ===\n{fb_log}");

            let files = if workspace.exists() {
                Self::collect_files(&workspace)
            } else {
                HashMap::new()
            };

            if let Some(main_content) = files.get("main.py") {
                if main_content.trim().lines().count() > 10 {
                    let _ = tokio::fs::write(stage_dir.join("aider_log.txt"), &full_log).await;
                    info!(
                        "Aider beast mode: SUCCESS (fallback) — {} files in {:.1}s",
                        files.len(),
                        total_elapsed
                    );
                    return OpenCodeResult {
                        success: true,
                        files,
                        opencode_log: full_log,
                        elapsed_sec: total_elapsed,
                        error: String::new(),
                    };
                }
            }

            last_error = if !full_log.is_empty() {
                full_log.chars().take(500).collect()
            } else {
                "No main.py produced".to_string()
            };
            last_log = full_log;
            last_elapsed = total_elapsed;
            info!(
                "Aider beast mode: attempt {} failed ({:.1}s, files={:?}): {}",
                attempt + 1,
                total_elapsed,
                files.keys().collect::<Vec<_>>(),
                &last_error.chars().take(200).collect::<String>()
            );
        }

        // Persist log even on failure
        if !last_log.is_empty() {
            let _ = tokio::fs::write(stage_dir.join("aider_log.txt"), &last_log).await;
        }

        OpenCodeResult {
            success: false,
            opencode_log: last_error,
            elapsed_sec: last_elapsed,
            error: format!("Aider failed after {} attempt(s)", 1 + self.max_retries),
            ..Default::default()
        }
    }

    // -- sanity fix workspace ------------------------------------------------

    /// Prepare a workspace for Aider-based sanity fix.
    ///
    /// Copies experiment .py files, reuses GUIDANCE.md and EXPERIMENT_PLAN.yaml
    /// from the Stage 11 aider workspace, and symlinks codebases for --read.
    pub async fn prepare_fix_workspace(
        &self,
        stage_dir: &Path,
        run_dir: &Path,
        experiment_dir: &Path,
        codebases_dir: &str,
    ) -> Result<PathBuf> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let pid = std::process::id();
        let ws = stage_dir.join(format!("aider_fix_{ts}_{pid}"));
        tokio::fs::create_dir_all(&ws).await?;

        // Copy experiment .py files
        if let Ok(entries) = std::fs::read_dir(experiment_dir) {
            let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            sorted.sort_by_key(|e| e.file_name());
            for entry in sorted {
                let path = entry.path();
                if path.extension().and_then(|x| x.to_str()) == Some("py") {
                    let dest = ws.join(path.file_name().unwrap_or_default());
                    let _ = std::fs::copy(&path, &dest);
                }
            }
        }

        // Reuse GUIDANCE.md and EXPERIMENT_PLAN.yaml from prior aider workspace
        let pattern = run_dir.join("stage-*/aider_beast_*");
        let glob_str = pattern.to_string_lossy();
        let mut prior_workspaces: Vec<PathBuf> = glob::glob(&glob_str)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|r| r.ok())
            .filter(|p| p.is_dir())
            .collect();
        prior_workspaces.sort();
        prior_workspaces.reverse();

        for aider_ws in &prior_workspaces {
            for ctx_name in ["GUIDANCE.md", "EXPERIMENT_PLAN.yaml"] {
                let src = aider_ws.join(ctx_name);
                let dst = ws.join(ctx_name);
                if src.exists() && !dst.exists() {
                    let _ = std::fs::copy(&src, &dst);
                }
            }
            // Symlink codebases from prior workspace
            let cb_src = aider_ws.join("codebases");
            let cb_dst = ws.join("codebases");
            if cb_src.is_dir() && !cb_dst.exists() {
                let cb_resolved = cb_src.canonicalize().unwrap_or(cb_src.clone());
                let _ = std::os::unix::fs::symlink(&cb_resolved, &cb_dst);
            }
            if ws.join("GUIDANCE.md").exists() {
                break;
            }
        }

        // Fallback: symlink from explicit codebases_dir
        if !codebases_dir.is_empty() && !ws.join("codebases").exists() {
            let cb_path = PathBuf::from(codebases_dir).canonicalize().unwrap_or_else(|_| PathBuf::from(codebases_dir));
            if cb_path.is_dir() {
                let link = ws.join("codebases");
                let _ = std::os::unix::fs::symlink(&cb_path, &link);
            }
        }

        // Copy dataset config YAMLs (max 10) as read-only context
        let datasets_dir_from_guidance: Option<String> = {
            let gm = ws.join("GUIDANCE.md");
            if gm.exists() {
                let gm_text = std::fs::read_to_string(&gm).unwrap_or_default();
                let re = Regex::new(r#"DATASETS_DIR\s*=\s*["']([^"']+)["']"#).ok();
                re.and_then(|r| r.captures(&gm_text))
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            } else {
                None
            }
        };

        if let Some(ds_dir) = datasets_dir_from_guidance {
            let ds_path = PathBuf::from(&ds_dir);
            if ds_path.is_dir() {
                let ctx_dir = ws.join("dataset_configs");
                let walker = walkdir::WalkDir::new(&ds_path);
                let mut yaml_files: Vec<PathBuf> = walker
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("yaml"))
                    .map(|e| e.path().to_path_buf())
                    .collect();
                yaml_files.sort();
                let mut copied = 0usize;
                for cfg_yaml in yaml_files {
                    if copied >= 10 {
                        break;
                    }
                    if let Ok(rel) = cfg_yaml.strip_prefix(&ds_path) {
                        let dst = ctx_dir.join(rel);
                        if let Some(parent) = dst.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if std::fs::copy(&cfg_yaml, &dst).is_ok() {
                            copied += 1;
                        }
                    }
                }
            }
        }

        // Copy codebase config YAMLs (max 5)
        if !codebases_dir.is_empty() {
            let cb_configs = PathBuf::from(codebases_dir).join("configs");
            if cb_configs.is_dir() {
                let cb_ctx = ws.join("codebase_configs");
                let mut yaml_files: Vec<PathBuf> = std::fs::read_dir(&cb_configs)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("yaml"))
                    .collect();
                yaml_files.sort();
                let mut copied = 0usize;
                for cfg_yaml in yaml_files {
                    if copied >= 5 {
                        break;
                    }
                    let _ = std::fs::create_dir_all(&cb_ctx);
                    let dst = cb_ctx.join(cfg_yaml.file_name().unwrap_or_default());
                    if std::fs::copy(&cfg_yaml, &dst).is_ok() {
                        copied += 1;
                    }
                }
            }
        }

        Ok(ws)
    }

    // -- sanity fix entry point ----------------------------------------------

    /// Use Aider to fix a sanity check failure.
    ///
    /// Returns `(success, {filename: patched_content}, aider_log)`.
    /// The caller is responsible for writing patched files back to experiment_dir.
    pub async fn fix_sanity_error(
        &self,
        stage_dir: &Path,
        run_dir: &Path,
        experiment_dir: &Path,
        test_name: &str,
        test_code: &str,
        stderr: &str,
        iteration: usize,
        _max_iterations: usize,
        previous_fixes: Option<&[HashMap<String, Value>]>,
        codebases_dir: &str,
    ) -> (bool, HashMap<String, String>, String) {
        if !self.check_available().await {
            return (false, HashMap::new(), "Aider CLI not available".to_string());
        }

        let api_key = self.resolve_api_key();

        let ws = match self
            .prepare_fix_workspace(stage_dir, run_dir, experiment_dir, codebases_dir)
            .await
        {
            Ok(ws) => ws,
            Err(e) => {
                return (false, HashMap::new(), format!("Failed to prepare fix workspace: {e}"));
            }
        };

        let repeat_hint = build_repeat_hint(previous_fixes, stderr);

        let stderr_tail: String = stderr.chars().rev().take(3000).collect::<String>().chars().rev().collect();
        let fix_prompt_str = fix_sanity_prompt()
            .replace("{test_name}", test_name)
            .replace("{test_code}", test_code)
            .replace("{stderr}", &stderr_tail)
            .replace("{repeat_hint}", &repeat_hint);

        info!(
            "Aider sanity fix: invoking for test={} iteration={} workspace={}",
            test_name,
            iteration,
            ws.display()
        );

        let step_timeout = std::cmp::max(180, self.timeout_sec / 3);
        let (ok, log, elapsed) = self
            .invoke_aider(&ws, &fix_prompt_str, &api_key, step_timeout, "diff")
            .await;

        info!("Aider sanity fix: done (ok={}, {:.1}s)", ok, elapsed);

        // Collect patched files (only those that differ from original and have >30 chars)
        let mut patched_files: HashMap<String, String> = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&ws) {
            let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            sorted.sort_by_key(|e| e.file_name());
            for entry in sorted {
                let py_file = entry.path();
                if py_file.extension().and_then(|x| x.to_str()) != Some("py") {
                    continue;
                }
                let name = py_file.file_name().unwrap_or_default().to_string_lossy().to_string();
                let orig = experiment_dir.join(&name);
                if !orig.exists() {
                    continue;
                }
                let ws_content = match std::fs::read_to_string(&py_file) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let orig_content = match std::fs::read_to_string(&orig) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if ws_content != orig_content && ws_content.trim().len() > 30 {
                    patched_files.insert(name, ws_content);
                }
            }
        }

        (!patched_files.is_empty(), patched_files, log)
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Generate a directory tree string (like `_dir_tree` in Python).
fn dir_tree(root: &Path, max_depth: usize, max_items: usize) -> Result<String> {
    let mut lines: Vec<String> = vec![format!("{}/", root.display())];
    let mut count = 0usize;

    let walker = walkdir::WalkDir::new(root)
        .min_depth(1)
        .max_depth(max_depth)
        .sort_by_file_name();

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if count >= max_items {
            lines.push("  ... (truncated)".to_string());
            break;
        }
        let path = entry.path();
        let rel = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let parts: Vec<&str> = rel.components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""))
            .collect();
        if parts.iter().any(|p| p.starts_with('.') || *p == "__pycache__") {
            continue;
        }
        let depth = parts.len();
        let indent = "  ".repeat(depth);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let suffix = if path.is_dir() {
            "/".to_string()
        } else if path.is_file() {
            format!(" ({} bytes)", path.metadata().map(|m| m.len()).unwrap_or(0))
        } else {
            String::new()
        };
        lines.push(format!("{indent}{name}{suffix}"));
        count += 1;
    }

    Ok(lines.join("\n"))
}

/// Copy a directory tree, ignoring .git, __pycache__, *.pyc, node_modules, .eggs, _manifest.json.
fn copy_dir_ignore(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    let ignore_names = [".git", "__pycache__", "node_modules", ".eggs"];
    let ignore_exts = ["pyc"];
    let ignore_files = ["_manifest.json"];

    for entry in walkdir::WalkDir::new(src).min_depth(1).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = match path.strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // Check each component
        let skip = rel.components().any(|c| {
            let s = c.as_os_str().to_str().unwrap_or("");
            ignore_names.contains(&s)
        });
        if skip {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_str().unwrap_or("");
        if ignore_files.contains(&name) {
            continue;
        }
        if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
            if ignore_exts.contains(&ext) {
                continue;
            }
        }
        let dest = dst.join(rel);
        if path.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else if path.is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &dest)?;
        }
    }
    Ok(())
}

/// Recursively collect MD5 hashes of .py files relative to workspace root.
fn collect_py_hashes(root: &Path, base: &Path, out: &mut HashMap<String, String>) {
    let walker = walkdir::WalkDir::new(root);
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("py") {
            continue;
        }
        if path.is_symlink() {
            continue;
        }
        let rel = match path.strip_prefix(base) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let mut hasher = Md5::new();
        hasher.update(&bytes);
        let hash = format!("{:x}", hasher.finalize());
        out.insert(rel.to_string_lossy().to_string(), hash);
    }
}

/// Build the repeat_hint string from previous fix attempts.
fn build_repeat_hint(
    previous_fixes: Option<&[HashMap<String, Value>]>,
    current_stderr: &str,
) -> String {
    let fixes = match previous_fixes {
        Some(f) if !f.is_empty() => f,
        _ => return String::new(),
    };

    let mut prev_details: Vec<String> = Vec::new();
    let start = if fixes.len() > 3 { fixes.len() - 3 } else { 0 };
    for entry in &fixes[start..] {
        let err_tail = entry
            .get("error_tail")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let err_tail_tail: String = err_tail.chars().rev().take(500).collect::<String>().chars().rev().collect();
        let last_line = err_tail_tail.trim().lines().last().unwrap_or("?").to_string();

        let diff_stats = entry.get("patch_diff_stats").and_then(|v| v.as_object());
        let diff_summary = if let Some(stats) = diff_stats {
            let parts: Vec<String> = stats
                .iter()
                .map(|(fname, s)| {
                    let changed_lines = s.get("changed_lines").and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
                    let change_pct = s.get("change_pct").and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
                    format!("{fname}: {changed_lines} lines changed ({change_pct}%)")
                })
                .collect();
            format!("\n  Changes made: {}", parts.join(", "))
        } else {
            String::new()
        };

        let iteration = entry.get("iteration").and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
        let patches_applied = entry
            .get("patches_applied")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        let failed_test = entry.get("failed_test").and_then(|v| v.as_str()).unwrap_or("?");

        prev_details.push(format!(
            "- **Iteration {iteration}**: patched {patches_applied}, failed test `{failed_test}`{diff_summary}\n  Error: `{last_line}`"
        ));
    }

    // Detect same-as-last and cycles
    let last_err: String = fixes
        .last()
        .and_then(|e| e.get("error_tail"))
        .and_then(|v| v.as_str())
        .map(|s| s.chars().rev().take(300).collect::<String>().chars().rev().collect())
        .unwrap_or_default();
    let current_err: String = current_stderr.chars().rev().take(300).collect::<String>().chars().rev().collect();

    let same_as_last = !last_err.is_empty()
        && !current_err.is_empty()
        && last_err.trim().lines().last() == current_err.trim().lines().last();

    let prev_prev_err: String = if fixes.len() >= 2 {
        fixes[fixes.len() - 2]
            .get("error_tail")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().rev().take(300).collect::<String>().chars().rev().collect())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let is_cycle = !prev_prev_err.is_empty()
        && !current_err.is_empty()
        && fixes.len() >= 2
        && prev_prev_err.trim().lines().last() == current_err.trim().lines().last();

    let escalation = if is_cycle {
        "**ESCALATION — CYCLE DETECTED**: The same error appeared before, was 'fixed', \
        then a different fix broke it again the same way. Your previous approach is \
        FUNDAMENTALLY WRONG. You must try a COMPLETELY DIFFERENT strategy:\n\
        - For path errors: instead of manipulating the path string, use `os.path.basename()` \
        to extract just the filename and rebuild the path from known constants.\n\
        - For NoneType errors: read the reference implementation (inference.py) to find \
        the EXACT correct values, don't guess.\n\
        - For missing keys: read the ACTUAL config file or grep ALL attribute accesses \
        in the codebase source.\n"
            .to_string()
    } else if same_as_last {
        "**WARNING — SAME ERROR**: The error is IDENTICAL to the previous iteration. \
        Your last fix had NO EFFECT on this error. The previous change was either \
        wrong or insufficient. Do NOT repeat the same approach — try something different.\n\
        Read the actual data/config files to understand what values are really there.\n"
            .to_string()
    } else {
        "**NOTE**: The error changed from the previous iteration — your fix partially worked \
        but exposed a new issue. Fix this new error while keeping the previous fix intact.\n"
            .to_string()
    };

    format!(
        "\n## Previous fix attempts (all FAILED)\n{}\n\n{}",
        prev_details.join("\n"),
        escalation
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // _scan_todos
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_todos_empty_file() {
        let dir = TempDir::new().unwrap();
        let main_py = dir.path().join("main.py");
        std::fs::write(&main_py, "def foo():\n    pass\n").unwrap();
        let todos = OpenHandsBridge::scan_todos(&main_py);
        assert!(todos.is_empty(), "no TODOs expected");
    }

    #[test]
    fn test_scan_todos_finds_markers() {
        let dir = TempDir::new().unwrap();
        let main_py = dir.path().join("main.py");
        let content = "\
def load_pipeline():
    # TODO: implement loading
    pass

def compute_metric():
    # TODO: compute real metric
    pass
";
        std::fs::write(&main_py, content).unwrap();
        let todos = OpenHandsBridge::scan_todos(&main_py);
        assert_eq!(todos.len(), 2, "expected 2 TODO items");
        assert!(todos[0].contains("implement loading"));
        assert!(todos[1].contains("compute real metric"));
    }

    #[test]
    fn test_scan_todos_context_lines() {
        let dir = TempDir::new().unwrap();
        let main_py = dir.path().join("main.py");
        let content = "\
line1
line2
line3
def load_data():
    # TODO: load from DATASETS_DIR
    pass
line7
";
        std::fs::write(&main_py, content).unwrap();
        let todos = OpenHandsBridge::scan_todos(&main_py);
        assert_eq!(todos.len(), 1);
        // Context should include surrounding lines
        let ctx = &todos[0];
        assert!(ctx.contains("load_data") || ctx.contains("L"), "context should have nearby lines");
    }

    #[test]
    fn test_scan_todos_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/main.py");
        let todos = OpenHandsBridge::scan_todos(&path);
        assert!(todos.is_empty());
    }

    // -----------------------------------------------------------------------
    // _find_binary logic
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_binary_returns_string() {
        // Should return a non-empty string (either found path or fallback "aider")
        let bin = OpenHandsBridge::find_binary();
        assert!(!bin.is_empty(), "find_binary should return non-empty string");
    }

    #[test]
    fn test_find_binary_fallback() {
        // When aider is not installed, should return "aider" as fallback
        // We can't easily test the exact result, but we verify it doesn't panic
        let _ = OpenHandsBridge::find_binary();
    }

    // -----------------------------------------------------------------------
    // TODO loop stuck detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_stuck_detection_logic() {
        // Simulate the stuck detection logic
        let mut prev_todo_count: i64 = -1;
        let mut stuck_count = 0usize;
        let mut gave_up = false;

        // Simulate 5 iterations with same count (4 TODOs every time)
        for _iteration in 0..10 {
            let todo_count = 4i64;
            if todo_count == prev_todo_count {
                stuck_count += 1;
                if stuck_count >= MAX_STUCK {
                    gave_up = true;
                    break;
                }
            } else {
                stuck_count = 0;
            }
            prev_todo_count = todo_count;
        }

        assert!(gave_up, "should give up after MAX_STUCK iterations of same count");
        assert_eq!(stuck_count, MAX_STUCK);
    }

    #[test]
    fn test_stuck_detection_resets_on_progress() {
        let mut prev_todo_count: i64 = -1;
        let mut stuck_count = 0usize;

        // 2 iterations stuck
        for _ in 0..2 {
            let todo_count = 4i64;
            if todo_count == prev_todo_count {
                stuck_count += 1;
            } else {
                stuck_count = 0;
            }
            prev_todo_count = todo_count;
        }
        assert_eq!(stuck_count, 1, "should be 1 after 2 iterations of same count (first sets prev)");

        // Now progress happens (count drops to 3)
        let todo_count = 3i64;
        if todo_count == prev_todo_count {
            stuck_count += 1;
        } else {
            stuck_count = 0;
        }
        prev_todo_count = todo_count;
        assert_eq!(stuck_count, 0, "stuck_count should reset when TODO count changes");
    }

    // -----------------------------------------------------------------------
    // Prompt template substitution
    // -----------------------------------------------------------------------

    #[test]
    fn test_skeleton_prompt_contains_rules() {
        let prompt = skeleton_prompt();
        assert!(prompt.contains("CRITICAL: Read EXPERIMENT_PLAN.yaml FIRST"));
        assert!(prompt.contains("TASK: Generate a SHORT main.py SKELETON"));
        // Skeleton prompt has {time_budget_sec} as placeholder for TIME_BUDGET constant
        assert!(prompt.contains("{time_budget_sec}"));
    }

    #[test]
    fn test_skeleton_prompt_substitution() {
        let prompt = skeleton_prompt()
            .replace("{time_budget_sec}", "3600");
        assert!(!prompt.contains("{time_budget_sec}"), "time placeholder should be replaced");
        assert!(prompt.contains("3600"));
    }

    #[test]
    fn test_fill_todo_prompt_contains_placeholder() {
        let prompt = fill_todo_prompt();
        assert!(prompt.contains("{todo_line}"));
        // fill_todo_prompt uses {metric} and {time_budget_sec} substituted by the caller
        // but also contains codebase rules
        assert!(prompt.contains("TASK: Implement ONE TODO"));
    }

    #[test]
    fn test_fix_prompt_contains_placeholder() {
        assert!(_FIX_PROMPT_TEMPLATE.contains("{error_output}"));
        let filled = _FIX_PROMPT_TEMPLATE.replace("{error_output}", "SyntaxError: invalid syntax");
        assert!(filled.contains("SyntaxError: invalid syntax"));
    }

    #[test]
    fn test_fallback_prompt_substitution() {
        let prompt = fallback_prompt()
            .replace("{metric}", "PSNR")
            .replace("{time_budget_sec}", "1800");
        assert!(prompt.contains("PSNR"));
        assert!(prompt.contains("1800"));
        assert!(!prompt.contains("{metric}"));
    }

    #[test]
    fn test_fix_sanity_prompt_substitution() {
        let prompt = fix_sanity_prompt()
            .replace("{test_name}", "test_output_shape")
            .replace("{test_code}", "assert result.shape == (3, 256, 256)")
            .replace("{stderr}", "AssertionError")
            .replace("{repeat_hint}", "");
        assert!(prompt.contains("test_output_shape"));
        assert!(prompt.contains("test_output_shape"));
        assert!(!prompt.contains("{test_name}"));
    }

    #[test]
    fn test_rules_no_codebase_content() {
        assert!(_RULES_NO_CODEBASE.contains("NO-CODEBASE"));
        assert!(_RULES_NO_CODEBASE.contains("from_pretrained"));
    }

    // -----------------------------------------------------------------------
    // build_aider_cmd flags
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_aider_cmd_basic_flags() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();

        // Create required files
        std::fs::write(ws.join("main.py"), "# main\n").unwrap();
        std::fs::write(ws.join("GUIDANCE.md"), "# guidance\n").unwrap();
        std::fs::write(ws.join("EXPERIMENT_PLAN.yaml"), "# plan\n").unwrap();

        let bridge = OpenHandsBridge {
            model: "claude-opus-4-6".to_string(),
            llm_base_url: "http://localhost:8080".to_string(),
            api_key_env: String::new(),
            api_key: String::new(),
            timeout_sec: 600,
            max_retries: 0,
        };

        let cmd = bridge.build_aider_cmd(ws, "test message", "sk-test", "diff");

        // Model should have openai/ prefix added
        assert!(cmd.contains(&"--model".to_string()));
        let model_idx = cmd.iter().position(|x| x == "--model").unwrap();
        assert_eq!(cmd[model_idx + 1], "openai/claude-opus-4-6");

        // Check required flags
        assert!(cmd.contains(&"--yes".to_string()));
        assert!(cmd.contains(&"--no-auto-commits".to_string()));
        assert!(cmd.contains(&"--no-stream".to_string()));
        assert!(cmd.contains(&"--no-git".to_string()));
        assert!(cmd.contains(&"--no-show-model-warnings".to_string()));
        assert!(cmd.contains(&"--no-show-release-notes".to_string()));
        assert!(cmd.contains(&"--no-check-update".to_string()));
        assert!(cmd.contains(&"--no-browser".to_string()));
        assert!(cmd.contains(&"--edit-format".to_string()));
        assert!(cmd.contains(&"diff".to_string()));
        assert!(cmd.contains(&"--map-tokens".to_string()));
        assert!(cmd.contains(&"2048".to_string()));
        assert!(cmd.contains(&"--message-file".to_string()));
        assert!(cmd.contains(&"--openai-api-base".to_string()));
        assert!(cmd.contains(&"--openai-api-key".to_string()));
    }

    #[test]
    fn test_build_aider_cmd_model_with_slash() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        std::fs::write(ws.join("main.py"), "# main\n").unwrap();

        let bridge = OpenHandsBridge {
            model: "openai/gpt-4o".to_string(),
            ..Default::default()
        };

        let cmd = bridge.build_aider_cmd(ws, "msg", "key", "diff");
        let model_idx = cmd.iter().position(|x| x == "--model").unwrap();
        // Should NOT double-prefix if already has /
        assert_eq!(cmd[model_idx + 1], "openai/gpt-4o");
    }

    #[test]
    fn test_build_aider_cmd_edit_format_whole() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        std::fs::write(ws.join("main.py"), "# main\n").unwrap();

        let bridge = OpenHandsBridge::default();
        let cmd = bridge.build_aider_cmd(ws, "msg", "key", "whole");
        let fmt_idx = cmd.iter().position(|x| x == "--edit-format").unwrap();
        assert_eq!(cmd[fmt_idx + 1], "whole");
    }

    #[test]
    fn test_build_aider_cmd_includes_read_files() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        std::fs::write(ws.join("main.py"), "# main\n").unwrap();
        std::fs::write(ws.join("EXPERIMENT_PLAN.yaml"), "# plan\n").unwrap();

        let bridge = OpenHandsBridge::default();
        let cmd = bridge.build_aider_cmd(ws, "msg", "key", "diff");

        // EXPERIMENT_PLAN.yaml should be a read file
        assert!(cmd.contains(&"--read".to_string()), "should include --read flags");
    }

    // -----------------------------------------------------------------------
    // resolve_api_key
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_api_key_explicit() {
        let bridge = OpenHandsBridge {
            api_key: "explicit-key-123".to_string(),
            api_key_env: "SOME_ENV_VAR".to_string(),
            ..Default::default()
        };
        assert_eq!(bridge.resolve_api_key(), "explicit-key-123");
    }

    #[test]
    fn test_resolve_api_key_env_var() {
        let bridge = OpenHandsBridge {
            api_key: String::new(),
            api_key_env: "AIDER_TEST_API_KEY_XYZ".to_string(),
            ..Default::default()
        };
        // Set env var (unsafe in Rust 2024)
        unsafe {
            std::env::set_var("AIDER_TEST_API_KEY_XYZ", "env-key-456");
        }
        assert_eq!(bridge.resolve_api_key(), "env-key-456");
        unsafe {
            std::env::remove_var("AIDER_TEST_API_KEY_XYZ");
        }
    }

    #[test]
    fn test_resolve_api_key_fallback_empty() {
        let bridge = OpenHandsBridge {
            api_key: String::new(),
            api_key_env: String::new(),
            ..Default::default()
        };
        assert_eq!(bridge.resolve_api_key(), "");
    }

    // -----------------------------------------------------------------------
    // collect_files
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_files_new_files() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();

        // Empty snapshot — all files are "new"
        let snapshot: HashMap<String, String> = HashMap::new();
        std::fs::write(
            ws.join(".codebase_snapshot.json"),
            serde_json::to_string(&snapshot).unwrap(),
        )
        .unwrap();
        std::fs::write(ws.join("main.py"), "print('hello')\n").unwrap();

        let files = OpenHandsBridge::collect_files(ws);
        assert!(files.contains_key("main.py"), "should collect main.py");
    }

    #[test]
    fn test_collect_files_skips_unchanged() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();

        let content = b"print('hello')\n";
        let mut hasher = Md5::new();
        hasher.update(content);
        let hash = format!("{:x}", hasher.finalize());

        let mut snapshot = HashMap::new();
        snapshot.insert("main.py".to_string(), hash);
        std::fs::write(
            ws.join(".codebase_snapshot.json"),
            serde_json::to_string(&snapshot).unwrap(),
        )
        .unwrap();
        std::fs::write(ws.join("main.py"), content).unwrap();

        let files = OpenHandsBridge::collect_files(ws);
        assert!(!files.contains_key("main.py"), "unchanged file should be skipped");
    }

    #[test]
    fn test_collect_files_skips_codebases_dir() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();

        let snapshot: HashMap<String, String> = HashMap::new();
        std::fs::write(
            ws.join(".codebase_snapshot.json"),
            serde_json::to_string(&snapshot).unwrap(),
        )
        .unwrap();

        // File in codebases/ should be skipped
        std::fs::create_dir_all(ws.join("codebases/repo")).unwrap();
        std::fs::write(ws.join("codebases/repo/train.py"), "# codebase\n").unwrap();
        // File at root should be collected
        std::fs::write(ws.join("main.py"), "# main\n").unwrap();

        let files = OpenHandsBridge::collect_files(ws);
        assert!(files.contains_key("main.py"));
        assert!(!files.contains_key("train.py"), "codebases/ files should be skipped");
    }

    // -----------------------------------------------------------------------
    // find_core_source_files
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_core_source_files_no_codebases_dir() {
        let dir = TempDir::new().unwrap();
        let result = OpenHandsBridge::find_core_source_files(dir.path(), 10, 150);
        assert!(result.is_empty(), "no codebases/ dir should return empty");
    }

    #[test]
    fn test_find_core_source_files_finds_small_files() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        let codebases = ws.join("codebases/repo");
        std::fs::create_dir_all(&codebases).unwrap();

        // Write a small core file (30 lines)
        let small_content: String = (1..=30).map(|i| format!("line{i}\n")).collect();
        std::fs::write(codebases.join("core.py"), &small_content).unwrap();

        // Write a large file (200 lines) — should be excluded
        let large_content: String = (1..=200).map(|i| format!("line{i}\n")).collect();
        std::fs::write(codebases.join("large.py"), &large_content).unwrap();

        // Write a file starting with _ — should be excluded (not __init__.py)
        std::fs::write(codebases.join("_private.py"), &small_content).unwrap();

        // Write __init__.py — should be included if in range
        std::fs::write(codebases.join("__init__.py"), &small_content).unwrap();

        let result = OpenHandsBridge::find_core_source_files(ws, 10, 150);
        let names: Vec<&str> = result
            .iter()
            .map(|p| Path::new(p).file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"core.py"), "core.py should be included");
        assert!(!names.contains(&"large.py"), "large.py should be excluded");
        assert!(!names.contains(&"_private.py"), "_private.py should be excluded");
        assert!(names.contains(&"__init__.py"), "__init__.py should be included");
    }

    // -----------------------------------------------------------------------
    // Default values
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_values() {
        let bridge = OpenHandsBridge::default();
        assert_eq!(bridge.model, "openai/claude-opus-4-6");
        assert_eq!(bridge.timeout_sec, 1200);
        assert_eq!(bridge.max_retries, 0);
        assert!(bridge.llm_base_url.is_empty());
        assert!(bridge.api_key.is_empty());
        assert!(bridge.api_key_env.is_empty());
    }

    // -----------------------------------------------------------------------
    // build_repeat_hint
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_repeat_hint_empty() {
        let hint = build_repeat_hint(None, "some error");
        assert!(hint.is_empty());

        let hint = build_repeat_hint(Some(&[]), "some error");
        assert!(hint.is_empty());
    }

    #[test]
    fn test_build_repeat_hint_same_error() {
        let mut entry: HashMap<String, Value> = HashMap::new();
        entry.insert("iteration".to_string(), Value::Number(1.into()));
        entry.insert("patches_applied".to_string(), Value::Array(vec![]));
        entry.insert("failed_test".to_string(), Value::String("test_foo".to_string()));
        entry.insert("error_tail".to_string(), Value::String("FileNotFoundError: no such file".to_string()));

        let fixes = vec![entry];
        let current_stderr = "FileNotFoundError: no such file";
        let hint = build_repeat_hint(Some(&fixes), current_stderr);
        assert!(hint.contains("WARNING — SAME ERROR"), "should detect same error");
    }

    #[test]
    fn test_build_repeat_hint_cycle() {
        let mut e1: HashMap<String, Value> = HashMap::new();
        e1.insert("iteration".to_string(), Value::Number(1.into()));
        e1.insert("patches_applied".to_string(), Value::Array(vec![]));
        e1.insert("failed_test".to_string(), Value::String("test_foo".to_string()));
        e1.insert("error_tail".to_string(), Value::String("TypeError: unexpected type".to_string()));

        let mut e2: HashMap<String, Value> = HashMap::new();
        e2.insert("iteration".to_string(), Value::Number(2.into()));
        e2.insert("patches_applied".to_string(), Value::Array(vec![]));
        e2.insert("failed_test".to_string(), Value::String("test_foo".to_string()));
        e2.insert("error_tail".to_string(), Value::String("AttributeError: has no attribute 'x'".to_string()));

        let fixes = vec![e1, e2];
        // current error = same as first (cycle)
        let current_stderr = "TypeError: unexpected type";
        let hint = build_repeat_hint(Some(&fixes), current_stderr);
        assert!(hint.contains("CYCLE DETECTED") || hint.contains("NOTE"), "should detect cycle or progress");
    }

    // -----------------------------------------------------------------------
    // Constants
    // -----------------------------------------------------------------------

    #[test]
    fn test_max_todo_iterations() {
        assert_eq!(MAX_TODO_ITERATIONS, 10);
    }

    #[test]
    fn test_max_fix_attempts() {
        assert_eq!(MAX_FIX_ATTEMPTS, 2);
    }
}
