//! arXiv API client using the Atom feed endpoint.
//!
//! Uses `reqwest` for HTTP and `quick-xml` for Atom feed parsing.
//!
//! Public API
//! ----------
//! - [`search_arxiv`] – keyword search returning `Vec<Paper>`
//! - [`get_paper_by_id`] – fetch a single paper by arXiv ID
//! - [`search_arxiv_advanced`] – field-specific search (title, author, etc.)
//!
//! Rate-limiting: arXiv requires ≥ 3 seconds between requests.  A simple
//! in-process delay is applied before each request.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::models::{Author, Paper};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BASE_URL: &str = "https://export.arxiv.org/api/query";
const MIN_REQUEST_GAP_MS: u64 = 3_100; // arXiv requires >= 3s between requests
const DEFAULT_TIMEOUT_SEC: u64 = 30;

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

/// Global last-request timestamp in milliseconds since UNIX epoch.
static LAST_REQUEST_MS: AtomicU64 = AtomicU64::new(0);

/// Per-process rate-limit mutex so concurrent async tasks serialise requests.
static RATE_LOCK: std::sync::LazyLock<Arc<Mutex<()>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(())));

async fn rate_limit_wait() {
    let _guard = RATE_LOCK.lock().await;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_REQUEST_MS.load(Ordering::Relaxed);
    let elapsed = now_ms.saturating_sub(last);
    if elapsed < MIN_REQUEST_GAP_MS {
        let wait = MIN_REQUEST_GAP_MS - elapsed;
        debug!("arXiv rate limit: waiting {wait}ms");
        tokio::time::sleep(Duration::from_millis(wait)).await;
    }
    LAST_REQUEST_MS.store(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        Ordering::Relaxed,
    );
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Search arXiv for papers matching `query`.
///
/// # Arguments
/// * `query`   – Free-text query, supports arXiv field syntax (`ti:`, `au:`, `cat:`…)
/// * `limit`   – Maximum results (capped at 300).
/// * `sort_by` – `"relevance"`, `"submitted_date"`, or `"last_updated"`.
/// * `year_min` – If > 0, filter out papers published before this year.
pub async fn search_arxiv(
    query: &str,
    limit: usize,
    sort_by: &str,
    year_min: u32,
) -> Result<Vec<Paper>> {
    let limit = limit.min(300);
    let sort_by_param = match sort_by {
        "submitted_date" => "submittedDate",
        "last_updated" => "lastUpdatedDate",
        _ => "relevance",
    };

    let url = format!(
        "{BASE_URL}?search_query={query}&start=0&max_results={limit}\
         &sortBy={sort_by_param}&sortOrder=descending",
        query = urlencoding::encode(query),
    );

    let xml = fetch_xml(&url).await?;
    let papers = parse_atom_feed(&xml)?;

    let filtered: Vec<Paper> = if year_min > 0 {
        papers.into_iter().filter(|p| p.year >= year_min).collect()
    } else {
        papers
    };

    info!("arXiv: found {} papers for {:?}", filtered.len(), query);
    Ok(filtered)
}

/// Fetch a single paper by arXiv ID (e.g. `"2301.00001"` or `"2301.00001v2"`).
pub async fn get_paper_by_id(arxiv_id: &str) -> Result<Option<Paper>> {
    let url = format!("{BASE_URL}?id_list={arxiv_id}");
    let xml = fetch_xml(&url).await?;
    let papers = parse_atom_feed(&xml)?;
    Ok(papers.into_iter().next())
}

/// Advanced arXiv search using field-specific queries.
///
/// Any combination of `title`, `author`, `abstract_text`, `category` may be
/// specified.  Fields are joined with `AND`.
pub async fn search_arxiv_advanced(
    title: &str,
    author: &str,
    abstract_text: &str,
    category: &str,
    limit: usize,
    year_min: u32,
) -> Result<Vec<Paper>> {
    let mut parts = Vec::new();
    if !title.is_empty() {
        parts.push(format!("ti:{title}"));
    }
    if !author.is_empty() {
        parts.push(format!("au:{author}"));
    }
    if !abstract_text.is_empty() {
        parts.push(format!("abs:{abstract_text}"));
    }
    if !category.is_empty() {
        parts.push(format!("cat:{category}"));
    }
    if parts.is_empty() {
        return Ok(vec![]);
    }
    let query = parts.join("+AND+");
    search_arxiv(&query, limit, "relevance", year_min).await
}

// ---------------------------------------------------------------------------
// HTTP helper
// ---------------------------------------------------------------------------

async fn fetch_xml(url: &str) -> Result<String> {
    rate_limit_wait().await;

    let client = Client::builder()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SEC))
        .user_agent("mol-literature/0.1")
        .build()?;

    debug!("GET {url}");
    let resp = client.get(url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("arXiv HTTP {status}: {body}"));
    }

    Ok(resp.text().await?)
}

// ---------------------------------------------------------------------------
// Atom feed parser
// ---------------------------------------------------------------------------

/// Parse an arXiv Atom feed and return a list of [`Paper`]s.
fn parse_atom_feed(xml: &str) -> Result<Vec<Paper>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut papers: Vec<Paper> = Vec::new();

    // Entry-level state
    let mut in_entry = false;
    let mut current: EntryBuilder = EntryBuilder::default();
    let mut current_tag = String::new();
    let mut in_author = false;
    let mut author_name_buf = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "entry" => {
                        in_entry = true;
                        current = EntryBuilder::default();
                    }
                    "author" if in_entry => {
                        in_author = true;
                        author_name_buf.clear();
                    }
                    _ => {
                        current_tag = name;
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = local_name(e.name().as_ref());
                if in_entry {
                    match name.as_str() {
                        "primary_category" | "category" => {
                            // <arxiv:primary_category term="cs.LG"/>
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"term" && current.primary_category.is_empty() {
                                    current.primary_category = String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        }
                        "link" => {
                            // <link rel="related" type="text/html" href="..."/>
                            let mut rel = String::new();
                            let mut href = String::new();
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"rel" => rel = String::from_utf8_lossy(&attr.value).to_string(),
                                    b"href" => href = String::from_utf8_lossy(&attr.value).to_string(),
                                    _ => {}
                                }
                            }
                            if rel == "alternate" || (rel.is_empty() && !href.is_empty()) {
                                current.url = href;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_entry {
                    let text = e.unescape().unwrap_or_default().trim().to_owned();
                    if in_author && current_tag == "name" {
                        author_name_buf.push_str(&text);
                    } else {
                        match current_tag.as_str() {
                            "id" => current.id = text,
                            "title" => current.title = normalise_whitespace(&text),
                            "summary" => current.summary = text,
                            "published" => current.published = text,
                            "doi" => current.doi = text,
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "entry" if in_entry => {
                        in_entry = false;
                        if let Some(paper) = current.build() {
                            papers.push(paper);
                        }
                        current = EntryBuilder::default();
                    }
                    "author" if in_author => {
                        in_author = false;
                        if !author_name_buf.is_empty() {
                            current.authors.push(Author::new(author_name_buf.trim()));
                        }
                        author_name_buf.clear();
                    }
                    _ => {}
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("arXiv XML parse error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(papers)
}

// ---------------------------------------------------------------------------
// Entry builder
// ---------------------------------------------------------------------------

#[derive(Default)]
struct EntryBuilder {
    id: String,
    title: String,
    summary: String,
    published: String,
    doi: String,
    url: String,
    primary_category: String,
    authors: Vec<Author>,
}

impl EntryBuilder {
    fn build(self) -> Option<Paper> {
        // Extract arXiv ID from the entry id URL:
        // http://arxiv.org/abs/2301.00001v1
        let arxiv_id = extract_arxiv_id(&self.id);

        let year: u32 = self
            .published
            .split('-')
            .next()
            .and_then(|y| y.parse().ok())
            .unwrap_or(0);

        let url = if !arxiv_id.is_empty() {
            format!("https://arxiv.org/abs/{arxiv_id}")
        } else if !self.url.is_empty() {
            self.url
        } else {
            self.id.clone()
        };

        let paper_id = if !arxiv_id.is_empty() {
            format!("arxiv-{arxiv_id}")
        } else {
            format!("arxiv-{}", &self.id)
        };

        Some(Paper {
            paper_id,
            title: self.title,
            authors: self.authors,
            year,
            abstract_text: self.summary,
            venue: self.primary_category,
            citation_count: 0,
            doi: self.doi,
            arxiv_id,
            url,
            source: "arxiv".to_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the numeric arXiv ID from a full entry URL or bare ID.
///
/// Handles: `http://arxiv.org/abs/2301.00001v1`, `2301.00001`, `2301.00001v2`.
fn extract_arxiv_id(raw: &str) -> String {
    // Match patterns like 2301.00001 or 2301.00001v2
    let re = regex::Regex::new(r"(\d{4}\.\d{4,5})(?:v\d+)?").unwrap();
    re.captures(raw)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
        .unwrap_or_default()
}

/// Strip namespace prefix from an XML element name byte slice.
fn local_name(name: &[u8]) -> String {
    let s = std::str::from_utf8(name).unwrap_or("");
    // Clark notation: "{http://...}localname"
    if let Some(brace) = s.rfind('}') {
        return s[brace + 1..].to_owned();
    }
    // Prefix notation: "prefix:localname"
    if let Some(colon) = s.find(':') {
        return s[colon + 1..].to_owned();
    }
    s.to_owned()
}

/// Replace runs of whitespace (including newlines) with a single space.
fn normalise_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// urlencoding (inline, avoids extra dep)
// ---------------------------------------------------------------------------

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len() * 3);
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
                | b'-' | b'_' | b'.' | b'~' | b':' | b'+' => out.push(b as char),
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_arxiv_id() {
        assert_eq!(extract_arxiv_id("http://arxiv.org/abs/2301.00001v1"), "2301.00001");
        assert_eq!(extract_arxiv_id("2301.00001v2"), "2301.00001");
        assert_eq!(extract_arxiv_id("2301.00001"), "2301.00001");
        assert_eq!(extract_arxiv_id("not-an-id"), "");
    }

    #[test]
    fn test_local_name() {
        assert_eq!(local_name(b"atom:title"), "title");
        assert_eq!(local_name(b"title"), "title");
        assert_eq!(local_name(b"{http://www.w3.org/2005/Atom}title"), "title");
    }

    #[test]
    fn test_parse_minimal_atom() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry>
    <id>http://arxiv.org/abs/2301.00001v1</id>
    <title>Test Paper Title</title>
    <summary>An abstract.</summary>
    <published>2023-01-01T00:00:00Z</published>
    <author><name>Alice Smith</name></author>
    <arxiv:primary_category term="cs.LG"/>
    <link rel="alternate" href="https://arxiv.org/abs/2301.00001"/>
  </entry>
</feed>"#;
        let papers = parse_atom_feed(xml).unwrap();
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.title, "Test Paper Title");
        assert_eq!(p.arxiv_id, "2301.00001");
        assert_eq!(p.year, 2023);
        assert_eq!(p.authors.len(), 1);
        assert_eq!(p.authors[0].name, "Alice Smith");
        assert_eq!(p.venue, "cs.LG");
        assert_eq!(p.source, "arxiv");
    }
}
