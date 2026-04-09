//! Discussion stage executor.
//!
//! Generates a multi-agent discussion transcript via LLM, covering synthesis,
//! hypotheses, and experiment findings from a HEP analysis perspective.

use crate::executor::{StageContext, StageResult};
use crate::stages::Stage;
#[cfg(test)]
use crate::stages::StageStatus;

// ---------------------------------------------------------------------------
// Discussion
// ---------------------------------------------------------------------------

/// Execute the Discussion stage via multi-agent parallel executor.
///
/// Primary agent (note-writer) produces `discussion_notes.md`, then three
/// independent participants contribute in parallel: lead-analyst writes
/// `analyst_perspective.md`, theory-scout writes `theory_perspective.md`,
/// and cross-checker writes `checker_perspective.md`.
pub async fn execute_discussion(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_multi_agentic};

    let primary_specs = vec![
        ArtifactSpec {
            filename: "discussion_notes.md".into(),
            description: "multi-perspective discussion transcript — structured debate between \
                virtual experts covering methodology strengths/weaknesses, result interpretation, \
                implications, limitations, and future directions".into(),
        },
    ];

    let reviewer_specs: Vec<(&str, Vec<ArtifactSpec>)> = vec![
        ("lead-analyst", vec![
            ArtifactSpec {
                filename: "analyst_perspective.md".into(),
                description: "lead analyst perspective — methodology evaluation, result \
                    significance, physics implications, and strategic recommendations".into(),
            },
        ]),
        ("theory-scout", vec![
            ArtifactSpec {
                filename: "theory_perspective.md".into(),
                description: "theory perspective — theoretical context, model comparisons, \
                    BSM implications, and connections to broader physics landscape".into(),
            },
        ]),
        ("cross-checker", vec![
            ArtifactSpec {
                filename: "checker_perspective.md".into(),
                description: "cross-checker perspective — internal consistency checks, \
                    potential biases, reproducibility concerns, and validation gaps".into(),
            },
        ]),
    ];

    execute_multi_agentic(stage, ctx, &primary_specs, &reviewer_specs).await
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
            auto_approve_gates: false,
            llm: None,
            prompt_engine: None,
        }
    }

    #[tokio::test]
    async fn discussion_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "jet classification");
        let result = execute_discussion(Stage::Discussion, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
