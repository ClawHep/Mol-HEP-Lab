//! OpenAlex API client.
//!
//! OpenAlex provides generous rate limits (10 000 requests/day for the polite
//! pool) and indexes arXiv, PubMed, CrossRef, and many other sources.
//!
//! Public API
//! ----------
//! - [`search_openalex`] – free-text search returning `Vec<Paper>`
//!
//! Adds the `mailto` query parameter to opt into the "polite pool" with
//! higher rate limits.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::models::{Author, Paper};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BASE_URL: &str = "https://api.openalex.org/works";
const POLITE_EMAIL: &str = "mol-hep-lab@users.noreply.github.com";
const MAX_PER_REQUEST: usize = 50;
const TIMEOUT_SEC: u64 = 10;
const MAX_RETRIES: u32 = 2;
const RATE_LIMIT_MS: u64 = 200; // OpenAlex is generous: 200ms spacing is plenty

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

static LAST_REQUEST_MS: AtomicU64 = AtomicU64::new(0);
static RATE_LOCK: std::sync::LazyLock<Arc<Mutex<()>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(())));

async fn rate_limit_wait() {
    let _guard = RATE_LOCK.lock().await;
    let now = now_ms();
    let last = LAST_REQUEST_MS.load(Ordering::Relaxed);
    let elapsed = now.saturating_sub(last);
    if elapsed < RATE_LIMIT_MS {
        tokio::time::sleep(Duration::from_millis(RATE_LIMIT_MS - elapsed)).await;
    }
    LAST_REQUEST_MS.store(now_ms(), Ordering::Relaxed);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Search OpenAlex for papers matching `query`.
///
/// # Arguments
/// * `query`    – free-text search string
/// * `limit`    – max results (capped at 50)
/// * `year_min` – if > 0, restrict to papers from this year onward
/// * `email`    – polite-pool email; defaults to the package-level constant
pub async fn search_openalex(
    query: &str,
    limit: usize,
    year_min: u32,
    email: &str,
) -> Result<Vec<Paper>> {
    let email = if email.is_empty() { POLITE_EMAIL } else { email };
    let limit = limit.min(MAX_PER_REQUEST);

    rate_limit_wait().await;

    let mut params = format!(
        "search={}&per_page={}&mailto={}&select={}",
        urlencoding(query),
        limit,
        urlencoding(email),
        "id,title,authorships,publication_year,primary_location,\
         cited_by_count,doi,ids,abstract_inverted_index,type",
    );

    if year_min > 0 {
        params.push_str(&format!("&filter=from_publication_date:{year_min}-01-01"));
    }

    let url = format!("{BASE_URL}?{params}");
    let data = request_with_retry(&url, email, MAX_RETRIES).await?;

    let results = data.get("results").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut papers = Vec::with_capacity(results.len());
    for item in &results {
        match parse_openalex_work(item) {
            Ok(p) => papers.push(p),
            Err(e) => {
                let id = item.get("id").and_then(Value::as_str).unwrap_or("?");
                debug!("Failed to parse OpenAlex work {id}: {e}");
            }
        }
    }

    info!("OpenAlex: found {} papers for {:?}", papers.len(), query);
    Ok(papers)
}

// ---------------------------------------------------------------------------
// HTTP helper with retry
// ---------------------------------------------------------------------------

async fn request_with_retry(url: &str, email: &str, max_retries: u32) -> Result<Value> {
    let client = Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SEC))
        .user_agent(format!("mol-literature/0.1 (mailto:{email})"))
        .build()?;

    for attempt in 0..max_retries {
        debug!("GET {url}");
        let resp = client.get(url).send().await?;
        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Respect Retry-After if present and reasonable
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1u64 << (attempt + 1));

            if retry_after > 300 {
                warn!("OpenAlex Retry-After={retry_after}s (>300s). Skipping.");
                return Err(anyhow!("OpenAlex rate limit: retry-after too long ({retry_after}s)"));
            }

            let wait_ms = (retry_after * 1000).min(20_000);
            warn!("OpenAlex 429. Waiting {wait_ms}ms (attempt {}/{max_retries})", attempt + 1);
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            continue;
        }

        if status.as_u16() / 100 == 5 {
            let wait_ms = (1u64 << attempt) * 1000;
            warn!("OpenAlex HTTP {}. Waiting {wait_ms}ms", status.as_u16());
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            continue;
        }

        if !status.is_success() {
            return Err(anyhow!("OpenAlex HTTP {}", status.as_u16()));
        }

        let data: Value = resp.json().await?;
        return Ok(data);
    }

    Err(anyhow!("OpenAlex request exhausted {max_retries} retries for {url}"))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_openalex_work(item: &Value) -> Result<Paper> {
    // Title
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    // Authors
    let authorships = item.get("authorships").and_then(Value::as_array).cloned().unwrap_or_default();
    let authors: Vec<Author> = authorships
        .iter()
        .filter_map(|a| {
            let name = a
                .get("author")
                .and_then(|au| au.get("display_name"))
                .and_then(Value::as_str)?;
            let affiliation = a
                .get("institutions")
                .and_then(Value::as_array)
                .and_then(|insts| insts.first())
                .and_then(|inst| inst.get("display_name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            Some(Author::with_affiliation(name, affiliation))
        })
        .collect();

    // Year
    let year = item.get("publication_year").and_then(Value::as_u64).unwrap_or(0) as u32;

    // Abstract (inverted index format)
    let abstract_text = reconstruct_abstract(item.get("abstract_inverted_index"));

    // Venue
    let venue_raw = item
        .get("primary_location")
        .and_then(|loc| loc.get("source"))
        .and_then(|src| src.get("display_name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    // Filter out arXiv category codes used as venue (e.g. "cs.LG")
    let venue = if is_arxiv_category_code(&venue_raw) { String::new() } else { venue_raw };

    // Citation count
    let citation_count = item.get("cited_by_count").and_then(Value::as_u64).unwrap_or(0) as u32;

    // DOI
    let raw_doi = item.get("doi").and_then(Value::as_str).unwrap_or("").trim().to_owned();
    let doi = raw_doi
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .to_owned();

    // IDs
    let ids = item.get("ids").and_then(Value::as_object);
    let openalex_id = ids
        .and_then(|m| m.get("openalex").and_then(Value::as_str))
        .or_else(|| item.get("id").and_then(Value::as_str))
        .unwrap_or("")
        .to_owned();

    // arXiv ID
    let arxiv_id = ids
        .and_then(|m| m.get("arxiv").and_then(Value::as_str))
        .and_then(|raw| {
            // Extract numeric ID from URLs like https://arxiv.org/abs/2301.00001
            let re = regex::Regex::new(r"(\d{4}\.\d{4,5})").ok()?;
            re.captures(raw)?.get(1).map(|m| m.as_str().to_owned())
        })
        .unwrap_or_default();

    // URL
    let url = if !arxiv_id.is_empty() {
        format!("https://arxiv.org/abs/{arxiv_id}")
    } else if !doi.is_empty() {
        format!("https://doi.org/{doi}")
    } else {
        openalex_id.clone()
    };

    // Paper ID
    let paper_id = openalex_id
        .split('/')
        .next_back()
        .map(|s| format!("oalex-{s}"))
        .unwrap_or_else(|| format!("oalex-{}", &title[..title.len().min(20)]));

    Ok(Paper {
        paper_id,
        title,
        authors,
        year,
        abstract_text,
        venue,
        citation_count,
        doi,
        arxiv_id,
        url,
        source: "openalex".to_owned(),
    })
}

/// Reconstruct abstract text from OpenAlex inverted-index format.
///
/// The inverted index maps each word to the list of positions it appears at.
fn reconstruct_abstract(inverted_index: Option<&Value>) -> String {
    let obj = match inverted_index.and_then(Value::as_object) {
        Some(o) => o,
        None => return String::new(),
    };

    let mut words: Vec<(u64, &str)> = Vec::new();
    for (word, positions) in obj {
        if let Some(arr) = positions.as_array() {
            for pos in arr {
                if let Some(p) = pos.as_u64() {
                    words.push((p, word.as_str()));
                }
            }
        }
    }
    words.sort_unstable_by_key(|&(pos, _)| pos);
    words.iter().map(|(_, w)| *w).collect::<Vec<_>>().join(" ")
}

/// Returns `true` for short arXiv category codes like `cs.LG`, `stat.ML`.
fn is_arxiv_category_code(s: &str) -> bool {
    // e.g. "cs.LG" or "stat.ML"
    let re = regex::Regex::new(r"^[a-z]{2,}\.[A-Z]{2}$").unwrap();
    re.is_match(s)
}

// ---------------------------------------------------------------------------
// URL encoding helper
// ---------------------------------------------------------------------------

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'@' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconstruct_abstract() {
        let v = serde_json::json!({
            "The": [0],
            "quick": [1],
            "fox": [2]
        });
        let s = reconstruct_abstract(Some(&v));
        assert_eq!(s, "The quick fox");
    }

    #[test]
    fn test_arxiv_category_code() {
        assert!(is_arxiv_category_code("cs.LG"));
        assert!(is_arxiv_category_code("stat.ML"));
        assert!(!is_arxiv_category_code("NeurIPS"));
        assert!(!is_arxiv_category_code("cs"));
    }
}
