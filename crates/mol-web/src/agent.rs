//! Unified web search agent that orchestrates all mol-web capabilities.
//!
//! Combines web search (Tavily/DuckDuckGo), Google Scholar scraping, HTML
//! crawling, and PDF extraction into a single pipeline for gathering research
//! context on a given topic.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! use mol_web::agent::{WebSearchAgent, WebSearchAgentConfig};
//!
//! let config = WebSearchAgentConfig {
//!     tavily_api_key: "tvly-YOUR_KEY".into(),
//!     ..Default::default()
//! };
//! let agent = WebSearchAgent::new(config);
//! let result = agent.search_and_extract("knowledge distillation", None, None, None).await?;
//! println!("{}", result.to_context_string(30_000));
//! # Ok(())
//! # }
//! ```

use crate::crawler::{CrawlResult, WebCrawler};
use crate::pdf::{self, PdfContent};
use crate::scholar::{ScholarClient, ScholarPaper};
use crate::search::{WebSearchClient, WebSearchResult};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Aggregated results from the web search agent pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebSearchAgentResult {
    /// The topic that was searched.
    pub topic: String,
    /// Web search results from Tavily / DuckDuckGo.
    pub web_results: Vec<WebSearchResult>,
    /// Academic papers from Google Scholar.
    pub scholar_papers: Vec<ScholarPaper>,
    /// Full-text crawled web pages.
    pub crawled_pages: Vec<CrawlResult>,
    /// Extracted PDF content.
    pub pdf_extractions: Vec<PdfContent>,
    /// AI-generated search summary (from Tavily answer field).
    pub search_answer: String,
    /// Total wall-clock time for the pipeline in seconds.
    pub elapsed_seconds: f64,
}

impl WebSearchAgentResult {
    /// Total number of individual results across all sources.
    pub fn total_results(&self) -> usize {
        self.web_results.len()
            + self.scholar_papers.len()
            + self.crawled_pages.len()
            + self.pdf_extractions.len()
    }

    /// Format results as Markdown context suitable for LLM injection.
    ///
    /// Produces sections for AI Summary, Web Results, Scholar Papers, Crawled
    /// Content, and PDF Extractions.  Truncates the output to `max_length`
    /// characters, appending a `[... truncated]` marker if necessary.
    pub fn to_context_string(&self, max_length: usize) -> String {
        let mut ctx = String::with_capacity(max_length.min(64_000));

        // AI Summary
        if !self.search_answer.is_empty() {
            ctx.push_str("## AI Search Summary\n\n");
            ctx.push_str(&self.search_answer);
            ctx.push_str("\n\n");
        }

        // Web Results
        if !self.web_results.is_empty() {
            ctx.push_str("## Web Search Results\n\n");
            for (i, r) in self.web_results.iter().enumerate() {
                ctx.push_str(&format!(
                    "### {}. {}\n**URL:** {}\n\n{}\n\n",
                    i + 1,
                    r.title,
                    r.url,
                    if r.snippet.is_empty() { &r.content } else { &r.snippet },
                ));
                if ctx.len() >= max_length {
                    break;
                }
            }
        }

        // Scholar Papers
        if !self.scholar_papers.is_empty() {
            ctx.push_str("## Google Scholar Papers\n\n");
            for (i, p) in self.scholar_papers.iter().enumerate() {
                ctx.push_str(&format!(
                    "### {}. {} ({})\n**Authors:** {}\n**Citations:** {}\n**URL:** {}\n\n{}\n\n",
                    i + 1,
                    p.title,
                    p.year,
                    p.authors.join(", "),
                    p.citation_count,
                    p.url,
                    p.abstract_snippet,
                ));
                if ctx.len() >= max_length {
                    break;
                }
            }
        }

        // Crawled Content
        if !self.crawled_pages.is_empty() {
            ctx.push_str("## Crawled Content\n\n");
            for (i, c) in self.crawled_pages.iter().enumerate() {
                if !c.success {
                    continue;
                }
                let preview = if c.text_content.len() > 2000 {
                    &c.text_content[..2000]
                } else {
                    &c.text_content
                };
                ctx.push_str(&format!(
                    "### {}. {}\n**URL:** {}\n\n{}\n\n",
                    i + 1,
                    c.title,
                    c.url,
                    preview,
                ));
                if ctx.len() >= max_length {
                    break;
                }
            }
        }

        // PDF Extractions
        if !self.pdf_extractions.is_empty() {
            ctx.push_str("## PDF Extractions\n\n");
            for (i, p) in self.pdf_extractions.iter().enumerate() {
                if !p.success {
                    continue;
                }
                let title = p
                    .metadata
                    .title
                    .as_deref()
                    .unwrap_or("Untitled PDF");
                let preview = if p.text.len() > 2000 {
                    &p.text[..2000]
                } else {
                    &p.text
                };
                ctx.push_str(&format!(
                    "### {}. {} ({} pages)\n\n{}\n\n",
                    i + 1,
                    title,
                    p.pages,
                    preview,
                ));
                if ctx.len() >= max_length {
                    break;
                }
            }
        }

        // Truncate if necessary
        if ctx.len() > max_length {
            ctx.truncate(max_length);
            ctx.push_str("\n\n[... truncated]");
        }

        ctx
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the [`WebSearchAgent`].
#[derive(Debug, Clone)]
pub struct WebSearchAgentConfig {
    /// Tavily API key (falls back to `TAVILY_API_KEY` env var if empty).
    pub tavily_api_key: String,
    /// Whether to query Google Scholar.
    pub enable_scholar: bool,
    /// Whether to crawl top web result URLs.
    pub enable_crawling: bool,
    /// Whether to extract text from PDF URLs found in search results.
    pub enable_pdf: bool,
    /// Maximum number of web search results per query.
    pub max_web_results: usize,
    /// Maximum number of Scholar papers to retrieve.
    pub max_scholar_results: usize,
    /// Maximum number of URLs to crawl.
    pub max_crawl_urls: usize,
}

impl Default for WebSearchAgentConfig {
    fn default() -> Self {
        Self {
            tavily_api_key: String::new(),
            enable_scholar: true,
            enable_crawling: true,
            enable_pdf: true,
            max_web_results: 10,
            max_scholar_results: 10,
            max_crawl_urls: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// Unified web search agent that orchestrates search, scholar, crawl, and PDF
/// extraction into a single research pipeline.
pub struct WebSearchAgent {
    config: WebSearchAgentConfig,
    search_client: WebSearchClient,
    scholar_client: ScholarClient,
    crawler: WebCrawler,
}

impl WebSearchAgent {
    /// Create a new agent with the given configuration.
    pub fn new(config: WebSearchAgentConfig) -> Self {
        let search_client = WebSearchClient::new(&config.tavily_api_key);
        let scholar_client = ScholarClient::new();
        let crawler = WebCrawler::new();

        Self {
            config,
            search_client,
            scholar_client,
            crawler,
        }
    }

    /// Run the full search-and-extract pipeline for a given topic.
    ///
    /// # Arguments
    ///
    /// * `topic` — The research topic to search for.
    /// * `queries` — Optional explicit search queries; if `None`, queries are
    ///   auto-generated from the topic.
    /// * `crawl_urls` — Optional explicit URLs to crawl; if `None`, top
    ///   non-PDF URLs from search results are selected.
    /// * `pdf_urls` — Optional explicit PDF URLs to extract; if `None`, PDF
    ///   URLs are discovered from search results.
    pub async fn search_and_extract(
        &self,
        topic: &str,
        queries: Option<&[String]>,
        crawl_urls: Option<&[String]>,
        pdf_urls: Option<&[String]>,
    ) -> Result<WebSearchAgentResult> {
        let start = Instant::now();

        let mut result = WebSearchAgentResult {
            topic: topic.to_owned(),
            ..Default::default()
        };

        // Step 1: Web search
        let search_queries = match queries {
            Some(q) => q.to_vec(),
            None => Self::generate_queries(topic),
        };
        self.run_web_search(&mut result, &search_queries).await;

        // Step 2: Scholar search (if enabled)
        if self.config.enable_scholar {
            self.run_scholar_search(&mut result, topic).await;
        }

        // Step 3: Crawl pages (if enabled)
        if self.config.enable_crawling {
            let urls_to_crawl = match crawl_urls {
                Some(urls) => urls.to_vec(),
                None => Self::select_urls_to_crawl(&result.web_results, self.config.max_crawl_urls),
            };
            self.run_crawling(&mut result, &urls_to_crawl).await;
        }

        // Step 4: PDF extraction (if enabled)
        if self.config.enable_pdf {
            let urls = match pdf_urls {
                Some(urls) => urls.to_vec(),
                None => Self::find_pdf_urls(&result.web_results),
            };
            self.run_pdf_extraction(&mut result, &urls).await;
        }

        result.elapsed_seconds = start.elapsed().as_secs_f64();

        info!(
            "Web search agent completed for {:?}: {} total results in {:.1}s",
            topic,
            result.total_results(),
            result.elapsed_seconds,
        );

        Ok(result)
    }

    // ------------------------------------------------------------------
    // Query generation
    // ------------------------------------------------------------------

    /// Generate default search queries from a topic.
    ///
    /// Returns three queries: the raw topic, a survey variant, and a benchmark
    /// variant.
    pub fn generate_queries(topic: &str) -> Vec<String> {
        vec![
            topic.to_owned(),
            format!("{topic} survey"),
            format!("{topic} benchmark"),
        ]
    }

    // ------------------------------------------------------------------
    // URL selection helpers
    // ------------------------------------------------------------------

    /// Select top non-PDF URLs from search results for crawling.
    pub fn select_urls_to_crawl(results: &[WebSearchResult], max: usize) -> Vec<String> {
        results
            .iter()
            .filter(|r| {
                let lower = r.url.to_lowercase();
                !lower.ends_with(".pdf") && !lower.contains(".pdf?")
            })
            .take(max)
            .map(|r| r.url.clone())
            .collect()
    }

    /// Find PDF URLs among search results (capped at 3, matching Python behaviour).
    pub fn find_pdf_urls(results: &[WebSearchResult]) -> Vec<String> {
        results
            .iter()
            .filter(|r| {
                let lower = r.url.to_lowercase();
                lower.ends_with(".pdf") || lower.contains(".pdf?")
            })
            .take(3)
            .map(|r| r.url.clone())
            .collect()
    }

    // ------------------------------------------------------------------
    // Pipeline steps
    // ------------------------------------------------------------------

    async fn run_web_search(&self, result: &mut WebSearchAgentResult, queries: &[String]) {
        for query in queries {
            debug!("Web search query: {query:?}");
            match self
                .search_client
                .search_full(query, self.config.max_web_results)
                .await
            {
                Ok(response) => {
                    // Capture the AI answer from the first query that has one
                    if result.search_answer.is_empty() && !response.answer.is_empty() {
                        result.search_answer = response.answer;
                    }
                    result.web_results.extend(response.results);
                }
                Err(e) => {
                    warn!("Web search failed for {query:?}: {e}");
                }
            }
        }
    }

    async fn run_scholar_search(&self, result: &mut WebSearchAgentResult, topic: &str) {
        debug!("Scholar search for: {topic:?}");
        let papers = self
            .scholar_client
            .search(topic, self.config.max_scholar_results)
            .await;
        result.scholar_papers = papers;
    }

    async fn run_crawling(&self, result: &mut WebSearchAgentResult, urls: &[String]) {
        for url in urls {
            debug!("Crawling: {url}");
            match self.crawler.crawl(url).await {
                Ok(crawl_result) => {
                    result.crawled_pages.push(crawl_result);
                }
                Err(e) => {
                    warn!("Crawl failed for {url}: {e}");
                    result.crawled_pages.push(CrawlResult {
                        url: url.clone(),
                        title: String::new(),
                        text_content: String::new(),
                        links: Vec::new(),
                        success: false,
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    async fn run_pdf_extraction(&self, result: &mut WebSearchAgentResult, urls: &[String]) {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("MolHEP/0.1 (Academic Research Bot)")
            .build()
            .unwrap_or_default();

        for url in urls {
            debug!("PDF extraction: {url}");
            match client.get(url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.bytes().await {
                            Ok(bytes) => match pdf::extract_text_from_bytes(&bytes) {
                                Ok(content) => {
                                    result.pdf_extractions.push(content);
                                }
                                Err(e) => {
                                    warn!("PDF parse failed for {url}: {e}");
                                }
                            },
                            Err(e) => {
                                warn!("PDF download body failed for {url}: {e}");
                            }
                        }
                    } else {
                        warn!("PDF download failed for {url}: HTTP {}", resp.status());
                    }
                }
                Err(e) => {
                    warn!("PDF download request failed for {url}: {e}");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_queries_produces_three() {
        let queries = WebSearchAgent::generate_queries("knowledge distillation");
        assert_eq!(queries.len(), 3);
        assert!(queries[0].contains("knowledge distillation"));
        assert!(queries[1].contains("survey"));
    }

    #[test]
    fn to_context_string_formats_markdown() {
        let mut result = WebSearchAgentResult::default();
        result.topic = "test topic".into();
        result.search_answer = "AI summary here".into();
        let ctx = result.to_context_string(30_000);
        assert!(ctx.contains("## AI Search Summary"));
        assert!(ctx.contains("AI summary here"));
    }

    #[test]
    fn to_context_string_truncates() {
        let mut result = WebSearchAgentResult::default();
        result.search_answer = "x".repeat(50_000);
        let ctx = result.to_context_string(1000);
        assert!(ctx.len() <= 1100); // some slack for truncation message
        assert!(ctx.contains("[... truncated]"));
    }

    #[test]
    fn select_urls_skips_pdf() {
        let make_result = |url: &str| WebSearchResult {
            title: String::new(),
            url: url.to_owned(),
            snippet: String::new(),
            content: String::new(),
            score: 0.0,
            source: String::new(),
        };
        let results = vec![
            make_result("https://example.com/page"),
            make_result("https://example.com/paper.pdf"),
        ];
        let urls = WebSearchAgent::select_urls_to_crawl(&results, 5);
        assert_eq!(urls.len(), 1);
        assert!(!urls[0].ends_with(".pdf"));
    }

    #[test]
    fn find_pdf_urls_finds_pdfs() {
        let make_result = |url: &str| WebSearchResult {
            title: String::new(),
            url: url.to_owned(),
            snippet: String::new(),
            content: String::new(),
            score: 0.0,
            source: String::new(),
        };
        let results = vec![
            make_result("https://example.com/page"),
            make_result("https://example.com/paper.pdf"),
            make_result("https://example.com/doc.pdf?v=2"),
        ];
        let urls = WebSearchAgent::find_pdf_urls(&results);
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn total_results_sums_all_sources() {
        let mut result = WebSearchAgentResult::default();
        result.web_results.push(WebSearchResult {
            title: "t".into(),
            url: "u".into(),
            snippet: String::new(),
            content: String::new(),
            score: 0.0,
            source: String::new(),
        });
        result.scholar_papers.push(ScholarPaper {
            title: "t".into(),
            authors: vec![],
            year: 2024,
            abstract_snippet: String::new(),
            citation_count: 0,
            url: String::new(),
            scholar_id: String::new(),
            venue: String::new(),
        });
        assert_eq!(result.total_results(), 2);
    }
}
