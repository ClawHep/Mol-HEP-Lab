//! Phase 3: Execution — ExperimentDesign (3.1), CodebaseSearch (3.2),
//! CodeDevelop (3.3, ← CodeGeneration + SanityCheck),
//! ExperimentCycle (3.4, ← ResourcePlanning + ExperimentRun + IterativeRefine).

use crate::executor::{StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// ExperimentDesign (GATE)
// ---------------------------------------------------------------------------

/// Execute the ExperimentDesign stage via agentic executor.
///
/// Reads hypotheses and synthesis; produces `exp_plan.md`.
/// This is a GATE stage — returns BlockedApproval when not auto-approved.
pub async fn execute_experiment_design(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "exp_plan.md".into(),
            description: "experiment design plan — methodology, variables, controls, \
                datasets, evaluation metrics, and success criteria".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // GATE: block for approval if not auto-approved
    if !ctx.auto_approve_gates {
        result.status = StageStatus::BlockedApproval;
        result.decision = "awaiting_approval".to_owned();
    }

    result
}

// ---------------------------------------------------------------------------
// CodebaseSearch
// ---------------------------------------------------------------------------

/// Execute the CodebaseSearch stage via agentic executor.
///
/// Reads exp_plan.md and produces `codebase_context.json` and
/// `relevant_files.json`.
pub async fn execute_codebase_search(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let stage_dir = ctx.stage_dir(stage);
    let _ = std::fs::create_dir_all(&stage_dir);

    // If no codebases setting is configured, skip LLM call and write defaults.
    // This prevents the ACP agent from exploring the entire filesystem (16+ min hang).
    let has_codebases = ctx.config.settings.get("codebases_dir")
        .map(|v| !v.is_empty()).unwrap_or(false);
    if !has_codebases {
        tracing::info!("No codebases_dir configured; generating default codebase context");
        let now = chrono::Utc::now().to_rfc3339();
        // Domain-aware defaults: provide standard tools for the configured domain
        let (frameworks, patterns, dependencies) = match ctx.config.domain.as_str() {
            "hep" => (
                vec!["uproot", "awkward-array", "hist", "mplhep"],
                vec!["data_loader", "histogram_fill", "cut_flow", "signal_extraction"],
                vec!["numpy", "scipy", "matplotlib", "pyhf"],
            ),
            "ml" | "ai" => (
                vec!["pytorch", "transformers", "scikit-learn", "wandb"],
                vec!["data_pipeline", "model_train", "evaluation", "hyperparameter_search"],
                vec!["numpy", "pandas", "matplotlib", "torch"],
            ),
            "physics" | "astro" | "cosmology" => (
                vec!["astropy", "healpy", "camb", "emcee"],
                vec!["data_loader", "spectrum_analysis", "model_fit", "visualization"],
                vec!["numpy", "scipy", "matplotlib", "h5py"],
            ),
            _ => (
                vec!["numpy", "scipy", "pandas", "matplotlib"],
                vec!["data_loader", "analysis", "visualization", "report"],
                vec!["numpy", "scipy", "matplotlib", "jupyter"],
            ),
        };
        let context = serde_json::json!({
            "frameworks": frameworks,
            "patterns": patterns,
            "dependencies": dependencies,
            "notes": format!("No existing codebase configured; experiment will start from scratch using standard {} Python tools", ctx.config.domain)
        });
        let files = serde_json::json!({
            "generated_at": now,
            "files": []
        });
        let _ = std::fs::write(stage_dir.join("codebase_context.json"), serde_json::to_string_pretty(&context).unwrap());
        let _ = std::fs::write(stage_dir.join("relevant_files.json"), serde_json::to_string_pretty(&files).unwrap());
        return StageResult {
            stage,
            status: crate::stages::StageStatus::Done,
            artifacts: vec!["codebase_context.json".into(), "relevant_files.json".into()],
            error: None,
            decision: "proceed".into(),
            elapsed_secs: 0.0,
            retry_from_stage: None,
        };
    }

    let specs = vec![
        ArtifactSpec {
            filename: "codebase_context.json".into(),
            description: "codebase analysis context — relevant frameworks, libraries, \
                patterns, and code structures identified for the experiment".into(),
        },
        ArtifactSpec {
            filename: "relevant_files.json".into(),
            description: "relevant files list — files and directories that the experiment \
                code should reference or build upon".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// CodeDevelop (← merged CodeGeneration + SanityCheck)
// ---------------------------------------------------------------------------

/// Execute the CodeDevelop stage — write code, run, check figures, fix, repeat.
///
/// Inner loop: generate code → run → sanity check → fix issues → re-run.
/// Produces experiment code, spec, and sanity report.
pub async fn execute_code_develop(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic, llm_generate, strip_thinking_blocks};

    // Phase 1: Generate code
    let code_specs = vec![
        ArtifactSpec {
            filename: "experiment_spec.md".into(),
            description: "experiment specification — entry point, dependencies, smoke test \
                command, and experiment code overview".into(),
        },
        ArtifactSpec {
            filename: "experiment_code.md".into(),
            description: "complete experiment code — Python script(s) implementing the \
                experiment with data loading, model training, evaluation, and result output. \
                Include all code in fenced code blocks with filenames as headers.".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &code_specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Extract experiment/main.py from generated code
    let stage_dir = ctx.stage_dir(stage);
    let experiment_dir = stage_dir.join("experiment");
    let _ = fs::create_dir_all(&experiment_dir);
    let code_content = fs::read_to_string(stage_dir.join("experiment_code.md")).unwrap_or_default();
    let main_py = crate::executor::strip_markdown_fences(&code_content);
    let _ = fs::write(experiment_dir.join("main.py"), &main_py);
    result.artifacts.push("experiment/".to_owned());

    // Phase 2: Iterative sanity check (run code, check, fix)
    {
        use crate::runtimes::sanity_check::{
            prepare_workspace, list_experiment_files,
            build_system_prompt, build_user_message, check_success,
            check_verdict_file, check_outputs_exist,
            copy_fixes_back, load_plan_summary,
        };

        let workspace = match prepare_workspace(&stage_dir, &experiment_dir, &ctx.run_dir, &ctx.config) {
            Ok(ws) => ws,
            Err(e) => {
                tracing::warn!("Failed to prepare sanity workspace: {e}; skipping iterative check");
                // Fall through to multi-agent review
                let _ = fs::write(stage_dir.join("sanity_summary.md"), format!("Workspace prep failed: {e}"));
                return run_code_develop_review(stage, ctx, result).await;
            }
        };

        let max_iterations = ctx.config.settings.get("sanity_check_max_iterations")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(5);
        let python_path = ctx.config.settings.get("python_path")
            .map(|s| s.as_str())
            .unwrap_or("python3");

        let system_prompt = build_system_prompt(python_path, &workspace);
        let plan_summary = load_plan_summary(&ctx.run_dir);
        let files = list_experiment_files(&workspace);
        let user_prompt = build_user_message(&workspace, &files, &plan_summary);

        let mut iteration = 0u32;
        let mut last_response = String::new();
        let mut passed = false;
        let mut current_user_prompt = user_prompt.clone();

        while iteration < max_iterations {
            iteration += 1;
            tracing::info!(stage = %stage.name(), iteration, max_iterations, "Code develop sanity iteration");

            match llm_generate(ctx, stage, &system_prompt, &current_user_prompt).await {
                Ok(response) => {
                    last_response = response;
                    // Prefer structured verdict file over phrase matching
                    if let Some(verdict) = check_verdict_file(&workspace) {
                        passed = verdict;
                        tracing::info!(stage = %stage.name(), iteration, passed, "Sanity check verdict from JSON");
                        if passed { break; }
                        // Feed verdict back into next iteration so agent knows what failed
                        let verdict_content = std::fs::read_to_string(workspace.join("sanity_verdict.json")).unwrap_or_default();
                        current_user_prompt = format!(
                            "{user_prompt}\n\n## Previous attempt (iteration {iteration})\n\n\
                             The sanity check did NOT pass. Verdict file contents:\n```json\n{verdict_content}\n```\n\n\
                             Fix the issues identified above and try again."
                        );
                    } else {
                        // Fallback: also check output existence as structural signal
                        passed = check_success(&last_response, &[], iteration, max_iterations)
                            || check_outputs_exist(&workspace);
                        if passed {
                            tracing::info!(stage = %stage.name(), iteration, "Sanity check passed (phrase/structural)");
                            break;
                        }
                        // Feed last response back so agent can learn from failure
                        current_user_prompt = format!(
                            "{user_prompt}\n\n## Previous attempt (iteration {iteration})\n\n\
                             The sanity check did NOT pass. Your previous response:\n{last_response}\n\n\
                             Fix the issues and try again."
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(stage = %stage.name(), iteration, "LLM call failed: {e}");
                    break;
                }
            }
        }

        // Copy fixes back
        match copy_fixes_back(&workspace, &experiment_dir) {
            Ok(n) if n > 0 => tracing::info!(stage = %stage.name(), files_copied = n, "Copied fixes back"),
            _ => {}
        }

        let clean_response = strip_thinking_blocks(&last_response);
        let summary = format!(
            "# Sanity Check Summary\n\n- Iterations: {iteration}/{max_iterations}\n- Result: {}\n\n## Last Response\n\n{clean_response}",
            if passed { "PASSED" } else { "NOT PASSED" },
        );
        let _ = fs::write(stage_dir.join("sanity_summary.md"), &summary);
    }

    // Phase 3: Multi-agent review (cross-checker + plot-validator) with rework
    run_code_develop_review(stage, ctx, result).await
}

/// Lightweight review for CodeDevelop: single reviewer (plot-validator) only.
///
/// Phase 1 (code generation) and Phase 2 (sanity check) already validate the code
/// thoroughly. Phase 3 only needs an independent plot-quality review — NOT a full
/// re-generation of the sanity report via another primary agent call.
///
/// Previous design ran `execute_multi_agentic_with_rework()` which spawned a primary
/// agent (to re-generate sanity_report.md ~5min) + a reviewer (~5min) — effectively
/// doubling the LLM calls and adding 10+ minutes. Now we run only the reviewer via
/// a single `execute_agentic()` call.
async fn run_code_develop_review(stage: Stage, ctx: &StageContext, base_result: StageResult) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let reviewer_specs = vec![
        ArtifactSpec {
            filename: "plot_validation.md".into(),
            description: "plot validation report — programmatic checks on plotting code, \
                physics sanity of distributions, and figure quality assessment. \
                Review the existing experiment code and figures in this stage directory.".into(),
        },
    ];

    tracing::info!(stage = %stage.name(), "Running plot-validator review (lightweight)");

    // Reset session before reviewer
    if let Some(provider) = ctx.llm.as_ref() {
        let _ = provider.reset_session().await;
    }

    let review_result = execute_agentic(stage, ctx, &reviewer_specs).await;

    // Merge artifacts from code generation phase + review
    let mut all_artifacts = base_result.artifacts;
    all_artifacts.extend(review_result.artifacts);
    StageResult {
        artifacts: all_artifacts,
        ..review_result
    }
}

// ---------------------------------------------------------------------------
// ExperimentCycle (← merged ResourcePlanning + ExperimentRun + IterativeRefine)
// ---------------------------------------------------------------------------

/// Execute the ExperimentCycle stage — plan resources, run, iterate until convergence.
///
/// Single agentic call: the agent decides its own workflow (plan → run → iterate).
/// Declared artifacts are the expected outputs; the agent may produce additional files
/// (extra figures, intermediate results, etc.) which are auto-discovered.
pub async fn execute_experiment_cycle(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "resource_plan.md".into(),
            description: "resource plan — compute requirements, memory needs, storage, \
                GPU hours, and cost estimates for the experiment".into(),
        },
        ArtifactSpec {
            filename: "run_report.md".into(),
            description: "experiment run report — execution results including metrics, \
                hyperparameters used, training logs summary, and any errors encountered".into(),
        },
        ArtifactSpec {
            filename: "refinement_log.md".into(),
            description: "iterative refinement log — changes made, metrics before/after, \
                convergence status, and remaining issues".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;

    // Post-processing: create runs/ and experiment_final/ from agent outputs
    let stage_dir = ctx.stage_dir(stage);

    let runs_dir = stage_dir.join("runs");
    let _ = fs::create_dir_all(&runs_dir);
    if let Ok(report) = fs::read_to_string(stage_dir.join("run_report.md")) {
        let _ = fs::write(runs_dir.join("run_report.md"), &report);
    }

    let final_dir = stage_dir.join("experiment_final");
    let _ = fs::create_dir_all(&final_dir);
    // Look for any .py files the agent wrote and copy to experiment_final/
    if let Ok(entries) = fs::read_dir(&stage_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".py") {
                let _ = fs::copy(entry.path(), final_dir.join(&name));
            }
        }
    }

    // Ensure standard directory artifacts are registered
    if runs_dir.is_dir() && !result.artifacts.contains(&"runs/".to_owned()) {
        result.artifacts.push("runs/".to_owned());
    }
    if final_dir.is_dir() && !result.artifacts.contains(&"experiment_final/".to_owned()) {
        result.artifacts.push("experiment_final/".to_owned());
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::MolConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_ctx(dir: &std::path::Path, topic: &str, auto_approve: bool) -> StageContext {
        StageContext {
            run_dir: dir.to_owned(),
            run_id: "test".to_owned(),
            config: MolConfig {
                topic: topic.to_owned(),
                settings: HashMap::new(),
                domain: "hep".to_owned(),
                analysis_type: None,
                knowledge_chain: mol_common::KnowledgeChain::new(vec![std::path::PathBuf::from("hep"), std::path::PathBuf::from("generic")]),
                datasets_dir: String::new(),
            },
            prior_artifacts: HashMap::new(),
            auto_approve_gates: auto_approve,
            llm: None,
            prompt_engine: None,
        }
    }

    #[tokio::test]
    async fn experiment_design_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "graph neural networks", false);
        let result = execute_experiment_design(Stage::ExperimentDesign, &ctx).await;
        // Without a prompt engine configured, the stage must fail honestly.
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn code_develop_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "contrastive learning", true);
        let result = execute_code_develop(Stage::CodeDevelop, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn experiment_cycle_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "diffusion models", true);
        let result = execute_experiment_cycle(Stage::ExperimentCycle, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
