//! Codegen strategy trait and built-in implementations.
//!
//! Ported from `pipeline/codegen/strategies/claw_agent.py` and
//! `pipeline/codegen/strategies/fallback.py`.
//!
//! Strategies are selected by the [`super::runtime::CodegenRuntime`] based on
//! the [`super::types::CodegenContext`]. The `MolAgentStrategy` is the primary
//! path; `FallbackStrategy` produces a minimal numpy stub when the agent fails.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use crate::session::StageSession;
use crate::turn_loop::{AgentTurnLoop, LlmConfig};

use super::types::{CodegenContext, CodegenPhase, CodegenResult};

// ---------------------------------------------------------------------------
// CodegenStrategy trait
// ---------------------------------------------------------------------------

/// A code generation strategy.
///
/// All strategies share the same interface so the runtime can select and
/// fall back between them transparently.
#[async_trait]
pub trait CodegenStrategy: Send + Sync {
    /// Short identifier for this strategy (used in logs and artifacts).
    fn name(&self) -> &str;

    /// Returns true if this strategy can handle the given context.
    fn can_handle(&self, ctx: &CodegenContext) -> bool;

    /// Execute the strategy and return generated files.
    async fn generate(
        &self,
        ctx: &CodegenContext,
        llm_config: &LlmConfig,
        session: &mut StageSession,
        system_prompt: Option<&str>,
        user_message: Option<&str>,
    ) -> Result<CodegenResult>;
}

// ---------------------------------------------------------------------------
// MolAgentStrategy (was ClawAgentStrategy)
// ---------------------------------------------------------------------------

/// Primary agentic code generation strategy.
///
/// The LLM uses the tool loop to explore codebases, write experiment files,
/// run them, and fix errors — iterating until the code works or max iterations
/// are reached.
pub struct MolAgentStrategy {
    /// Maximum bash timeout (seconds).
    pub bash_timeout_sec: u64,
    /// Maximum turn loop iterations.
    pub max_iterations: usize,
}

impl Default for MolAgentStrategy {
    fn default() -> Self {
        Self { bash_timeout_sec: 120, max_iterations: 40 }
    }
}

impl MolAgentStrategy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bash_timeout(mut self, secs: u64) -> Self {
        self.bash_timeout_sec = secs;
        self
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Prepare a clean workspace directory.
    fn prepare_workspace(ctx: &CodegenContext, session: &mut StageSession) -> Result<PathBuf> {
        let stage_dir = ctx
            .stage_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("CodegenContext.stage_dir is required for MolAgentStrategy"))?;

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ws = stage_dir.join(format!("mol_workspace_{ts}_{}", std::process::id()));
        std::fs::create_dir_all(&ws)?;

        // Write experiment plan to workspace root
        if !ctx.exp_plan.is_empty() {
            std::fs::write(ws.join("EXPERIMENT_PLAN.yaml"), &ctx.exp_plan)?;
        }

        // Write CODEGEN.md if available
        if let Some(run_dir) = &ctx.run_dir {
            let user_codegen = run_dir.join("CODEGEN.md");
            if user_codegen.is_file() {
                let content = std::fs::read_to_string(&user_codegen)?;
                std::fs::write(ws.join("CODEGEN.md"), &content)?;
                session.log(
                    CodegenPhase::Generate.as_str(),
                    &format!("Using user-provided CODEGEN.md ({} chars)", content.len()),
                );
            }
        }

        Ok(ws)
    }

    /// Build allowed read directories from context paths.
    fn allowed_read_dirs(ctx: &CodegenContext) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        for raw in [&ctx.datasets_dir, &ctx.checkpoints_dir, &ctx.codebases_dir] {
            if !raw.is_empty() {
                let p = PathBuf::from(raw);
                if p.is_dir() {
                    dirs.push(p);
                }
            }
        }
        dirs
    }
}

#[async_trait]
impl CodegenStrategy for MolAgentStrategy {
    fn name(&self) -> &str {
        "mol_agent"
    }

    fn can_handle(&self, _ctx: &CodegenContext) -> bool {
        true
    }

    async fn generate(
        &self,
        ctx: &CodegenContext,
        llm_config: &LlmConfig,
        session: &mut StageSession,
        system_prompt: Option<&str>,
        user_message: Option<&str>,
    ) -> Result<CodegenResult> {
        session.log(CodegenPhase::Generate.as_str(), "MolAgentStrategy started");
        let t0 = Instant::now();

        let workspace = Self::prepare_workspace(ctx, session)?;
        session.log(
            CodegenPhase::Generate.as_str(),
            &format!("Workspace: {}", workspace.display()),
        );

        let system_prompt = system_prompt
            .unwrap_or("You are MolAgent, an expert scientific code generation assistant. \
                Write clean, correct, executable experiment code based on the provided plan.")
            .to_owned();

        let user_message = user_message
            .unwrap_or("Please generate the experiment code according to the plan.")
            .to_owned();

        session.log(
            CodegenPhase::Generate.as_str(),
            &format!("System prompt: {} chars", system_prompt.len()),
        );

        let allowed_reads = Self::allowed_read_dirs(ctx);
        let mut policy = crate::tools::permissions::PermissionPolicy::new(workspace.clone());
        for dir in allowed_reads {
            policy = policy.allow_read_dir(dir);
        }

        let mut loop_ = AgentTurnLoop::new(llm_config.clone(), workspace.clone(), system_prompt)
            .with_bash_timeout(self.bash_timeout_sec)
            .with_max_iterations(self.max_iterations)
            .with_policy(policy)
            .with_trace("generation");

        session.log(CodegenPhase::Generate.as_str(), "Starting turn loop...");
        let turn_result = loop_.run(&user_message).await?;

        let elapsed = t0.elapsed().as_secs_f64();
        session.llm_calls += turn_result.tool_calls;

        // Save agent log
        if let Some(stage_dir) = &ctx.stage_dir {
            let mut files_produced: Vec<&String> = turn_result.artifacts_produced.keys().collect();
            files_produced.sort();
            let log = serde_json::json!({
                "success": turn_result.artifacts_produced.contains_key("main.py"),
                "iterations": turn_result.iterations,
                "tool_calls": turn_result.tool_calls,
                "files_produced": files_produced,
                "errors": turn_result.errors,
                "elapsed_sec": (elapsed * 10.0).round() / 10.0,
                "final_text_length": turn_result.response.len(),
            });
            let _ = std::fs::write(
                stage_dir.join("mol_agent_log.json"),
                serde_json::to_string_pretty(&log)?,
            );
            session.add_artifact("mol_agent_log.json");
        }

        if turn_result.artifacts_produced.contains_key("main.py") {
            session.log(
                CodegenPhase::Generate.as_str(),
                &format!(
                    "MolAgent SUCCESS — {} files, {} iterations, {} tool calls, {elapsed:.1}s",
                    turn_result.artifacts_produced.len(),
                    turn_result.iterations,
                    turn_result.tool_calls,
                ),
            );
            Ok(CodegenResult {
                files: turn_result.artifacts_produced,
                strategy_name: self.name().to_owned(),
                elapsed_sec: elapsed,
                ..Default::default()
            })
        } else {
            session.log(
                CodegenPhase::Generate.as_str(),
                &format!(
                    "MolAgent produced no main.py — {} files: {:?}",
                    turn_result.artifacts_produced.len(),
                    {
                        let mut keys: Vec<_> = turn_result.artifacts_produced.keys().collect();
                        keys.sort();
                        keys
                    }
                ),
            );
            Ok(CodegenResult {
                files: turn_result.artifacts_produced,
                strategy_name: self.name().to_owned(),
                elapsed_sec: elapsed,
                error: "No main.py produced".to_owned(),
                ..Default::default()
            })
        }
    }
}

// ---------------------------------------------------------------------------
// FallbackStrategy
// ---------------------------------------------------------------------------

/// Minimal fallback strategy that produces a stub `main.py` when the agent fails.
///
/// The stub is a valid Python script that prints a metric line so the
/// experiment runner does not crash.
pub struct FallbackStrategy;

impl FallbackStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FallbackStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CodegenStrategy for FallbackStrategy {
    fn name(&self) -> &str {
        "fallback"
    }

    fn can_handle(&self, _ctx: &CodegenContext) -> bool {
        true
    }

    async fn generate(
        &self,
        ctx: &CodegenContext,
        _llm_config: &LlmConfig,
        session: &mut StageSession,
        _system_prompt: Option<&str>,
        _user_message: Option<&str>,
    ) -> Result<CodegenResult> {
        session.log(
            CodegenPhase::Fallback.as_str(),
            "FallbackStrategy: generating stub main.py",
        );

        let metric = if ctx.metric.is_empty() { "result" } else { &ctx.metric };
        let topic_comment = if ctx.topic.is_empty() {
            String::new()
        } else {
            format!("# Topic: {}\n", &ctx.topic[..ctx.topic.len().min(120)])
        };

        let stub = format!(
            r#"#!/usr/bin/env python3
"""Fallback stub generated by mol-engine FallbackStrategy.

The primary code generation strategy did not produce a main.py.
This stub ensures the experiment runner can complete without crashing.
"""
{topic_comment}
import os
import json

SMOKE_TEST = os.environ.get("SMOKE_TEST", "0") == "1"


def main():
    print("mol-engine fallback stub: primary code generation failed.")
    # Emit a placeholder metric so downstream stages do not crash.
    print(f"{metric}: 0.0")
    # Write an empty results JSON for artifact collection.
    results = {{"error": "fallback_stub", "{metric}": 0.0, "skipped_reason": "codegen_failed"}}
    with open("results.json", "w") as f:
        json.dump(results, f, indent=2)
    print("results.json written.")


if __name__ == "__main__":
    main()
"#
        );

        let mut files = std::collections::HashMap::new();
        files.insert("main.py".to_owned(), stub);

        Ok(CodegenResult {
            files,
            strategy_name: self.name().to_owned(),
            skip_review: true,
            ..Default::default()
        })
    }
}
