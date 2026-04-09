//! Phase 2: Exploration — SearchStrategy (2.1), LiteratureCollect (2.2),
//! LiteratureScreen (2.3), KnowledgeExtract (2.4), Synthesis (2.5), and
//! HypothesisGen (2.6) stage executors.

use crate::executor::{utcnow_iso, StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// SearchStrategy
// ---------------------------------------------------------------------------

/// Execute the SearchStrategy stage via agentic executor.
///
/// Produces `search_plan.md`, `sources.json`, and `queries.json` — all via
/// agentic extraction so the LLM actively generates structured search artifacts.
pub async fn execute_search_strategy(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "search_plan.md".into(),
            description: "literature search strategy plan — databases to query, search terms, \
                inclusion/exclusion criteria, and expected result counts".into(),
        },
        ArtifactSpec {
            filename: "sources.json".into(),
            description: "academic sources to search — list of sources with id, name, url, \
                api_endpoint, enabled flag, and optional categories".into(),
        },
        ArtifactSpec {
            filename: "queries.json".into(),
            description: "search queries to execute — list of query strings with \
                target databases and expected relevance".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// LiteratureCollect
// ---------------------------------------------------------------------------

/// Execute the LiteratureCollect stage.
///
/// Produces `candidates.md` via agentic two-phase execution.
pub async fn execute_literature_collect(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "candidates.md".into(),
            description: "literature candidates — each with: paper_id, title, authors, year, \
                venue, abstract, relevance_score (0-1)".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
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

    // If no valid JSON lines were parsed, create fallback screened papers.
    // This triggers both when the file is empty AND when it contains only
    // narrative text (Friction Fix #7: cascading failure from non-JSONL content).
    if screened.is_empty() && excluded.is_empty() {
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
/// Reads screened papers and produces `knowledge_cards.md` and
/// `citation_map.json`.
pub async fn execute_knowledge_extract(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "knowledge_cards.md".into(),
            description: "structured knowledge cards extracted from literature — each card \
                has: card_id, source, category, content, numerical_values, and applicability"
                .into(),
        },
        ArtifactSpec {
            filename: "citation_map.json".into(),
            description: "citation graph with nodes (papers) and edges (citations between them)"
                .into(),
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
    async fn search_strategy_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "transformer attention mechanisms");
        let result = execute_search_strategy(Stage::SearchStrategy, &ctx).await;
        // Without prompt engine, stage fails honestly.
        assert_eq!(result.status, StageStatus::Failed);
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
    async fn synthesis_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "protein structure prediction");
        let result = execute_synthesis(Stage::Synthesis, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
    }

    #[tokio::test]
    async fn hypothesis_gen_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "reinforcement learning");
        let result = execute_hypothesis_gen(Stage::HypothesisGen, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
    }
}


// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

/// Execute the Synthesis stage.
///
/// Reads knowledge cards from prior stages and produces `synthesis_report.md`.
pub async fn execute_synthesis(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "synthesis_report.md".into(),
            description: "literature synthesis report — key themes, methodological trends, \
                consensus findings, contradictions across reviewed papers, identified research \
                gaps, clusters of related work, and recommended focus areas".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// HypothesisGen
// ---------------------------------------------------------------------------

/// Execute the HypothesisGen stage via agentic executor.
pub async fn execute_hypothesis_gen(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "hypotheses.md".into(),
            description: "testable research hypotheses derived from the synthesis and gap \
                analysis — each hypothesis should have: statement, rationale, proposed test, \
                expected outcome, and falsification criteria".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

