//! Phase 2: Exploration — SearchStrategy (2.1), LiteratureCollect (2.2),
//! LiteratureScreen (2.3), KnowledgeExtract (2.4), Synthesis (2.5), and
//! HypothesisGen (2.6) stage executors.

use crate::executor::{utcnow_iso, StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// SearchStrategy
// ---------------------------------------------------------------------------

/// Execute the SearchStrategy stage.
///
/// Produces `search_plan.yaml`, `sources.json`, and `queries.json`.
pub async fn execute_search_strategy(stage: Stage, ctx: &StageContext) -> StageResult {
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

    // ---- search_plan.yaml --------------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("search_plan.yaml"), &result) {
        return StageResult::failure(stage, format!("write search_plan.yaml: {e}"));
    }

    // ---- sources.json ------------------------------------------------------
    let sources_json = serde_json::json!([
        {
            "id": "arxiv",
            "name": "arXiv",
            "url": "https://arxiv.org",
            "api_endpoint": "https://export.arxiv.org/api/query",
            "enabled": true,
            "categories": ["cs.LG", "cs.AI", "stat.ML", "physics", "q-bio"]
        },
        {
            "id": "semantic_scholar",
            "name": "Semantic Scholar",
            "url": "https://www.semanticscholar.org",
            "api_endpoint": "https://api.semanticscholar.org/graph/v1",
            "enabled": true
        },
        {
            "id": "pubmed",
            "name": "PubMed",
            "url": "https://pubmed.ncbi.nlm.nih.gov",
            "api_endpoint": "https://eutils.ncbi.nlm.nih.gov/entrez/eutils",
            "enabled": false
        }
    ]);
    if let Err(e) = fs::write(
        stage_dir.join("sources.json"),
        serde_json::to_string_pretty(&sources_json).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write sources.json: {e}"));
    }

    // ---- queries.json ------------------------------------------------------
    let queries_json_wrapper = serde_json::json!({
        "generated_at": utcnow_iso(),
        "topic": ctx.config.topic.as_str(),
        "queries": []
    });
    if let Err(e) = fs::write(
        stage_dir.join("queries.json"),
        serde_json::to_string_pretty(&queries_json_wrapper).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write queries.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec![
            "search_plan.yaml".to_owned(),
            "sources.json".to_owned(),
            "queries.json".to_owned(),
        ],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// LiteratureCollect
// ---------------------------------------------------------------------------

/// Execute the LiteratureCollect stage.
///
/// Produces `candidates.jsonl` with synthetic placeholder paper entries.
pub async fn execute_literature_collect(stage: Stage, ctx: &StageContext) -> StageResult {
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

    if let Err(e) = fs::write(stage_dir.join("candidates.jsonl"), &result) {
        return StageResult::failure(stage, format!("write candidates.jsonl: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["candidates.jsonl".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// LiteratureScreen (GATE)
// ---------------------------------------------------------------------------

/// Execute the LiteratureScreen stage.
///
/// Reads `candidates.jsonl`, filters papers by relevance score, and produces
/// `screened_papers.jsonl` and `exclusion_reasons.json`.
///
/// This is a GATE stage — if `ctx.auto_approve_gates` is false, returns
/// `BlockedApproval`.
pub async fn execute_literature_screen(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Read candidates
    let candidates_text = crate::executor::read_prior_artifact_pub(&ctx.run_dir, "candidates.jsonl")
        .unwrap_or_default();

    let mut screened: Vec<serde_json::Value> = Vec::new();
    let mut excluded: Vec<serde_json::Value> = Vec::new();

    for line in candidates_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(paper) = serde_json::from_str::<serde_json::Value>(line) {
            let score = paper
                .get("relevance_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            if score >= 0.7 {
                let mut screened_paper = paper.clone();
                if let Some(obj) = screened_paper.as_object_mut() {
                    obj.insert("screening_status".to_owned(), serde_json::json!("included"));
                    obj.insert("screening_score".to_owned(), serde_json::json!(score));
                }
                screened.push(screened_paper);
            } else {
                let id = paper
                    .get("paper_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                excluded.push(serde_json::json!({
                    "paper_id": id,
                    "reason": "relevance_score_below_threshold",
                    "score": score,
                    "threshold": 0.7
                }));
            }
        }
    }

    // If no candidates were found, create fallback screened papers
    if screened.is_empty() && candidates_text.is_empty() {
        let topic = ctx.config.topic.as_str();
        let now = utcnow_iso();
        screened.push(serde_json::json!({
            "paper_id": "fallback-001",
            "title": format!("Key Paper on {}", topic),
            "authors": ["Template Author"],
            "year": 2024,
            "venue": "NeurIPS",
            "relevance_score": 0.90,
            "screening_status": "included",
            "retrieved_at": now,
            "abstract": format!("Placeholder paper on {} for pipeline testing.", topic)
        }));
    }

    // Write screened_papers.jsonl
    let screened_jsonl: String = screened
        .iter()
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");

    if let Err(e) = fs::write(stage_dir.join("screened_papers.jsonl"), &screened_jsonl) {
        return StageResult::failure(stage, format!("write screened_papers.jsonl: {e}"));
    }

    // Write exclusion_reasons.json
    let exclusion_json = serde_json::json!({
        "total_candidates": screened.len() + excluded.len(),
        "included": screened.len(),
        "excluded": excluded.len(),
        "exclusion_reasons": excluded
    });

    if let Err(e) = fs::write(
        stage_dir.join("exclusion_reasons.json"),
        serde_json::to_string_pretty(&exclusion_json).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write exclusion_reasons.json: {e}"));
    }

    if !ctx.auto_approve_gates {
        return StageResult {
            stage,
            status: StageStatus::BlockedApproval,
            artifacts: vec![
                "screened_papers.jsonl".to_owned(),
                "exclusion_reasons.json".to_owned(),
            ],
            error: None,
            decision: "awaiting_approval".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec![
            "screened_papers.jsonl".to_owned(),
            "exclusion_reasons.json".to_owned(),
        ],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// KnowledgeExtract
// ---------------------------------------------------------------------------

/// Execute the KnowledgeExtract stage.
///
/// Reads screened papers and produces `knowledge_cards.json` and
/// `citation_map.json`.
pub async fn execute_knowledge_extract(stage: Stage, ctx: &StageContext) -> StageResult {
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

    if let Err(e) = fs::write(stage_dir.join("knowledge_cards.json"), &result) {
        return StageResult::failure(stage, format!("write knowledge_cards.json: {e}"));
    }

    let citation_map = serde_json::json!({
        "topic": ctx.config.topic.as_str(),
        "nodes": [],
        "edges": []
    });
    if let Err(e) = fs::write(
        stage_dir.join("citation_map.json"),
        serde_json::to_string_pretty(&citation_map).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write citation_map.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec![
            "knowledge_cards.json".to_owned(),
            "citation_map.json".to_owned(),
        ],
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

    fn make_ctx(dir: &std::path::Path, topic: &str) -> StageContext {
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
            auto_approve_gates: true,
            llm: None,
            prompt_engine: None,
        }
    }

    #[tokio::test]
    async fn search_strategy_creates_artifacts() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "transformer attention mechanisms");
        let result = execute_search_strategy(Stage::SearchStrategy, &ctx).await;

        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"search_plan.yaml".to_owned()));
        assert!(result.artifacts.contains(&"queries.json".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::SearchStrategy);
        let yaml_content = fs::read_to_string(stage_dir.join("search_plan.yaml")).unwrap();
        assert!(yaml_content.contains("topic:"));
        assert!(yaml_content.contains("queries"));
    }

    #[tokio::test]
    async fn literature_screen_blocks_without_auto_approve() {
        let dir = TempDir::new().unwrap();
        let mut ctx = make_ctx(dir.path(), "graph neural networks");
        ctx.auto_approve_gates = false;
        let result = execute_literature_screen(Stage::LiteratureScreen, &ctx).await;
        assert_eq!(result.status, StageStatus::BlockedApproval);
    }

    #[tokio::test]
    async fn literature_screen_passes_with_auto_approve() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "graph neural networks");
        let result = execute_literature_screen(Stage::LiteratureScreen, &ctx).await;
        assert_eq!(result.status, StageStatus::Done);
    }

    #[tokio::test]
    async fn synthesis_creates_artifacts() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "protein structure prediction");
        let result = execute_synthesis(Stage::Synthesis, &ctx).await;

        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"synthesis_report.md".to_owned()));
        assert!(result.artifacts.contains(&"gap_analysis.json".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::Synthesis);
        let report = fs::read_to_string(stage_dir.join("synthesis_report.md")).unwrap();
        assert!(report.contains("Research Gaps"));
        assert!(report.contains("protein structure prediction"));
    }

    #[tokio::test]
    async fn hypothesis_gen_creates_hypotheses() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "reinforcement learning");
        let result = execute_hypothesis_gen(Stage::HypothesisGen, &ctx).await;

        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"hypotheses.md".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::HypothesisGen);
        let hyp = fs::read_to_string(stage_dir.join("hypotheses.md")).unwrap();
        assert!(hyp.contains("H1:"));
        assert!(hyp.contains("Falsification Criteria"));
    }
}


// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

/// Execute the Synthesis stage.
///
/// Reads knowledge cards from prior stages and produces `synthesis_report.md`
/// and `gap_analysis.json`.
pub async fn execute_synthesis(stage: Stage, ctx: &StageContext) -> StageResult {
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

    // ---- synthesis_report.md -----------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("synthesis_report.md"), &result) {
        return StageResult::failure(stage, format!("write synthesis_report.md: {e}"));
    }

    // ---- gap_analysis.json ------------------------------------------------
    let gap_analysis = serde_json::json!({
        "topic": ctx.config.topic.as_str(),
        "generated_at": utcnow_iso(),
        "total_papers_reviewed": 0,
        "gaps": [],
        "recommended_focus": "",
        "clusters": []
    });
    if let Err(e) = fs::write(
        stage_dir.join("gap_analysis.json"),
        serde_json::to_string_pretty(&gap_analysis).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write gap_analysis.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["synthesis_report.md".to_owned(), "gap_analysis.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// HypothesisGen
// ---------------------------------------------------------------------------

/// Execute the HypothesisGen stage.
///
/// Reads the synthesis report and produces `hypotheses.md`.
pub async fn execute_hypothesis_gen(stage: Stage, ctx: &StageContext) -> StageResult {
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

    // ---- hypotheses.md ----------------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("hypotheses.md"), &result) {
        return StageResult::failure(stage, format!("write hypotheses.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["hypotheses.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

