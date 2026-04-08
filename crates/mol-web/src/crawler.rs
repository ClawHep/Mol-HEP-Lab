//! Web page crawler using reqwest + scraper for HTML parsing.
//!
//! Fetches a URL, parses HTML with the `scraper` crate, and returns plain
//! text content along with extracted links and title.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! use mol_web::crawler::crawl;
//!
//! let result = crawl("https://arxiv.org/abs/2301.00001").await.unwrap();
//! println!("Title: {}", result.title);
//! println!("Text preview: {}", &result.text_content[..200]);
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of crawling a single URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    /// The URL that was fetched (after any redirects).
    pub url: String,
    /// Page `<title>` text, if found.
    pub title: String,
    /// Visible text content extracted from the page body.
    pub text_content: String,
    /// Absolute and relative href values found in `<a>` tags.
    pub links: Vec<String>,
    /// Whether the crawl succeeded.
    pub success: bool,
    /// Error message if `success` is false.
    pub error: String,
}

impl CrawlResult {
    /// Returns `true` if the page contains non-trivial text.
    pub fn has_content(&self) -> bool {
        self.success && self.text_content.trim().len() > 50
    }
}

// ---------------------------------------------------------------------------
// Crawler
// ---------------------------------------------------------------------------

/// Web page crawler.
///
/// Fetches HTML with reqwest and extracts visible text content and links.
pub struct WebCrawler {
    client: Client,
    max_content_length: usize,
}

impl Default for WebCrawler {
    fn default() -> Self {
        Self::new()
    }
}

impl WebCrawler {
    /// Create a new crawler with sensible defaults.
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("MolHEP/0.1 (Academic Research Bot)")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("failed to build reqwest client");

        Self {
            client,
            max_content_length: 50_000,
        }
    }

    /// Builder: set the maximum number of characters to return in
    /// `text_content`.
    pub fn with_max_content_length(mut self, n: usize) -> Self {
        self.max_content_length = n;
        self
    }

    /// Crawl a single URL and return a [`CrawlResult`].
    pub async fn crawl(&self, url: &str) -> Result<CrawlResult> {
        let resp = match self.client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(CrawlResult {
                    url: url.to_owned(),
                    title: String::new(),
                    text_content: String::new(),
                    links: Vec::new(),
                    success: false,
                    error: e.to_string(),
                });
            }
        };

        let final_url = resp.url().to_string();

        if !resp.status().is_success() {
            return Ok(CrawlResult {
                url: final_url,
                title: String::new(),
                text_content: String::new(),
                links: Vec::new(),
                success: false,
                error: format!("HTTP {}", resp.status()),
            });
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();

        // Only try to parse HTML; binary / PDF content should go through
        // the pdf module.
        if content_type.contains("application/pdf") {
            return Ok(CrawlResult {
                url: final_url,
                title: String::new(),
                text_content: String::new(),
                links: Vec::new(),
                success: false,
                error: "Content-Type is application/pdf; use mol_web::pdf instead".to_owned(),
            });
        }

        let html = resp.text().await?;
        debug!("Crawled {} ({} bytes HTML)", url, html.len());

        let (title, text_content, links) = parse_html(&html, &final_url, self.max_content_length);

        Ok(CrawlResult {
            url: final_url,
            title,
            text_content,
            links,
            success: true,
            error: String::new(),
        })
    }

    /// Crawl multiple URLs sequentially and return results.
    pub async fn crawl_many(&self, urls: &[&str]) -> Vec<CrawlResult> {
        let mut results = Vec::with_capacity(urls.len());
        for url in urls {
            results.push(self.crawl(url).await.unwrap_or_else(|e| CrawlResult {
                url: (*url).to_owned(),
                title: String::new(),
                text_content: String::new(),
                links: Vec::new(),
                success: false,
                error: e.to_string(),
            }));
        }
        results
    }
}

// ---------------------------------------------------------------------------
// HTML parsing helpers
// ---------------------------------------------------------------------------

/// Parse an HTML document and return `(title, visible_text, links)`.
fn parse_html(html: &str, base_url: &str, max_len: usize) -> (String, String, Vec<String>) {
    let document = Html::parse_document(html);

    // --- Title ---
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .unwrap_or_default();

    // --- Strip <script>, <style>, <nav>, <footer>, <header> ---
    // We extract text from <body> only, skipping noise elements.
    let body_sel = Selector::parse("body").ok();
    let noise_tags = ["script", "style", "nav", "footer", "header", "aside", "form"];

    let mut text_parts: Vec<String> = Vec::new();

    let root = body_sel
        .as_ref()
        .and_then(|s| document.select(s).next())
        .map(|n| scraper::ElementRef::wrap(*n))
        .flatten();

    // Walk every element in the body, collecting text from leaf nodes while
    // skipping noise subtrees.
    if let Some(body) = root {
        collect_text(body, &noise_tags, &mut text_parts);
    } else {
        // No <body>: fall back to full document text
        let plain = document.root_element().text().collect::<String>();
        text_parts.push(plain);
    }

    let raw_text = text_parts.join("\n");
    // Collapse runs of blank lines
    let text_content = collapse_whitespace(&raw_text, max_len);

    // --- Links ---
    let link_sel = Selector::parse("a[href]").unwrap();
    let links: Vec<String> = document
        .select(&link_sel)
        .filter_map(|el| el.value().attr("href"))
        .map(|href| resolve_url(href, base_url))
        .filter(|u| !u.is_empty())
        .collect();

    (title, text_content, links)
}

/// Recursively collect visible text, skipping noise subtrees.
fn collect_text(
    el: scraper::ElementRef<'_>,
    noise_tags: &[&str],
    out: &mut Vec<String>,
) {
    for child in el.children() {
        if let Some(elem) = scraper::ElementRef::wrap(child) {
            let tag = elem.value().name();
            if noise_tags.contains(&tag) {
                continue;
            }
            // Block elements: add a newline before/after content
            if is_block_element(tag) {
                out.push("\n".to_owned());
                collect_text(elem, noise_tags, out);
                out.push("\n".to_owned());
            } else {
                collect_text(elem, noise_tags, out);
            }
        } else if let Some(text) = child.value().as_text() {
            let t = text.trim();
            if !t.is_empty() {
                out.push(t.to_owned());
                out.push(" ".to_owned());
            }
        }
    }
}

fn is_block_element(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "ul"
            | "ol"
            | "blockquote"
            | "pre"
            | "section"
            | "article"
            | "main"
            | "td"
            | "th"
            | "tr"
            | "br"
    )
}

/// Collapse multiple blank lines and trim to `max_len`.
fn collapse_whitespace(text: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max_len + 20));
    let mut blank_count = 0u32;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                out.push('\n');
            }
        } else {
            blank_count = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
        if out.len() >= max_len {
            out.truncate(max_len);
            out.push_str("\n\n[... truncated]");
            break;
        }
    }

    out.trim().to_owned()
}

/// Best-effort resolution of an href against a base URL.
fn resolve_url(href: &str, base: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_owned();
    }
    if href.starts_with("//") {
        // Protocol-relative
        if let Some(scheme) = base.split("://").next() {
            return format!("{scheme}:{href}");
        }
        return format!("https:{href}");
    }
    if href.starts_with('/') {
        // Absolute path
        if let Some(origin) = extract_origin(base) {
            return format!("{origin}{href}");
        }
    }
    // Relative path or fragment — omit (not useful for downstream consumers)
    String::new()
}

fn extract_origin(url: &str) -> Option<&str> {
    // "https://example.com/some/path" -> "https://example.com"
    let after_scheme = url.find("://").map(|i| i + 3)?;
    let rest = &url[after_scheme..];
    let end = rest.find('/').map(|i| after_scheme + i).unwrap_or(url.len());
    Some(&url[..end])
}

// ---------------------------------------------------------------------------
// Top-level convenience function
// ---------------------------------------------------------------------------

/// Crawl a single URL and return a [`CrawlResult`].
pub async fn crawl(url: &str) -> Result<CrawlResult> {
    WebCrawler::new().crawl(url).await
}
