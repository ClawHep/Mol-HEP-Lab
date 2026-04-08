//! Citation verification engine — detect hallucinated references.
//!
//! Verifies each BibTeX entry against real academic APIs using a three-layer
//! strategy:
//!
//!   L1: DOI resolution via CrossRef (fast, generous limits)
//!   L2: OpenAlex title search (10 000/day polite pool)
//!   L3a: arXiv ID lookup
//!   L3b: Semantic Scholar title search (last resort)
//!
//! # Classifications
//! - `VERIFIED`    – API confirms existence + title similarity ≥ 0.80
//! - `SUSPICIOUS`  – Found a paper but metadata diverges (0.50 ≤ sim < 0.80)
//! - `HALLUCINATED`– Not found via any API or sim < 0.50
//! - `SKIPPED`     – Cannot be verified (no title, all APIs unreachable)

use std::time::Duration;

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::models::Paper;

// ---------------------------------------------------------------------------
// API endpoints
// ---------------------------------------------------------------------------

const CROSSREF_API: &str = "https://api.crossref.org/works";
const DATACITE_API: &str = "https://api.datacite.org/dois";
const OPENALEX_API: &str = "https://api.openalex.org/works";
const ARXIV_API: &str = "https://export.arxiv.org/api/query";
const OPENALEX_EMAIL: &str = "mol-hep-lab@users.noreply.github.com";
const HTTP_TIMEOUT_SEC: u64 = 20;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Verification outcome for a single citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    Verified,
    Suspicious,
    Hallucinated,
    Skipped,
}

impl std::fmt::Display for VerifyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verified => write!(f, "verified"),
            Self::Suspicious => write!(f, "suspicious"),
            Self::Hallucinated => write!(f, "hallucinated"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Verification result for one BibTeX entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationResult {
    pub cite_key: String,
    pub title: String,
    pub status: VerifyStatus,
    /// 0.0–1.0
    pub confidence: f64,
    /// `"doi"` | `"arxiv_id"` | `"openalex"` | `"title_search"` | `"skipped"`
    pub method: String,
    pub details: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_paper: Option<MatchedPaper>,
}

/// Compact summary of a matched paper (to avoid embedding the full [`Paper`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedPaper {
    pub title: String,
    pub authors: Vec<String>,
    pub year: u32,
    pub source: String,
}

impl From<&Paper> for MatchedPaper {
    fn from(p: &Paper) -> Self {
        Self {
            title: p.title.clone(),
            authors: p.authors.iter().map(|a| a.name.clone()).collect(),
            year: p.year,
            source: p.source.clone(),
        }
    }
}

/// Aggregate report for all citations in a BibTeX block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub total: usize,
    pub verified: usize,
    pub suspicious: usize,
    pub hallucinated: usize,
    pub skipped: usize,
    pub results: Vec<CitationResult>,
}

impl VerificationReport {
    /// Fraction of verifiable citations that are verified (0.0–1.0).
    pub fn integrity_score(&self) -> f64 {
        let verifiable = self.total.saturating_sub(self.skipped);
        if verifiable == 0 {
            return 1.0;
        }
        (self.verified as f64 / verifiable as f64 * 1000.0).round() / 1000.0
    }
}

// ---------------------------------------------------------------------------
// BibTeX parsing
// ---------------------------------------------------------------------------

/// Parse BibTeX text into a list of field maps.
///
/// Each map contains at least `"key"` and `"type"`, plus any parsed fields
/// (`"title"`, `"author"`, `"year"`, `"doi"`, `"eprint"`, `"url"`, …).
pub fn parse_bibtex_entries(bib_text: &str) -> Vec<std::collections::HashMap<String, String>> {
    let mut entries = Vec::new();

    // Match @Type{key, ...fields... }
    let entry_re = regex::Regex::new(r"(?s)@(\w+)\s*\{\s*([^,\s]+)\s*,\s*(.*?)\n\s*\}").unwrap();
    let field_re = regex::Regex::new(r"(\w+)\s*=\s*\{((?:[^{}]|\{[^{}]*\})*)\}").unwrap();

    for cap in entry_re.captures_iter(bib_text) {
        let mut entry = std::collections::HashMap::new();
        entry.insert("type".to_owned(), cap[1].to_lowercase());
        entry.insert("key".to_owned(), cap[2].trim().to_owned());
        let body = &cap[3];
        for fc in field_re.captures_iter(body) {
            entry.insert(fc[1].to_lowercase(), fc[2].trim().to_owned());
        }
        entries.push(entry);
    }
    entries
}

// ---------------------------------------------------------------------------
// Title similarity (word-overlap Jaccard)
// ---------------------------------------------------------------------------

/// Word-overlap Jaccard similarity between two titles (0.0–1.0).
pub fn title_similarity(a: &str, b: &str) -> f64 {
    fn words(t: &str) -> std::collections::HashSet<String> {
        t.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect()
    }

    let wa = words(a);
    let wb = words(b);
    if wa.is_empty() || wb.is_empty() {
        return 0.0;
    }
    let intersection = wa.intersection(&wb).count();
    let max_len = wa.len().max(wb.len());
    intersection as f64 / max_len as f64
}

// ---------------------------------------------------------------------------
// Layer implementations
// ---------------------------------------------------------------------------

/// L1: DOI verification via CrossRef (with DataCite fallback for arXiv DOIs).
async fn verify_by_doi(client: &Client, doi: &str, expected_title: &str) -> Option<CitationResult> {
    let encoded = urlencoding(doi);
    let url = format!("{CROSSREF_API}/{encoded}");

    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                // Try DataCite for arXiv/DataCite DOIs
                if doi.starts_with("10.48550/") || doi.starts_with("10.5281/") {
                    if let Some(r) = verify_by_datacite(client, doi, expected_title).await {
                        return Some(r);
                    }
                }
                return Some(CitationResult {
                    cite_key: String::new(),
                    title: expected_title.to_owned(),
                    status: VerifyStatus::Hallucinated,
                    confidence: 0.9,
                    method: "doi".to_owned(),
                    details: format!("DOI {doi} not found (HTTP 404)"),
                    matched_paper: None,
                });
            }
            if !status.is_success() {
                debug!("CrossRef HTTP {} for DOI {doi}", status.as_u16());
                return None;
            }
            let data: Value = resp.json().await.ok()?;
            let message = data.get("message")?;
            let titles = message.get("title").and_then(Value::as_array)?;
            let found_title = titles.first().and_then(Value::as_str).unwrap_or("");
            if found_title.is_empty() {
                return Some(CitationResult {
                    cite_key: String::new(),
                    title: expected_title.to_owned(),
                    status: VerifyStatus::Verified,
                    confidence: 0.85,
                    method: "doi".to_owned(),
                    details: format!("DOI {doi} resolves via CrossRef (no title comparison)"),
                    matched_paper: None,
                });
            }
            let sim = title_similarity(expected_title, found_title);
            classify_by_sim(sim, expected_title, found_title, "doi")
        }
        Err(e) => {
            debug!("CrossRef request failed for DOI {doi}: {e}");
            None
        }
    }
}

async fn verify_by_datacite(client: &Client, doi: &str, expected_title: &str) -> Option<CitationResult> {
    let encoded = urlencoding(doi);
    let url = format!("{DATACITE_API}/{encoded}");

    let resp = client.get(&url).send().await.ok()?;
    let status = resp.status();

    if status == reqwest::StatusCode::NOT_FOUND {
        return Some(CitationResult {
            cite_key: String::new(),
            title: expected_title.to_owned(),
            status: VerifyStatus::Hallucinated,
            confidence: 0.9,
            method: "doi".to_owned(),
            details: format!("DOI {doi} not found via CrossRef or DataCite"),
            matched_paper: None,
        });
    }
    if !status.is_success() {
        return None;
    }
    let data: Value = resp.json().await.ok()?;
    let attrs = data.get("data")?.get("attributes")?;
    let titles = attrs.get("titles").and_then(Value::as_array)?;
    let found_title = titles
        .first()
        .and_then(|t| t.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if found_title.is_empty() {
        return Some(CitationResult {
            cite_key: String::new(),
            title: expected_title.to_owned(),
            status: VerifyStatus::Verified,
            confidence: 0.85,
            method: "doi".to_owned(),
            details: format!("DOI {doi} resolves via DataCite (no title comparison)"),
            matched_paper: None,
        });
    }
    let sim = title_similarity(expected_title, found_title);
    classify_by_sim(sim, expected_title, found_title, "doi")
}

/// L2: OpenAlex title search.
async fn verify_by_openalex(client: &Client, title: &str) -> Option<CitationResult> {
    let encoded = urlencoding(title);
    let url = format!(
        "{OPENALEX_API}?filter=title.search:{encoded}&per_page=5&mailto={OPENALEX_EMAIL}"
    );

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let data: Value = resp.json().await.ok()?;
    let results = data.get("results").and_then(Value::as_array)?;

    if results.is_empty() {
        return Some(CitationResult {
            cite_key: String::new(),
            title: title.to_owned(),
            status: VerifyStatus::Hallucinated,
            confidence: 0.7,
            method: "openalex".to_owned(),
            details: "No results found via OpenAlex".to_owned(),
            matched_paper: None,
        });
    }

    let (best_sim, best_title) = results
        .iter()
        .filter_map(|r| r.get("title").and_then(Value::as_str))
        .map(|t| (title_similarity(title, t), t))
        .fold((0.0_f64, ""), |acc, x| if x.0 > acc.0 { x } else { acc });

    if best_sim < 1e-9 {
        return Some(CitationResult {
            cite_key: String::new(),
            title: title.to_owned(),
            status: VerifyStatus::Hallucinated,
            confidence: 0.7,
            method: "openalex".to_owned(),
            details: "No close match found via OpenAlex".to_owned(),
            matched_paper: None,
        });
    }

    classify_by_sim(best_sim, title, best_title, "openalex")
}

/// L3a: arXiv ID verification.
async fn verify_by_arxiv_id(client: &Client, arxiv_id: &str, expected_title: &str) -> Option<CitationResult> {
    let url = format!("{ARXIV_API}?id_list={arxiv_id}&max_results=1");
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;

    // Quick XML scan for the title element
    let found_title = extract_atom_title(&text)?;

    // arXiv returns an error entry when the ID is invalid
    if found_title.to_lowercase() == "error" || found_title.is_empty() {
        return Some(CitationResult {
            cite_key: String::new(),
            title: expected_title.to_owned(),
            status: VerifyStatus::Hallucinated,
            confidence: 0.9,
            method: "arxiv_id".to_owned(),
            details: format!("arXiv ID {arxiv_id} not found or returned error"),
            matched_paper: None,
        });
    }

    let sim = title_similarity(expected_title, &found_title);
    classify_by_sim(sim, expected_title, &found_title, "arxiv_id")
}

/// L3b: Semantic Scholar title search via our unified search module.
async fn verify_by_title_search(title: &str, s2_api_key: &str) -> Option<CitationResult> {
    let opts = crate::search::SearchOptions {
        limit: 5,
        s2_api_key: s2_api_key.to_owned(),
        ..Default::default()
    };
    let results = crate::search::search_papers(title, &opts).await.ok()?;

    if results.is_empty() {
        return Some(CitationResult {
            cite_key: String::new(),
            title: title.to_owned(),
            status: VerifyStatus::Hallucinated,
            confidence: 0.7,
            method: "title_search".to_owned(),
            details: "No results found via Semantic Scholar + arXiv".to_owned(),
            matched_paper: None,
        });
    }

    let (best_sim, best_paper) = results
        .iter()
        .map(|p| (title_similarity(title, &p.title), p))
        .fold((0.0_f64, &results[0]), |acc, x| if x.0 > acc.0 { x } else { acc });

    let mut r = classify_by_sim(best_sim, title, &best_paper.title, "title_search")?;
    r.matched_paper = Some(MatchedPaper::from(best_paper));
    Some(r)
}

// ---------------------------------------------------------------------------
// Shared classification helper
// ---------------------------------------------------------------------------

fn classify_by_sim(sim: f64, expected: &str, found: &str, method: &str) -> Option<CitationResult> {
    let (status, details) = if sim >= 0.80 {
        (
            VerifyStatus::Verified,
            format!("Confirmed via {method}: '{found}'"),
        )
    } else if sim >= 0.50 {
        (
            VerifyStatus::Suspicious,
            format!("{method}: title differs (sim={sim:.2}): '{found}'"),
        )
    } else {
        (
            VerifyStatus::Hallucinated,
            format!("{method}: title mismatch (sim={sim:.2}): '{found}'"),
        )
    };

    Some(CitationResult {
        cite_key: String::new(),
        title: expected.to_owned(),
        status,
        confidence: sim,
        method: method.to_owned(),
        details,
        matched_paper: None,
    })
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Verify all BibTeX entries against real academic APIs.
///
/// Returns a [`VerificationReport`] with per-entry results.
pub async fn verify_citations(
    bib_text: &str,
    s2_api_key: &str,
    inter_verify_delay_ms: u64,
) -> Result<VerificationReport> {
    let entries = parse_bibtex_entries(bib_text);
    let total = entries.len();
    let mut report = VerificationReport {
        total,
        verified: 0,
        suspicious: 0,
        hallucinated: 0,
        skipped: 0,
        results: Vec::with_capacity(total),
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SEC))
        .user_agent("mol-literature/0.1 (mailto:mol-hep-lab@users.noreply.github.com)")
        .build()?;

    let cache = crate::cache::FileCache::default_cache();

    let start = std::time::Instant::now();
    let timeout_sec = 300;

    for (i, entry) in entries.iter().enumerate() {
        if start.elapsed().as_secs() > timeout_sec {
            warn!(
                "Verification timeout. Marking remaining {}/{} as SKIPPED.",
                total - i, total
            );
            for remaining in &entries[i..] {
                let k = remaining.get("key").cloned().unwrap_or_else(|| format!("unknown_{i}"));
                let t = remaining.get("title").cloned().unwrap_or_default();
                report.results.push(CitationResult {
                    cite_key: k,
                    title: t,
                    status: VerifyStatus::Skipped,
                    confidence: 0.0,
                    method: "skipped".to_owned(),
                    details: "Verification timeout exceeded".to_owned(),
                    matched_paper: None,
                });
                report.skipped += 1;
            }
            break;
        }

        let key = entry.get("key").cloned().unwrap_or_else(|| format!("unknown_{i}"));
        let title = entry.get("title").cloned().unwrap_or_default();
        let arxiv_id = entry.get("eprint").cloned().unwrap_or_default();
        let doi = entry.get("doi").cloned().unwrap_or_default();

        if title.is_empty() {
            let result = CitationResult {
                cite_key: key.clone(),
                title: String::new(),
                status: VerifyStatus::Skipped,
                confidence: 0.0,
                method: "skipped".to_owned(),
                details: "No title in BibTeX entry".to_owned(),
                matched_paper: None,
            };
            report.skipped += 1;
            report.results.push(result);
            continue;
        }

        // Cache check
        let cache_key = crate::cache::cache_key(&title.to_lowercase());
        if let Some(cached) = cache.get::<CitationResult>("citation_verify", &cache_key) {
            let mut r = cached;
            r.cite_key = key;
            tally(&mut report, r.status);
            report.results.push(r);
            continue;
        }

        let mut result: Option<CitationResult> = None;

        // L1: DOI via CrossRef
        if result.is_none() && !doi.is_empty() {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            result = verify_by_doi(&client, &doi, &title).await;
            if let Some(ref r) = result {
                info!("L1 DOI [{key}] {doi} → {} ({:.2})", r.status, r.confidence);
            }
        }

        // L2: OpenAlex title search
        if result.is_none() {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            result = verify_by_openalex(&client, &title).await;
            if let Some(ref r) = result {
                info!("L2 OpenAlex [{key}] {} → {} ({:.2})", &title[..title.len().min(50)], r.status, r.confidence);
            }
        }

        // L3a: arXiv ID
        if result.is_none() && !arxiv_id.is_empty() {
            tokio::time::sleep(Duration::from_millis(inter_verify_delay_ms)).await;
            result = verify_by_arxiv_id(&client, &arxiv_id, &title).await;
            if let Some(ref r) = result {
                info!("L3a arXiv [{key}] {arxiv_id} → {} ({:.2})", r.status, r.confidence);
            }
        }

        // L3b: S2 title search
        if result.is_none() {
            result = verify_by_title_search(&title, s2_api_key).await;
            if let Some(ref r) = result {
                info!("L3b S2 [{key}] {} → {} ({:.2})", &title[..title.len().min(50)], r.status, r.confidence);
            }
        }

        let mut r = result.unwrap_or_else(|| CitationResult {
            cite_key: String::new(),
            title: title.clone(),
            status: VerifyStatus::Skipped,
            confidence: 0.0,
            method: "skipped".to_owned(),
            details: "All verification methods failed (network error?)".to_owned(),
            matched_paper: None,
        });
        r.cite_key = key;

        // Cache successful lookups
        if r.status != VerifyStatus::Skipped {
            cache.set("citation_verify", &cache_key, &r);
        }

        tally(&mut report, r.status);
        report.results.push(r);
    }

    Ok(report)
}

fn tally(report: &mut VerificationReport, status: VerifyStatus) {
    match status {
        VerifyStatus::Verified => report.verified += 1,
        VerifyStatus::Suspicious => report.suspicious += 1,
        VerifyStatus::Hallucinated => report.hallucinated += 1,
        VerifyStatus::Skipped => report.skipped += 1,
    }
}

// ---------------------------------------------------------------------------
// Post-processing helpers
// ---------------------------------------------------------------------------

/// Return a cleaned BibTeX string with only verified (and optionally
/// suspicious) entries.
pub fn filter_verified_bibtex(
    bib_text: &str,
    report: &VerificationReport,
    include_suspicious: bool,
) -> String {
    let keep_keys: std::collections::HashSet<&str> = report
        .results
        .iter()
        .filter(|r| {
            r.status == VerifyStatus::Verified
                || (include_suspicious && r.status == VerifyStatus::Suspicious)
                || r.status == VerifyStatus::Skipped
        })
        .map(|r| r.cite_key.as_str())
        .collect();

    let entry_re = regex::Regex::new(r"(?s)@(\w+)\s*\{\s*([^,\s]+)\s*,\s*(.*?)\n\s*\}").unwrap();
    let mut kept: Vec<&str> = Vec::new();
    for cap in entry_re.captures_iter(bib_text) {
        let key = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        if keep_keys.contains(key) {
            kept.push(cap.get(0).map(|m| m.as_str()).unwrap_or(""));
        }
    }

    if kept.is_empty() {
        String::new()
    } else {
        format!("{}\n", kept.join("\n\n"))
    }
}

/// Remove hallucinated citations from paper text.
///
/// - `HALLUCINATED` keys are removed from every cite command they appear in.
/// - `SUSPICIOUS`, `VERIFIED`, and `SKIPPED` entries are left as-is.
///
/// Supports two citation formats:
/// - LaTeX: `\cite{key1, key2, key3}`
/// - Markdown: `[key1, key2]` or `[key1; key2]` where keys match `[A-Za-z]+\d{4}[A-Za-z]*`
///
/// After removing keys the function also cleans up:
/// - Multiple consecutive spaces → single space
/// - Empty `()` or `[]` parenthetical artifacts
pub fn annotate_paper_hallucinations(paper_text: &str, report: &VerificationReport) -> String {
    let hallucinated: std::collections::HashSet<&str> = report
        .results
        .iter()
        .filter(|r| r.status == VerifyStatus::Hallucinated)
        .map(|r| r.cite_key.as_str())
        .collect();

    if hallucinated.is_empty() {
        return paper_text.to_owned();
    }

    // Replace \cite{...} removing only hallucinated keys.
    let latex_re = regex::Regex::new(r"\\cite\{([^}]+)\}").unwrap();
    let text = latex_re.replace_all(paper_text, |caps: &regex::Captures| {
        let keys: Vec<&str> = caps[1]
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty() && !hallucinated.contains(*k))
            .collect();
        if keys.is_empty() {
            String::new()
        } else {
            format!("\\cite{{{}}}", keys.join(", "))
        }
    });

    // Replace [key1, key2] / [key1; key2] Markdown citations.
    let cite_key_pat = r"[A-Za-z]+\d{4}[A-Za-z]*";
    let md_re = regex::Regex::new(&format!(
        r"\[({ck}(?:\s*[,;]\s*{ck})*)\]",
        ck = cite_key_pat
    ))
    .unwrap();

    let text = md_re.replace_all(&text, |caps: &regex::Captures| {
        let keys: Vec<&str> = regex::Regex::new(r"[,;]\s*")
            .unwrap()
            .split(&caps[1])
            .map(str::trim)
            .filter(|k| !k.is_empty() && !hallucinated.contains(*k))
            .collect();
        if keys.is_empty() {
            String::new()
        } else {
            format!("[{}]", keys.join(", "))
        }
    });

    // Clean up artifacts.
    let text = regex::Regex::new(r" {2,}").unwrap().replace_all(&text, " ");
    let text = regex::Regex::new(r"\(\s*\)").unwrap().replace_all(&text, "");
    let text = regex::Regex::new(r"\[\s*\]").unwrap().replace_all(&text, "");

    text.into_owned()
}

// ---------------------------------------------------------------------------
// Atom title extractor (minimal, no full XML parse needed here)
// ---------------------------------------------------------------------------

fn extract_atom_title(xml: &str) -> Option<String> {
    // Find first <title>...</title> that is NOT the feed-level title
    // (arXiv entries start after the <entry> tag).
    let entry_start = xml.find("<entry")?;
    let after_entry = &xml[entry_start..];
    let title_start = after_entry.find("<title")?;
    let after_tag = &after_entry[title_start..];
    let gt = after_tag.find('>')?;
    let content = &after_tag[gt + 1..];
    let end = content.find("</title>")?;
    let raw = content[..end].trim();
    // Normalise whitespace
    Some(raw.split_whitespace().collect::<Vec<_>>().join(" "))
}

// ---------------------------------------------------------------------------
// URL encoding
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
    fn test_title_similarity_identical() {
        assert_eq!(title_similarity("Attention Is All You Need", "Attention Is All You Need"), 1.0);
    }

    #[test]
    fn test_title_similarity_zero() {
        let s = title_similarity("quantum mechanics", "deep learning transformers");
        assert!(s < 0.15, "expected low similarity, got {s}");
    }

    #[test]
    fn test_parse_bibtex_entries() {
        let bib = r#"@article{smith2024test,
  title = {Test Paper},
  author = {Alice Smith},
  year = {2024},
  doi = {10.1234/test},
  eprint = {2401.00001},
}"#;
        let entries = parse_bibtex_entries(bib);
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.get("key").unwrap(), "smith2024test");
        assert_eq!(e.get("title").unwrap(), "Test Paper");
        assert_eq!(e.get("doi").unwrap(), "10.1234/test");
        assert_eq!(e.get("eprint").unwrap(), "2401.00001");
    }

    #[test]
    fn test_extract_atom_title() {
        let xml = r#"<feed>
<entry>
<id>http://arxiv.org/abs/2301.00001</id>
<title>My Test Paper</title>
</entry>
</feed>"#;
        assert_eq!(extract_atom_title(xml).as_deref(), Some("My Test Paper"));
    }

    #[test]
    fn test_integrity_score() {
        let report = VerificationReport {
            total: 4,
            verified: 3,
            suspicious: 0,
            hallucinated: 1,
            skipped: 0,
            results: vec![],
        };
        assert!((report.integrity_score() - 0.75).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // classify_by_sim boundary tests
    // -----------------------------------------------------------------------

    #[test]
    fn classify_by_sim_boundaries() {
        // sim = 0.0 → Hallucinated
        let r = classify_by_sim(0.0, "a", "b", "test").unwrap();
        assert_eq!(r.status, VerifyStatus::Hallucinated);

        // sim = 0.49 → Hallucinated
        let r = classify_by_sim(0.49, "a", "b", "test").unwrap();
        assert_eq!(r.status, VerifyStatus::Hallucinated);

        // sim = 0.50 → Suspicious
        let r = classify_by_sim(0.50, "a", "b", "test").unwrap();
        assert_eq!(r.status, VerifyStatus::Suspicious);

        // sim = 0.79 → Suspicious
        let r = classify_by_sim(0.79, "a", "b", "test").unwrap();
        assert_eq!(r.status, VerifyStatus::Suspicious);

        // sim = 0.80 → Verified
        let r = classify_by_sim(0.80, "a", "b", "test").unwrap();
        assert_eq!(r.status, VerifyStatus::Verified);

        // sim = 1.0 → Verified
        let r = classify_by_sim(1.0, "a", "a", "test").unwrap();
        assert_eq!(r.status, VerifyStatus::Verified);
    }

    // -----------------------------------------------------------------------
    // annotate_paper_hallucinations tests
    // -----------------------------------------------------------------------

    fn make_report_with_hallucinated(keys: &[&str]) -> VerificationReport {
        let results: Vec<CitationResult> = keys
            .iter()
            .map(|k| CitationResult {
                cite_key: k.to_string(),
                title: k.to_string(),
                status: VerifyStatus::Hallucinated,
                confidence: 0.0,
                method: "test".to_owned(),
                details: String::new(),
                matched_paper: None,
            })
            .collect();
        let total = results.len();
        VerificationReport {
            total,
            verified: 0,
            suspicious: 0,
            hallucinated: total,
            skipped: 0,
            results,
        }
    }

    #[test]
    fn annotate_no_hallucinations_unchanged() {
        let report = VerificationReport {
            total: 0,
            verified: 0,
            suspicious: 0,
            hallucinated: 0,
            skipped: 0,
            results: vec![],
        };
        let text = r"Some text \cite{smith2024a, jones2023b}. More text.";
        let result = annotate_paper_hallucinations(text, &report);
        assert_eq!(result, text);
    }

    #[test]
    fn annotate_single_latex_cite_removed() {
        let report = make_report_with_hallucinated(&["fake2024x"]);
        let text = r"Text \cite{fake2024x} here.";
        let result = annotate_paper_hallucinations(text, &report);
        assert!(!result.contains("fake2024x"));
        // Empty cite{} should be absent
        assert!(!result.contains(r"\cite{fake2024x}"));
    }

    #[test]
    fn annotate_multi_cite_partial_removal() {
        let report = make_report_with_hallucinated(&["bad2024x"]);
        let text = r"Text \cite{good2023a, bad2024x, also2022b} here.";
        let result = annotate_paper_hallucinations(text, &report);
        assert!(!result.contains("bad2024x"), "hallucinated key should be gone");
        assert!(result.contains("good2023a"), "real key should remain");
        assert!(result.contains("also2022b"), "real key should remain");
    }

    #[test]
    fn annotate_all_keys_hallucinated_removes_cite() {
        let report = make_report_with_hallucinated(&["fake2024a", "fake2024b"]);
        let text = r"Text \cite{fake2024a, fake2024b} end.";
        let result = annotate_paper_hallucinations(text, &report);
        assert!(!result.contains(r"\cite{"), "empty cite command should be removed");
    }

    #[test]
    fn annotate_markdown_cite_partial_removal() {
        let report = make_report_with_hallucinated(&["bad2024x"]);
        let text = "Text [good2023a, bad2024x] here.";
        let result = annotate_paper_hallucinations(text, &report);
        assert!(!result.contains("bad2024x"), "hallucinated key should be gone");
        assert!(result.contains("good2023a"), "real key should remain");
    }

    #[test]
    fn annotate_markdown_cite_all_removed() {
        let report = make_report_with_hallucinated(&["bad2024x"]);
        let text = "Text [bad2024x] end.";
        let result = annotate_paper_hallucinations(text, &report);
        assert!(!result.contains("bad2024x"), "key should be gone");
        // Should not leave bare []
        assert!(!result.contains("[]"), "empty brackets should be cleaned");
    }
}
