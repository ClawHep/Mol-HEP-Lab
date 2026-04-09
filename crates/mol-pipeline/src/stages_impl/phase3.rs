//! Phase 3: Processing — ExperimentDesign (3.1), CodebaseSearch (3.2),
//! CodeGeneration (3.3), SanityCheck (3.4), ResourcePlanning (3.5),
//! ExperimentRun (3.6), and IterativeRefine (3.7) stage executors.

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
        let context = serde_json::json!({
            "frameworks": ["uproot", "awkward-array", "hist", "mplhep"],
            "patterns": ["data_loader", "histogram_fill", "cut_flow", "signal_extraction"],
            "dependencies": ["numpy", "scipy", "matplotlib", "pyhf"],
            "notes": "No existing codebase configured; experiment will start from scratch using standard HEP Python tools"
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
// CodeGeneration
// ---------------------------------------------------------------------------

/// Execute the CodeGeneration stage via agentic executor.
///
/// Reads exp_plan.md and codebase context; produces `experiment_spec.md`
/// (experiment code + specification) and `experiment/main.py`.
pub async fn execute_code_generation(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
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

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Create experiment/ directory and extract main.py from experiment_code.md
    let stage_dir = ctx.stage_dir(stage);
    let experiment_dir = stage_dir.join("experiment");
    if let Err(e) = fs::create_dir_all(&experiment_dir) {
        return StageResult::failure(stage, format!("create experiment dir: {e}"));
    }

    // Read the generated code artifact and write as main.py
    let code_content = fs::read_to_string(stage_dir.join("experiment_code.md"))
        .unwrap_or_default();
    // Extract the largest code block, or use as-is
    let main_py = crate::executor::strip_markdown_fences(&code_content);
    if let Err(e) = fs::write(experiment_dir.join("main.py"), &main_py) {
        return StageResult::failure(stage, format!("write experiment/main.py: {e}"));
    }
    result.artifacts.push("experiment/".to_owned());

    result
}

// ---------------------------------------------------------------------------
// SanityCheck
// ---------------------------------------------------------------------------

/// Execute the SanityCheck stage with iterative code-fix loop + multi-agent review.
///
/// 1. Find experiment code from prior stages
/// 2. Prepare isolated workspace
/// 3. LLM agent runs code, checks output, fixes errors (iterative loop)
/// 4. Copy fixes back to experiment directory
/// 5. Multi-agent review: cross-checker + plot-validator
pub async fn execute_sanity_check(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::llm_generate;
    use crate::runtimes::sanity_check::{
        find_experiment_dir, prepare_workspace, list_experiment_files,
        build_system_prompt, build_user_message, check_success,
        copy_fixes_back, load_plan_summary,
    };

    let stage_dir = ctx.stage_dir(stage);
    let _ = fs::create_dir_all(&stage_dir);

    // Step 1: Find experiment code
    let experiment_dir = match find_experiment_dir(&ctx.run_dir) {
        Some(d) => d,
        None => {
            tracing::warn!("No experiment directory found; skipping iterative sanity check");
            // Fall through to multi-agent review only
            return run_sanity_review(stage, ctx).await;
        }
    };

    // Step 2: Prepare workspace
    let workspace = match prepare_workspace(&stage_dir, &experiment_dir, &ctx.run_dir, &ctx.config) {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!("Failed to prepare sanity workspace: {e}; falling back to review-only");
            return run_sanity_review(stage, ctx).await;
        }
    };

    // Step 3: Iterative fix loop
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

    while iteration < max_iterations {
        iteration += 1;
        tracing::info!(
            stage = %stage.name(),
            iteration,
            max_iterations,
            "Sanity check iteration"
        );

        match llm_generate(ctx, stage, &system_prompt, &user_prompt).await {
            Ok(response) => {
                last_response = response;
                passed = check_success(&last_response, &[], iteration, max_iterations);
                if passed {
                    tracing::info!(stage = %stage.name(), iteration, "Sanity check passed");
                    break;
                }
                tracing::info!(
                    stage = %stage.name(),
                    iteration,
                    "Sanity check not yet passed, continuing..."
                );
            }
            Err(e) => {
                tracing::warn!(stage = %stage.name(), iteration, "LLM call failed: {e}");
                break;
            }
        }
    }

    // Step 4: Copy fixes back
    match copy_fixes_back(&workspace, &experiment_dir) {
        Ok(n) if n > 0 => {
            tracing::info!(stage = %stage.name(), files_copied = n, "Copied fixes back to experiment dir");
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!("Failed to copy fixes back: {e}");
        }
    }

    // Write iterative check summary
    let summary = format!(
        "# Sanity Check Summary\n\n\
         - Iterations: {iteration}/{max_iterations}\n\
         - Result: {}\n\
         - Workspace: {}\n\n\
         ## Agent Response (last iteration)\n\n{last_response}",
        if passed { "PASSED" } else { "NOT PASSED" },
        workspace.display(),
    );
    let _ = fs::write(stage_dir.join("sanity_summary.md"), &summary);

    // Step 5: Multi-agent review (cross-checker + plot-validator)
    run_sanity_review(stage, ctx).await
}

/// Run the multi-agent review portion of sanity check.
async fn run_sanity_review(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_multi_agentic_with_rework};

    let primary_specs = vec![
        ArtifactSpec {
            filename: "sanity_report.md".into(),
            description: "sanity check report — code review findings, potential issues, \
                dependency checks, and overall pass/fail status".into(),
        },
    ];

    let reviewer_specs: Vec<(&str, Vec<ArtifactSpec>)> = vec![
        ("plot-validator", vec![
            ArtifactSpec {
                filename: "plot_validation.md".into(),
                description: "plot validation report — programmatic checks on plotting code, \
                    physics sanity of distributions, and figure quality assessment".into(),
            },
        ]),
    ];

    execute_multi_agentic_with_rework(stage, ctx, &primary_specs, &reviewer_specs, 2).await
}

// ---------------------------------------------------------------------------
// ResourcePlanning
// ---------------------------------------------------------------------------

/// Execute the ResourcePlanning stage via agentic executor.
///
/// Reads exp_plan.md and hardware profile; produces `resource_plan.md`
/// and `schedule.json`.
pub async fn execute_resource_planning(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "resource_plan.md".into(),
            description: "resource plan — compute requirements, memory needs, storage, \
                GPU hours, and cost estimates for the experiment".into(),
        },
        ArtifactSpec {
            filename: "schedule.json".into(),
            description: "experiment schedule — milestones with dates, dependencies, \
                and time estimates".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}


// ---------------------------------------------------------------------------
// ExperimentRun (3.6)
// ---------------------------------------------------------------------------

/// Execute the ExperimentRun stage via agentic executor.
///
/// Produces `run_report.md` with experiment execution results and logs.
pub async fn execute_experiment_run(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "run_report.md".into(),
            description: "experiment run report — execution results including metrics, \
                hyperparameters used, training logs summary, and any errors encountered".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Create runs/ directory structure and move report there
    let stage_dir = ctx.stage_dir(stage);
    let runs_dir = stage_dir.join("runs");
    if let Err(e) = fs::create_dir_all(&runs_dir) {
        return StageResult::failure(stage, format!("create runs dir: {e}"));
    }
    // Copy run_report.md into runs/ as well
    let report_content = fs::read_to_string(stage_dir.join("run_report.md")).unwrap_or_default();
    let _ = fs::write(runs_dir.join("run_report.md"), &report_content);
    result.artifacts.push("runs/".to_owned());

    result
}

// ---------------------------------------------------------------------------
// IterativeRefine
// ---------------------------------------------------------------------------

/// Execute the IterativeRefine stage via agentic executor.
///
/// Produces `refinement_log.md` and `refined_code.md` with improved experiment code.
pub async fn execute_iterative_refine(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "refinement_log.md".into(),
            description: "iterative refinement log — changes made, metrics before/after, \
                convergence status, and remaining issues".into(),
        },
        ArtifactSpec {
            filename: "refined_code.md".into(),
            description: "refined experiment code — improved Python code with all refinements \
                applied, in fenced code blocks".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Create experiment_final/ directory with refined code
    let stage_dir = ctx.stage_dir(stage);
    let final_dir = stage_dir.join("experiment_final");
    if let Err(e) = fs::create_dir_all(&final_dir) {
        return StageResult::failure(stage, format!("create experiment_final dir: {e}"));
    }
    let refined = fs::read_to_string(stage_dir.join("refined_code.md")).unwrap_or_default();
    let main_py = crate::executor::strip_markdown_fences(&refined);
    let _ = fs::write(final_dir.join("main.py"), &main_py);
    result.artifacts.push("experiment_final/".to_owned());

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
    async fn code_generation_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "contrastive learning", true);
        let result = execute_code_generation(Stage::CodeGeneration, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn sanity_check_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "attention mechanisms", true);
        let result = execute_sanity_check(Stage::SanityCheck, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn resource_planning_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "diffusion models", true);
        let result = execute_resource_planning(Stage::ResourcePlanning, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn experiment_run_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "knowledge distillation", true);
        let result = execute_experiment_run(Stage::ExperimentRun, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn iterative_refine_fails_without_prompt_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "meta-learning", true);
        let result = execute_iterative_refine(Stage::IterativeRefine, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
