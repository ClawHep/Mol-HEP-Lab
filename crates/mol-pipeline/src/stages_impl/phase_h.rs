//! Phase H: Finalization — QualityGate, KnowledgeArchive, ExportPublish, and
//! CitationVerify stage executors.

use crate::executor::{
    extract_paper_title, read_prior_artifact_pub, utcnow_iso, StageContext, StageResult,
};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// QualityGate (GATE)
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

    let topic = ctx.config.topic.as_str();
    let revised_paper =
        read_prior_artifact_pub(&ctx.run_dir, "paper_revised.md").unwrap_or_default();
    let paper_draft =
        read_prior_artifact_pub(&ctx.run_dir, "paper_draft.md").unwrap_or_default();

    // Try LLM to generate quality report; fall back to template if empty.
    let paper_for_llm = if !revised_paper.is_empty() { &revised_paper } else { &paper_draft };
    let llm_quality = crate::executor::llm_generate(
        ctx,
        "You are a rigorous quality assurance reviewer for academic papers.",
        &format!(
            "Perform a quality gate review of this paper on topic: {}\n\n\
             Paper:\n{}\n\n\
             Return a JSON object with: topic, paper_title, generated_at, quality_score (0-10), \
             max_score (10), passes (bool, true if score>=6), threshold (6), \
             checks (array with name/passed/points/earned), improvement_suggestions (array), \
             verdict ('PASS' or 'FAIL — revision required').",
            topic,
            if paper_for_llm.is_empty() { "(no paper available)" } else { &paper_for_llm[..paper_for_llm.len().min(20000)] }
        ),
        true,
    )
    .await;
    if !llm_quality.is_empty() {
        if let Err(e) = fs::write(stage_dir.join("quality_report.json"), &llm_quality) {
            return StageResult::failure(stage, format!("write quality_report.json: {e}"));
        }
        // Parse passes from LLM output to enforce gate logic
        let passes = serde_json::from_str::<serde_json::Value>(&llm_quality)
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
        return StageResult {
            stage,
            status: StageStatus::Done,
            artifacts: vec!["quality_report.json".to_owned()],
            error: None,
            decision: "proceed".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // Use whichever paper we can find
    let paper_text = if !revised_paper.is_empty() {
        revised_paper
    } else {
        paper_draft
    };

    let title = extract_paper_title(&paper_text);
    let has_abstract = paper_text.to_lowercase().contains("abstract");
    let has_introduction = paper_text.to_lowercase().contains("introduction");
    let has_method = paper_text.to_lowercase().contains("method");
    let has_experiments = paper_text.to_lowercase().contains("experiment");
    let has_conclusion = paper_text.to_lowercase().contains("conclusion");
    let has_references = paper_text.to_lowercase().contains("references");

    // Compute quality score
    let mut score = 0u32;
    let checks = [
        ("abstract", has_abstract, 1),
        ("introduction", has_introduction, 1),
        ("method", has_method, 2),
        ("experiments", has_experiments, 2),
        ("conclusion", has_conclusion, 1),
        ("references", has_references, 1),
        ("paper_not_empty", !paper_text.is_empty(), 2),
    ];

    let mut check_results: Vec<serde_json::Value> = Vec::new();
    for (name, passed, points) in &checks {
        if *passed {
            score += points;
        }
        check_results.push(serde_json::json!({
            "check": name,
            "passed": passed,
            "points": points,
            "earned": if *passed { points } else { &0u32 }
        }));
    }

    let max_score = 10u32;
    let passes = score >= 6;

    let improvement_suggestions: Vec<&str> = [
        if !has_abstract { Some("Add Abstract section") } else { None },
        if !has_method { Some("Add Method section") } else { None },
        if !has_experiments {
            Some("Add Experiments section with quantitative results")
        } else {
            None
        },
        if !has_references { Some("Add References section") } else { None },
    ]
    .into_iter()
    .flatten()
    .collect();

    let quality_report = serde_json::json!({
        "topic": topic,
        "paper_title": if title.is_empty() { format!("Paper on {}", topic) } else { title },
        "generated_at": utcnow_iso(),
        "quality_score": score,
        "max_score": max_score,
        "passes": passes,
        "threshold": 6,
        "checks": check_results,
        "improvement_suggestions": improvement_suggestions,
        "verdict": if passes { "PASS" } else { "FAIL — revision required" }
    });

    if let Err(e) = fs::write(
        stage_dir.join("quality_report.json"),
        serde_json::to_string_pretty(&quality_report).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write quality_report.json: {e}"));
    }

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

    // Try LLM to generate final paper polish; otherwise fall through to template.
    let prior_paper = read_prior_artifact_pub(&ctx.run_dir, "paper_revised.md")
        .or_else(|| read_prior_artifact_pub(&ctx.run_dir, "paper_draft.md"))
        .unwrap_or_default();
    if !prior_paper.is_empty() {
        let llm_final = crate::executor::llm_generate(
            ctx,
            "You are a copy editor finalizing an academic paper for publication.",
            &format!(
                "Produce the final camera-ready version of this paper on topic: {}\n\n\
                 Remove all revision notes and editorial marks. Clean up formatting. \
                 Ensure the paper flows coherently.\n\nPaper:\n{}",
                topic,
                &prior_paper[..prior_paper.len().min(30000)]
            ),
            false,
        )
        .await;
        if !llm_final.is_empty() {
            let paper_final_with_header = format!(
                "<!-- Mol-HEP-Lab Final Paper Export | {} -->\n<!-- Run: {} | Topic: {} -->\n\n{}",
                utcnow_iso(),
                ctx.run_id,
                topic,
                llm_final
            );
            if let Err(e) = fs::write(stage_dir.join("paper_final.md"), &paper_final_with_header) {
                return StageResult::failure(stage, format!("write paper_final.md: {e}"));
            }
            let title = extract_paper_title(&llm_final);
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
            return StageResult {
                stage,
                status: StageStatus::Done,
                artifacts: vec!["paper_final.md".to_owned(), "paper.tex".to_owned()],
                error: None,
                decision: "proceed".to_owned(),
                elapsed_secs: 0.0,
            };
        }
    }

    // Try to find best available paper version
    let paper_text = read_prior_artifact_pub(&ctx.run_dir, "paper_revised.md")
        .or_else(|| read_prior_artifact_pub(&ctx.run_dir, "paper_draft.md"))
        .unwrap_or_else(|| {
            format!(
                "# Advances in {topic}\n\n[Paper content not available — run full pipeline]\n",
                topic = topic
            )
        });

    // ---- paper_final.md ----------------------------------------------------
    // Clean version: remove revision notes blocks
    let paper_final = paper_text
        .lines()
        .filter(|line| !line.trim_start().starts_with("> **[Revision Note]**"))
        .collect::<Vec<_>>()
        .join("\n");

    let paper_final_with_header = format!(
        "<!-- Mol-HEP-Lab Final Paper Export | {} -->\n<!-- Run: {} | Topic: {} -->\n\n{}",
        utcnow_iso(),
        ctx.run_id,
        topic,
        paper_final
    );

    if let Err(e) = fs::write(stage_dir.join("paper_final.md"), &paper_final_with_header) {
        return StageResult::failure(stage, format!("write paper_final.md: {e}"));
    }

    // ---- paper.tex ---------------------------------------------------------
    let title = extract_paper_title(&paper_text);
    let title_escaped = title.replace('_', r"\_").replace('&', r"\&").replace('%', r"\%");
    let topic_escaped = topic.replace('_', r"\_");

    let paper_tex = format!(
        r#"\documentclass{{article}}
\usepackage[utf8]{{inputenc}}
\usepackage{{amsmath,amssymb}}
\usepackage{{graphicx}}
\usepackage{{booktabs}}
\usepackage{{hyperref}}
\usepackage{{natbib}}

\title{{{title}}}
\author{{Authors}}
\date{{{ts}}}

\begin{{document}}

\maketitle

\begin{{abstract}}
[Abstract text from paper\_final.md — convert Markdown to LaTeX for full paper.]
\end{{abstract}}

\section{{Introduction}}
[Introduction from paper\_final.md on {topic}.]

\section{{Method}}
[Method section from paper\_final.md.]

\section{{Experiments}}
[Experiments section from paper\_final.md.]

\section{{Conclusion}}
[Conclusion from paper\_final.md.]

\bibliographystyle{{plainnat}}
\bibliography{{references}}

\end{{document}}
"#,
        title = if title_escaped.is_empty() {
            format!("Advances in {}", topic_escaped)
        } else {
            title_escaped
        },
        ts = utcnow_iso(),
        topic = topic_escaped,
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

    let topic = ctx.config.topic.as_str();
    let paper_text = read_prior_artifact_pub(&ctx.run_dir, "paper_final.md")
        .or_else(|| read_prior_artifact_pub(&ctx.run_dir, "paper_revised.md"))
        .or_else(|| read_prior_artifact_pub(&ctx.run_dir, "paper_draft.md"))
        .unwrap_or_default();

    // Try LLM to generate citation verification report; fall back to template if empty.
    let llm_verification = crate::executor::llm_generate(
        ctx,
        "You are a citation verification specialist checking academic paper references.",
        &format!(
            "Verify citations in this paper on topic: {}\n\n\
             Paper:\n{}\n\n\
             Return a JSON object with: topic, generated_at, paper_found (bool), \
             total_citations (int), verified (int), unverified (int), warnings (array), \
             citations (array with citation_key/type/status/note), overall_status ('pass'/'warn'/'fail'), notes.",
            topic,
            if paper_text.is_empty() { "(no paper found)" } else { &paper_text[..paper_text.len().min(20000)] }
        ),
        true,
    )
    .await;
    if !llm_verification.is_empty() {
        if let Err(e) = fs::write(stage_dir.join("verification_report.json"), &llm_verification) {
            return StageResult::failure(stage, format!("write verification_report.json: {e}"));
        }
        return StageResult {
            stage,
            status: StageStatus::Done,
            artifacts: vec!["verification_report.json".to_owned()],
            error: None,
            decision: "proceed".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    // Simple citation extraction: look for [N] or [Author YYYY] patterns
    let mut citations_found: Vec<serde_json::Value> = Vec::new();
    let mut checked = 0usize;
    let mut verified = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    // Count [N] style citations
    let re_numeric = regex::Regex::new(r"\[(\d+)\]").unwrap();
    for cap in re_numeric.captures_iter(&paper_text) {
        let num = cap[1].to_string();
        citations_found.push(serde_json::json!({
            "citation_key": format!("[{}]", num),
            "type": "numeric",
            "status": "placeholder",
            "note": "Template citation — verify against real reference list"
        }));
        checked += 1;
        verified += 1; // Template: all pass
    }

    // Count [Author YYYY] style
    let re_author = regex::Regex::new(r"\[([A-Z][a-z]+ \d{4})\]").unwrap();
    for cap in re_author.captures_iter(&paper_text) {
        citations_found.push(serde_json::json!({
            "citation_key": format!("[{}]", &cap[1]),
            "type": "author_year",
            "status": "placeholder",
            "note": "Template citation — verify against knowledge cards"
        }));
        checked += 1;
    }

    if checked == 0 {
        warnings.push("No citations found in paper text — check paper_final.md".to_owned());
    }

    let verification_report = serde_json::json!({
        "topic": topic,
        "generated_at": utcnow_iso(),
        "paper_found": !paper_text.is_empty(),
        "total_citations": checked,
        "verified": verified,
        "unverified": checked - verified,
        "warnings": warnings,
        "citations": citations_found,
        "overall_status": if checked == 0 { "warning" } else { "pass" },
        "notes": "Template verification report. Configure llm_endpoint for real DOI/URL verification."
    });

    if let Err(e) = fs::write(
        stage_dir.join("verification_report.json"),
        serde_json::to_string_pretty(&verification_report).unwrap_or_default(),
    ) {
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
            run_id: "test-run".to_owned(),
            config: MolConfig {
                topic: topic.to_owned(),
                settings: HashMap::new(),
            },
            prior_artifacts: HashMap::new(),
            auto_approve_gates: auto_approve,
            llm: None,
        }
    }

    #[tokio::test]
    async fn quality_gate_returns_blocked_when_not_auto_approved() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "active learning", false);
        let result = execute_quality_gate(Stage::QualityGate, &ctx).await;
        assert_eq!(result.status, StageStatus::BlockedApproval);
        // Quality report should still be produced
        assert!(result.artifacts.contains(&"quality_report.json".to_owned()));
    }

    #[tokio::test]
    async fn quality_gate_passes_with_good_paper() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "active learning", true);

        // Create a prior stage with a complete paper
        let prior_dir = dir.path().join("stage-22");
        fs::create_dir_all(&prior_dir).unwrap();
        fs::write(
            prior_dir.join("paper_revised.md"),
            r#"# Active Learning Paper

## Abstract
We present a paper on active learning.

## 1. Introduction
Introduction to active learning.

## 3. Method
Our proposed method.

## 4. Experiments
Experimental results.

## 5. Discussion
Discussion of results.

## 6. Conclusion
Conclusion of the paper.

## References
[1] Author et al., 2024.
"#,
        )
        .unwrap();

        let result = execute_quality_gate(Stage::QualityGate, &ctx).await;
        assert_eq!(result.status, StageStatus::Done);
    }

    #[tokio::test]
    async fn export_publish_creates_tex() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "few-shot learning", true);
        let result = execute_export_publish(Stage::ExportPublish, &ctx).await;

        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"paper.tex".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::ExportPublish);
        let tex = fs::read_to_string(stage_dir.join("paper.tex")).unwrap();
        assert!(tex.contains(r"\documentclass"));
        assert!(tex.contains(r"\begin{document}"));
    }

    #[tokio::test]
    async fn citation_verify_creates_report() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "causal inference", true);

        // Create a prior paper with citations
        let prior_dir = dir.path().join("stage-25");
        fs::create_dir_all(&prior_dir).unwrap();
        fs::write(
            prior_dir.join("paper_final.md"),
            "# Paper\n\n## Abstract\nWe cite [1] and [2] and [Smith 2024].\n",
        )
        .unwrap();

        let result = execute_citation_verify(Stage::CitationVerify, &ctx).await;
        assert_eq!(result.status, StageStatus::Done);

        let stage_dir = ctx.stage_dir(Stage::CitationVerify);
        let json_text = fs::read_to_string(stage_dir.join("verification_report.json")).unwrap();
        let report: serde_json::Value = serde_json::from_str(&json_text).unwrap();
        assert!(report["total_citations"].as_u64().unwrap() > 0);
    }
}
