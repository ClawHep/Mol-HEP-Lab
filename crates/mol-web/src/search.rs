//! Tavily web search API client with DuckDuckGo HTML-scrape fallback.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! use mol_web::search::{WebSearchClient, WebSearchResult};
//!
//! let client = WebSearchClient::new("tvly-YOUR_KEY");
//! let results = client.search("knowledge distillation survey 2024", 10).await.unwrap();
//! for r in &results {
//!     println!("{}: {}", r.title, r.url);
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{bail, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single web search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    /// Page title.
    pub title: String,
    /// Canonical URL.
    pub url: String,
    /// Short excerpt / snippet (≤500 chars).
    pub snippet: String,
    /// Full content body returned by Tavily (may be empty for DDG results).
    pub content: String,
    /// Relevance score assigned by the search engine (0.0–1.0).
    pub score: f64,
    /// Backend that produced this result: `"tavily"` or `"duckduckgo"`.
    pub source: String,
}

/// Aggregated response from a web search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResponse {
    /// The original query string.
    pub query: String,
    /// Individual results.
    pub results: Vec<WebSearchResult>,
    /// Tavily AI-generated direct answer (empty for DuckDuckGo results).
    pub answer: String,
    /// Which backend was used: `"tavily"` or `"duckduckgo"`.
    pub source: String,
}

// ---------------------------------------------------------------------------
// Tavily API types (internal)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
    #[serde(default)]
    answer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    score: f64,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// General-purpose web search client.
///
/// Uses Tavily as the primary engine. If `api_key` is empty or Tavily fails,
/// falls back to scraping DuckDuckGo's HTML search endpoint.
pub struct WebSearchClient {
    api_key: String,
    search_depth: String,
    include_answer: bool,
    client: Client,
}

impl WebSearchClient {
    /// Create a new client with the given Tavily API key.
    ///
    /// Falls back to `TAVILY_API_KEY` environment variable if `api_key` is
    /// empty.
    pub fn new(api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        let resolved = if key.is_empty() {
            std::env::var("TAVILY_API_KEY").unwrap_or_default()
        } else {
            key
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("MolHEP/0.1 (Academic Research Bot)")
            .build()
            .expect("failed to build reqwest client");

        Self {
            api_key: resolved,
            search_depth: "advanced".to_owned(),
            include_answer: true,
            client,
        }
    }

    /// Builder: set Tavily search depth (`"basic"` or `"advanced"`).
    pub fn with_search_depth(mut self, depth: impl Into<String>) -> Self {
        self.search_depth = depth.into();
        self
    }

    /// Builder: disable Tavily AI answer inclusion.
    pub fn without_answer(mut self) -> Self {
        self.include_answer = false;
        self
    }

    /// Search the web and return up to `max_results` results.
    ///
    /// Tries Tavily first; falls back to DuckDuckGo scraping if no API key is
    /// set or if Tavily returns an error.
    pub async fn search(&self, query: &str, max_results: usize) -> Result<Vec<WebSearchResult>> {
        if !self.api_key.is_empty() {
            match self.search_tavily(query, max_results).await {
                Ok(resp) => return Ok(resp.results),
                Err(e) => {
                    warn!("Tavily search failed, falling back to DuckDuckGo: {e}");
                }
            }
        }
        let resp = self.search_duckduckgo(query, max_results).await?;
        Ok(resp.results)
    }

    /// Full search returning the rich [`WebSearchResponse`] (includes answer
    /// field from Tavily).
    pub async fn search_full(&self, query: &str, max_results: usize) -> Result<WebSearchResponse> {
        if !self.api_key.is_empty() {
            match self.search_tavily(query, max_results).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    warn!("Tavily search failed, falling back to DuckDuckGo: {e}");
                }
            }
        }
        self.search_duckduckgo(query, max_results).await
    }

    // ------------------------------------------------------------------
    // Tavily backend
    // ------------------------------------------------------------------

    async fn search_tavily(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<WebSearchResponse> {
        let body = serde_json::json!({
            "api_key": self.api_key,
            "query": query,
            "max_results": max_results,
            "search_depth": self.search_depth,
            "include_answer": self.include_answer,
        });

        let resp = self
            .client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("Tavily API error {status}: {text}");
        }

        let tavily: TavilyResponse = resp.json().await?;
        debug!(
            "Tavily returned {} results for {:?}",
            tavily.results.len(),
            query
        );

        let results = tavily
            .results
            .into_iter()
            .map(|r| {
                let snippet = if r.content.len() > 500 {
                    r.content[..500].to_owned()
                } else {
                    r.content.clone()
                };
                WebSearchResult {
                    title: r.title,
                    url: r.url,
                    snippet,
                    content: r.content,
                    score: r.score,
                    source: "tavily".to_owned(),
                }
            })
            .collect();

        Ok(WebSearchResponse {
            query: query.to_owned(),
            results,
            answer: tavily.answer.unwrap_or_default(),
            source: "tavily".to_owned(),
        })
    }

    // ------------------------------------------------------------------
    // DuckDuckGo fallback
    // ------------------------------------------------------------------

    async fn search_duckduckgo(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<WebSearchResponse> {
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let url = format!("https://html.duckduckgo.com/html/?q={encoded}");

        let resp = self
            .client
            .get(&url)
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/120.0.0.0 Safari/537.36",
            )
            .send()
            .await?;

        if !resp.status().is_success() {
            bail!("DuckDuckGo returned HTTP {}", resp.status());
        }

        let html = resp.text().await?;
        let results = parse_ddg_html(&html, max_results);
        debug!(
            "DuckDuckGo returned {} results for {:?}",
            results.len(),
            query
        );

        Ok(WebSearchResponse {
            query: query.to_owned(),
            results,
            answer: String::new(),
            source: "duckduckgo".to_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// HTML parsing for DuckDuckGo results
// ---------------------------------------------------------------------------

fn parse_ddg_html(html: &str, limit: usize) -> Vec<WebSearchResult> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    let link_sel = Selector::parse("a.result__a").unwrap_or_else(|_| {
        // Fallback: any anchor inside a result block
        Selector::parse("h2 a").unwrap()
    });
    let snippet_sel = Selector::parse("a.result__snippet").ok();

    let mut results = Vec::new();

    let links: Vec<_> = document.select(&link_sel).collect();
    let snippets: Vec<_> = snippet_sel
        .as_ref()
        .map(|s| document.select(s).collect())
        .unwrap_or_default();

    for (i, link) in links.iter().enumerate().take(limit) {
        let href = match link.value().attr("href") {
            Some(h) => h.to_owned(),
            None => continue,
        };
        if href.contains("duckduckgo.com") {
            continue;
        }
        let title = link.text().collect::<String>().trim().to_owned();
        let snippet = snippets
            .get(i)
            .map(|s| s.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        results.push(WebSearchResult {
            title,
            url: href,
            snippet: snippet.clone(),
            content: snippet,
            score: 0.0,
            source: "duckduckgo".to_owned(),
        });
    }

    results
}

// ---------------------------------------------------------------------------
// Top-level convenience function
// ---------------------------------------------------------------------------

/// Search the web using a pre-configured Tavily client or DuckDuckGo fallback.
///
/// Reads `TAVILY_API_KEY` from the environment.
pub async fn search(query: &str, max_results: usize) -> Result<Vec<WebSearchResult>> {
    WebSearchClient::new("").search(query, max_results).await
}
