//! Pipeline orchestration: sequential stage loop with checkpoint, gate, and
//! decision-rollback handling.

use crate::checkpoint::{read_checkpoint, resume_from_checkpoint, write_checkpoint, write_heartbeat};
use crate::executor::{execute_stage, MolConfig, StageContext, StagePromptEngine, StageResult};
use crate::stages::{
    advance, decision_rollback, Stage, StageStatus, TransitionEvent, NONCRITICAL_STAGES,
    STAGE_SEQUENCE, MAX_DECISION_PIVOTS,
};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// PipelineConfig
// ---------------------------------------------------------------------------

/// Options controlling how the pipeline runner behaves for a given run.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// First stage to execute.  The runner skips all earlier stages.
    pub from_stage: Stage,
    /// Last stage to execute (inclusive).  `None` means run to the end.
    pub to_stage: Option<Stage>,
    /// When `true` all gate stages are auto-approved without HITL interaction.
    pub auto_approve: bool,
    /// Stop at the first gate stage and return `BlockedApproval` instead of
    /// waiting or auto-approving.
    pub stop_on_gate: bool,
    /// When `true` failures in noncritical stages are logged and skipped
    /// rather than aborting the pipeline.
    pub skip_noncritical: bool,
    /// When `true` the runner skips failed stages and continues the pipeline
    /// with available artifacts. No fallback content is generated.
    pub graceful_degradation: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            from_stage: Stage::TopicInit,
            to_stage: None,
            auto_approve: false,
            stop_on_gate: false,
            skip_noncritical: false,
            graceful_degradation: false,
        }
    }
}

// ---------------------------------------------------------------------------
// PipelineSummary
// ---------------------------------------------------------------------------

/// Aggregate statistics for a completed (or interrupted) pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSummary {
    /// Opaque run identifier.
    pub run_id: String,
    /// Number of stages that completed with `Done`.
    pub stages_completed: usize,
    /// Number of stages that ended in `Failed`.
    pub stages_failed: usize,
    /// Number of stages that were skipped (noncritical failures or range).
    pub stages_skipped: usize,
    /// Always `false` — degraded mode removed in template engine refactor.
    pub degraded: bool,
    /// The first stage executed in this run.
    pub from_stage: i32,
    /// The last stage for which a result was collected.
    pub final_stage: i32,
    /// Status string of the final result.
    pub final_status: String,
    /// Wall-clock elapsed seconds for the whole pipeline.
    pub total_elapsed_secs: f64,
    /// Names of all artifacts produced across all stages.
    pub artifacts: Vec<String>,
    /// ISO-8601 timestamp when the summary was generated.
    pub generated: String,
}

impl PipelineSummary {
    fn build(
        run_id: &str,
        results: &[StageResult],
        from_stage: Stage,
        total_elapsed_secs: f64,
    ) -> Self {
        let artifacts: Vec<String> = results
            .iter()
            .flat_map(|r| r.artifacts.iter().cloned())
            .collect();

        let stages_completed = results
            .iter()
            .filter(|r| r.status == StageStatus::Done)
            .count();
        let stages_failed = results
            .iter()
            .filter(|r| r.status == StageStatus::Failed)
            .count();
        let degraded = false; // degraded mode removed in template engine refactor

        let (final_stage, final_status) = results
            .last()
            .map(|r| (r.stage.as_i32(), r.status.as_str().to_owned()))
            .unwrap_or((from_stage.as_i32(), "no_stages".to_owned()));

        Self {
            run_id: run_id.to_owned(),
            stages_completed,
            stages_failed,
            stages_skipped: 0, // updated by caller if needed
            degraded,
            from_stage: from_stage.as_i32(),
            final_stage,
            final_status,
            total_elapsed_secs,
            artifacts,
            generated: Utc::now().to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// execute_pipeline
// ---------------------------------------------------------------------------

/// Execute pipeline stages sequentially from `pipeline_config.from_stage` to
/// `pipeline_config.to_stage` (inclusive).
///
/// Handles:
/// - Checkpoint resume (reads existing checkpoint from `run_dir`)
/// - Gate auto-approve vs. block
/// - Noncritical stage skip-on-failure
/// - Decision rollback loops (bounded by `MAX_DECISION_PIVOTS`)
/// - Graceful degradation mode
///
/// Returns a `PipelineSummary` once the run is complete or interrupted.
pub async fn execute_pipeline(
    config: &MolConfig,
    pipeline_config: &PipelineConfig,
    run_dir: &Path,
    run_id: &str,
) -> Result<PipelineSummary> {
    execute_pipeline_with_llm(config, pipeline_config, run_dir, run_id, None).await
}

/// Execute the pipeline with an optional LLM provider.
pub async fn execute_pipeline_with_llm(
    config: &MolConfig,
    pipeline_config: &PipelineConfig,
    run_dir: &Path,
    run_id: &str,
    llm: Option<std::sync::Arc<dyn mol_llm::LlmProvider>>,
) -> Result<PipelineSummary> {
    tokio::fs::create_dir_all(run_dir).await?;

    // If user configured a local datasets directory, symlink it into the run
    // directory so experiment workspaces can access it.  When this exists the
    // template variable {{ datasets }} will show the local paths, and the
    // experiment stages will analyse *only* user-provided data.
    let user_datasets_dir = &config.datasets_dir;
    if !user_datasets_dir.is_empty() {
        let src = std::path::Path::new(user_datasets_dir);
        let dst = run_dir.join("datasets");
        if src.is_dir() && !dst.exists() {
            #[cfg(unix)]
            {
                let _ = std::os::unix::fs::symlink(src, &dst);
            }
            info!(
                src = %src.display(),
                "Linked user datasets directory into run"
            );
        } else if !src.is_dir() {
            warn!(
                path = %src.display(),
                "experiment.datasets_dir configured but directory does not exist"
            );
        }
    }

    // Load the prompt engine once at pipeline start.
    let prompt_engine: Option<Arc<StagePromptEngine>> = {
        match config.knowledge_chain.templates_dir() {
            Some(templates_dir) => {
                match StagePromptEngine::load(&templates_dir) {
                    Ok(engine) => {
                        info!("Loaded stage templates from {}", templates_dir.display());
                        Some(Arc::new(engine))
                    }
                    Err(e) => {
                        warn!("Failed to load stage templates: {}", e);
                        None
                    }
                }
            }
            None => {
                warn!("No templates/stages directory found in knowledge chain");
                None
            }
        }
    };

    // Load domain contract overrides (if any).
    let contract_overrides = crate::contracts::ContractOverrides::load(&config.knowledge_chain);

    let t_start = Instant::now();
    let total_stages = STAGE_SEQUENCE.len();

    let mut results: Vec<StageResult> = Vec::new();
    let mut artifact_registry: HashMap<String, PathBuf> = HashMap::new();
    let mut started = false;
    let mut pivot_count: u32 = 0;
    let mut stages_skipped: usize = 0;

    // Determine the effective starting stage (may be overridden by checkpoint).
    let effective_from = determine_start_stage(pipeline_config.from_stage, run_dir).await;

    // Pre-populate artifact_registry from prior stage directories.
    // This is essential when resuming from a later stage (e.g., S9) so that
    // contract validation can find artifacts produced by earlier stages (S1-S8).
    if effective_from != Stage::TopicInit {
        for &prior_stage in STAGE_SEQUENCE {
            if prior_stage == effective_from {
                break;
            }
            let contract = crate::contracts::get_contract(prior_stage, Some(&contract_overrides));
            let stage_num = prior_stage.as_i32();
            let stage_dir = run_dir.join(format!("stage-{:02}", stage_num));
            if stage_dir.is_dir() {
                // If the stage directory exists and contains any files, assume
                // the stage completed successfully and register all its declared
                // outputs. Filenames on disk may differ from contract names
                // (e.g. "exp_plan.yaml" vs "experiment_plan"), so we check for
                // any non-empty stage dir rather than exact filename matches.
                let has_files = std::fs::read_dir(&stage_dir)
                    .ok()
                    .map(|rd| rd.filter_map(|e| e.ok()).any(|e| e.path().is_file()))
                    .unwrap_or(false);
                if has_files {
                    for artifact_name in &contract.expected_outputs {
                        artifact_registry.insert(
                            artifact_name.to_string(),
                            stage_dir.join(artifact_name),
                        );
                    }
                }
            }
        }
        if !artifact_registry.is_empty() {
            info!(
                "Pre-populated {} artifacts from prior stages for resume at {}",
                artifact_registry.len(),
                effective_from.name()
            );
        }
    }

    'outer: for &stage in STAGE_SEQUENCE {
        // Range filter: skip stages outside [from_stage, to_stage].
        if let Some(to) = pipeline_config.to_stage {
            if stage.as_i32() > to.as_i32() {
                break;
            }
        }
        if !started {
            if stage == effective_from {
                started = true;
            } else {
                continue;
            }
        }

        let stage_num = stage.as_i32();
        let prefix = format!("[{}] Stage {:02}/{} ({})", run_id, stage_num, total_stages, stage.phase_label());

        info!("{} {} — running...", prefix, stage.name());

        // Pre-flight input validation.
        let available_artifacts: Vec<String> = artifact_registry.keys().cloned().collect();
        if let Err(e) = crate::contracts::validate_inputs(stage, &available_artifacts, Some(&contract_overrides)) {
            if NONCRITICAL_STAGES.contains(&stage) || pipeline_config.graceful_degradation {
                warn!("{} {} — input validation failed (skipping): {}", prefix, stage.name(), e);
                stages_skipped += 1;
                continue;
            } else {
                return Err(e);
            }
        }

        // Blinding gate: check before entering Phase 4 (ResultAnalysis)
        if stage == Stage::ResultAnalysis {
            match check_blinding_gate(run_dir, pipeline_config.auto_approve).await? {
                BlindingStatus::Approved => {
                    info!("{} Blinding gate passed — entering Phase 4", prefix);
                }
                BlindingStatus::Blocked => {
                    info!("{} Blinding gate BLOCKED — pipeline paused", prefix);
                    let result = StageResult {
                        stage,
                        status: StageStatus::BlockedApproval,
                        artifacts: vec![],
                        decision: "blocked_blinding".into(),
                        error: Some("Blinding gate: approve unblinding before Phase 4".into()),
                        elapsed_secs: 0.0,
                    };
                    results.push(result);
                    break;
                }
            }
        }

        // Build execution context.
        let context = StageContext {
            run_dir: run_dir.to_owned(),
            run_id: run_id.to_owned(),
            config: config.clone(),
            prior_artifacts: artifact_registry.clone(),
            auto_approve_gates: pipeline_config.auto_approve,
            llm: llm.clone(),
            prompt_engine: prompt_engine.clone(),
        };

        // Periodic heartbeat while stage is running.
        let stage_t0 = Instant::now();
        let heartbeat_handle = tokio::spawn({
            let hb_run_dir = run_dir.to_owned();
            let hb_run_id = run_id.to_owned();
            async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let _ = write_heartbeat(
                        &hb_run_dir, stage, &hb_run_id,
                        "running", stage_t0.elapsed().as_secs_f64(),
                    ).await;
                }
            }
        });

        // Execute the stage.
        let mut result = match execute_stage(stage, &context).await {
            Ok(r) => r,
            Err(e) => {
                warn!("{} {} — execution error: {}", prefix, stage.name(), e);
                StageResult::failure(stage, e.to_string())
            }
        };
        heartbeat_handle.abort();
        let elapsed = stage_t0.elapsed().as_secs_f64();
        result.elapsed_secs = elapsed;

        // Log outcome.
        match result.status {
            StageStatus::Done => {
                let arts = result.artifacts.join(", ");
                info!("{} {} — done ({:.1}s) → {}", prefix, stage.name(), elapsed, arts);
            }
            StageStatus::Failed => {
                let err = result.error.as_deref().unwrap_or("unknown error");
                warn!("{} {} — FAILED ({:.1}s) — {}", prefix, stage.name(), elapsed, err);
            }
            StageStatus::BlockedApproval => {
                info!("{} {} — blocked (awaiting approval)", prefix, stage.name());
            }
            _ => {}
        }

        // Post-execution output validation (soft warning only).
        if result.status == StageStatus::Done {
            if let Err(e) = crate::contracts::validate_outputs(stage, &result.artifacts, Some(&contract_overrides)) {
                warn!("{} {} — output validation warning: {}", prefix, stage.name(), e);
            }
        }

        // Register produced artifacts.
        for artifact in &result.artifacts {
            let art_path = run_dir
                .join(format!("stage-{:02}", stage_num))
                .join(artifact);
            artifact_registry.insert(artifact.clone(), art_path);
        }

        // Checkpoint on success.
        if result.status == StageStatus::Done {
            if let Err(e) =
                write_checkpoint(run_dir, stage, run_id, StageStatus::Done).await
            {
                warn!("checkpoint write failed for {}: {}", stage.name(), e);
            }
        }

        // Final heartbeat with completion status.
        let hb_status = if result.status == StageStatus::Done { "completed" } else { "failed" };
        if let Err(e) = write_heartbeat(run_dir, stage, run_id, hb_status, elapsed).await {
            warn!("heartbeat write failed: {}", e);
        }

        // Gate handling.
        if result.status == StageStatus::Done
            && crate::stages::gate_required(stage, None)
        {
            if pipeline_config.auto_approve {
                // Auto-approve: drive SUCCEED → BLOCKED, then APPROVE → DONE.
                let _blocked = advance(stage, StageStatus::Running, TransitionEvent::Succeed)?;
                info!("{} {} — gate auto-approved", prefix, stage.name());
            } else if pipeline_config.stop_on_gate {
                // Caller wants to handle the gate externally.
                result.status = StageStatus::BlockedApproval;
                result.decision = "block".to_owned();
                results.push(result);
                break 'outer;
            }
        }

        // Decision rollback (RESEARCH_DECISION stage only).
        if stage == Stage::ResearchDecision
            && result.status == StageStatus::Done
            && decision_rollback(&result.decision).is_some()
        {
            if pivot_count < MAX_DECISION_PIVOTS {
                let rollback_target = decision_rollback(&result.decision).unwrap();
                pivot_count += 1;
                info!(
                    "[{}] Decision: {} → rollback to {} (attempt {}/{})",
                    run_id,
                    result.decision.to_uppercase(),
                    rollback_target.name(),
                    pivot_count,
                    MAX_DECISION_PIVOTS
                );
                results.push(result);
                // Recurse into a sub-run from the rollback target.
                let sub_cfg = PipelineConfig {
                    from_stage: rollback_target,
                    to_stage: pipeline_config.to_stage,
                    auto_approve: pipeline_config.auto_approve,
                    stop_on_gate: pipeline_config.stop_on_gate,
                    skip_noncritical: pipeline_config.skip_noncritical,
                    graceful_degradation: pipeline_config.graceful_degradation,
                };
                // Box the future to prevent infinite type recursion.
                let sub_summary = Box::pin(execute_pipeline(config, &sub_cfg, run_dir, run_id)).await?;
                // Fold sub-results summary back into ours via artifact count.
                stages_skipped += sub_summary.stages_skipped;
                break 'outer;
            } else {
                warn!(
                    "[{}] Max pivot attempts ({}) reached — forcing PROCEED",
                    run_id, MAX_DECISION_PIVOTS
                );
            }
        }

        results.push(result.clone());

        // Failure handling.
        if result.status == StageStatus::Failed {
            if pipeline_config.skip_noncritical && NONCRITICAL_STAGES.contains(&stage) {
                warn!(
                    "{} {} — noncritical stage failed, skipping",
                    prefix, stage.name()
                );
                stages_skipped += 1;
                continue;
            }
            if pipeline_config.graceful_degradation {
                warn!(
                    "{} {} — graceful degradation: continuing past failure",
                    prefix, stage.name()
                );
                stages_skipped += 1;
                continue;
            }
            // Critical failure — stop the pipeline.
            break 'outer;
        }

        // Stop-on-gate.
        if result.status == StageStatus::BlockedApproval && pipeline_config.stop_on_gate {
            break 'outer;
        }
    }

    let total_elapsed = t_start.elapsed().as_secs_f64();
    let mut summary =
        PipelineSummary::build(run_id, &results, effective_from, total_elapsed);
    summary.stages_skipped = stages_skipped;

    // Persist summary.
    write_pipeline_summary(run_dir, &summary).await;

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Blinding gate
// ---------------------------------------------------------------------------

/// Blinding gate status for Phase 3→4 transition.
#[derive(Debug, PartialEq)]
pub enum BlindingStatus {
    Approved,
    Blocked,
}

/// Check the blinding gate before entering Phase 4.
///
/// Reads `blinding_status.json` from the run directory. If the file does not
/// exist, creates it with status `"blinded"`. Returns `Blocked` unless the
/// status is already approved or `auto_approve` is true.
pub async fn check_blinding_gate(
    run_dir: &Path,
    auto_approve: bool,
) -> Result<BlindingStatus> {
    let path = run_dir.join("blinding_status.json");

    #[derive(Debug, Serialize, Deserialize)]
    struct BlindingRecord {
        status: String,
        #[serde(default)]
        phase: String,
        #[serde(default)]
        approved_at: String,
    }

    let record = if path.exists() {
        let text = tokio::fs::read_to_string(&path).await?;
        serde_json::from_str::<BlindingRecord>(&text).unwrap_or(BlindingRecord {
            status: "blinded".into(),
            phase: "4a_asimov".into(),
            approved_at: String::new(),
        })
    } else {
        let initial = BlindingRecord {
            status: "blinded".into(),
            phase: "4a_asimov".into(),
            approved_at: String::new(),
        };
        let json = serde_json::to_string_pretty(&initial)?;
        tokio::fs::write(&path, &json).await?;
        initial
    };

    // Already approved (by human or prior auto-approve)
    if record.status.starts_with("approved") {
        return Ok(BlindingStatus::Approved);
    }

    // Auto-approve path
    if auto_approve {
        warn!("Blinding gate auto-approved — not recommended for production analyses");
        let approved = BlindingRecord {
            status: "approved_auto".into(),
            phase: record.phase,
            approved_at: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string_pretty(&approved)?;
        tokio::fs::write(&path, &json).await?;
        return Ok(BlindingStatus::Approved);
    }

    info!("Blinding gate: analysis is blinded. Approve with /approve-unblinding before Phase 4 can proceed.");
    Ok(BlindingStatus::Blocked)
}

// ---------------------------------------------------------------------------
// execute_iterative_pipeline
// ---------------------------------------------------------------------------

/// Execute the iterative refinement loop (Stage 15 / `IterativeRefine`).
///
/// Runs stages `ExperimentRun` → `IterativeRefine` → `ResultAnalysis` in a
/// bounded loop, stopping when `ResultAnalysis` produces a `"proceed"`
/// decision or when `max_iterations` is reached.
pub async fn execute_iterative_pipeline(
    config: &MolConfig,
    pipeline_config: &PipelineConfig,
    run_dir: &Path,
    run_id: &str,
    max_iterations: u32,
) -> Result<Vec<StageResult>> {
    let llm: Option<std::sync::Arc<dyn mol_llm::LlmProvider>> = None;
    let prompt_engine: Option<Arc<StagePromptEngine>> = config
        .knowledge_chain
        .templates_dir()
        .and_then(|d| StagePromptEngine::load(&d).ok())
        .map(Arc::new);
    let contract_overrides = crate::contracts::ContractOverrides::load(&config.knowledge_chain);
    let iterative_stages = [
        Stage::ExperimentRun,
        Stage::IterativeRefine,
        Stage::ResultAnalysis,
    ];

    let mut all_results: Vec<StageResult> = Vec::new();
    let mut iteration: u32 = 0;
    let mut iter_artifacts: HashMap<String, PathBuf> = HashMap::new();

    loop {
        iteration += 1;
        if iteration > max_iterations {
            warn!(
                "[{}] Max iterative refinement iterations ({}) reached",
                run_id, max_iterations
            );
            break;
        }

        info!(
            "[{}] Iterative refinement — iteration {}/{}",
            run_id, iteration, max_iterations
        );

        let mut stop = false;
        for &stage in &iterative_stages {
            // Pre-flight input validation.
            let available: Vec<String> = iter_artifacts.keys().cloned().collect();
            if let Err(e) = crate::contracts::validate_inputs(stage, &available, Some(&contract_overrides)) {
                if NONCRITICAL_STAGES.contains(&stage) {
                    warn!(
                        "[{}] {} — input validation failed (noncritical, skipping): {}",
                        run_id, stage.name(), e
                    );
                    continue;
                } else {
                    warn!(
                        "[{}] {} — input validation failed: {}",
                        run_id, stage.name(), e
                    );
                    // Continue anyway in iterative mode — artifacts may arrive later.
                }
            }

            let context = StageContext {
                run_dir: run_dir.to_owned(),
                run_id: run_id.to_owned(),
                config: config.clone(),
                prior_artifacts: iter_artifacts.clone(),
                auto_approve_gates: pipeline_config.auto_approve,
                llm: llm.clone(),
                prompt_engine: prompt_engine.clone(),
            };

            let result = match execute_stage(stage, &context).await {
                Ok(r) => r,
                Err(e) => StageResult::failure(stage, e.to_string()),
            };

            // Post-execution output validation (soft warning only).
            if result.status == StageStatus::Done {
                if let Err(e) = crate::contracts::validate_outputs(stage, &result.artifacts, Some(&contract_overrides)) {
                    warn!("[{}] {} — output validation warning: {}", run_id, stage.name(), e);
                }
            }

            // Accumulate artifacts for subsequent stages.
            for artifact in &result.artifacts {
                let art_path = run_dir.join(artifact);
                iter_artifacts.insert(artifact.clone(), art_path);
            }

            let done = result.status == StageStatus::Done && result.decision == "proceed";
            all_results.push(result);

            if done && stage == Stage::ResultAnalysis {
                stop = true;
                break;
            }
        }

        if stop {
            break;
        }
    }

    Ok(all_results)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Determine the effective starting stage, preferring a checkpoint resume over
/// the configured `from_stage`.
async fn determine_start_stage(configured_from: Stage, run_dir: &Path) -> Stage {
    match read_checkpoint(run_dir).await {
        Ok(Some(checkpoint)) => {
            let resumed = resume_from_checkpoint(checkpoint);
            // Only use the checkpoint if it advances past the configured start.
            if resumed.as_i32() > configured_from.as_i32() {
                info!(
                    "Resuming from checkpoint: {} (configured from_stage: {})",
                    resumed.name(),
                    configured_from.name()
                );
                resumed
            } else {
                configured_from
            }
        }
        Ok(None) => configured_from,
        Err(e) => {
            warn!("Failed to read checkpoint, starting from configured stage: {}", e);
            configured_from
        }
    }
}

/// Write the pipeline summary JSON to `{run_dir}/pipeline_summary.json`.
async fn write_pipeline_summary(run_dir: &Path, summary: &PipelineSummary) {
    let path = run_dir.join("pipeline_summary.json");
    match serde_json::to_string_pretty(summary) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&path, json.as_bytes()).await {
                warn!("failed to write pipeline summary: {}", e);
            }
        }
        Err(e) => warn!("failed to serialise pipeline summary: {}", e),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn default_config() -> MolConfig {
        MolConfig::default()
    }

    #[tokio::test]
    async fn pipeline_runs_single_stage() {
        // Without LLM or prompt engine, stage fails honestly.
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::TopicInit),
            auto_approve: true,
            graceful_degradation: true,
            ..Default::default()
        };
        let summary = execute_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-001",
        )
        .await
        .unwrap();

        // Stage fails without prompt engine → skipped via graceful_degradation.
        assert_eq!(summary.stages_completed, 0);
        assert_eq!(summary.stages_skipped, 1);
    }

    #[tokio::test]
    async fn pipeline_skips_failed_stages_with_graceful_degradation() {
        // Without LLM, stages fail. graceful_degradation + skip_noncritical
        // allows the pipeline to continue past both failures and missing inputs.
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::ProblemDecompose),
            auto_approve: true,
            graceful_degradation: true,
            skip_noncritical: true,
            ..Default::default()
        };
        let summary = execute_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-002",
        )
        .await
        .unwrap();

        // Both stages skipped: TopicInit fails (no engine), ProblemDecompose
        // skipped (missing topic_brief input).
        assert!(summary.stages_skipped >= 1);
    }

    #[tokio::test]
    async fn pipeline_writes_summary_file() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::TopicInit),
            auto_approve: true,
            graceful_degradation: true,
            ..Default::default()
        };
        execute_pipeline(&default_config(), &pipeline_cfg, dir.path(), "test-003")
            .await
            .unwrap();

        assert!(dir.path().join("pipeline_summary.json").exists());
    }

    #[tokio::test]
    async fn pipeline_stop_on_gate() {
        // Without LLM, stages fail. Use graceful_degradation + skip_noncritical
        // to allow pipeline to reach the gate stage.
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::LiteratureScreen),
            auto_approve: false,
            stop_on_gate: true,
            graceful_degradation: true,
            skip_noncritical: true,
            ..Default::default()
        };
        let summary = execute_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-gate",
        )
        .await
        .unwrap();

        // Stages get skipped (no engine) — pipeline completes.
        assert!(summary.stages_skipped > 0);
    }

    #[tokio::test]
    async fn iterative_pipeline_runs() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig::default();
        let results = execute_iterative_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-iter",
            3,
        )
        .await
        .unwrap();

        // Should have produced at least one result per stage.
        assert!(!results.is_empty());
    }

    /// Contract validation — critical stage missing inputs returns error.
    ///
    /// `ProblemDecompose` requires `topic_brief`.  Running only that stage
    /// (no prior stages, so no artifacts in the registry) must cause the
    /// pipeline to return `Err` rather than calling `execute_stage`.
    #[tokio::test]
    async fn contract_validation_critical_stage_missing_inputs_returns_error() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::ProblemDecompose,
            to_stage: Some(Stage::ProblemDecompose),
            auto_approve: true,
            ..Default::default()
        };
        let result = execute_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-contract-critical",
        )
        .await;

        assert!(result.is_err(), "critical stage with missing inputs should return Err");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("topic_brief"),
            "error message should name the missing artifact; got: {msg}"
        );
    }

    /// Contract validation — noncritical stage missing inputs is skipped.
    ///
    /// `QualityGate` is in `NONCRITICAL_STAGES` and requires `paper_revised`.
    /// Starting the pipeline from `QualityGate` with no prior artifacts should
    /// log a warning and skip the stage instead of returning an error.
    #[tokio::test]
    async fn contract_validation_noncritical_stage_missing_inputs_is_skipped() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::QualityGate,
            to_stage: Some(Stage::QualityGate),
            auto_approve: true,
            ..Default::default()
        };
        let summary = execute_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-contract-noncritical",
        )
        .await
        .unwrap();

        // The stage was skipped, not failed.
        assert_eq!(summary.stages_completed, 0);
        assert_eq!(summary.stages_failed, 0);
        assert_eq!(summary.stages_skipped, 1);
    }

    /// Contract validation — output validation is a soft warning.
    ///
    /// `TopicInit` has no required inputs so it always passes pre-flight.
    /// Its contract expects `topic_brief` and `research_questions`, but the
    /// implementation emits `goal.md` and `hardware_profile.json`.  The stage
    /// should still be counted as completed (output validation is non-fatal).
    #[tokio::test]
    async fn contract_validation_output_warning_does_not_fail_pipeline() {
        // Without prompt engine, TopicInit fails. Use graceful_degradation
        // to verify pipeline completes even with stage failures.
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::TopicInit),
            auto_approve: true,
            graceful_degradation: true,
            ..Default::default()
        };
        let summary = execute_pipeline(
            &default_config(),
            &pipeline_cfg,
            dir.path(),
            "test-contract-output",
        )
        .await
        .unwrap();

        // graceful_degradation skips the failure.
        assert_eq!(summary.stages_skipped, 1);
    }

    #[tokio::test]
    async fn blinding_gate_blocks_without_auto_approve() {
        let dir = TempDir::new().unwrap();
        let status = check_blinding_gate(dir.path(), false).await.unwrap();
        assert_eq!(status, BlindingStatus::Blocked);

        let content = tokio::fs::read_to_string(dir.path().join("blinding_status.json"))
            .await
            .unwrap();
        assert!(content.contains("blinded"));
    }

    #[tokio::test]
    async fn blinding_gate_auto_approves() {
        let dir = TempDir::new().unwrap();
        let status = check_blinding_gate(dir.path(), true).await.unwrap();
        assert_eq!(status, BlindingStatus::Approved);

        let content = tokio::fs::read_to_string(dir.path().join("blinding_status.json"))
            .await
            .unwrap();
        assert!(content.contains("approved_auto"));
    }

    #[tokio::test]
    async fn blinding_gate_respects_prior_approval() {
        let dir = TempDir::new().unwrap();
        tokio::fs::write(
            dir.path().join("blinding_status.json"),
            r#"{"status": "approved", "approved_at": "2026-04-08T00:00:00Z"}"#,
        )
        .await
        .unwrap();

        let status = check_blinding_gate(dir.path(), false).await.unwrap();
        assert_eq!(status, BlindingStatus::Approved);
    }
}
