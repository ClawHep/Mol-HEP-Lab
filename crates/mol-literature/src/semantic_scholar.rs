//! Semantic Scholar Graph API client.
//!
//! Provides keyword search and batch paper-detail lookup.
//!
//! Public API
//! ----------
//! - [`search_semantic_scholar`] – free-text search
//! - [`batch_fetch_papers`]      – fetch paper metadata for a list of IDs
//!
//! Rate limiting: 1.5 req/s without an API key, ~0.3 req/s with one.
//! A three-state circuit breaker (CLOSED → OPEN → HALF_OPEN) trips on
//! repeated 429 responses and backs off automatically.

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

const BASE_URL: &str = "https://api.semanticscholar.org/graph/v1/paper/search";
const BATCH_URL: &str = "https://api.semanticscholar.org/graph/v1/paper/batch";
const FIELDS: &str = "paperId,title,abstract,year,venue,citationCount,authors,externalIds,url";
const MAX_PER_REQUEST: usize = 100;
const BATCH_MAX: usize = 500;
const TIMEOUT_SEC: u64 = 12;
const MAX_RETRIES: u32 = 2;

/// Rate limit intervals in milliseconds.
const RATE_LIMIT_MS_NO_KEY: u64 = 1_500;
const RATE_LIMIT_MS_WITH_KEY: u64 = 300;

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CbState {
    Closed,
    Open,
    HalfOpen,
}

struct CircuitBreaker {
    state: CbState,
    consecutive_429s: u32,
    cooldown_ms: u64,
    open_since_ms: u64,
    trip_count: u32,
}

impl CircuitBreaker {
    const THRESHOLD: u32 = 2;
    const INITIAL_COOLDOWN_MS: u64 = 30_000;
    const MAX_COOLDOWN_MS: u64 = 60_000;

    fn new() -> Self {
        Self {
            state: CbState::Closed,
            consecutive_429s: 0,
            cooldown_ms: Self::INITIAL_COOLDOWN_MS,
            open_since_ms: 0,
            trip_count: 0,
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn should_allow(&mut self) -> bool {
        match self.state {
            CbState::Closed => true,
            CbState::Open => {
                let elapsed = Self::now_ms().saturating_sub(self.open_since_ms);
                if elapsed >= self.cooldown_ms {
                    self.state = CbState::HalfOpen;
                    info!("S2 circuit breaker → HALF_OPEN after {elapsed}ms cooldown");
                    true
                } else {
                    false
                }
            }
            CbState::HalfOpen => true,
        }
    }

    fn on_success(&mut self) {
        self.consecutive_429s = 0;
        if self.state != CbState::Closed {
            info!("S2 circuit breaker → CLOSED");
            self.state = CbState::Closed;
            self.cooldown_ms = Self::INITIAL_COOLDOWN_MS;
        }
    }

    fn on_429(&mut self) -> bool {
        self.consecutive_429s += 1;
        if self.state == CbState::HalfOpen {
            self.cooldown_ms = (self.cooldown_ms * 2).min(Self::MAX_COOLDOWN_MS);
            self.state = CbState::Open;
            self.open_since_ms = Self::now_ms();
            self.trip_count += 1;
            warn!(
                "S2 circuit breaker → OPEN (probe failed, trip #{}, cooldown {}ms)",
                self.trip_count, self.cooldown_ms
            );
            return true;
        }
        if self.consecutive_429s >= Self::THRESHOLD {
            self.state = CbState::Open;
            self.open_since_ms = Self::now_ms();
            self.trip_count += 1;
            warn!(
                "S2 circuit breaker TRIPPED (trip #{}, cooldown {}ms)",
                self.trip_count, self.cooldown_ms
            );
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Global state (rate limiter + circuit breaker)
// ---------------------------------------------------------------------------

static LAST_REQUEST_MS: AtomicU64 = AtomicU64::new(0);

static STATE: std::sync::LazyLock<Arc<Mutex<CircuitBreaker>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(CircuitBreaker::new())));

static RATE_LOCK: std::sync::LazyLock<Arc<Mutex<()>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(())));

async fn rate_limit_wait(with_key: bool) {
    let _guard = RATE_LOCK.lock().await;
    let limit_ms = if with_key { RATE_LIMIT_MS_WITH_KEY } else { RATE_LIMIT_MS_NO_KEY };
    let now = now_ms();
    let last = LAST_REQUEST_MS.load(Ordering::Relaxed);
    let elapsed = now.saturating_sub(last);
    if elapsed < limit_ms {
        tokio::time::sleep(Duration::from_millis(limit_ms - elapsed)).await;
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

/// Search Semantic Scholar for papers matching `query`.
///
/// # Arguments
/// * `query`    – free-text search string
/// * `limit`    – max results (capped at 100)
/// * `year_min` – if > 0, restrict to papers from this year onward
/// * `api_key`  – optional S2 API key (raises rate limit to ~10 req/s)
pub async fn search_semantic_scholar(
    query: &str,
    limit: usize,
    year_min: u32,
    api_key: &str,
) -> Result<Vec<Paper>> {
    {
        let mut cb = STATE.lock().await;
        if !cb.should_allow() {
            info!("S2 circuit breaker OPEN — skipping search");
            return Ok(vec![]);
        }
    }

    rate_limit_wait(!api_key.is_empty()).await;

    let limit = limit.min(MAX_PER_REQUEST);
    let mut params = format!("query={}&limit={}&fields={}", urlencoding(query), limit, FIELDS);
    if year_min > 0 {
        params.push_str(&format!("&year={year_min}-"));
    }
    let url = format!("{BASE_URL}?{params}");

    let result = request_with_retry(&url, api_key, MAX_RETRIES).await;

    match result {
        Ok(data) => {
            STATE.lock().await.on_success();
            let raw_papers = data.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
            let mut papers = Vec::with_capacity(raw_papers.len());
            for item in raw_papers {
                match parse_s2_paper(&item) {
                    Ok(p) => papers.push(p),
                    Err(e) => debug!("Failed to parse S2 paper: {e}"),
                }
            }
            info!("S2: found {} papers for {:?}", papers.len(), query);
            Ok(papers)
        }
        Err(e) => {
            warn!("S2 search failed: {e}");
            Ok(vec![])
        }
    }
}

/// Batch-fetch paper details from Semantic Scholar by ID list.
///
/// Accepts S2 paper IDs, arXiv IDs (prefixed `"ARXIV:"`), or DOIs.
pub async fn batch_fetch_papers(
    paper_ids: &[String],
    api_key: &str,
) -> Result<Vec<Paper>> {
    if paper_ids.is_empty() {
        return Ok(vec![]);
    }

    {
        let mut cb = STATE.lock().await;
        if !cb.should_allow() {
            return Ok(vec![]);
        }
    }

    let url = format!("{BATCH_URL}?fields={FIELDS}");
    let client = build_client()?;
    let mut all_papers = Vec::new();

    for chunk in paper_ids.chunks(BATCH_MAX) {
        rate_limit_wait(!api_key.is_empty()).await;

        let body = serde_json::json!({ "ids": chunk });
        let mut req = client.post(&url).json(&body);
        if !api_key.is_empty() {
            req = req.header("x-api-key", api_key);
        }

        let resp = req.send().await?;
        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            STATE.lock().await.on_429();
            warn!("S2 batch 429 — skipping chunk");
            continue;
        }
        if !status.is_success() {
            warn!("S2 batch HTTP {}", status.as_u16());
            continue;
        }

        let data: Value = resp.json().await?;
        STATE.lock().await.on_success();

        if let Some(arr) = data.as_array() {
            for item in arr {
                if item.is_null() {
                    continue;
                }
                match parse_s2_paper(item) {
                    Ok(p) => all_papers.push(p),
                    Err(e) => debug!("Failed to parse batch S2 paper: {e}"),
                }
            }
        }
    }

    Ok(all_papers)
}

// ---------------------------------------------------------------------------
// Internal HTTP helper with retry
// ---------------------------------------------------------------------------

async fn request_with_retry(url: &str, api_key: &str, max_retries: u32) -> Result<Value> {
    let client = build_client()?;

    for attempt in 0..max_retries {
        let mut req = client.get(url);
        if !api_key.is_empty() {
            req = req.header("x-api-key", api_key);
        }

        let resp = req.send().await?;
        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let tripped = STATE.lock().await.on_429();
            if tripped {
                return Err(anyhow!("S2 circuit breaker tripped"));
            }
            let delay_ms = (1u64 << (attempt + 1)) * 1000;
            warn!("S2 429. Waiting {delay_ms}ms (attempt {}/{max_retries})", attempt + 1);
            tokio::time::sleep(Duration::from_millis(delay_ms.min(20_000))).await;
            continue;
        }

        if !status.is_success() {
            return Err(anyhow!("S2 HTTP {}", status.as_u16()));
        }

        let data: Value = resp.json().await?;
        return Ok(data);
    }

    Err(anyhow!("S2 request exhausted {max_retries} retries for {url}"))
}

fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SEC))
        .user_agent("mol-literature/0.1")
        .build()?)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_s2_paper(item: &Value) -> Result<Paper> {
    let ext_ids = item.get("externalIds").and_then(Value::as_object);
    let authors_raw = item.get("authors").and_then(Value::as_array).cloned().unwrap_or_default();

    let authors: Vec<Author> = authors_raw
        .iter()
        .filter_map(|a| a.get("name").and_then(Value::as_str))
        .map(Author::new)
        .collect();

    let doi = ext_ids
        .and_then(|ids| ids.get("DOI").and_then(Value::as_str))
        .unwrap_or("")
        .to_owned();
    let arxiv_id = ext_ids
        .and_then(|ids| ids.get("ArXiv").and_then(Value::as_str))
        .unwrap_or("")
        .to_owned();

    Ok(Paper {
        paper_id: format!("s2-{}", item.get("paperId").and_then(Value::as_str).unwrap_or("")),
        title: item.get("title").and_then(Value::as_str).unwrap_or("").trim().to_owned(),
        authors,
        year: item.get("year").and_then(Value::as_u64).unwrap_or(0) as u32,
        abstract_text: item.get("abstract").and_then(Value::as_str).unwrap_or("").trim().to_owned(),
        venue: item.get("venue").and_then(Value::as_str).unwrap_or("").trim().to_owned(),
        citation_count: item.get("citationCount").and_then(Value::as_u64).unwrap_or(0) as u32,
        doi,
        arxiv_id,
        url: item.get("url").and_then(Value::as_str).unwrap_or("").trim().to_owned(),
        source: "semantic_scholar".to_owned(),
    })
}

// ---------------------------------------------------------------------------
// URL encoding helper (inline, no extra dep)
// ---------------------------------------------------------------------------

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
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
    fn test_parse_s2_paper_minimal() {
        let item = serde_json::json!({
            "paperId": "abc123",
            "title": "Test Paper",
            "abstract": "Abstract text",
            "year": 2023,
            "venue": "NeurIPS",
            "citationCount": 42,
            "authors": [{"authorId": "1", "name": "Alice Smith"}],
            "externalIds": {"DOI": "10.1234/test", "ArXiv": "2301.00001"},
            "url": "https://example.com/paper"
        });
        let p = parse_s2_paper(&item).unwrap();
        assert_eq!(p.paper_id, "s2-abc123");
        assert_eq!(p.title, "Test Paper");
        assert_eq!(p.year, 2023);
        assert_eq!(p.citation_count, 42);
        assert_eq!(p.doi, "10.1234/test");
        assert_eq!(p.arxiv_id, "2301.00001");
        assert_eq!(p.source, "semantic_scholar");
    }
}
