//! Novelty assessment for proposed research hypotheses.
//!
//! Searches real academic APIs for papers that may overlap with proposed
//! research and produces a structured report with similarity scores and a
//! `go / differentiate / abort` recommendation.
//!
//! Public API
//! ----------
//! - [`check_novelty`] – primary entry point; returns a [`NoveltyReport`]

use std::collections::HashSet;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::models::Paper;
use crate::search::SearchOptions;

// ---------------------------------------------------------------------------
// Stop-words (excluded from keyword extraction)
// ---------------------------------------------------------------------------

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "of", "for", "to",
    "with", "by", "at", "from", "as", "is", "are", "was", "were", "be",
    "been", "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "can", "shall", "not", "no",
    "nor", "so", "yet", "both", "each", "every", "all", "any", "few", "more",
    "most", "other", "some", "such", "than", "too", "very", "just", "about",
    "above", "after", "again", "between", "into", "through", "during",
    "before", "under", "over", "using", "based", "via", "toward", "towards",
    "new", "novel", "approach", "method", "study", "research", "paper",
    "work", "propose", "proposed", "show", "results", "performance",
    "evaluation",
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Assessment of how novel a research proposal is relative to existing work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoveltyReport {
    pub topic: String,
    pub hypotheses_checked: usize,
    pub search_queries: Vec<String>,
    pub similar_papers_found: usize,
    /// 0.0 (not novel) – 1.0 (highly novel).
    pub novelty_score: f64,
    /// `"high"` | `"moderate"` | `"low"` | `"critical"` | `"insufficient_data"`
    pub assessment: String,
    pub similar_papers: Vec<SimilarPaper>,
    /// `"proceed"` | `"differentiate"` | `"abort"` | `"proceed_with_caution"`
    pub recommendation: String,
    pub similarity_threshold: f64,
    pub search_coverage: String,
    pub total_papers_retrieved: usize,
    pub generated: String,
}

/// A single paper flagged as potentially overlapping with the proposed work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarPaper {
    pub title: String,
    pub paper_id: String,
    pub year: u32,
    pub venue: String,
    pub citation_count: u32,
    pub similarity: f64,
    pub url: String,
    pub cite_key: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check whether the proposed research has significant overlap with existing work.
///
/// # Arguments
/// * `topic`              – research topic string
/// * `hypotheses_text`    – full text of generated hypotheses (markdown)
/// * `papers_already_seen` – papers already in the pipeline (optional pre-check)
/// * `max_search_results` – max papers to retrieve from APIs
/// * `similarity_threshold` – minimum similarity to flag a paper (default 0.25)
/// * `s2_api_key`         – optional Semantic Scholar API key
pub async fn check_novelty(
    topic: &str,
    hypotheses_text: &str,
    papers_already_seen: &[Paper],
    max_search_results: usize,
    similarity_threshold: f64,
    s2_api_key: &str,
) -> Result<NoveltyReport> {
    let combined_text = format!("{topic}\n{hypotheses_text}");
    let hyp_keywords = extract_keywords(&combined_text);

    let queries = build_novelty_queries(topic, hypotheses_text);
    let mut similar_papers: Vec<SimilarPaper> = Vec::new();
    let mut total_retrieved = 0usize;

    // Search real APIs
    let search_opts = SearchOptions {
        limit: (max_search_results / queries.len().max(1)).max(10).min(15),
        year_min: 0,
        s2_api_key: s2_api_key.to_owned(),
        ..Default::default()
    };

    for query in &queries {
        let found = crate::search::search_papers(query, &search_opts).await.unwrap_or_default();
        total_retrieved += found.len();
        for paper in found.into_iter().take(max_search_results) {
            let sim = compute_similarity(&hyp_keywords, &paper.title, &paper.abstract_text, "");
            if sim >= similarity_threshold {
                let title_lower = paper.title.to_lowercase();
                if !similar_papers.iter().any(|sp: &SimilarPaper| sp.title.to_lowercase() == title_lower) {
                    similar_papers.push(SimilarPaper {
                        title: paper.title.clone(),
                        paper_id: paper.paper_id.clone(),
                        year: paper.year,
                        venue: paper.venue.clone(),
                        citation_count: paper.citation_count,
                        similarity: sim,
                        url: paper.url.clone(),
                        cite_key: paper.cite_key(),
                    });
                }
            }
        }
    }

    // Also check pre-loaded pipeline papers
    for p in papers_already_seen {
        let sim = compute_similarity(&hyp_keywords, &p.title, &p.abstract_text, "");
        if sim >= similarity_threshold {
            let title_lower = p.title.to_lowercase();
            if !similar_papers.iter().any(|sp: &SimilarPaper| sp.title.to_lowercase() == title_lower) {
                similar_papers.push(SimilarPaper {
                    title: p.title.clone(),
                    paper_id: p.paper_id.clone(),
                    year: p.year,
                    venue: p.venue.clone(),
                    citation_count: p.citation_count,
                    similarity: sim,
                    url: p.url.clone(),
                    cite_key: p.cite_key(),
                });
            }
        }
    }

    similar_papers.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));

    let (novelty_score, assessment) = assess_novelty(&similar_papers, similarity_threshold);

    let search_coverage = if total_retrieved == 0 && papers_already_seen.is_empty() {
        "insufficient"
    } else if total_retrieved < 5 {
        "partial"
    } else {
        "full"
    };

    let (assessment, recommendation) = if search_coverage == "insufficient" && similar_papers.is_empty() {
        ("insufficient_data".to_owned(), "proceed_with_caution".to_owned())
    } else {
        let rec = match assessment.as_str() {
            "critical" => "abort",
            "low" => "differentiate",
            _ => "proceed",
        };
        (assessment, rec.to_owned())
    };

    let hyp_count = count_hypotheses(hypotheses_text);

    info!(
        "Novelty: score={:.3}, assessment={}, similar={}, retrieved={}",
        novelty_score, assessment, similar_papers.len(), total_retrieved
    );

    Ok(NoveltyReport {
        topic: topic.to_owned(),
        hypotheses_checked: hyp_count,
        search_queries: queries,
        similar_papers_found: similar_papers.len(),
        novelty_score,
        assessment,
        similar_papers: similar_papers.into_iter().take(20).collect(),
        recommendation,
        similarity_threshold,
        search_coverage: search_coverage.to_owned(),
        total_papers_retrieved: total_retrieved,
        generated: chrono::Utc::now().to_rfc3339(),
    })
}

// ---------------------------------------------------------------------------
// Internal: keyword extraction
// ---------------------------------------------------------------------------

fn extract_keywords(text: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result = Vec::new();
    for token in text.to_lowercase().split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
        let t = token.trim_matches(|c: char| !c.is_alphanumeric());
        if t.len() >= 3 && !STOP_WORDS.contains(&t) && seen.insert(t.to_owned()) {
            result.push(t.to_owned());
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Internal: similarity metrics
// ---------------------------------------------------------------------------

fn jaccard_keywords(a: &[String], b: &[String]) -> f64 {
    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    intersection as f64 / union as f64
}

/// Combined similarity: keyword Jaccard + optional title sequence similarity.
fn compute_similarity(
    hypothesis_keywords: &[String],
    paper_title: &str,
    paper_abstract: &str,
    hypothesis_title: &str,
) -> f64 {
    let paper_keywords = extract_keywords(&format!("{paper_title} {paper_abstract}"));
    let kw_sim = jaccard_keywords(hypothesis_keywords, &paper_keywords);
    let raw = if !hypothesis_title.is_empty() && !paper_title.is_empty() {
        let t_sim = sequence_similarity(hypothesis_title, paper_title);
        (0.7 * kw_sim + 0.3 * t_sim).min(1.0)
    } else {
        kw_sim
    };
    (raw * 10000.0).round() / 10000.0
}

/// Character-level similarity ratio matching Python's `difflib.SequenceMatcher.ratio()`.
///
/// Finds the longest contiguous matching block, then recursively finds matches
/// in the regions before and after.  Returns `2.0 * M / T` where M is the total
/// number of matched characters and T is the total length of both strings.
fn sequence_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let ab = a_lower.as_bytes();
    let bb = b_lower.as_bytes();
    let m = matching_blocks_count(ab, 0, ab.len(), bb, 0, bb.len());
    2.0 * m as f64 / (ab.len() + bb.len()) as f64
}

/// Recursively count matched characters using the longest-contiguous-block approach
/// (mirrors Python `difflib.SequenceMatcher`).
fn matching_blocks_count(a: &[u8], alo: usize, ahi: usize, b: &[u8], blo: usize, bhi: usize) -> usize {
    let (i, j, k) = find_longest_match(a, alo, ahi, b, blo, bhi);
    if k == 0 {
        return 0;
    }
    let mut count = k;
    if alo < i && blo < j {
        count += matching_blocks_count(a, alo, i, b, blo, j);
    }
    if i + k < ahi && j + k < bhi {
        count += matching_blocks_count(a, i + k, ahi, b, j + k, bhi);
    }
    count
}

/// Find the longest contiguous matching block in `a[alo..ahi]` and `b[blo..bhi]`.
/// Returns `(i, j, k)` where `a[i..i+k] == b[j..j+k]` and `k` is maximised.
fn find_longest_match(a: &[u8], alo: usize, ahi: usize, b: &[u8], blo: usize, bhi: usize) -> (usize, usize, usize) {
    let mut best_i = alo;
    let mut best_j = blo;
    let mut best_k = 0usize;

    // j2len[j] = length of longest match ending at b[j] for the current a[i]
    let blen = bhi.saturating_sub(blo);
    let mut j2len = vec![0usize; blen + 1];
    let mut new_j2len = vec![0usize; blen + 1];

    for i in alo..ahi {
        new_j2len.fill(0);
        for j in blo..bhi {
            let jj = j - blo;
            if a[i] == b[j] {
                let k = j2len[jj] + 1;
                new_j2len[jj + 1] = k;
                if k > best_k {
                    best_i = i + 1 - k;
                    best_j = j + 1 - k;
                    best_k = k;
                }
            }
        }
        std::mem::swap(&mut j2len, &mut new_j2len);
    }
    (best_i, best_j, best_k)
}

// ---------------------------------------------------------------------------
// Internal: query building and scoring
// ---------------------------------------------------------------------------

fn build_novelty_queries(topic: &str, hypotheses_text: &str) -> Vec<String> {
    let mut queries = vec![topic.to_owned()];

    // Extract hypothesis titles (## H1, ## H2, …)
    for line in hypotheses_text.lines() {
        if let Some(rest) = line.strip_prefix("## H") {
            if let Some(colon_or_space) = rest.find(|c: char| c == ':' || c == ' ') {
                let title = rest[colon_or_space + 1..].trim();
                if title.len() > 10 {
                    queries.push(title[..title.len().min(200)].to_owned());
                }
            }
        }
    }

    // Build a keyword query from top keywords
    let keywords = extract_keywords(hypotheses_text);
    let kw_query = keywords.iter().take(5).cloned().collect::<Vec<_>>().join(" ");
    if !kw_query.is_empty() && !queries.contains(&kw_query) {
        queries.push(kw_query);
    }

    queries.truncate(5);
    queries
}

fn assess_novelty(similar_papers: &[SimilarPaper], _threshold: f64) -> (f64, String) {
    if similar_papers.is_empty() {
        return (1.0, "high".to_owned());
    }

    let top: &[SimilarPaper] = &similar_papers[..similar_papers.len().min(5)];
    let max_sim = top.iter().map(|p| p.similarity).fold(f64::NEG_INFINITY, f64::max);

    let high_cite_overlap = top
        .iter()
        .filter(|p| p.similarity >= 0.4 && p.citation_count >= 50)
        .count();

    let mut raw_score = 1.0 - max_sim;
    if high_cite_overlap >= 2 {
        raw_score *= 0.7;
    }
    let novelty_score = (raw_score.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;

    let assessment = if novelty_score >= 0.7 {
        "high"
    } else if novelty_score >= 0.45 {
        "moderate"
    } else if novelty_score >= 0.25 {
        "low"
    } else {
        "critical"
    };

    (novelty_score, assessment.to_owned())
}

fn count_hypotheses(text: &str) -> usize {
    let h_headers = text.lines().filter(|l| l.starts_with("## H")).count();
    if h_headers > 0 {
        return h_headers;
    }
    let by_word = text.to_lowercase().matches("hypothesis").count();
    by_word.max(1)
}

// ---------------------------------------------------------------------------
// Chrono dep: add to Cargo.toml check
// ---------------------------------------------------------------------------

// `chrono` is already a workspace dep.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_keywords() {
        let kw = extract_keywords("Adaptive learning rate schedules for transformers");
        assert!(kw.contains(&"adaptive".to_owned()));
        assert!(kw.contains(&"rate".to_owned()));
        assert!(kw.contains(&"schedules".to_owned()));
        assert!(kw.contains(&"transformers".to_owned()));
        // Stop words excluded
        assert!(!kw.contains(&"for".to_owned()));
    }

    #[test]
    fn test_jaccard() {
        let a = vec!["transformer".to_owned(), "attention".to_owned(), "bert".to_owned()];
        let b = vec!["transformer".to_owned(), "gpt".to_owned(), "bert".to_owned()];
        let j = jaccard_keywords(&a, &b);
        assert!((j - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_assess_novelty_no_overlap() {
        let (score, assessment) = assess_novelty(&[], 0.25);
        assert_eq!(score, 1.0);
        assert_eq!(assessment, "high");
    }
}
