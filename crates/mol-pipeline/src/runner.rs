//! Pipeline orchestration: sequential stage loop with checkpoint, gate, and
//! decision-rollback handling.
//!
//! Ported from `backend/agent/researchclaw/pipeline/runner.py`.

use crate::checkpoint::{read_checkpoint, resume_from_checkpoint, write_checkpoint, write_heartbeat};
use crate::executor::{execute_stage, MolConfig, StageContext, StageResult};
use crate::stages::{
    advance, decision_rollback, Stage, StageStatus, TransitionEvent, NONCRITICAL_STAGES,
    STAGE_SEQUENCE, MAX_DECISION_PIVOTS,
};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    /// When `true` the runner continues past failures using degraded-mode
    /// fallbacks where available.
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
    /// Whether any stage ran in degraded mode.
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
        let degraded = results.iter().any(|r| r.decision == "degraded");

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
    tokio::fs::create_dir_all(run_dir).await?;

    let t_start = Instant::now();
    let total_stages = STAGE_SEQUENCE.len();

    let mut results: Vec<StageResult> = Vec::new();
    let mut artifact_registry: HashMap<String, PathBuf> = HashMap::new();
    let mut started = false;
    let mut pivot_count: u32 = 0;
    let mut stages_skipped: usize = 0;

    // Determine the effective starting stage (may be overridden by checkpoint).
    let effective_from = determine_start_stage(pipeline_config.from_stage, run_dir).await;

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
        let prefix = format!("[{}] Stage {:02}/{}", run_id, stage_num, total_stages);

        info!("{} {} — running...", prefix, stage.name());

        // Build execution context.
        let context = StageContext {
            run_dir: run_dir.to_owned(),
            run_id: run_id.to_owned(),
            config: config.clone(),
            prior_artifacts: artifact_registry.clone(),
            auto_approve_gates: pipeline_config.auto_approve,
        };

        // Execute the stage.
        let stage_t0 = Instant::now();
        let mut result = match execute_stage(stage, &context).await {
            Ok(r) => r,
            Err(e) => {
                warn!("{} {} — execution error: {}", prefix, stage.name(), e);
                StageResult::failure(stage, e.to_string())
            }
        };
        let elapsed = stage_t0.elapsed().as_secs_f64();
        result.elapsed_secs = elapsed;

        // Log outcome.
        match result.status {
            StageStatus::Done => {
                let arts = result.artifacts.join(", ");
                if result.decision == "degraded" {
                    info!(
                        "{} {} — DEGRADED ({:.1}s) — continuing with sanitization → {}",
                        prefix, stage.name(), elapsed, arts
                    );
                } else {
                    info!("{} {} — done ({:.1}s) → {}", prefix, stage.name(), elapsed, arts);
                }
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

        // Heartbeat for sentinel watchdog.
        if let Err(e) = write_heartbeat(run_dir, stage, run_id).await {
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
    let iterative_stages = [
        Stage::ExperimentRun,
        Stage::IterativeRefine,
        Stage::ResultAnalysis,
    ];

    let mut all_results: Vec<StageResult> = Vec::new();
    let mut iteration: u32 = 0;

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
            let context = StageContext {
                run_dir: run_dir.to_owned(),
                run_id: run_id.to_owned(),
                config: config.clone(),
                prior_artifacts: HashMap::new(),
                auto_approve_gates: pipeline_config.auto_approve,
            };

            let result = match execute_stage(stage, &context).await {
                Ok(r) => r,
                Err(e) => StageResult::failure(stage, e.to_string()),
            };

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
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::TopicInit),
            auto_approve: true,
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

        assert_eq!(summary.stages_completed, 1);
        assert_eq!(summary.stages_failed, 0);
    }

    #[tokio::test]
    async fn pipeline_runs_phase_a() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::ProblemDecompose),
            auto_approve: true,
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

        assert_eq!(summary.stages_completed, 2);
    }

    #[tokio::test]
    async fn pipeline_writes_summary_file() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::TopicInit),
            auto_approve: true,
            ..Default::default()
        };
        execute_pipeline(&default_config(), &pipeline_cfg, dir.path(), "test-003")
            .await
            .unwrap();

        assert!(dir.path().join("pipeline_summary.json").exists());
    }

    #[tokio::test]
    async fn pipeline_stop_on_gate() {
        let dir = TempDir::new().unwrap();
        let pipeline_cfg = PipelineConfig {
            from_stage: Stage::TopicInit,
            to_stage: Some(Stage::LiteratureScreen),
            auto_approve: false,
            stop_on_gate: true,
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

        // The pipeline should have stopped at or before LiteratureScreen.
        assert!(summary.stages_completed >= 1);
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
}
