//! Discussion stage executor.
//!
//! Generates a multi-agent discussion transcript via LLM, covering synthesis,
//! hypotheses, and experiment findings from a HEP analysis perspective.

use crate::executor::{StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// Discussion
// ---------------------------------------------------------------------------

/// Execute the Discussion stage.
///
/// Uses the discussion template + LLM to produce `discussion_notes.md` with a
/// structured multi-agent discussion transcript. Same pattern as all other stages.
pub async fn execute_discussion(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Template + LLM — same pattern as all other stages
    let vars = ctx.template_vars(stage);
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => {
            return StageResult::failure(
                stage,
                format!("No prompt engine configured for {}", stage.name()),
            );
        }
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => {
            return StageResult::failure(stage, format!("Template render error: {e}"));
        }
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(text) => text,
        Err(e) => {
            return StageResult::failure(stage, format!("LLM call failed: {e}"));
        }
    };

    if result.is_empty() {
        return StageResult::failure(stage, "LLM returned empty discussion".to_owned());
    }

    // Write discussion_notes.md
    let notes_path = stage_dir.join("discussion_notes.md");
    if let Err(e) = fs::write(&notes_path, &result) {
        return StageResult::failure(stage, format!("write discussion_notes.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["discussion_notes.md".into()],
        decision: "proceed".into(),
        error: None,
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

    fn make_ctx(dir: &std::path::Path, topic: &str) -> StageContext {
        StageContext {
            run_dir: dir.to_owned(),
            run_id: "test".to_owned(),
            config: MolConfig {
                topic: topic.to_owned(),
                settings: HashMap::new(),
                domain: "hep".to_owned(),
                analysis_type: None,
                knowledge_root: std::path::PathBuf::from("hep"),
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
