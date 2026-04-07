//! Google Scholar scraping client with rate limiting and user-agent rotation.
//!
//! Scrapes Google Scholar search results pages without relying on a third-party
//! Python library.  Implements polite rate limiting (configurable delay between
//! requests) and rotates through a set of common User-Agent strings to reduce
//! the chance of bot detection.
//!
//! **Note:** Google Scholar actively blocks automated access.  Use with care
//! and prefer lower request rates.  Consider Semantic Scholar or OpenAlex for
//! programmatic access.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! use mol_web::scholar::ScholarClient;
//!
//! let client = ScholarClient::new();
//! let papers = client.search("attention is all you need", 5).await;
//! for p in &papers {
//!     println!("{} ({}): {} citations", p.title, p.year, p.citation_count);
//! }
//! # Ok(())
//! # }
//! ```

use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// User-agent pool
// ---------------------------------------------------------------------------

static USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
     AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/121.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) \
     AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/119.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) \
     Gecko/20100101 Firefox/121.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) \
     Version/17.2 Safari/605.1.15",
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A paper result from Google Scholar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScholarPaper {
    /// Paper title.
    pub title: String,
    /// Author names as listed on Scholar.
    pub authors: Vec<String>,
    /// Publication year (0 if not found).
    pub year: u32,
    /// Abstract snippet shown on the search result.
    pub abstract_snippet: String,
    /// Number of citations shown on Scholar (0 if not available).
    pub citation_count: u64,
    /// URL to the paper (may be a Scholar redirect).
    pub url: String,
    /// Google Scholar cluster ID (used to retrieve citing papers).
    pub scholar_id: String,
    /// Venue / journal / conference name.
    pub venue: String,
}

/// A Google Scholar author record returned by [`ScholarClient::search_author`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorRecord {
    /// Author's display name.
    pub name: String,
    /// Institutional affiliation shown on Scholar.
    pub affiliation: String,
    /// Google Scholar author profile ID.
    pub scholar_id: String,
    /// Total cited-by count as shown on Scholar (0 if not available).
    pub citedby: u64,
    /// Research interests listed on the author profile.
    pub interests: Vec<String>,
}

impl ScholarPaper {
    /// Returns `true` if the paper has an actionable URL.
    pub fn has_url(&self) -> bool {
        !self.url.is_empty() && (self.url.starts_with("http://") || self.url.starts_with("https://"))
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Google Scholar scraping client.
///
/// Implements:
/// - Configurable inter-request delay (default 2 s).
/// - Round-robin user-agent rotation.
/// - Cached reachability check so that an unreachable Scholar network causes
///   fast early returns instead of repeated timeouts.
pub struct ScholarClient {
    client: Client,
    /// Minimum delay between successive requests in milliseconds.
    inter_request_delay_ms: u64,
    /// Index into USER_AGENTS, rotated on each request.
    ua_index: AtomicUsize,
    /// Monotonic timestamp (ms since startup) of the last request.
    last_request_ms: AtomicU64,
    /// Cached reachability: 0 = unknown, 1 = reachable, 2 = unreachable.
    reachable: AtomicUsize,
}

impl ScholarClient {
    /// Create a new client with default settings (2 s rate limit).
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("failed to build reqwest client");

        Self {
            client,
            inter_request_delay_ms: 2_000,
            ua_index: AtomicUsize::new(0),
            last_request_ms: AtomicU64::new(0),
            reachable: AtomicUsize::new(0),
        }
    }

    /// Builder: set the minimum delay between requests.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.inter_request_delay_ms = delay.as_millis() as u64;
        self
    }

    /// Search Google Scholar for papers matching `query`, returning up to
    /// `limit` results.
    ///
    /// Returns an empty `Vec` if Scholar is unreachable or returns a CAPTCHA.
    pub async fn search(&self, query: &str, limit: usize) -> Vec<ScholarPaper> {
        if !self.check_reachable().await {
            return Vec::new();
        }

        let mut results = Vec::new();
        let page_size = 10usize;

        let mut start = 0usize;
        while results.len() < limit {
            self.rate_limit().await;

            let q_enc: String =
                url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
            let url = format!(
                "https://scholar.google.com/scholar?q={q_enc}&start={}&num={}",
                start,
                page_size.min(limit - results.len()),
            );

            let ua = self.next_user_agent();
            debug!("Scholar GET {url} (ua: {ua})");

            let resp = match self.client.get(&url).header("User-Agent", ua).send().await {
                Ok(r) => r,
                Err(e) => {
                    warn!("Scholar request failed: {e}");
                    self.reachable.store(2, Ordering::Relaxed);
                    break;
                }
            };

            if !resp.status().is_success() {
                warn!("Scholar returned HTTP {}", resp.status());
                break;
            }

            let html = match resp.text().await {
                Ok(h) => h,
                Err(e) => {
                    warn!("Scholar body read failed: {e}");
                    break;
                }
            };

            // Detect CAPTCHA / robot check pages
            if html.contains("please show you&#39;re not a robot")
                || html.contains("gs_captcha_ccl")
                || html.contains("recaptcha")
            {
                warn!("Google Scholar CAPTCHA detected — stopping");
                self.reachable.store(2, Ordering::Relaxed);
                break;
            }

            let page_results = parse_scholar_html(&html);
            if page_results.is_empty() {
                break;
            }

            let took = page_results.len();
            results.extend(page_results.into_iter().take(limit - results.len()));

            if took < page_size {
                // Fewer results than requested — no more pages
                break;
            }
            start += page_size;
        }

        debug!("Scholar: {} results for {:?}", results.len(), query);
        results
    }

    /// Retrieve papers that cite the given Google Scholar cluster ID.
    ///
    /// Uses Scholar's `cites=` query parameter.  Returns an empty `Vec` if
    /// Scholar is unreachable or `scholar_id` is empty.
    pub async fn get_citations(&self, scholar_id: &str, limit: usize) -> Vec<ScholarPaper> {
        if scholar_id.is_empty() {
            return Vec::new();
        }
        if !self.check_reachable().await {
            return Vec::new();
        }

        let mut results = Vec::new();
        let page_size = 10usize;
        let mut start = 0usize;

        while results.len() < limit {
            self.rate_limit().await;

            let url = format!(
                "https://scholar.google.com/scholar?cites={}&start={}&num={}",
                scholar_id,
                start,
                page_size.min(limit - results.len()),
            );

            let ua = self.next_user_agent();
            debug!("Scholar citations GET {url} (ua: {ua})");

            let resp = match self.client.get(&url).header("User-Agent", ua).send().await {
                Ok(r) => r,
                Err(e) => {
                    warn!("Scholar citations request failed: {e}");
                    self.reachable.store(2, Ordering::Relaxed);
                    break;
                }
            };

            if !resp.status().is_success() {
                warn!("Scholar citations returned HTTP {}", resp.status());
                break;
            }

            let html = match resp.text().await {
                Ok(h) => h,
                Err(e) => {
                    warn!("Scholar citations body read failed: {e}");
                    break;
                }
            };

            if html.contains("please show you&#39;re not a robot")
                || html.contains("gs_captcha_ccl")
                || html.contains("recaptcha")
            {
                warn!("Google Scholar CAPTCHA detected — stopping");
                self.reachable.store(2, Ordering::Relaxed);
                break;
            }

            let page_results = parse_scholar_html(&html);
            if page_results.is_empty() {
                break;
            }

            let took = page_results.len();
            results.extend(page_results.into_iter().take(limit - results.len()));

            if took < page_size {
                break;
            }
            start += page_size;
        }

        debug!(
            "Scholar: {} citations for scholar_id={:?}",
            results.len(),
            scholar_id
        );
        results
    }

    /// Search for authors on Google Scholar.
    ///
    /// Returns up to 5 author records (same cap as the Python implementation).
    /// Each record contains `name`, `affiliation`, `scholar_id`, `citedby`, and
    /// `interests`.
    pub async fn search_author(&self, name: &str) -> Vec<AuthorRecord> {
        if !self.check_reachable().await {
            return Vec::new();
        }

        self.rate_limit().await;

        let q_enc: String = url::form_urlencoded::byte_serialize(name.as_bytes()).collect();
        let url = format!("https://scholar.google.com/scholar?q=author:{q_enc}&hl=en");

        let ua = self.next_user_agent();
        debug!("Scholar author search GET {url}");

        let resp = match self.client.get(&url).header("User-Agent", ua).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Scholar author search request failed: {e}");
                self.reachable.store(2, Ordering::Relaxed);
                return Vec::new();
            }
        };

        if !resp.status().is_success() {
            warn!("Scholar author search returned HTTP {}", resp.status());
            return Vec::new();
        }

        let html = match resp.text().await {
            Ok(h) => h,
            Err(e) => {
                warn!("Scholar author search body read failed: {e}");
                return Vec::new();
            }
        };

        if html.contains("please show you&#39;re not a robot")
            || html.contains("gs_captcha_ccl")
            || html.contains("recaptcha")
        {
            warn!("Google Scholar CAPTCHA detected — stopping");
            self.reachable.store(2, Ordering::Relaxed);
            return Vec::new();
        }

        parse_author_html(&html)
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Return the next user-agent string in the rotation.
    fn next_user_agent(&self) -> &'static str {
        let idx = self.ua_index.fetch_add(1, Ordering::Relaxed) % USER_AGENTS.len();
        USER_AGENTS[idx]
    }

    /// Sleep until the configured minimum delay since the last request has
    /// elapsed.
    async fn rate_limit(&self) {
        let now_ms = {
            // Use a simple monotonic counter via Instant
            use std::sync::OnceLock;
            static EPOCH: OnceLock<Instant> = OnceLock::new();
            let epoch = EPOCH.get_or_init(Instant::now);
            epoch.elapsed().as_millis() as u64
        };

        let last = self.last_request_ms.load(Ordering::Relaxed);
        if last > 0 {
            let elapsed = now_ms.saturating_sub(last);
            if elapsed < self.inter_request_delay_ms {
                let sleep_ms = self.inter_request_delay_ms - elapsed;
                tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
            }
        }

        // Record the time we are about to make the request
        let now_ms2 = {
            use std::sync::OnceLock;
            static EPOCH2: OnceLock<Instant> = OnceLock::new();
            let epoch = EPOCH2.get_or_init(Instant::now);
            epoch.elapsed().as_millis() as u64
        };
        self.last_request_ms.store(now_ms2, Ordering::Relaxed);
    }

    /// Check if Scholar is reachable (cached after first probe).
    async fn check_reachable(&self) -> bool {
        match self.reachable.load(Ordering::Relaxed) {
            1 => return true,
            2 => return false,
            _ => {}
        }

        let ua = self.next_user_agent();
        let ok = self
            .client
            .head("https://scholar.google.com/")
            .header("User-Agent", ua)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().as_u16() < 500)
            .unwrap_or(false);

        self.reachable.store(if ok { 1 } else { 2 }, Ordering::Relaxed);
        if !ok {
            warn!("Google Scholar unreachable — disabling for this session");
        }
        ok
    }
}

impl Default for ScholarClient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HTML parsing
// ---------------------------------------------------------------------------

fn parse_scholar_html(html: &str) -> Vec<ScholarPaper> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    // Each result is in a <div class="gs_r gs_or gs_scl"> or similar
    let result_sel = match Selector::parse("div.gs_r.gs_or") {
        Ok(s) => s,
        Err(_) => return results,
    };

    let title_sel = Selector::parse("h3.gs_rt a").ok();
    let meta_sel = Selector::parse(".gs_a").ok();
    let snippet_sel = Selector::parse(".gs_rs").ok();
    let cites_sel = Selector::parse("a[href*='cites=']").ok();
    let cluster_sel = Selector::parse("a[href*='cluster=']").ok();

    for result_el in document.select(&result_sel) {
        let (title, url) = if let Some(sel) = &title_sel {
            let link = result_el.select(sel).next();
            let title = link
                .map(|el| el.text().collect::<String>().trim().to_owned())
                .unwrap_or_default();
            let url = link
                .and_then(|el| el.value().attr("href"))
                .unwrap_or("")
                .to_owned();
            (title, url)
        } else {
            (String::new(), String::new())
        };

        if title.is_empty() {
            continue;
        }

        // Meta line: "Author, Author - Venue, Year - Publisher"
        let meta_text = meta_sel
            .as_ref()
            .and_then(|s| result_el.select(s).next())
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();

        let (authors, year, venue) = parse_meta_line(&meta_text);

        let abstract_snippet = snippet_sel
            .as_ref()
            .and_then(|s| result_el.select(s).next())
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        // Citation count from "Cited by N" link
        let citation_count = cites_sel
            .as_ref()
            .and_then(|s| result_el.select(s).next())
            .map(|el| el.text().collect::<String>())
            .and_then(|t| {
                t.replace("Cited by", "")
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0);

        // Scholar cluster ID for citation traversal
        let scholar_id = cluster_sel
            .as_ref()
            .and_then(|s| result_el.select(s).next())
            .and_then(|el| el.value().attr("href"))
            .and_then(|href| {
                href.split("cluster=")
                    .nth(1)
                    .map(|s| s.split('&').next().unwrap_or(s).to_owned())
            })
            .unwrap_or_default();

        results.push(ScholarPaper {
            title,
            authors,
            year,
            abstract_snippet,
            citation_count,
            url,
            scholar_id,
            venue,
        });
    }

    results
}

/// Parse the Google Scholar meta line "A, B, C - Venue, 2023 - Publisher"
/// and return `(authors, year, venue)`.
fn parse_meta_line(meta: &str) -> (Vec<String>, u32, String) {
    // Meta is typically: "Author1, Author2 - Venue, Year - Publisher"
    // Split on " - " to get parts
    let parts: Vec<&str> = meta.splitn(3, " - ").collect();

    let authors = parts
        .first()
        .map(|s| {
            s.split(',')
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let (year, venue) = parts.get(1).map(|s| parse_venue_year(s)).unwrap_or((0, String::new()));

    (authors, year, venue)
}

/// Parse Google Scholar author search HTML into up to 5 [`AuthorRecord`]s.
///
/// Scholar renders author cards as `<div class="gs_ai gs_scl gs_afr">` elements.
fn parse_author_html(html: &str) -> Vec<AuthorRecord> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    let card_sel = match Selector::parse("div.gs_ai.gs_scl") {
        Ok(s) => s,
        Err(_) => return results,
    };
    let name_sel = Selector::parse("h3.gs_ai_name a").ok();
    let affil_sel = Selector::parse("div.gs_ai_aff").ok();
    let cited_sel = Selector::parse("div.gs_ai_cby").ok();
    let interests_sel = Selector::parse("div.gs_ai_int a").ok();

    for card in document.select(&card_sel) {
        if results.len() >= 5 {
            break;
        }

        let (name, scholar_id) = if let Some(sel) = &name_sel {
            let link = card.select(sel).next();
            let name = link
                .map(|el| el.text().collect::<String>().trim().to_owned())
                .unwrap_or_default();
            // href="/citations?user=XXXXX&hl=en"
            let scholar_id = link
                .and_then(|el| el.value().attr("href"))
                .and_then(|href| {
                    href.split("user=")
                        .nth(1)
                        .map(|s| s.split('&').next().unwrap_or(s).to_owned())
                })
                .unwrap_or_default();
            (name, scholar_id)
        } else {
            (String::new(), String::new())
        };

        if name.is_empty() {
            continue;
        }

        let affiliation = affil_sel
            .as_ref()
            .and_then(|s| card.select(s).next())
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();

        let citedby = cited_sel
            .as_ref()
            .and_then(|s| card.select(s).next())
            .map(|el| el.text().collect::<String>())
            .and_then(|t| {
                // "Cited by 12345"
                t.replace("Cited by", "")
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0);

        let interests = interests_sel
            .as_ref()
            .map(|s| {
                card.select(s)
                    .map(|el| el.text().collect::<String>().trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        results.push(AuthorRecord {
            name,
            affiliation,
            scholar_id,
            citedby,
            interests,
        });
    }

    results
}

/// Extract year and venue from "Venue Name, 2023" strings.
fn parse_venue_year(s: &str) -> (u32, String) {
    // Try to find a 4-digit year
    let mut year = 0u32;
    let mut venue_parts: Vec<&str> = Vec::new();

    for part in s.split(',') {
        let trimmed = part.trim();
        if trimmed.len() == 4 {
            if let Ok(y) = trimmed.parse::<u32>() {
                if y >= 1900 && y <= 2100 {
                    year = y;
                    continue;
                }
            }
        }
        if !trimmed.is_empty() {
            venue_parts.push(trimmed);
        }
    }

    (year, venue_parts.join(", "))
}
