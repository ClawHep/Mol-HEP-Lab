//! Stage dispatch and execution.
//!
//! `execute_stage` is the single dispatch point that the runner calls for
//! every pipeline stage.  Individual stage implementations live (or will
//! live) in domain-specific crates; for now every stage returns a stub
//! `StageResult` so the pipeline wiring can be exercised end-to-end.

use crate::stages::{Stage, StageStatus};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// TODO: replace with real mol-config / mol-llm types once those crates
// expose a stable public API.
// ---------------------------------------------------------------------------

/// Minimal configuration surface used by stage executors.
///
/// TODO: replace with `mol_config::MolConfig` once that crate is stable.
#[derive(Debug, Clone)]
pub struct MolConfig {
    /// Research topic / title.
    pub topic: String,
    /// Free-form key-value settings forwarded to individual stage executors.
    pub settings: HashMap<String, String>,
}

impl Default for MolConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            settings: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// StageResult
// ---------------------------------------------------------------------------

/// Outcome of executing a single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// The stage that was executed.
    pub stage: Stage,
    /// Final execution status.
    pub status: StageStatus,
    /// Names of artifacts produced (relative to the stage output directory).
    pub artifacts: Vec<String>,
    /// Error message when `status` is `Failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Decision token emitted by the stage: `"proceed"`, `"pivot"`,
    /// `"refine"`, `"degraded"`, etc.
    pub decision: String,
    /// Wall-clock execution time in seconds.
    pub elapsed_secs: f64,
}

impl StageResult {
    /// Create a successful stub result with no artifacts.
    pub fn stub_success(stage: Stage) -> Self {
        Self {
            stage,
            status: StageStatus::Done,
            artifacts: Vec::new(),
            error: None,
            decision: "proceed".to_owned(),
            elapsed_secs: 0.0,
        }
    }

    /// Create a failed result.
    pub fn failure(stage: Stage, error: impl Into<String>) -> Self {
        Self {
            stage,
            status: StageStatus::Failed,
            artifacts: Vec::new(),
            error: Some(error.into()),
            decision: "retry".to_owned(),
            elapsed_secs: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// StageContext
// ---------------------------------------------------------------------------

/// Runtime context passed to every stage executor.
///
/// Carries the run directory, configuration, and a snapshot of all artifacts
/// produced by earlier stages.
#[derive(Debug, Clone)]
pub struct StageContext {
    /// Root directory for this pipeline run (e.g. `runs/run-abc123/`).
    pub run_dir: PathBuf,
    /// Run identifier string.
    pub run_id: String,
    /// Pipeline configuration.
    pub config: MolConfig,
    /// Artifacts produced by stages that completed before this one.
    /// Keys are artifact names; values are paths relative to `run_dir`.
    pub prior_artifacts: HashMap<String, PathBuf>,
    /// Whether gate stages should be auto-approved without HITL interaction.
    pub auto_approve_gates: bool,
}

impl StageContext {
    /// Return the output directory for `stage` within this run.
    pub fn stage_dir(&self, stage: Stage) -> PathBuf {
        self.run_dir.join(format!("stage-{:02}", stage.as_i32()))
    }
}

// ---------------------------------------------------------------------------
// Stage dispatch
// ---------------------------------------------------------------------------

/// Execute `stage` using the supplied context, returning a `StageResult`.
///
/// Currently every stage is a stub that logs and returns a synthetic
/// `Done` result.  Each arm should be replaced with a call to the
/// corresponding domain-crate implementation as those crates are built.
pub async fn execute_stage(stage: Stage, context: &StageContext) -> Result<StageResult> {
    let stage_dir = context.stage_dir(stage);
    tokio::fs::create_dir_all(&stage_dir).await?;

    debug!(
        stage = stage.name(),
        run_id = %context.run_id,
        dir = %stage_dir.display(),
        "dispatching stage"
    );

    let t0 = std::time::Instant::now();

    let mut result = match stage {
        // Phase A: Research Scoping ----------------------------------------
        Stage::TopicInit => stub_stage(stage, &["topic_brief", "research_questions"]).await,
        Stage::ProblemDecompose => stub_stage(stage, &["problem_tree", "sub_problems"]).await,

        // Phase B: Literature Discovery ------------------------------------
        Stage::SearchStrategy => stub_stage(stage, &["search_queries", "source_list"]).await,
        Stage::LiteratureCollect => stub_stage(stage, &["raw_papers", "paper_metadata"]).await,
        Stage::LiteratureScreen => {
            stub_stage(stage, &["screened_papers", "exclusion_reasons"]).await
        }
        Stage::KnowledgeExtract => stub_stage(stage, &["knowledge_cards", "citation_map"]).await,

        // Phase C: Knowledge Synthesis -------------------------------------
        Stage::Synthesis => stub_stage(stage, &["synthesis_report", "gap_analysis"]).await,
        Stage::HypothesisGen => stub_stage(stage, &["hypotheses", "rationale"]).await,

        // Phase D: Experiment Design ----------------------------------------
        Stage::ExperimentDesign => {
            stub_stage(stage, &["experiment_plan", "success_criteria"]).await
        }
        Stage::CodebaseSearch => {
            stub_stage(stage, &["codebase_context", "relevant_files"]).await
        }
        Stage::CodeGeneration => stub_stage(stage, &["experiment_code", "code_readme"]).await,
        Stage::SanityCheck => stub_stage(stage, &["sanity_report"]).await,
        Stage::ResourcePlanning => {
            stub_stage(stage, &["resource_plan", "compute_estimate"]).await
        }

        // Phase E: Experiment Execution ------------------------------------
        Stage::ExperimentRun => stub_stage(stage, &["raw_results", "run_logs"]).await,
        Stage::IterativeRefine => {
            stub_stage(stage, &["refined_results", "refinement_log"]).await
        }

        // Phase F: Analysis & Decision -------------------------------------
        Stage::ResultAnalysis => stub_stage(stage, &["analysis_report", "figures"]).await,
        Stage::ResearchDecision => stub_stage(stage, &["decision_record"]).await,
        Stage::KnowledgeSummary => stub_stage(stage, &["knowledge_summary"]).await,

        // Phase G: Paper Writing -------------------------------------------
        Stage::PaperOutline => stub_stage(stage, &["paper_outline"]).await,
        Stage::PaperDraft => stub_stage(stage, &["paper_draft"]).await,
        Stage::PeerReview => stub_stage(stage, &["review_comments"]).await,
        Stage::PaperRevision => stub_stage(stage, &["paper_revised", "revision_notes"]).await,

        // Phase H: Finalization --------------------------------------------
        Stage::QualityGate => stub_stage(stage, &["quality_report"]).await,
        Stage::KnowledgeArchive => stub_stage(stage, &["archive_manifest"]).await,
        Stage::ExportPublish => stub_stage(stage, &["paper_final", "paper_tex"]).await,
        Stage::CitationVerify => {
            stub_stage(stage, &["verification_report", "paper_final_verified"]).await
        }

        // Special ----------------------------------------------------------
        Stage::Discussion => stub_stage(stage, &["discussion_notes"]).await,
    };

    result.elapsed_secs = t0.elapsed().as_secs_f64();

    info!(
        stage = stage.name(),
        status = %result.status,
        elapsed = result.elapsed_secs,
        artifacts = ?result.artifacts,
        "stage complete"
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return a stub `Done` result listing `artifact_names` as produced outputs.
async fn stub_stage(stage: Stage, artifact_names: &[&str]) -> StageResult {
    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: artifact_names.iter().map(|s| s.to_string()).collect(),
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
    use std::path::Path;
    use tempfile::TempDir;

    fn make_context(run_dir: &Path) -> StageContext {
        StageContext {
            run_dir: run_dir.to_owned(),
            run_id: "test-run".to_owned(),
            config: MolConfig::default(),
            prior_artifacts: HashMap::new(),
            auto_approve_gates: false,
        }
    }

    #[tokio::test]
    async fn execute_topic_init_returns_done() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let result = execute_stage(Stage::TopicInit, &ctx).await.unwrap();
        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"topic_brief".to_owned()));
    }

    #[tokio::test]
    async fn execute_creates_stage_dir() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        execute_stage(Stage::TopicInit, &ctx).await.unwrap();
        assert!(dir.path().join("stage-01").exists());
    }

    #[tokio::test]
    async fn stub_result_decision_is_proceed() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let result = execute_stage(Stage::SanityCheck, &ctx).await.unwrap();
        assert_eq!(result.decision, "proceed");
    }
}
