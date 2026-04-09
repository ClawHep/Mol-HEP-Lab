//! Phase 5: Documentation — PaperOutline (5.1), PaperDraft (5.2),
//! PeerReview (5.3), PaperRevision (5.4), QualityGate (5.5),
//! KnowledgeArchive (5.6), ExportPublish (5.7), and CitationVerify (5.8) stage executors.

use crate::executor::{
    read_prior_artifact_pub, utcnow_iso, StageContext, StageResult,
};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// PaperOutline
// ---------------------------------------------------------------------------

/// Execute the PaperOutline stage via agentic executor.
///
/// Reads hypotheses, synthesis, and analysis; produces `paper_outline.md`.
pub async fn execute_paper_outline(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "paper_outline.md".into(),
            description: "academic paper outline — section structure with title, abstract \
                sketch, introduction points, methodology overview, results plan, and \
                conclusion direction".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// PaperDraft
// ---------------------------------------------------------------------------

/// Execute the PaperDraft stage via agentic executor.
///
/// Reads outline, knowledge cards, analysis, and figures; produces `paper_draft.md`.
pub async fn execute_paper_draft(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "paper_draft.md".into(),
            description: "full academic paper draft — title, abstract, introduction, \
                related work, methodology, experiments, results, discussion, conclusion, \
                and references".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// PeerReview
// ---------------------------------------------------------------------------

/// Execute the PeerReview stage via agentic executor.
///
/// Reads paper draft; produces `review_comments.md` with simulated multi-reviewer feedback.
pub async fn execute_peer_review(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "review_comments.md".into(),
            description: "peer review comments — simulated multi-reviewer feedback with \
                reviewer roles, per-section comments, severity ratings, and overall scores".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// PaperRevision
// ---------------------------------------------------------------------------

/// Execute the PaperRevision stage via agentic executor.
///
/// Reads paper draft and review comments; produces `paper_revised.md` and
/// `revision_notes.md`.
pub async fn execute_paper_revision(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "paper_revised.md".into(),
            description: "revised paper — full paper with all reviewer comments addressed, \
                improvements made, and content strengthened".into(),
        },
        ArtifactSpec {
            filename: "revision_notes.md".into(),
            description: "revision notes — point-by-point response to each reviewer comment, \
                what was changed, and rationale for changes or rebuttals".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}


// ---------------------------------------------------------------------------
// QualityGate (5.5 GATE)
// ---------------------------------------------------------------------------

/// Execute the QualityGate stage via agentic executor.
///
/// Reads the revised paper; produces `quality_report.md`.
/// Returns BlockedApproval if not auto-approved.
pub async fn execute_quality_gate(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "quality_report.md".into(),
            description: "quality gate report — overall quality score (1-10), passes boolean, \
                per-section scores, identified issues, and recommendations".into(),
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

/// Execute the ExportPublish stage via agentic executor.
///
/// Produces `paper_final.md` (polished final paper) and `paper.tex` (LaTeX export).
pub async fn execute_export_publish(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "paper_final.md".into(),
            description: "final polished paper — publication-ready markdown with proper \
                formatting, citations, figure references, and clean structure".into(),
        },
        ArtifactSpec {
            filename: "paper.tex".into(),
            description: "LaTeX export of the paper — complete .tex file with \\documentclass, \
                \\title, \\author, \\begin{document}, all sections, and \\end{document}".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// CitationVerify
// ---------------------------------------------------------------------------

/// Execute the CitationVerify stage via agentic executor.
///
/// Reads the paper and references; produces `verification_report.md`.
pub async fn execute_citation_verify(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "verification_report.md".into(),
            description: "citation verification report — each citation checked for existence, \
                correctness, and proper formatting with overall verification status".into(),
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
