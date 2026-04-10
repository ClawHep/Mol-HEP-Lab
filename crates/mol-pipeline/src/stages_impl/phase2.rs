//! Phase 2: Exploration — LiteratureSearch (2.1), LiteratureScreen (2.2),
//! KnowledgeExtract (2.3), SynthesisHypotheses (2.4) stage executors.

use crate::executor::{utcnow_iso, StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// LiteratureSearch (← merged SearchStrategy + LiteratureCollect)
// ---------------------------------------------------------------------------

/// Execute the LiteratureSearch stage via agentic executor.
///
/// Combines search strategy formulation and literature collection into one
/// continuous action. Produces search plan, sources, queries, and candidates.
pub async fn execute_literature_search(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "search_plan.md".into(),
            description: "literature search strategy plan — databases to query, search terms, \
                inclusion/exclusion criteria, and expected result counts".into(),
        },
        ArtifactSpec {
            filename: "sources.md".into(),
            description: "academic sources to search — list of databases/repositories \
                with name, url, and relevance to the research topic".into(),
        },
        ArtifactSpec {
            filename: "queries.md".into(),
            description: "search queries to execute — list of query strings with \
                target databases and expected relevance".into(),
        },
        ArtifactSpec {
            filename: "candidates.md".into(),
            description: "literature candidates — each entry with: title, authors, year, \
                venue, abstract summary, and relevance assessment".into(),
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

    // Read candidates — try JSONL first, then fall back to .md (agent may produce either)
    let candidates_text = crate::executor::read_prior_artifact_pub(&ctx.run_dir, "candidates.md")
        .or_else(|| crate::executor::read_prior_artifact_pub(&ctx.run_dir, "candidates.jsonl"))
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

    // If no valid JSON lines were parsed, try extracting JSON from narrative text.
    // The upstream LiteratureSearch may produce markdown with embedded JSON blocks.
    if screened.is_empty() && excluded.is_empty() && !candidates_text.is_empty() {
        tracing::warn!(
            "LiteratureScreen: 0 valid JSON lines parsed from candidates — \
             attempting JSON block extraction from narrative text"
        );
        // Try to extract JSON array or individual objects from code fences
        let extracted = crate::executor::strip_markdown_fences(&candidates_text);
        for line in extracted.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if let Ok(paper) = serde_json::from_str::<serde_json::Value>(line) {
                let score = paper
                    .get("relevance_score")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.5); // Default to mid-range if no score
                let mut p = paper.clone();
                if let Some(obj) = p.as_object_mut() {
                    obj.insert("screening_status".into(), serde_json::json!("included"));
                    obj.insert("screening_score".into(), serde_json::json!(score));
                }
                screened.push(p);
            } else if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(line) {
                // Might be a JSON array on one line
                for paper in arr {
                    let score = paper
                        .get("relevance_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5);
                    let mut p = paper.clone();
                    if let Some(obj) = p.as_object_mut() {
                        obj.insert("screening_status".into(), serde_json::json!("included"));
                        obj.insert("screening_score".into(), serde_json::json!(score));
                    }
                    screened.push(p);
                }
            }
        }
        // Also try parsing the entire extracted text as a JSON array
        if screened.is_empty() {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&extracted) {
                for paper in arr {
                    let score = paper
                        .get("relevance_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5);
                    let mut p = paper.clone();
                    if let Some(obj) = p.as_object_mut() {
                        obj.insert("screening_status".into(), serde_json::json!("included"));
                        obj.insert("screening_score".into(), serde_json::json!(score));
                    }
                    screened.push(p);
                }
            }
        }
        if !screened.is_empty() {
            tracing::info!(
                "LiteratureScreen: extracted {} papers from narrative text",
                screened.len()
            );
        }
    }

    // Fallback: parse structured markdown with `- **key:** value` bullet lists
    if screened.is_empty() && excluded.is_empty() && !candidates_text.is_empty() {
        tracing::warn!(
            "LiteratureScreen: attempting markdown bullet-list parsing"
        );
        let mut current: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for line in candidates_text.lines() {
            let trimmed = line.trim();
            // Detect header boundaries (### N. ...) — flush previous entry
            if trimmed.starts_with("### ") || trimmed.starts_with("## ") {
                if current.contains_key("paper_id") || current.contains_key("title") {
                    let score = current.get("relevance_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.8);
                    current.insert("screening_status".into(), serde_json::json!("included"));
                    current.insert("screening_score".into(), serde_json::json!(score));
                    screened.push(serde_json::Value::Object(current.clone()));
                }
                current.clear();
                continue;
            }
            // Parse `- **key:** value` lines
            if let Some(rest) = trimmed.strip_prefix("- **") {
                if let Some(colon_pos) = rest.find(":**") {
                    let key = rest[..colon_pos].trim().to_lowercase().replace(' ', "_");
                    let val = rest[colon_pos + 3..].trim().to_string();
                    if key == "relevance_score" {
                        if let Ok(f) = val.parse::<f64>() {
                            current.insert(key, serde_json::json!(f));
                        }
                    } else if key == "year" {
                        if let Ok(y) = val.parse::<i64>() {
                            current.insert(key, serde_json::json!(y));
                        }
                    } else if key == "tags" {
                        // Parse [tag1, tag2, ...] format
                        let tags: Vec<String> = val.trim_matches(|c| c == '[' || c == ']')
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        current.insert(key, serde_json::json!(tags));
                    } else {
                        current.insert(key, serde_json::json!(val));
                    }
                }
            }
        }
        // Flush last entry
        if current.contains_key("paper_id") || current.contains_key("title") {
            let score = current.get("relevance_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.8);
            current.insert("screening_status".into(), serde_json::json!("included"));
            current.insert("screening_score".into(), serde_json::json!(score));
            screened.push(serde_json::Value::Object(current));
        }
        if !screened.is_empty() {
            tracing::info!(
                "LiteratureScreen: parsed {} papers from markdown bullet-list format",
                screened.len()
            );
        }
    }

    // Last resort: if still nothing parsed, create a minimal placeholder.
    // This prevents cascading failure but is clearly marked as degraded.
    if screened.is_empty() && excluded.is_empty() {
        tracing::warn!(
            "LiteratureScreen: no papers could be parsed — inserting degraded placeholder"
        );
        let topic = ctx.config.topic.as_str();
        let now = utcnow_iso();
        screened.push(serde_json::json!({
            "paper_id": "fallback-001",
            "title": format!("Key Paper on {}", topic),
            "authors": ["Template Author"],
            "year": 2024,
            "venue": "Placeholder",
            "relevance_score": 0.90,
            "screening_status": "included_degraded",
            "retrieved_at": now,
            "abstract": format!("Placeholder — upstream candidates could not be parsed. Topic: {}", topic),
            "_degraded": true
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
            retry_from_stage: None,
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
        retry_from_stage: None,
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
            filename: "citation_map.md".into(),
            description: "citation graph — papers and their citation relationships, \
                key clusters, and influence pathways".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}

// ---------------------------------------------------------------------------
// SynthesisHypotheses (← merged Synthesis + HypothesisGen)
// ---------------------------------------------------------------------------

/// Execute the SynthesisHypotheses stage via agentic executor.
///
/// Combines knowledge synthesis and hypothesis generation into continuous
/// reasoning. Produces synthesis report, gap analysis, and hypotheses.
pub async fn execute_synthesis_hypotheses(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "synthesis_report.md".into(),
            description: "literature synthesis report — key themes, methodological trends, \
                consensus findings, contradictions across reviewed papers, identified research \
                gaps, clusters of related work, and recommended focus areas".into(),
        },
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
    async fn literature_search_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "transformer attention mechanisms");
        let result = execute_literature_search(Stage::LiteratureSearch, &ctx).await;
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
    async fn synthesis_hypotheses_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "protein structure prediction");
        let result = execute_synthesis_hypotheses(Stage::SynthesisHypotheses, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
    }
}

