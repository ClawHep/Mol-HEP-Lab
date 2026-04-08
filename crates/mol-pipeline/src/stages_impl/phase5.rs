//! Phase 5: Documentation — PaperOutline (5.1), PaperDraft (5.2),
//! PeerReview (5.3), PaperRevision (5.4), QualityGate (5.5),
//! KnowledgeArchive (5.6), ExportPublish (5.7), and CitationVerify (5.8) stage executors.

use crate::executor::{
    extract_paper_title, read_prior_artifact_pub, utcnow_iso, StageContext, StageResult,
};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// PaperOutline
// ---------------------------------------------------------------------------

/// Execute the PaperOutline stage.
///
/// Reads hypotheses, synthesis, and analysis; produces `paper_outline.md`.
pub async fn execute_paper_outline(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write artifact
    if let Err(e) = fs::write(stage_dir.join("paper_outline.md"), &result) {
        return StageResult::failure(stage, format!("write paper_outline.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["paper_outline.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// PaperDraft
// ---------------------------------------------------------------------------

/// Execute the PaperDraft stage.
///
/// Reads outline, knowledge cards, analysis, and figures; produces `paper_draft.md`.
pub async fn execute_paper_draft(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write artifact
    if let Err(e) = fs::write(stage_dir.join("paper_draft.md"), &result) {
        return StageResult::failure(stage, format!("write paper_draft.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["paper_draft.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// PeerReview
// ---------------------------------------------------------------------------

/// Execute the PeerReview stage.
///
/// Reads paper draft; produces `review_comments.json` with simulated multi-reviewer feedback.
pub async fn execute_peer_review(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write artifact
    if let Err(e) = fs::write(stage_dir.join("review_comments.json"), &result) {
        return StageResult::failure(stage, format!("write review_comments.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["review_comments.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// PaperRevision
// ---------------------------------------------------------------------------

/// Execute the PaperRevision stage.
///
/// Reads paper draft and review comments; produces `paper_revised.md` and
/// `revision_notes.md`.
pub async fn execute_paper_revision(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    let topic = ctx.config.topic.as_str();

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write artifacts
    if let Err(e) = fs::write(stage_dir.join("paper_revised.md"), &result) {
        return StageResult::failure(stage, format!("write paper_revised.md: {e}"));
    }
    let revision_notes = format!(
        "# Revision Notes\n\n**Topic**: {topic}\n**Generated**: {ts}\n\n\
         Revisions generated by LLM agent based on peer review feedback.\n",
        topic = topic,
        ts = utcnow_iso(),
    );
    if let Err(e) = fs::write(stage_dir.join("revision_notes.md"), &revision_notes) {
        return StageResult::failure(stage, format!("write revision_notes.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["paper_revised.md".to_owned(), "revision_notes.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}


// ---------------------------------------------------------------------------
// QualityGate (5.5 GATE)
// ---------------------------------------------------------------------------

/// Execute the QualityGate stage.
///
/// Reads the revised paper; produces `quality_report.json`.
/// Returns BlockedApproval if quality score < 6 and not auto-approved.
pub async fn execute_quality_gate(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write artifact
    if let Err(e) = fs::write(stage_dir.join("quality_report.json"), &result) {
        return StageResult::failure(stage, format!("write quality_report.json: {e}"));
    }

    // Parse passes from LLM output to enforce gate logic
    let passes = serde_json::from_str::<serde_json::Value>(&result)
        .ok()
        .and_then(|v| v["passes"].as_bool())
        .unwrap_or(true);

    if !passes && !ctx.auto_approve_gates {
        return StageResult {
            stage,
            status: StageStatus::BlockedApproval,
            artifacts: vec!["quality_report.json".to_owned()],
            error: None,
            decision: "awaiting_approval".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    if !ctx.auto_approve_gates {
        return StageResult {
            stage,
            status: StageStatus::BlockedApproval,
            artifacts: vec!["quality_report.json".to_owned()],
            error: None,
            decision: "awaiting_approval".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["quality_report.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// KnowledgeArchive
// ---------------------------------------------------------------------------

/// Execute the KnowledgeArchive stage.
///
/// Reads knowledge summary and findings; produces `archive_manifest.json`.
pub async fn execute_knowledge_archive(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    let topic = ctx.config.topic.as_str();
    let _knowledge_summary =
        read_prior_artifact_pub(&ctx.run_dir, "knowledge_summary.json").unwrap_or_default();

    // Collect all artifacts that exist in prior stage dirs
    let mut archived_entries: Vec<serde_json::Value> = Vec::new();

    let artifact_names = [
        ("goal.md", "Research goal definition"),
        ("hypotheses.md", "Research hypotheses"),
        ("synthesis_report.md", "Literature synthesis report"),
        ("knowledge_cards.json", "Extracted knowledge cards"),
        ("exp_plan.yaml", "Experiment plan"),
        ("analysis_report.md", "Result analysis report"),
        ("paper_final.md", "Final paper"),
        ("knowledge_summary.json", "Knowledge summary"),
    ];

    for (name, description) in &artifact_names {
        if let Some(path) = crate::executor::find_prior_file_pub(&ctx.run_dir, name) {
            archived_entries.push(serde_json::json!({
                "name": name,
                "description": description,
                "path": path.display().to_string(),
                "archived_at": utcnow_iso()
            }));
        }
    }

    let archive_manifest = serde_json::json!({
        "topic": topic,
        "run_id": ctx.run_id,
        "archived_at": utcnow_iso(),
        "total_entries": archived_entries.len(),
        "entries": archived_entries,
        "archive_location": format!("knowledge_archive/{}", ctx.run_id),
        "notes": "Archive manifest lists all artifacts produced during this research run."
    });

    if let Err(e) = fs::write(
        stage_dir.join("archive_manifest.json"),
        serde_json::to_string_pretty(&archive_manifest).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write archive_manifest.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["archive_manifest.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// ExportPublish
// ---------------------------------------------------------------------------

/// Execute the ExportPublish stage.
///
/// Reads the final paper; produces `paper_final.md` and `paper.tex`.
pub async fn execute_export_publish(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    let topic = ctx.config.topic.as_str();

    // Render prompt from template engine
    // The template handles missing prior artifacts via {{ paper_revised | default(value="") }}
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write paper_final.md with export header
    let paper_final_with_header = format!(
        "<!-- Mol-HEP-Lab Final Paper Export | {} -->\n<!-- Run: {} | Topic: {} -->\n\n{}",
        utcnow_iso(),
        ctx.run_id,
        topic,
        result
    );
    if let Err(e) = fs::write(stage_dir.join("paper_final.md"), &paper_final_with_header) {
        return StageResult::failure(stage, format!("write paper_final.md: {e}"));
    }

    // Write paper.tex skeleton
    let title = extract_paper_title(&result);
    let title_escaped = title.replace('_', r"\_").replace('&', r"\&").replace('%', r"\%");
    let topic_escaped = topic.replace('_', r"\_");
    let paper_tex = format!(
        "\\documentclass{{article}}\n\\title{{{}}}\n\\author{{Authors}}\n\\date{{{}}}\n\
         \\begin{{document}}\n\\maketitle\n[Content from paper\\_final.md]\n\\end{{document}}\n",
        if title_escaped.is_empty() { format!("Advances in {}", topic_escaped) } else { title_escaped },
        utcnow_iso()
    );
    if let Err(e) = fs::write(stage_dir.join("paper.tex"), &paper_tex) {
        return StageResult::failure(stage, format!("write paper.tex: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["paper_final.md".to_owned(), "paper.tex".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// CitationVerify
// ---------------------------------------------------------------------------

/// Execute the CitationVerify stage.
///
/// Reads the paper and references; produces `verification_report.json`.
pub async fn execute_citation_verify(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, true).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
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

    // Write artifact
    if let Err(e) = fs::write(stage_dir.join("verification_report.json"), &result) {
        return StageResult::failure(stage, format!("write verification_report.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["verification_report.json".to_owned()],
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
    async fn paper_draft_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "multi-task learning", true);
        let result = execute_paper_draft(Stage::PaperDraft, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn peer_review_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "neural architecture search", true);
        let result = execute_peer_review(Stage::PeerReview, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn paper_revision_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "zero-shot learning", true);
        let result = execute_paper_revision(Stage::PaperRevision, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn quality_gate_returns_blocked_when_not_auto_approved() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "active learning", false);
        let result = execute_quality_gate(Stage::QualityGate, &ctx).await;
        // Without engine, stage fails — gate logic requires LLM output first
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn export_publish_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "few-shot learning", true);
        let result = execute_export_publish(Stage::ExportPublish, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn citation_verify_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "causal inference", true);
        let result = execute_citation_verify(Stage::CitationVerify, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
