//! Codegen orchestration runtime.
//!
//! Ported from `pipeline/codegen/runtime.py`. Orchestrates the multi-phase
//! code generation pipeline:
//!
//! 1. CONTEXT    — Assemble CodegenContext from paths and discovered data
//! 2. LLM_SETUP  — Resolve LLM configuration
//! 3. ROUTING    — Select a strategy (mol_agent or fallback)
//! 4. GENERATE   — Execute the strategy's turn loop
//! 5. FALLBACK   — Use FallbackStrategy if no main.py was produced
//! 6. FINALIZE   — Write experiment/ dir, experiment_spec.md, artifacts list

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde_json::Value;

use crate::session::StageSession;
use crate::turn_loop::LlmConfig;

use super::strategies::{CodegenStrategy, FallbackStrategy, MolAgentStrategy};
use super::types::{
    CodegenContext, CodegenPhase, CodegenResult, DiscoveredData, GeneratedFiles,
};

// ---------------------------------------------------------------------------
// Context discovery
// ---------------------------------------------------------------------------

/// Pre-discover filesystem context before the prompt is built.
///
/// Analogous to claw-code's `ProjectContext.discover_with_git()`.
pub fn discover_data(
    checkpoints_dir: &str,
    datasets_dir: &str,
    codebases_dir: &str,
    session: &mut StageSession,
) -> DiscoveredData {
    let mut d = DiscoveredData::default();

    // ── Checkpoint ───────────────────────────────────────────────────────
    if !checkpoints_dir.is_empty() {
        let ckpt = PathBuf::from(checkpoints_dir);
        if ckpt.is_dir() {
            // Search for model_index.json
            let candidates: Vec<PathBuf> = [ckpt.join("model_index.json")]
                .into_iter()
                .chain(
                    glob::glob(&format!("{}/*/model_index.json", ckpt.display()))
                        .into_iter()
                        .flatten()
                        .flatten(),
                )
                .collect();

            for candidate in candidates {
                if candidate.is_file() {
                    if let Ok(raw) = std::fs::read_to_string(&candidate) {
                        if let Ok(idx) = serde_json::from_str::<HashMap<String, Value>>(&raw) {
                            let class_name = idx
                                .get("_class_name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            session.log(
                                CodegenPhase::Context.as_str(),
                                &format!(
                                    "Discovered model_index.json: _class_name={class_name:?} at {}",
                                    candidate.display()
                                ),
                            );
                            d.checkpoint_model_index = idx;
                            d.checkpoint_class_name = class_name;
                            d.checkpoint_model_index_raw = raw[..raw.len().min(3000)].to_owned();
                            break;
                        }
                    }
                }
            }

            // List top-level files
            d.checkpoint_files = list_dir_names(&ckpt, 30);
        }
    }

    // ── Dataset ──────────────────────────────────────────────────────────
    if !datasets_dir.is_empty() {
        let ds = PathBuf::from(datasets_dir);
        if ds.is_dir() {
            d.dataset_files = list_dir_names(&ds, 30);
            // Read first lines of the first .txt file
            if let Some(txt) = glob::glob(&format!("{}/*.txt", ds.display()))
                .into_iter()
                .flatten()
                .flatten()
                .next()
            {
                if let Ok(content) = std::fs::read_to_string(&txt) {
                    let sample: String = content.lines().take(5).collect::<Vec<_>>().join("\n");
                    d.dataset_sample =
                        format!("Sample from {}:\n{sample}", txt.file_name().unwrap_or_default().to_string_lossy());
                }
            }
        }
    }

    // ── Codebase ─────────────────────────────────────────────────────────
    if !codebases_dir.is_empty() {
        let cb = PathBuf::from(codebases_dir);
        if cb.is_dir() {
            if let Ok(paths) = glob::glob(&format!("{}/**/*.py", cb.display())) {
                d.codebase_files = paths
                    .flatten()
                    .filter(|p| {
                        p.strip_prefix(&cb)
                            .map(|r| {
                                !r.components().any(|c| {
                                    let s = c.as_os_str().to_string_lossy();
                                    s.starts_with('.') || s == "__pycache__"
                                })
                            })
                            .unwrap_or(false)
                    })
                    .take(50)
                    .map(|p| {
                        p.strip_prefix(&cb)
                            .map(|r| r.to_string_lossy().into_owned())
                            .unwrap_or_else(|_| p.to_string_lossy().into_owned())
                    })
                    .collect();
            }
            // README
            for readme in [cb.join("README.md"), cb.join("README.txt")] {
                if readme.is_file() {
                    if let Ok(text) = std::fs::read_to_string(&readme) {
                        d.codebase_readme = text[..text.len().min(2000)].to_owned();
                        break;
                    }
                }
            }
        }
    }

    d
}

fn list_dir_names(dir: &Path, limit: usize) -> Vec<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') { None } else { Some(name) }
        })
        .take(limit)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

// ---------------------------------------------------------------------------
// CodegenRuntime
// ---------------------------------------------------------------------------

/// Orchestrates the full code generation pipeline.
pub struct CodegenRuntime {
    /// Ordered list of strategies to try (first `can_handle` wins).
    strategies: Vec<Box<dyn CodegenStrategy>>,
}

impl Default for CodegenRuntime {
    fn default() -> Self {
        Self {
            strategies: vec![Box::new(MolAgentStrategy::default())],
        }
    }
}

impl CodegenRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the default strategy list.
    pub fn with_strategies(mut self, strategies: Vec<Box<dyn CodegenStrategy>>) -> Self {
        self.strategies = strategies;
        self
    }

    /// Execute the full codegen pipeline.
    ///
    /// Returns the generated files and an updated session.
    pub async fn execute(
        &self,
        ctx: &CodegenContext,
        llm_config: &LlmConfig,
        session: &mut StageSession,
        system_prompt: Option<&str>,
        user_message: Option<&str>,
    ) -> Result<GeneratedFiles> {
        let t0 = Instant::now();
        session.log(CodegenPhase::Context.as_str(), "CodegenRuntime started");

        // ── Phase 3: ROUTING ─────────────────────────────────────────────
        let strategy = self
            .strategies
            .iter()
            .find(|s| s.can_handle(ctx))
            .map(|s| s.as_ref())
            .ok_or_else(|| anyhow::anyhow!("No strategy can handle this context"))?;

        session.log(
            CodegenPhase::Routing.as_str(),
            &format!("Selected strategy: {}", strategy.name()),
        );

        // ── Phase 4: GENERATE ─────────────────────────────────────────────
        session.log(CodegenPhase::Generate.as_str(), &format!("Invoking {}", strategy.name()));
        let result = match strategy.generate(ctx, llm_config, session, system_prompt, user_message).await {
            Ok(r) => r,
            Err(e) => {
                session.log_error(
                    CodegenPhase::Generate.as_str(),
                    &format!("Strategy {} raised", strategy.name()),
                    Some(&e),
                );
                CodegenResult::default()
            }
        };

        session.log(
            CodegenPhase::Generate.as_str(),
            &format!(
                "Strategy returned: {} files, error={:?}",
                result.files.len(),
                if result.error.is_empty() { None } else { Some(&result.error) }
            ),
        );

        // ── Phase 5: FALLBACK ─────────────────────────────────────────────
        let files = if result.has_entrypoint() {
            result.files
        } else {
            session.log(
                CodegenPhase::Fallback.as_str(),
                "No main.py from primary strategy — using FallbackStrategy",
            );
            let fallback = FallbackStrategy::new();
            let fb_result = fallback
                .generate(ctx, llm_config, session, None, None)
                .await?;
            fb_result.files
        };

        // ── Phase 6: FINALIZE ─────────────────────────────────────────────
        let exp_dir = self.write_experiment_dir(ctx, &files, session)?;
        self.write_experiment_spec(ctx, &files, &exp_dir, session)?;

        session.log(
            CodegenPhase::Finalize.as_str(),
            &format!(
                "COMPLETE: {} files, {:.1}s total",
                files.len(),
                t0.elapsed().as_secs_f64(),
            ),
        );

        Ok(files)
    }

    // -----------------------------------------------------------------------
    // Finalize helpers
    // -----------------------------------------------------------------------

    fn write_experiment_dir(
        &self,
        ctx: &CodegenContext,
        files: &GeneratedFiles,
        session: &mut StageSession,
    ) -> Result<PathBuf> {
        let stage_dir = ctx
            .stage_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("stage_dir required for writing experiment/"))?;

        let exp_dir = stage_dir.join("experiment");
        std::fs::create_dir_all(&exp_dir)?;

        for (fname, content) in files {
            let dest = exp_dir.join(fname);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, content)?;
        }

        session.log(
            CodegenPhase::Finalize.as_str(),
            &format!("Wrote {} files to {}", files.len(), exp_dir.display()),
        );
        session.add_artifact("experiment/");

        Ok(exp_dir)
    }

    fn write_experiment_spec(
        &self,
        ctx: &CodegenContext,
        files: &GeneratedFiles,
        _exp_dir: &Path,
        session: &mut StageSession,
    ) -> Result<()> {
        let stage_dir = match &ctx.stage_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        let file_list: String = {
            let mut keys: Vec<_> = files.keys().collect();
            keys.sort();
            keys.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
        };

        let now = chrono::Utc::now().to_rfc3339();
        let spec = format!(
            "# Experiment Specification\n\n\
             ## Topic\n{topic}\n\n\
             ## Project Structure\n\
             Multi-file experiment project with {n} file(s): {file_list}\n\n\
             ## Entry Point\n\
             `main.py` — executed directly via sandbox\n\n\
             ## Outputs\n\
             - `main.py` emits metric lines in `name: value` format\n\
             - Primary metric key: `{metric}`\n\n\
             ## Strategy\n\
             mol-engine agentic turn loop\n\n\
             ## Generated\n\
             {now}\n",
            topic = ctx.topic,
            n = files.len(),
            metric = ctx.metric,
        );

        let spec_path = stage_dir.join("experiment_spec.md");
        std::fs::write(&spec_path, &spec)?;
        session.add_artifact("experiment_spec.md");

        Ok(())
    }
}
