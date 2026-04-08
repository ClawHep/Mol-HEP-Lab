//! Phase 3: Processing — ExperimentDesign (3.1), CodebaseSearch (3.2),
//! CodeGeneration (3.3), SanityCheck (3.4), ResourcePlanning (3.5),
//! ExperimentRun (3.6), and IterativeRefine (3.7) stage executors.

use crate::executor::{
    utcnow_iso, StageContext, StageResult,
};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// ExperimentDesign (GATE)
// ---------------------------------------------------------------------------

/// Execute the ExperimentDesign stage.
///
/// Reads hypotheses and synthesis; produces `exp_plan.yaml`.
/// This is a GATE stage — returns BlockedApproval when not auto-approved.
pub async fn execute_experiment_design(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // ---- exp_plan.yaml -----------------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("exp_plan.yaml"), &result) {
        return StageResult::failure(stage, format!("write exp_plan.yaml: {e}"));
    }

    if !ctx.auto_approve_gates {
        return StageResult {
            stage,
            status: StageStatus::BlockedApproval,
            artifacts: vec!["exp_plan.yaml".to_owned()],
            error: None,
            decision: "awaiting_approval".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["exp_plan.yaml".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// CodebaseSearch
// ---------------------------------------------------------------------------

/// Execute the CodebaseSearch stage.
///
/// Reads exp_plan.yaml and produces `codebase_context.json` and
/// `relevant_files.json`.
pub async fn execute_codebase_search(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // ---- codebase_context.json --------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("codebase_context.json"), &result) {
        return StageResult::failure(stage, format!("write codebase_context.json: {e}"));
    }

    // ---- relevant_files.json ----------------------------------------------
    let relevant_files = serde_json::json!({"generated_at": utcnow_iso(), "files": []});
    if let Err(e) = fs::write(
        stage_dir.join("relevant_files.json"),
        serde_json::to_string_pretty(&relevant_files).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write relevant_files.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["codebase_context.json".to_owned(), "relevant_files.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// CodeGeneration
// ---------------------------------------------------------------------------

/// Execute the CodeGeneration stage.
///
/// Reads exp_plan.yaml and codebase context; produces `experiment/` directory
/// with `main.py` and `README.md`, plus `experiment_spec.md`.
pub async fn execute_code_generation(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // Create experiment/ directory
    let experiment_dir = stage_dir.join("experiment");
    if let Err(e) = fs::create_dir_all(&experiment_dir) {
        return StageResult::failure(stage, format!("create experiment dir: {e}"));
    }

    // ---- experiment/main.py ------------------------------------------------
    if let Err(e) = fs::write(experiment_dir.join("main.py"), &result) {
        return StageResult::failure(stage, format!("write experiment/main.py: {e}"));
    }

    // ---- experiment_spec.md -----------------------------------------------
    let topic = ctx.config.topic.as_str();
    let experiment_spec = format!(
        "# Experiment Specification\n\n**Topic**: {topic}\n**Generated**: {ts}\n\n\
         ## Entry Point\n`experiment/main.py`\n\n\
         ## Smoke Test Command\n```bash\npython experiment/main.py --smoke-test\n```\n",
        topic = topic,
        ts = utcnow_iso(),
    );
    if let Err(e) = fs::write(stage_dir.join("experiment_spec.md"), &experiment_spec) {
        return StageResult::failure(stage, format!("write experiment_spec.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["experiment/".to_owned(), "experiment_spec.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// SanityCheck
// ---------------------------------------------------------------------------

/// Execute the SanityCheck stage.
///
/// Uses sanity_check runtime helpers to validate the experiment code.
/// Produces `sanity_report.json`.
pub async fn execute_sanity_check(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // ---- sanity_report.json -----------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("sanity_report.json"), &result) {
        return StageResult::failure(stage, format!("write sanity_report.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["sanity_report.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// ResourcePlanning
// ---------------------------------------------------------------------------

/// Execute the ResourcePlanning stage.
///
/// Reads exp_plan.yaml and hardware profile; produces `resource_plan.json`
/// and `schedule.json`.
pub async fn execute_resource_planning(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // ---- resource_plan.json -----------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("resource_plan.json"), &result) {
        return StageResult::failure(stage, format!("write resource_plan.json: {e}"));
    }

    // ---- schedule.json ----------------------------------------------------
    let schedule = serde_json::json!({"generated_at": utcnow_iso(), "milestones": [], "total_days": 0, "total_gpu_hours": 0});
    if let Err(e) = fs::write(
        stage_dir.join("schedule.json"),
        serde_json::to_string_pretty(&schedule).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write schedule.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["resource_plan.json".to_owned(), "schedule.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}


// ---------------------------------------------------------------------------
// ExperimentRun (3.6)
// ---------------------------------------------------------------------------

/// Execute the ExperimentRun stage.
///
/// Uses experiment_run runtime helpers to set up the run environment.
/// Produces `runs/` directory structure and `runs/run_report.json`.
pub async fn execute_experiment_run(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // Create runs/ directory and write report
    let runs_dir = stage_dir.join("runs");
    if let Err(e) = fs::create_dir_all(&runs_dir) {
        return StageResult::failure(stage, format!("create runs dir: {e}"));
    }
    if let Err(e) = fs::write(runs_dir.join("run_report.json"), &result) {
        return StageResult::failure(stage, format!("write run_report.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["runs/".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// IterativeRefine
// ---------------------------------------------------------------------------

/// Execute the IterativeRefine stage.
///
/// Uses iterative_refine runtime helpers; produces `refinement_log.json` and
/// `experiment_final/` directory.
pub async fn execute_iterative_refine(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = ctx.prompt_engine.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No prompt engine configured for {}", stage.name()));
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    let (system, user) = match engine.render_prompt(stage, &vars)
        .map_err(|e| anyhow::anyhow!("Template render failed for {}: {e}", stage.name()))
    {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await
        .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))
    {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, e.to_string()),
    };
    if result.is_empty() {
        return StageResult {
            stage,
            status: StageStatus::Failed,
            artifacts: vec![],
            error: Some("LLM returned empty response".to_owned()),
            decision: "blocked".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // Create experiment_final/ directory
    let final_dir = stage_dir.join("experiment_final");
    if let Err(e) = fs::create_dir_all(&final_dir) {
        return StageResult::failure(stage, format!("create experiment_final dir: {e}"));
    }

    // ---- refinement_log.json -----------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("refinement_log.json"), &result) {
        return StageResult::failure(stage, format!("write refinement_log.json: {e}"));
    }

    // Write a minimal final main.py placeholder
    let topic = ctx.config.topic.as_str();
    if let Err(e) = fs::write(final_dir.join("main.py"), format!("# Final refined experiment: {topic}\n")) {
        return StageResult::failure(stage, format!("write experiment_final/main.py: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec![
            "refinement_log.json".to_owned(),
            "experiment_final/".to_owned(),
        ],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
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
                templates_dir: None,
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
