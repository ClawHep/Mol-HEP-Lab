//! Unified search interface across all literature providers.
//!
//! This module provides a single entry point for literature search that fans
//! out to arXiv, Semantic Scholar, and OpenAlex, deduplicates results, and
//! returns a merged `Vec<Paper>`.
//!
//! Public API
//! ----------
//! - [`search_papers`]             – single query, all providers
//! - [`search_papers_multi_query`] – multiple queries, deduplicated

use std::collections::HashSet;

use anyhow::Result;
use tracing::debug;

use crate::models::Paper;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Options for a literature search request.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum papers to return (applies per-provider internally).
    pub limit: usize,
    /// If > 0, filter papers published before this year.
    pub year_min: u32,
    /// Optional Semantic Scholar API key.
    pub s2_api_key: String,
    /// Optional polite-pool email for OpenAlex.
    pub openalex_email: String,
    /// Whether to deduplicate results across providers.
    pub deduplicate: bool,
    /// Providers to include. Empty = all.
    pub providers: Vec<Provider>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 20,
            year_min: 0,
            s2_api_key: String::new(),
            openalex_email: String::new(),
            deduplicate: true,
            providers: vec![],
        }
    }
}

/// Available search providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Arxiv,
    SemanticScholar,
    OpenAlex,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Search for papers matching `query` across all enabled providers.
///
/// Results are merged and optionally deduplicated by title.
pub async fn search_papers(query: &str, opts: &SearchOptions) -> Result<Vec<Paper>> {
    let providers = if opts.providers.is_empty() {
        vec![Provider::OpenAlex, Provider::SemanticScholar, Provider::Arxiv]
    } else {
        opts.providers.clone()
    };

    // Per-provider limit (over-fetch a bit, then trim after dedup)
    let per_limit = (opts.limit * 2).max(10).min(50);

    // Fan out concurrently
    let mut handles = Vec::new();

    for provider in &providers {
        let query = query.to_owned();
        let opts = opts.clone();
        let limit = per_limit;
        let prov = *provider;

        handles.push(tokio::spawn(async move {
            fetch_from_provider(prov, &query, limit, &opts).await
        }));
    }

    let mut all_papers: Vec<Paper> = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(papers)) => all_papers.extend(papers),
            Ok(Err(e)) => debug!("Provider error: {e}"),
            Err(e) => debug!("Task join error: {e}"),
        }
    }

    if opts.deduplicate {
        all_papers = deduplicate(all_papers);
    }

    all_papers.truncate(opts.limit);
    Ok(all_papers)
}

/// Search across multiple queries, deduplicate by title, and return merged results.
pub async fn search_papers_multi_query(
    queries: &[String],
    opts: &SearchOptions,
) -> Result<Vec<Paper>> {
    let mut all_papers: Vec<Paper> = Vec::new();
    for query in queries {
        let results = search_papers(query, opts).await?;
        all_papers.extend(results);
    }
    Ok(deduplicate(all_papers))
}

// ---------------------------------------------------------------------------
// Internal: dispatch to providers
// ---------------------------------------------------------------------------

async fn fetch_from_provider(
    provider: Provider,
    query: &str,
    limit: usize,
    opts: &SearchOptions,
) -> Result<Vec<Paper>> {
    match provider {
        Provider::Arxiv => {
            crate::arxiv::search_arxiv(query, limit, "relevance", opts.year_min).await
        }
        Provider::SemanticScholar => {
            crate::semantic_scholar::search_semantic_scholar(
                query,
                limit,
                opts.year_min,
                &opts.s2_api_key,
            )
            .await
        }
        Provider::OpenAlex => {
            crate::openalex::search_openalex(
                query,
                limit,
                opts.year_min,
                &opts.openalex_email,
            )
            .await
        }
    }
}

// ---------------------------------------------------------------------------
// Deduplication
// ---------------------------------------------------------------------------

/// Remove duplicate papers by normalised title.
///
/// When duplicates exist, prefer the entry with the highest citation count.
fn deduplicate(mut papers: Vec<Paper>) -> Vec<Paper> {
    // Sort by citation count descending so that when we encounter duplicates
    // we keep the richest entry.
    papers.sort_by(|a, b| b.citation_count.cmp(&a.citation_count));

    let mut seen: HashSet<String> = HashSet::new();
    papers.retain(|p| {
        let key = normalise_title(&p.title);
        seen.insert(key)
    });
    papers
}

fn normalise_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Convenience wrappers matching Python API signatures
// ---------------------------------------------------------------------------

/// Search all providers with default options.
pub async fn search_all(query: &str, limit: usize) -> Result<Vec<Paper>> {
    search_papers(query, &SearchOptions { limit, ..Default::default() }).await
}

/// Search with year filtering.
pub async fn search_recent(query: &str, limit: usize, year_min: u32) -> Result<Vec<Paper>> {
    search_papers(query, &SearchOptions { limit, year_min, ..Default::default() }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Author;

    fn make_paper(title: &str, citations: u32) -> Paper {
        Paper {
            paper_id: title.to_owned(),
            title: title.to_owned(),
            authors: vec![Author::new("Test Author")],
            year: 2023,
            abstract_text: String::new(),
            venue: String::new(),
            citation_count: citations,
            doi: String::new(),
            arxiv_id: String::new(),
            url: String::new(),
            source: "test".to_owned(),
        }
    }

    #[test]
    fn test_deduplicate_keeps_highest_citation() {
        let papers = vec![
            make_paper("Attention Is All You Need", 10_000),
            make_paper("Attention Is All You Need", 5_000),
            make_paper("BERT: Pretraining", 8_000),
        ];
        let deduped = deduplicate(papers);
        assert_eq!(deduped.len(), 2);
        // The high-citation version should be kept
        let attn = deduped.iter().find(|p| p.title.starts_with("Attention")).unwrap();
        assert_eq!(attn.citation_count, 10_000);
    }

    #[test]
    fn test_normalise_title() {
        assert_eq!(
            normalise_title("Attention  Is ALL you Need!"),
            "attention is all you need"
        );
    }
}
