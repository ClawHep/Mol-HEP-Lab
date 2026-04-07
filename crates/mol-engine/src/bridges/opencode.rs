//! OpenCode "Beast Mode" bridge — routes complex code generation to OpenCode CLI.
//!
//! OpenCode (<https://github.com/anomalyco/opencode>) is an external AI coding agent
//! invoked via `opencode run --format json "prompt"`.  This module provides:
//!
//! 1. [`ComplexityScore`] / [`score_complexity`] — analyses an experiment plan to
//!    decide whether beast mode is warranted.
//! 2. [`OpenCodeBridge`] — manages workspace creation, OpenCode invocation, file
//!    collection, and cleanup.
//! 3. [`count_historical_failures`] — counts past stage failures from log files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use std::sync::LazyLock;

use anyhow::Result;
use super::common::{copy_dir_filtered, md5_hex};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Pre-compiled regexes (LazyLock — compiled once on first use)
// ---------------------------------------------------------------------------

static RE_CONDITION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:condition|ablation|variant|experiment)\s*[-_:]?\s*\d+").unwrap()
});

static RE_URL_HOST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://([^.]+)").unwrap()
});

// ---------------------------------------------------------------------------
// Keyword constants
// ---------------------------------------------------------------------------

/// Keywords that indicate multi-component architectures.
const COMPONENT_KEYWORDS: &[&str] = &[
    "encoder",
    "decoder",
    "discriminator",
    "generator",
    "critic",
    "actor",
    "teacher",
    "student",
    "backbone",
    "head",
    "neck",
    "classifier",
    "embedder",
    "attention",
    "transformer",
    "tokenizer",
    "vae",
    "autoencoder",
];

/// Indicators that multi-file generation is needed.
const FILE_HINT_KEYWORDS: &[&str] = &[
    "model.py",
    "trainer.py",
    "dataset.py",
    "utils.py",
    "config.py",
    "multiple files",
    "modular",
    "separate module",
    "multi-file",
];

/// Domain-complexity keywords.
const DOMAIN_COMPLEX_KEYWORDS: &[&str] = &[
    "multi-modal",
    "multimodal",
    "distributed",
    "gan",
    "diffusion",
    "nerf",
    "mixture of experts",
    "moe",
    "meta-learning",
    "meta learning",
    "maml",
    "neural ode",
    "neural sde",
    "physics-informed",
    "pinn",
    "graph neural",
    "gnn",
    "reinforcement learning",
    "multi-agent",
    "world model",
    "vision-language",
    "text-to-image",
    "image-to-text",
];

/// Patterns suggesting deep dependency chains.
const DEPENDENCY_KEYWORDS: &[&str] = &[
    "custom layer",
    "custom loss",
    "wrapper",
    "registry",
    "hook",
    "callback",
    "scheduler",
    "custom optimizer",
    "custom dataset",
    "custom sampler",
    "custom transform",
];

// ---------------------------------------------------------------------------
// Complexity scoring
// ---------------------------------------------------------------------------

/// Result of complexity analysis on an experiment plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityScore {
    /// Composite score in [0.0, 1.0].
    pub score: f64,
    /// Individual signal contributions.
    pub signals: HashMap<String, f64>,
    /// Routing recommendation: `"beast_mode"` | `"code_agent"` | `"legacy"`.
    pub recommendation: String,
    /// Human-readable reason for the recommendation.
    pub reason: String,
}

/// Count how many keywords from the slice appear in pre-lowercased `text`.
fn count_keyword_hits(lower_text: &str, keywords: &[&str]) -> usize {
    keywords.iter().filter(|&&kw| lower_text.contains(kw)).count()
}

/// Score the complexity of an experiment to determine if beast mode is warranted.
///
/// Returns a [`ComplexityScore`] with `score` in `[0.0, 1.0]`.
pub fn score_complexity(
    exp_plan: &str,
    topic: &str,
    historical_failures: usize,
    threshold: f64,
) -> ComplexityScore {
    if exp_plan.is_empty() && topic.is_empty() {
        return ComplexityScore {
            score: 0.0,
            signals: HashMap::new(),
            recommendation: "legacy".to_string(),
            reason: "Empty plan".to_string(),
        };
    }

    let combined = format!("{topic}\n{exp_plan}");
    let combined_lower = combined.to_lowercase();

    // Signal 1: Component count (weight 0.25)
    let comp_hits = count_keyword_hits(&combined_lower, COMPONENT_KEYWORDS);
    let component_score = (comp_hits as f64 / 5.0_f64).min(1.0);

    // Signal 2: File count hint (weight 0.20)
    let file_hits = count_keyword_hits(&combined_lower, FILE_HINT_KEYWORDS);
    let file_score = (file_hits as f64 / 3.0_f64).min(1.0);

    // Signal 3: Domain complexity (weight 0.20)
    let domain_hits = count_keyword_hits(&combined_lower, DOMAIN_COMPLEX_KEYWORDS);
    let domain_score = (domain_hits as f64 / 3.0_f64).min(1.0);

    // Signal 4: Condition count (weight 0.15)
    // Look for numbered conditions, ablation mentions, variant mentions
    let mut condition_matches = RE_CONDITION.find_iter(&combined).count();
    // Also count "baseline" occurrences
    condition_matches += combined_lower.matches("baseline").count();
    let condition_score = (condition_matches as f64 / 8.0_f64).min(1.0);

    // Signal 5: Historical failures (weight 0.10)
    let failure_score = (historical_failures as f64 / 3.0_f64).min(1.0);

    // Signal 6: Dependency depth (weight 0.10)
    let dep_hits = count_keyword_hits(&combined_lower, DEPENDENCY_KEYWORDS);
    let dep_score = (dep_hits as f64 / 3.0_f64).min(1.0);

    // Weighted sum
    let weighted = 0.25 * component_score
        + 0.20 * file_score
        + 0.20 * domain_score
        + 0.15 * condition_score
        + 0.10 * failure_score
        + 0.10 * dep_score;
    let final_score = weighted.clamp(0.0, 1.0);

    // Round each signal to 3 decimal places
    let round3 = |v: f64| (v * 1000.0).round() / 1000.0;
    let signals: HashMap<String, f64> = [
        ("component_count", round3(component_score)),
        ("file_count_hint", round3(file_score)),
        ("domain_complexity", round3(domain_score)),
        ("condition_count", round3(condition_score)),
        ("historical_failure", round3(failure_score)),
        ("dependency_depth", round3(dep_score)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let (recommendation, reason) = if final_score >= threshold {
        // Top 3 signals by value (descending)
        let mut sorted_signals: Vec<(&String, &f64)> = signals.iter().collect();
        sorted_signals.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top3: Vec<String> = sorted_signals
            .iter()
            .take(3)
            .map(|(k, v)| format!("{k}={v:.2}"))
            .collect();
        let reason = format!(
            "Complexity {final_score:.2} >= threshold {threshold:.2}: top signals: {}",
            top3.join(", ")
        );
        ("beast_mode".to_string(), reason)
    } else {
        let reason = format!("Complexity {final_score:.2} < threshold {threshold:.2}");
        ("code_agent".to_string(), reason)
    };

    let round4 = |v: f64| (v * 10000.0).round() / 10000.0;
    ComplexityScore {
        score: round4(final_score),
        signals,
        recommendation,
        reason,
    }
}

// ---------------------------------------------------------------------------
// OpenCode result
// ---------------------------------------------------------------------------

/// Result from an OpenCode invocation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenCodeResult {
    pub success: bool,
    pub files: HashMap<String, String>,
    pub opencode_log: String,
    pub elapsed_sec: f64,
    pub error: String,
}

// ---------------------------------------------------------------------------
// Mega-prompt template
// ---------------------------------------------------------------------------

const MEGA_PROMPT_TEMPLATE: &str = r#"You are implementing a complete, runnable ML/science experiment.

STEP 1 — READ THE WORKSPACE:
The workspace contains the COMPLETE source code of a local codebase plus context files:
- EXPERIMENT_PLAN.yaml — the experiment design
- GUIDANCE.md — topic, metric, environment, constraints
- Source code files — the actual codebase you MUST build upon
- `data/` — symlink to local datasets (reference images, masks, configs)
- `checkpoints/` — symlink to pretrained model weights

Before writing ANY code, read the existing source files to understand the codebase:
1. Find and read any example/demo scripts (e.g. *_demo.py, run_*.py, example_*.py) to see how the pipeline works end-to-end.
2. Read the core modules to understand the API (class signatures, function arguments).
3. Read the dataset configs or sample data in data/ to understand the data format.
4. Check what checkpoints/weights are available in checkpoints/.

STEP 2 — IMPLEMENTATION RULES:
- Build ON TOP of the existing codebase. Import and extend existing modules.
- Load pretrained models from `checkpoints/` using the codebase's loading API, NOT from the internet.
- Load real data from `data/` using the codebase's data loading utilities, NOT synthetic torch.randn().
- Compute REAL metrics from actual model outputs. NEVER use np.random or random.uniform as a metric placeholder.
- Each experimental condition must produce genuinely different behavior.
- Do NOT rewrite modules that already exist — import and extend them.
- NEVER invent module names — only use modules visible in the workspace.

STEP 3 — CREATE main.py:
1. main.py is the NEW entry point that runs ALL experimental conditions.
2. It must print the primary metric as: {metric}: <value>
3. Use multi-seed evaluation (seeds 0, 1, 2) and report mean +/- std.
4. Implement a time guard: stop gracefully at 80% of the time budget ({time_budget_sec} seconds).
5. Each condition must be wrapped in try/except for crash resilience.
6. Print per-condition results: condition=<name> seed=<s> {metric}: <value>

IMPORTANT CONSTRAINTS:
- Do NOT use argparse or CLI arguments — hardcode all configuration.
- All output must go to stdout (print statements).
- Keep the experiment feasible within {time_budget_sec} seconds total.
"#;

// ---------------------------------------------------------------------------
// OpenCodeBridge
// ---------------------------------------------------------------------------

/// Manages OpenCode CLI invocations for beast mode code generation.
#[derive(Debug, Clone)]
pub struct OpenCodeBridge {
    model: String,
    llm_base_url: String,
    api_key_env: String,
    llm_provider: String,
    timeout_sec: u64,
    max_retries: usize,
    workspace_cleanup: bool,
}

impl Default for OpenCodeBridge {
    fn default() -> Self {
        Self {
            model: String::new(),
            llm_base_url: String::new(),
            api_key_env: String::new(),
            llm_provider: "openai-compatible".to_string(),
            timeout_sec: 600,
            max_retries: 1,
            workspace_cleanup: true,
        }
    }
}

impl OpenCodeBridge {
    /// Create a new bridge with the given configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: impl Into<String>,
        llm_base_url: impl Into<String>,
        api_key_env: impl Into<String>,
        llm_provider: impl Into<String>,
        timeout_sec: u64,
        max_retries: usize,
        workspace_cleanup: bool,
    ) -> Self {
        Self {
            model: model.into(),
            llm_base_url: llm_base_url.into(),
            api_key_env: api_key_env.into(),
            llm_provider: llm_provider.into(),
            timeout_sec,
            max_retries,
            workspace_cleanup,
        }
    }

    // -- availability check ---------------------------------------------------

    /// Return `true` if the `opencode` CLI is installed and callable.
    pub async fn check_available() -> bool {
        match Command::new("opencode")
            .arg("--version")
            .output()
            .await
        {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    // -- Azure detection ------------------------------------------------------

    /// Detect Azure OpenAI from base URL or provider string.
    pub fn is_azure(&self) -> bool {
        self.llm_base_url.to_lowercase().contains("azure")
            || self.llm_provider.to_lowercase().contains("azure")
    }

    // -- opencode.json config -------------------------------------------------

    /// Build the `opencode.json` configuration object.
    pub fn build_opencode_config(&self) -> Value {
        let mut cfg = serde_json::json!({
            "$schema": "https://opencode.ai/config.json"
        });

        if self.is_azure() {
            // Azure OpenAI provider
            // Extract resource name from URL like:
            //   https://myresource-eastus2.services.ai.azure.com/openai/v1
            //   https://myresource.openai.azure.com/openai
            let resource_name = if !self.llm_base_url.is_empty() {
                RE_URL_HOST.captures(&self.llm_base_url)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };

            // Normalize base URL: Azure provider wants the /openai path
            let base_url = {
                let mut url = self.llm_base_url.trim_end_matches('/').to_string();
                if !url.ends_with("/openai") {
                    // Strip /v1 suffix if present, add /openai if needed
                    if url.ends_with("/v1") {
                        url = url[..url.len() - 3].to_string();
                    }
                    if !url.ends_with("/openai") {
                        url.push_str("/openai");
                    }
                }
                url
            };

            let api_key_val = if !self.api_key_env.is_empty() {
                format!("{{env:{}}}", self.api_key_env)
            } else {
                String::new()
            };

            if !self.model.is_empty() {
                let model_key = if self.model.contains('/') {
                    self.model.clone()
                } else {
                    format!("azure/{}", self.model)
                };
                cfg["model"] = Value::String(model_key);
            }

            let model_registry: Value = if !self.model.is_empty() {
                serde_json::json!({
                    &self.model: {
                        "name": &self.model,
                        "modalities": {
                            "input": ["text"],
                            "output": ["text"]
                        }
                    }
                })
            } else {
                serde_json::json!({})
            };

            cfg["provider"] = serde_json::json!({
                "azure": {
                    "options": {
                        "apiKey": api_key_val,
                        "baseURL": base_url,
                        "resourceName": resource_name,
                    },
                    "models": model_registry
                }
            });
        } else if !self.llm_base_url.is_empty() {
            let resolved_model = if self.model.contains('/') {
                self.model.clone()
            } else {
                format!("openai/{}", self.model)
            };
            if !self.model.is_empty() {
                cfg["model"] = Value::String(resolved_model);
            }

            let api_key_val = if !self.api_key_env.is_empty() {
                format!("{{env:{}}}", self.api_key_env)
            } else {
                String::new()
            };

            let bare_name = if self.model.contains('/') {
                self.model.split('/').last().unwrap_or(&self.model).to_string()
            } else {
                self.model.clone()
            };

            let model_registry: Value = if !self.model.is_empty() {
                let bare_name_clone = bare_name.clone();
                serde_json::json!({
                    bare_name_clone: {
                        "name": bare_name,
                        "modalities": {
                            "input": ["text"],
                            "output": ["text"]
                        }
                    }
                })
            } else {
                serde_json::json!({})
            };

            let mut provider_cfg = serde_json::json!({
                "options": {
                    "baseURL": &self.llm_base_url,
                    "apiKey": api_key_val
                }
            });
            if !self.model.is_empty() {
                provider_cfg["models"] = model_registry;
            }
            cfg["provider"] = serde_json::json!({ "openai": provider_cfg });
        } else if !self.model.is_empty() {
            let model_val = if self.model.contains('/') {
                self.model.clone()
            } else {
                format!("openai/{}", self.model)
            };
            cfg["model"] = Value::String(model_val);
        }

        cfg
    }

    // -- model resolution -----------------------------------------------------

    /// Resolve the model identifier for OpenCode CLI's `-m` flag.
    ///
    /// Azure OpenAI endpoints use the Responses API which many Azure deployments
    /// don't support.  When the configured provider is Azure, we fall back to
    /// using Anthropic models directly (which OpenCode supports natively).
    ///
    /// Resolution order:
    /// 1. If model already contains `/` (e.g. `"anthropic/claude-sonnet-4-6"`) → use as-is
    /// 2. If NOT Azure → `"openai/{model}"`
    /// 3. If Azure → fall back to `"anthropic/claude-sonnet-4-6"` (reliable default)
    pub fn resolve_opencode_model(&self) -> String {
        if self.model.is_empty() {
            return "anthropic/claude-sonnet-4-6".to_string();
        }
        if self.model.contains('/') {
            return self.model.clone();
        }
        if self.is_azure() {
            info!(
                "Beast mode: Azure endpoint detected — using Anthropic model \
                 for OpenCode (Azure doesn't support Responses API)"
            );
            return "anthropic/claude-sonnet-4-6".to_string();
        }
        format!("openai/{}", self.model)
    }

    // -- workspace preparation ------------------------------------------------

    /// Create a temporary workspace directory with context files.
    ///
    /// Sets up EXPERIMENT_PLAN.yaml, GUIDANCE.md, opencode.json config,
    /// copies codebases, symlinks datasets/checkpoints, and writes
    /// `.codebase_snapshot.json` with MD5 hashes for change detection.
    #[allow(clippy::too_many_arguments)]
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
    ) -> Result<PathBuf> {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mono_part = {
            // Use a simple counter via instant for workspace uniqueness
            let t = Instant::now();
            t.elapsed().subsec_nanos() % 100_000
        };
        let ws = stage_dir.join(format!("opencode_beast_{ts}_{mono_part}"));
        fs::create_dir_all(&ws)?;

        // Write experiment plan
        fs::write(
            ws.join("EXPERIMENT_PLAN.yaml"),
            if exp_plan.is_empty() {
                "# No experiment plan provided\n"
            } else {
                exp_plan
            },
        )?;

        // Write guidance document
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
        fs::write(ws.join("GUIDANCE.md"), guidance_parts.join("\n"))?;

        // Write opencode.json config
        let opencode_cfg = self.build_opencode_config();
        fs::write(
            ws.join("opencode.json"),
            serde_json::to_string_pretty(&opencode_cfg)?,
        )?;

        // Copy local codebases into workspace root
        if !codebases_dir.is_empty() {
            let cb_path = Path::new(codebases_dir);
            if cb_path.is_dir() {
                let mut entries: Vec<_> = fs::read_dir(cb_path)?
                    .filter_map(|e| e.ok())
                    .collect();
                entries.sort_by_key(|e| e.file_name());
                for entry in entries {
                    let repo = entry.path();
                    if repo.is_dir() {
                        let name = repo.file_name().unwrap_or_default().to_string_lossy();
                        if !name.starts_with('.') {
                            copy_dir_filtered(&repo, &ws)?;
                        }
                    }
                }
            }
        }

        // Symlink datasets and checkpoints into workspace
        if !datasets_dir.is_empty() {
            let ds_path = Path::new(datasets_dir);
            if ds_path.is_dir() {
                let link = ws.join("data");
                if !link.exists() {
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(ds_path.canonicalize()?, &link)?;
                    #[cfg(windows)]
                    std::os::windows::fs::symlink_dir(ds_path.canonicalize()?, &link)?;
                }
            }
        }

        if !checkpoints_dir.is_empty() {
            let ck_path = Path::new(checkpoints_dir);
            if ck_path.is_dir() {
                let link = ws.join("checkpoints");
                if !link.exists() {
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(ck_path.canonicalize()?, &link)?;
                    #[cfg(windows)]
                    std::os::windows::fs::symlink_dir(ck_path.canonicalize()?, &link)?;
                }
            }
        }

        // Append concrete usage hints to GUIDANCE.md
        let mut usage_hints: Vec<String> = Vec::new();
        let data_link = ws.join("data");
        if data_link.exists() {
            if let Ok(ds_root) = Path::new(datasets_dir).canonicalize() {
                if let Ok(entries) = fs::read_dir(&ds_root) {
                    let mut ds_items: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path().is_dir()
                                && !e.file_name().to_string_lossy().starts_with('.')
                        })
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect();
                    ds_items.sort();
                    usage_hints.push(format!(
                        "## Local Data Layout\nDatasets available in `data/`: {}\n",
                        ds_items.join(", ")
                    ));
                    for ds_name in ds_items.iter().take(3) {
                        let ds_sub = ds_root.join(ds_name);
                        let mut sub_items: Vec<String> = walkdir::WalkDir::new(&ds_sub)
                            .min_depth(1)
                            .into_iter()
                            .filter_map(|e| e.ok())
                            .filter(|e| e.path().is_file())
                            .map(|e| {
                                e.file_name().to_string_lossy().into_owned()
                            })
                            .take(10)
                            .collect();
                        sub_items.sort();
                        if !sub_items.is_empty() {
                            usage_hints.push(format!(
                                "- `data/{ds_name}/`: {}",
                                sub_items.join(", ")
                            ));
                        }
                    }
                }
            }
        }
        let ck_link = ws.join("checkpoints");
        if ck_link.exists() {
            if let Ok(ck_root) = Path::new(checkpoints_dir).canonicalize() {
                if let Ok(entries) = fs::read_dir(&ck_root) {
                    let mut ck_items: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect();
                    ck_items.sort();
                    let mut ck_hint = format!(
                        "\n## Local Checkpoints\nAvailable in `checkpoints/`: {}\n\
                         Load pretrained models from this path instead of downloading.\n",
                        ck_items.join(", ")
                    );
                    if let Some(first) = ck_items.first() {
                        ck_hint.push_str(&format!(
                            "Example: `model.from_pretrained('checkpoints/{first}')`"
                        ));
                    }
                    usage_hints.push(ck_hint);
                }
            }
        }

        if !usage_hints.is_empty() {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(ws.join("GUIDANCE.md"))?;
            writeln!(f, "\n\n{}", usage_hints.join("\n"))?;
        }

        // Snapshot codebase file hashes for change detection in _collect_files
        let mut snapshot: HashMap<String, String> = HashMap::new();
        for entry in walkdir::WalkDir::new(&ws)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && !e.path_is_symlink()
                    && e.path()
                        .extension()
                        .map(|x| x == "py")
                        .unwrap_or(false)
            })
        {
            if let Ok(rel) = entry.path().strip_prefix(&ws) {
                let rel_str = rel.to_string_lossy().into_owned();
                match std::fs::read(entry.path()) {
                    Ok(bytes) => {
                        let hash = md5_hex(&bytes);
                        snapshot.insert(rel_str, hash);
                    }
                    Err(_) => {}
                }
            }
        }
        fs::write(
            ws.join(".codebase_snapshot.json"),
            serde_json::to_string(&snapshot)?,
        )?;

        Ok(ws)
    }

    // -- invocation -----------------------------------------------------------

    /// Run `opencode run` in the workspace.
    ///
    /// Returns `(success, log, elapsed_secs)`.
    pub async fn invoke_opencode(
        &self,
        workspace: &Path,
        prompt: &str,
    ) -> (bool, String, f64) {
        let workspace = match workspace.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return (false, format!("Failed to resolve workspace: {e}"), 0.0);
            }
        };

        let resolved_model = self.resolve_opencode_model();

        let mut cmd = Command::new("opencode");
        cmd.arg("run")
            .arg("-m")
            .arg(&resolved_model)
            .arg("--format")
            .arg("json")
            .arg("--dir")
            .arg(&workspace)
            .arg(prompt)
            .current_dir(&workspace)
            .stdin(std::process::Stdio::null())
            .env("DO_NOT_TRACK", "1");

        // Pass API key via environment if configured
        if !self.api_key_env.is_empty() {
            if let Ok(api_key) = std::env::var(&self.api_key_env) {
                if !api_key.is_empty() {
                    if self.is_azure() {
                        cmd.env("AZURE_API_KEY", &api_key);
                    } else {
                        cmd.env("OPENAI_API_KEY", &api_key);
                    }
                }
            }
        }

        let t0 = Instant::now();
        let timeout_dur = std::time::Duration::from_secs(self.timeout_sec);

        match tokio::time::timeout(timeout_dur, cmd.output()).await {
            Ok(Ok(output)) => {
                let elapsed = t0.elapsed().as_secs_f64();
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let log = format!("{stdout}\n{stderr}");
                (output.status.success(), log, elapsed)
            }
            Ok(Err(e)) => {
                let elapsed = t0.elapsed().as_secs_f64();
                if e.kind() == std::io::ErrorKind::NotFound {
                    (false, "opencode CLI not found".to_string(), 0.0)
                } else {
                    (false, format!("Unexpected error: {e}"), elapsed)
                }
            }
            Err(_) => {
                let elapsed = t0.elapsed().as_secs_f64();
                (
                    false,
                    format!("TIMEOUT after {elapsed:.1}s"),
                    elapsed,
                )
            }
        }
    }

    // -- file collection ------------------------------------------------------

    /// Collect generated Python files, `requirements.txt`, and `setup.py`.
    ///
    /// Only collects files that are **new or modified** relative to the
    /// codebase snapshot taken during workspace preparation.  Unchanged
    /// codebase files are skipped.
    ///
    /// File names are flattened to basenames (`src/main.py` → `main.py`).
    /// If two files share the same basename, the one closer to the workspace
    /// root wins.
    pub fn collect_files(workspace: &Path) -> HashMap<String, String> {
        // Load the original codebase snapshot (if any)
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

        // Collect .py files sorted by path depth (shallow first)
        let mut py_files: Vec<PathBuf> = walkdir::WalkDir::new(workspace)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && !e.path_is_symlink()
                    && e.path()
                        .extension()
                        .map(|x| x == "py")
                        .unwrap_or(false)
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        // Sort by depth (number of path components relative to workspace)
        py_files.sort_by_key(|p| {
            p.strip_prefix(workspace)
                .map(|r| r.components().count())
                .unwrap_or(usize::MAX)
        });

        for py_file in &py_files {
            let rel = match py_file.strip_prefix(workspace) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Skip __pycache__ and hidden directories
            let skip = rel.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s.starts_with("__pycache__") || s.starts_with('.')
            });
            if skip {
                continue;
            }

            let rel_str = rel.to_string_lossy().into_owned();

            // Check if file is unchanged from snapshot
            if let Some(original_hash) = original_hashes.get(&rel_str) {
                match std::fs::read(py_file) {
                    Ok(bytes) => {
                        let current_hash = md5_hex(&bytes);
                        if &current_hash == original_hash {
                            continue; // unchanged codebase file
                        }
                    }
                    Err(_) => continue,
                }
            }

            let basename = rel
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if basename.is_empty() {
                continue;
            }

            // Shallow-first ordering means first occurrence wins
            if !files.contains_key(&basename) {
                match std::fs::read_to_string(py_file) {
                    Ok(content) => {
                        files.insert(basename, content);
                    }
                    Err(e) => {
                        warn!("Beast mode: failed to read {}: {}", py_file.display(), e);
                    }
                }
            }
        }

        // Collect requirements.txt and setup.py from workspace root
        for extra in &["requirements.txt", "setup.py"] {
            let p = workspace.join(extra);
            if p.exists() && !files.contains_key(*extra) {
                if let Some(orig_hash) = original_hashes.get(*extra) {
                    match std::fs::read(&p) {
                        Ok(bytes) => {
                            let cur = md5_hex(&bytes);
                            if &cur == orig_hash {
                                continue; // unchanged
                            }
                        }
                        Err(_) => continue,
                    }
                }
                if let Ok(content) = std::fs::read_to_string(&p) {
                    files.insert(extra.to_string(), content);
                }
            }
        }

        files
    }

    // -- main entry point -----------------------------------------------------

    /// Run OpenCode to generate experiment code.
    ///
    /// Returns an [`OpenCodeResult`] with success status and generated files.
    #[allow(clippy::too_many_arguments)]
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
    ) -> OpenCodeResult {
        if !Self::check_available().await {
            return OpenCodeResult {
                success: false,
                error: "OpenCode CLI not installed or not callable".to_string(),
                ..Default::default()
            };
        }

        let mut last_error = String::new();

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
                )
                .await
            {
                Ok(ws) => ws,
                Err(e) => {
                    last_error = format!("Failed to prepare workspace: {e}");
                    warn!("Beast mode: {last_error}");
                    continue;
                }
            };

            // Build the mega-prompt using string replacement to avoid
            // issues with curly braces in metric names like "F{1}"
            let prompt = MEGA_PROMPT_TEMPLATE
                .replace("{metric}", metric)
                .replace("{time_budget_sec}", &time_budget_sec.to_string());

            info!(
                "Beast mode: invoking OpenCode (attempt {}/{}, timeout={}s)",
                attempt + 1,
                1 + self.max_retries,
                self.timeout_sec,
            );

            let (success, log, elapsed) = self.invoke_opencode(&workspace, &prompt).await;

            // Collect files regardless of exit status — OpenCode may have
            // written valid code before a timeout or non-zero exit.
            let files = if workspace.exists() {
                Self::collect_files(&workspace)
            } else {
                HashMap::new()
            };

            if files.contains_key("main.py") {
                if !success {
                    info!(
                        "Beast mode: OpenCode exited non-zero / timed out but \
                         main.py found ({} files) — treating as success",
                        files.len()
                    );
                }

                // Write log to stage_dir
                if let Err(e) = std::fs::write(stage_dir.join("opencode_log.txt"), &log) {
                    warn!("Beast mode: failed to write opencode_log.txt: {e}");
                }

                if self.workspace_cleanup && workspace.exists() {
                    let _ = std::fs::remove_dir_all(&workspace);
                }

                return OpenCodeResult {
                    success: true,
                    files,
                    opencode_log: log,
                    elapsed_sec: elapsed,
                    error: String::new(),
                };
            }

            last_error = if !success {
                log.clone()
            } else {
                "No main.py in OpenCode output".to_string()
            };
            warn!(
                "Beast mode: OpenCode attempt {} failed ({:.1}s, files={:?}): {}",
                attempt + 1,
                elapsed,
                files.keys().collect::<Vec<_>>(),
                &last_error[..last_error.len().min(500)],
            );
            if self.workspace_cleanup && workspace.exists() {
                let _ = std::fs::remove_dir_all(&workspace);
            }
        }

        // All attempts failed
        OpenCodeResult {
            success: false,
            opencode_log: last_error,
            error: format!("OpenCode failed after {} attempt(s)", 1 + self.max_retries),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: count historical failures
// ---------------------------------------------------------------------------

/// Count past stage failures from stage directories and logs.
///
/// Each stage directory is counted at most once, even if multiple failure
/// indicators are present.
pub fn count_historical_failures(run_dir: &Path, stage_name: &str) -> usize {
    let mut failures = 0usize;
    let pattern = format!("{stage_name}*");
    if let Ok(entries) = std::fs::read_dir(run_dir) {
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .map(|n| {
                            let s = n.to_string_lossy();
                            glob_matches(&s, &pattern)
                        })
                        .unwrap_or(false)
            })
            .collect();
        dirs.sort();

        for d in dirs {
            let mut failed = false;

            // Check beast_mode_log.json
            if !failed {
                let bm_log = d.join("beast_mode_log.json");
                if bm_log.exists() {
                    if let Ok(text) = std::fs::read_to_string(&bm_log) {
                        if let Ok(data) = serde_json::from_str::<Value>(&text) {
                            if data.get("success").and_then(|v| v.as_bool()) == Some(false) {
                                failed = true;
                            }
                        }
                    }
                }
            }

            // Check stage_health.json
            if !failed {
                let health = d.join("stage_health.json");
                if health.exists() {
                    if let Ok(text) = std::fs::read_to_string(&health) {
                        if let Ok(data) = serde_json::from_str::<Value>(&text) {
                            if data.get("status").and_then(|v| v.as_str()) == Some("FAILED") {
                                failed = true;
                            }
                        }
                    }
                }
            }

            // Check validation_report.md
            if !failed {
                let vr = d.join("validation_report.md");
                if vr.exists() {
                    if let Ok(content) = std::fs::read_to_string(&vr) {
                        if content.contains("BLOCKED") || content.contains("FAILED") {
                            failed = true;
                        }
                    }
                }
            }

            if failed {
                failures += 1;
            }
        }
    }
    failures
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Simple glob match: `pattern` may contain `*` wildcards (prefix match: `name*`).
fn glob_matches(name: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // -- score_complexity tests -----------------------------------------------

    #[test]
    fn test_score_complexity_empty_returns_legacy() {
        let result = score_complexity("", "", 0, 0.6);
        assert_eq!(result.score, 0.0);
        assert_eq!(result.recommendation, "legacy");
        assert_eq!(result.reason, "Empty plan");
        assert!(result.signals.is_empty());
    }

    #[test]
    fn test_score_complexity_below_threshold() {
        // Only one component keyword → low score
        let result = score_complexity("Use an encoder", "", 0, 0.6);
        assert_eq!(result.recommendation, "code_agent");
        assert!(result.score < 0.6, "score={}", result.score);
        assert!(result.reason.contains('<'));
    }

    #[test]
    fn test_score_complexity_above_threshold() {
        // Many component keywords + domain complexity keywords
        let plan = "encoder decoder discriminator generator critic actor teacher student \
                    multi-modal diffusion gan nerf model.py trainer.py dataset.py modular";
        let result = score_complexity(plan, "vision-language", 0, 0.6);
        assert_eq!(result.recommendation, "beast_mode");
        assert!(result.score >= 0.6, "score={}", result.score);
        assert!(result.reason.contains("top signals"));
    }

    #[test]
    fn test_score_complexity_with_historical_failures() {
        // 3 failures saturates failure_score at 1.0 (weight 0.10)
        let result_0 = score_complexity("encoder", "topic", 0, 0.6);
        let result_3 = score_complexity("encoder", "topic", 3, 0.6);
        assert!(
            result_3.score > result_0.score,
            "failures should increase score"
        );
        let sig = result_3.signals.get("historical_failure").unwrap();
        assert!((sig - 1.0).abs() < 1e-9, "historical_failure signal should be 1.0");
    }

    #[test]
    fn test_score_complexity_condition_count() {
        let plan = "condition 1 ablation 2 variant 3 baseline baseline experiment 4";
        let result = score_complexity(plan, "", 0, 0.6);
        let cond = result.signals.get("condition_count").unwrap();
        // 4 regex matches + 2 "baseline" = 6 → 6/8 = 0.75
        assert!(*cond > 0.0, "condition_count should be non-zero");
    }

    #[test]
    fn test_score_signals_sum_to_expected_range() {
        let result = score_complexity("test plan", "test", 0, 0.6);
        assert!(result.score >= 0.0 && result.score <= 1.0);
        assert_eq!(result.signals.len(), 6);
    }

    // -- keyword counting tests -----------------------------------------------

    #[test]
    fn test_count_keyword_hits_case_insensitive() {
        // count_keyword_hits expects pre-lowered text (caller lowercases once)
        assert_eq!(count_keyword_hits(&"ENCODER Decoder".to_lowercase(), COMPONENT_KEYWORDS), 2);
        assert_eq!(count_keyword_hits(&"nothing here".to_lowercase(), COMPONENT_KEYWORDS), 0);
    }

    #[test]
    fn test_count_keyword_hits_multi_word() {
        assert_eq!(
            count_keyword_hits("mixture of experts and gnn", DOMAIN_COMPLEX_KEYWORDS),
            2
        );
    }

    // -- count_historical_failures tests --------------------------------------

    #[test]
    fn test_count_historical_failures_no_dirs() {
        let tmp = TempDir::new().unwrap();
        let count = count_historical_failures(tmp.path(), "stage-10");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_historical_failures_beast_mode_log() {
        let tmp = TempDir::new().unwrap();
        let stage_dir = tmp.path().join("stage-10-abc");
        fs::create_dir_all(&stage_dir).unwrap();
        fs::write(
            stage_dir.join("beast_mode_log.json"),
            r#"{"success": false}"#,
        )
        .unwrap();

        let count = count_historical_failures(tmp.path(), "stage-10");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_historical_failures_stage_health() {
        let tmp = TempDir::new().unwrap();
        let stage_dir = tmp.path().join("stage-10-xyz");
        fs::create_dir_all(&stage_dir).unwrap();
        fs::write(
            stage_dir.join("stage_health.json"),
            r#"{"status": "FAILED"}"#,
        )
        .unwrap();

        let count = count_historical_failures(tmp.path(), "stage-10");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_historical_failures_validation_report() {
        let tmp = TempDir::new().unwrap();
        let stage_dir = tmp.path().join("stage-10-001");
        fs::create_dir_all(&stage_dir).unwrap();
        fs::write(
            stage_dir.join("validation_report.md"),
            "Status: BLOCKED — experiments did not converge",
        )
        .unwrap();

        let count = count_historical_failures(tmp.path(), "stage-10");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_historical_failures_count_each_dir_once() {
        let tmp = TempDir::new().unwrap();
        let stage_dir = tmp.path().join("stage-10-multi");
        fs::create_dir_all(&stage_dir).unwrap();
        // Write both indicators
        fs::write(
            stage_dir.join("beast_mode_log.json"),
            r#"{"success": false}"#,
        )
        .unwrap();
        fs::write(
            stage_dir.join("stage_health.json"),
            r#"{"status": "FAILED"}"#,
        )
        .unwrap();

        // Should count as only 1 failure (not 2)
        let count = count_historical_failures(tmp.path(), "stage-10");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_historical_failures_success_not_counted() {
        let tmp = TempDir::new().unwrap();
        let stage_dir = tmp.path().join("stage-10-ok");
        fs::create_dir_all(&stage_dir).unwrap();
        fs::write(
            stage_dir.join("beast_mode_log.json"),
            r#"{"success": true}"#,
        )
        .unwrap();
        fs::write(
            stage_dir.join("stage_health.json"),
            r#"{"status": "PASSED"}"#,
        )
        .unwrap();

        let count = count_historical_failures(tmp.path(), "stage-10");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_historical_failures_different_stage_name() {
        let tmp = TempDir::new().unwrap();
        // Create stage-10 failure + stage-20 failure
        for name in &["stage-10-a", "stage-20-b"] {
            let d = tmp.path().join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("stage_health.json"), r#"{"status": "FAILED"}"#).unwrap();
        }
        assert_eq!(count_historical_failures(tmp.path(), "stage-10"), 1);
        assert_eq!(count_historical_failures(tmp.path(), "stage-20"), 1);
    }

    // -- file collection logic tests ------------------------------------------

    #[test]
    fn test_collect_files_returns_new_files() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        // No snapshot → all .py files are "new"
        fs::write(ws.join(".codebase_snapshot.json"), "{}").unwrap();
        fs::write(ws.join("main.py"), "print('hello')").unwrap();
        fs::write(ws.join("helper.py"), "x = 1").unwrap();

        let files = OpenCodeBridge::collect_files(ws);
        assert!(files.contains_key("main.py"));
        assert!(files.contains_key("helper.py"));
    }

    #[test]
    fn test_collect_files_skips_unchanged() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        let content = b"print('original')";
        let hash = md5_hex(content);

        // Put original.py in snapshot with matching hash
        let snapshot = serde_json::json!({ "original.py": hash });
        fs::write(ws.join(".codebase_snapshot.json"), snapshot.to_string()).unwrap();
        fs::write(ws.join("original.py"), content).unwrap();
        fs::write(ws.join("new.py"), "print('new')").unwrap();

        let files = OpenCodeBridge::collect_files(ws);
        assert!(!files.contains_key("original.py"), "unchanged file should be skipped");
        assert!(files.contains_key("new.py"));
    }

    #[test]
    fn test_collect_files_shallow_wins() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        fs::write(ws.join(".codebase_snapshot.json"), "{}").unwrap();
        fs::write(ws.join("main.py"), "shallow").unwrap();

        let sub = ws.join("subdir");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("main.py"), "deep").unwrap();

        let files = OpenCodeBridge::collect_files(ws);
        assert_eq!(files.get("main.py").map(|s| s.as_str()), Some("shallow"));
    }

    #[test]
    fn test_collect_files_skips_pycache() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        fs::write(ws.join(".codebase_snapshot.json"), "{}").unwrap();

        let cache = ws.join("__pycache__");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("module.cpython-311.pyc"), b"garbage" as &[u8]).unwrap();
        // Note: .pyc files won't be collected anyway (extension filter), but
        // test that __pycache__ directories are excluded for .py files too
        let sub_cache = ws.join("__pycache__");
        fs::create_dir_all(&sub_cache).unwrap();
        // Create a .py file inside __pycache__ (unusual but should be skipped)
        fs::write(sub_cache.join("cached.py"), "cached content").unwrap();

        let files = OpenCodeBridge::collect_files(ws);
        assert!(!files.contains_key("cached.py"));
    }

    // -- build_opencode_config tests ------------------------------------------

    #[test]
    fn test_build_opencode_config_default() {
        let bridge = OpenCodeBridge::default();
        let cfg = bridge.build_opencode_config();
        assert_eq!(cfg["$schema"], "https://opencode.ai/config.json");
        assert!(cfg.get("model").is_none());
        assert!(cfg.get("provider").is_none());
    }

    #[test]
    fn test_build_opencode_config_openai_model_only() {
        let bridge = OpenCodeBridge::new("gpt-4o", "", "", "openai-compatible", 600, 1, true);
        let cfg = bridge.build_opencode_config();
        assert_eq!(cfg["model"], "openai/gpt-4o");
    }

    #[test]
    fn test_build_opencode_config_azure() {
        let bridge = OpenCodeBridge::new(
            "my-deployment",
            "https://myresource.openai.azure.com/openai/v1",
            "AZURE_KEY",
            "openai-compatible",
            600,
            1,
            true,
        );
        let cfg = bridge.build_opencode_config();
        assert_eq!(cfg["model"], "azure/my-deployment");
        assert!(cfg["provider"]["azure"].is_object());
        let base = cfg["provider"]["azure"]["options"]["baseURL"].as_str().unwrap();
        assert!(base.ends_with("/openai"), "baseURL={base}");
    }

    // -- resolve_opencode_model tests -----------------------------------------

    #[test]
    fn test_resolve_model_empty() {
        let bridge = OpenCodeBridge::default();
        assert_eq!(bridge.resolve_opencode_model(), "anthropic/claude-sonnet-4-6");
    }

    #[test]
    fn test_resolve_model_with_slash() {
        let bridge = OpenCodeBridge::new(
            "anthropic/claude-opus-4-5",
            "",
            "",
            "openai-compatible",
            600,
            1,
            true,
        );
        assert_eq!(bridge.resolve_opencode_model(), "anthropic/claude-opus-4-5");
    }

    #[test]
    fn test_resolve_model_openai() {
        let bridge =
            OpenCodeBridge::new("gpt-4o", "", "", "openai-compatible", 600, 1, true);
        assert_eq!(bridge.resolve_opencode_model(), "openai/gpt-4o");
    }

    #[test]
    fn test_resolve_model_azure_falls_back() {
        let bridge = OpenCodeBridge::new(
            "my-deployment",
            "https://myresource.azure.com",
            "",
            "openai-compatible",
            600,
            1,
            true,
        );
        assert_eq!(
            bridge.resolve_opencode_model(),
            "anthropic/claude-sonnet-4-6"
        );
    }

    // -- is_azure tests -------------------------------------------------------

    #[test]
    fn test_is_azure_url() {
        let bridge = OpenCodeBridge::new(
            "",
            "https://myresource.openai.azure.com",
            "",
            "openai-compatible",
            600,
            1,
            true,
        );
        assert!(bridge.is_azure());
    }

    #[test]
    fn test_is_azure_provider() {
        let bridge = OpenCodeBridge::new("", "", "", "azure-openai", 600, 1, true);
        assert!(bridge.is_azure());
    }

    #[test]
    fn test_is_azure_false() {
        let bridge = OpenCodeBridge::default();
        assert!(!bridge.is_azure());
    }
}
