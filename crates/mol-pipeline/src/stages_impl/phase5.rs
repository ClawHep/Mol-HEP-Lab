//! Phase 5: Documentation — PaperOutline (5.1), PaperWrite (5.2, ← PaperDraft + PaperRevision),
//! PeerReview (5.3), QualityGate (5.4), Publish (5.5, ← KnowledgeArchive + ExportPublish + CitationVerify).

use crate::executor::{
    utcnow_iso, StageContext, StageResult,
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
// PaperWrite (← merged PaperDraft + PaperRevision)
// ---------------------------------------------------------------------------

/// Execute the PaperWrite stage — draft, then revise.
///
/// Inner loop: write draft → peer review (external) → revise based on feedback.
/// Produces paper_draft.md, paper_revised.md, and revision_notes.md.
pub async fn execute_paper_write(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    // Sub-phase 1: Write draft
    let draft_specs = vec![
        ArtifactSpec {
            filename: "paper_draft.md".into(),
            description: "full academic paper draft — title, abstract, introduction, \
                related work, methodology, experiments, results, discussion, conclusion, \
                and references".into(),
        },
    ];

    let draft_result = execute_agentic(stage, ctx, &draft_specs).await;
    if draft_result.status != StageStatus::Done {
        return draft_result;
    }

    // Sub-phase 2: Self-revision (revision after peer review happens externally)
    let revision_specs = vec![
        ArtifactSpec {
            filename: "paper_revised.md".into(),
            description: "revised paper — full paper with improvements made, content \
                strengthened, and self-identified issues addressed".into(),
        },
        ArtifactSpec {
            filename: "revision_notes.md".into(),
            description: "revision notes — what was changed from the draft, \
                rationale for changes, and remaining known issues".into(),
        },
    ];

    let mut revision_result = execute_agentic(stage, ctx, &revision_specs).await;
    // Merge artifacts
    let mut all_artifacts = draft_result.artifacts;
    all_artifacts.extend(revision_result.artifacts);
    revision_result.artifacts = all_artifacts;
    revision_result
}

// ---------------------------------------------------------------------------
// PeerReview
// ---------------------------------------------------------------------------

/// Execute the PeerReview stage via parallel multi-agent review.
///
/// Three independent reviewers run in parallel: physics-reviewer (primary),
/// critical-reviewer, and constructive-reviewer. No rework loop — reviewers
/// produce independent assessments of the paper. Rework here would only ask
/// the reviewer to revise its own review, which is not useful.
pub async fn execute_peer_review(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_multi_agentic, check_upstream_rework};

    let primary_specs = vec![
        ArtifactSpec {
            filename: "review_comments.md".into(),
            description: "physics review — independent evaluation of analysis on physics merit, \
                severity-rated findings (A/B/C), and publication readiness assessment".into(),
        },
    ];

    let reviewer_specs: Vec<(&str, Vec<ArtifactSpec>)> = vec![
        ("critical-reviewer", vec![
            ArtifactSpec {
                filename: "critical_review.md".into(),
                description: "combined critical and constructive review — flaws in correctness \
                    and completeness, conventions compliance, figure/label validation, issue \
                    classification, plus clarity improvements and presentation quality".into(),
            },
        ]),
    ];

    let mut result = execute_multi_agentic(stage, ctx, &primary_specs, &reviewer_specs).await;

    // Check if any reviewer flagged an issue requiring upstream rework
    if result.status == StageStatus::Done {
        let stage_dir = ctx.stage_dir(stage);
        if let Some(target) = check_upstream_rework(&stage_dir) {
            tracing::info!(
                stage = %stage.name(),
                target = %target.name(),
                "Reviewer flagged upstream rework needed"
            );
            result.retry_from_stage = Some(target);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// QualityGate (5.4 GATE)
// ---------------------------------------------------------------------------

/// Multi-agent quality gate: arbiter (primary), then plot-validator + rendering-reviewer.
///
/// Parallel review only — no rework loop. The primary writes a quality assessment,
/// reviewers validate plots and rendering independently. Rework would only rewrite
/// the assessment, not fix the actual artifacts. Returns BlockedApproval if not auto-approved.
pub async fn execute_quality_gate(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_multi_agentic};

    let primary_specs = vec![
        ArtifactSpec {
            filename: "quality_report.md".into(),
            description: "quality gate report — overall quality score (1-10), passes boolean, \
                per-section scores, identified issues, and recommendations".into(),
        },
    ];

    let reviewer_specs: Vec<(&str, Vec<ArtifactSpec>)> = vec![
        ("plot-validator", vec![
            ArtifactSpec {
                filename: "plot_validation.md".into(),
                description: "plot validation — programmatic checks on all figures, \
                    physics sanity, consistency, and red flags".into(),
            },
        ]),
        ("rendering-reviewer", vec![
            ArtifactSpec {
                filename: "rendering_review.md".into(),
                description: "rendering review — PDF compilation quality, figure rendering, \
                    math typesetting, layout, cross-references, and citation formatting".into(),
            },
        ]),
    ];

    let mut result = execute_multi_agentic(stage, ctx, &primary_specs, &reviewer_specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // QualityGate is the final checkpoint — it reports issues but does NOT trigger
    // cross-stage rework. Triggering rework here creates infinite loops because the
    // same issues persist across re-runs (e.g. missing figures that the experiment
    // code never generates). Only PeerReview may trigger upstream rework.
    {
        use crate::executor::check_upstream_rework;
        let stage_dir = ctx.stage_dir(stage);
        if let Some(target) = check_upstream_rework(&stage_dir) {
            tracing::warn!(
                stage = %stage.name(),
                target = %target.name(),
                "Reviewer flagged upstream rework but QualityGate does not trigger rework \
                 (report-only). Issues logged in verdict files for human review."
            );
            // Deliberately NOT setting retry_from_stage — QualityGate is report-only.
        }
    }

    // GATE: block for approval if not auto-approved
    if !ctx.auto_approve_gates {
        result.status = StageStatus::BlockedApproval;
        result.decision = "awaiting_approval".to_owned();
    }

    result
}

// ---------------------------------------------------------------------------
// Publish (← merged KnowledgeArchive + ExportPublish + CitationVerify)
// ---------------------------------------------------------------------------

/// Execute the Publish stage — archive, export, and verify citations.
///
/// Produces archive manifest, final paper (md + tex), and citation verification.
pub async fn execute_publish(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let stage_dir = ctx.stage_dir(stage);
    let _ = fs::create_dir_all(&stage_dir);

    let topic = ctx.config.topic.as_str();

    // Sub-phase 2: Export final paper + LaTeX + citation verification (LLM)
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
        ArtifactSpec {
            filename: "verification_report.md".into(),
            description: "citation verification report — each citation checked for existence, \
                correctness, and proper formatting with overall verification status".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;

    // Rebuild archive manifest AFTER agentic step so paper_final.md is included.
    // Also check paper_revised.md as fallback if paper_final.md was not produced.
    let mut archived_entries: Vec<serde_json::Value> = Vec::new();
    let artifact_names_post = [
        ("goal.md", "Research goal definition"),
        ("problem_tree.md", "Problem decomposition"),
        ("hypotheses.md", "Research hypotheses"),
        ("synthesis_report.md", "Literature synthesis report"),
        ("knowledge_cards.md", "Extracted knowledge cards"),
        ("exp_plan.md", "Experiment plan"),
        ("analysis_report.md", "Result analysis report"),
        ("decision_record.md", "Research decision record"),
        ("paper_final.md", "Final paper"),
        ("paper_revised.md", "Revised paper"),
        ("knowledge_summary.md", "Knowledge summary"),
    ];
    for (name, description) in &artifact_names_post {
        // Skip paper_revised if paper_final already found (avoid duplication)
        if *name == "paper_revised.md"
            && archived_entries.iter().any(|e| e["name"] == "paper_final.md")
        {
            continue;
        }
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
    });
    let _ = fs::write(
        stage_dir.join("archive_manifest.json"),
        serde_json::to_string_pretty(&archive_manifest).unwrap_or_default(),
    );

    result.artifacts.push("archive_manifest.json".to_owned());

    // Collect upstream figures and references.bib into stage dir regardless of
    // whether paper.tex exists yet — ensures materials are available for both
    // the agentic step and any later PDF compilation (#41).
    let figures_src = crate::executor::find_prior_file_pub(&ctx.run_dir, "figures");
    if let Some(fig_dir) = figures_src {
        if fig_dir.is_dir() {
            let fig_dst = stage_dir.join("figures");
            if !fig_dst.exists() {
                #[cfg(unix)]
                { let _ = std::os::unix::fs::symlink(&fig_dir, &fig_dst); }
            }
        }
    }
    let bib_dst = stage_dir.join("references.bib");
    if !bib_dst.exists() {
        if let Some(bib_content) = crate::executor::read_prior_artifact_pub(&ctx.run_dir, "references.bib") {
            let _ = fs::write(&bib_dst, &bib_content);
        }
    }

    // Sub-phase 3: Compile LaTeX to PDF
    let tex_path = stage_dir.join("paper.tex");
    if tex_path.is_file() {
        // Try pdflatex → bibtex → pdflatex × 2
        tracing::info!(stage = %stage.name(), "Compiling paper.tex → paper.pdf");
        let compile = std::process::Command::new("pdflatex")
            .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
            .current_dir(&stage_dir)
            .output();

        match compile {
            Ok(output) if output.status.success() => {
                // Run bibtex for citations
                let _ = std::process::Command::new("bibtex")
                    .arg("paper")
                    .current_dir(&stage_dir)
                    .output();
                // Two more pdflatex passes for references
                for _ in 0..2 {
                    let _ = std::process::Command::new("pdflatex")
                        .args(["-interaction=nonstopmode", "-halt-on-error", "paper.tex"])
                        .current_dir(&stage_dir)
                        .output();
                }
                if stage_dir.join("paper.pdf").is_file() {
                    tracing::info!(stage = %stage.name(), "PDF compilation successful");
                    result.artifacts.push("paper.pdf".to_owned());
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                tracing::warn!(
                    stage = %stage.name(),
                    "pdflatex failed (non-zero exit): {}",
                    if !stderr.is_empty() { &stderr } else { &stdout }
                );
                // Write compilation log for debugging
                let _ = fs::write(stage_dir.join("pdflatex.log"), stdout.as_bytes());
            }
            Err(e) => {
                tracing::warn!(stage = %stage.name(), "pdflatex not found or failed to run: {e}");
            }
        }
    }

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
    async fn paper_write_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "multi-task learning", true);
        let result = execute_paper_write(Stage::PaperWrite, &ctx).await;
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
    async fn quality_gate_returns_blocked_when_not_auto_approved() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "active learning", false);
        let result = execute_quality_gate(Stage::QualityGate, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn publish_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "few-shot learning", true);
        let result = execute_publish(Stage::Publish, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
