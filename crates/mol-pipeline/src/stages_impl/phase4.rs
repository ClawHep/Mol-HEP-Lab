//! Phase 4: Inference — ResultAnalysis, ResearchDecision, and
//! KnowledgeSummary (4.3) stage executors.

use crate::executor::{StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// ResultAnalysis
// ---------------------------------------------------------------------------

/// Execute the ResultAnalysis stage via agentic executor.
///
/// Produces `analysis_report.md` with structured analysis of experiment results.
pub async fn execute_result_analysis(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "analysis_report.md".into(),
            description: "experiment result analysis report — statistical analysis, \
                comparison with baselines, significance tests, key findings, primary metrics, \
                number of runs, mean/std/best scores, baseline comparison, and recommendation".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// ResearchDecision
// ---------------------------------------------------------------------------

/// Execute the ResearchDecision stage via agentic executor.
///
/// Reads analysis report; produces `decision_record.md` and extracts the
/// decision keyword (proceed/pivot/stop) to set the stage result decision.
pub async fn execute_research_decision(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic, extract_decision_from_md};

    let specs = vec![
        ArtifactSpec {
            filename: "decision_record.md".into(),
            description: "research decision record — rationale, confidence, alternative paths \
                considered, and next steps. \
                IMPORTANT: Include a line: **Decision: proceed** (or pivot/refine/stop)".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Extract decision keyword from the produced markdown artifact
    let stage_dir = ctx.stage_dir(stage);
    if let Ok(content) = fs::read_to_string(stage_dir.join("decision_record.md")) {
        result.decision = extract_decision_from_md(&content).to_owned();
    }

    result
}

// ---------------------------------------------------------------------------
// KnowledgeSummary
// ---------------------------------------------------------------------------

/// Execute the KnowledgeSummary stage via agentic executor.
///
/// Reads findings, hypotheses, analysis; produces `knowledge_summary.md`.
pub async fn execute_knowledge_summary(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "knowledge_summary.md".into(),
            description: "knowledge summary — key findings, validated hypotheses, methodology \
                insights, limitations, and recommendations for future work".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
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

    fn make_ctx(dir: &std::path::Path, topic: &str) -> StageContext {
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
            auto_approve_gates: true,
            llm: None,
            prompt_engine: None,
        }
    }

    #[tokio::test]
    async fn result_analysis_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "federated learning");
        let result = execute_result_analysis(Stage::ResultAnalysis, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn research_decision_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "continual learning");
        let result = execute_research_decision(Stage::ResearchDecision, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn knowledge_summary_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "self-supervised learning");
        let result = execute_knowledge_summary(Stage::KnowledgeSummary, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
