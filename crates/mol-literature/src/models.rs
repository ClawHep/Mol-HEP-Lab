//! Data models for literature search results.
//!
//! [`Author`] and [`Paper`] are the primary types shared across all API clients.
//! [`Paper`] provides BibTeX generation and a computed citation key.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Author
// ---------------------------------------------------------------------------

/// A paper author with optional affiliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    #[serde(default)]
    pub affiliation: String,
}

impl Author {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), affiliation: String::new() }
    }

    pub fn with_affiliation(name: impl Into<String>, affiliation: impl Into<String>) -> Self {
        Self { name: name.into(), affiliation: affiliation.into() }
    }

    /// ASCII-folded last name, used when building citation keys.
    pub fn last_name(&self) -> String {
        let parts: Vec<&str> = self.name.trim().split_whitespace().collect();
        let raw = parts.last().copied().unwrap_or("unknown");
        // Fold to ASCII: keep only a-zA-Z
        let ascii: String = raw
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_lowercase();
        if ascii.is_empty() { "unknown".to_owned() } else { ascii }
    }
}

// ---------------------------------------------------------------------------
// Paper
// ---------------------------------------------------------------------------

/// A single paper from any of the supported academic sources.
///
/// The fields are a union of what Semantic Scholar, arXiv, and OpenAlex
/// provide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paper {
    pub paper_id: String,
    pub title: String,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub year: u32,
    #[serde(default)]
    pub abstract_text: String,
    #[serde(default)]
    pub venue: String,
    #[serde(default)]
    pub citation_count: u32,
    #[serde(default)]
    pub doi: String,
    #[serde(default)]
    pub arxiv_id: String,
    #[serde(default)]
    pub url: String,
    /// Source identifier: `"arxiv"`, `"semantic_scholar"`, or `"openalex"`.
    #[serde(default)]
    pub source: String,
}

// Common English stopwords to skip when picking a keyword for cite_key.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "into", "over",
    "upon", "about", "through", "using", "based", "towards", "toward",
    "between", "under", "more", "than", "when", "what", "which", "where",
    "does", "have", "been", "some", "each", "also", "much", "very",
    "learning",
];

impl Paper {
    /// Normalised citation key: `lastname<year><keyword>`.
    ///
    /// Example: `smith2024transformer`
    pub fn cite_key(&self) -> String {
        let last = self.authors.first().map(|a| a.last_name()).unwrap_or_else(|| "anon".to_owned());
        let yr = if self.year > 0 { self.year.to_string() } else { "0000".to_owned() };
        let kw = self
            .title
            .split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase())
            .find(|w| w.len() > 3 && !STOPWORDS.contains(&w.as_str()))
            .unwrap_or_default();
        format!("{last}{yr}{kw}")
    }

    /// Generate a BibTeX entry string from metadata.
    pub fn to_bibtex(&self) -> String {
        let key = self.cite_key();
        let authors_str = if self.authors.is_empty() {
            "Unknown".to_owned()
        } else {
            self.authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(" and ")
        };

        // Detect arXiv category codes used as venue (e.g. "cs.CY", "math.OC")
        let is_arxiv_cat = is_arxiv_category(&self.venue);

        // Decide entry type
        let venue_lower = self.venue.to_lowercase();
        let (entry_type, venue_field) = if !self.venue.is_empty()
            && !is_arxiv_cat
            && ["conference", "proc", "workshop", "neurips", "icml", "iclr",
                "aaai", "cvpr", "acl", "emnlp", "naacl", "eccv", "iccv",
                "sigir", "kdd", "www", "ijcai"]
                .iter()
                .any(|kw| venue_lower.contains(kw))
        {
            ("inproceedings", format!("  booktitle = {{{}}},", self.venue))
        } else if !self.arxiv_id.is_empty() && (self.venue.is_empty() || is_arxiv_cat) {
            (
                "article",
                format!("  journal = {{arXiv preprint arXiv:{}}},", self.arxiv_id),
            )
        } else {
            let vf = if !self.venue.is_empty() {
                format!("  journal = {{{}}},", self.venue)
            } else {
                String::new()
            };
            ("article", vf)
        };

        let mut lines = vec![format!("@{entry_type}{{{key},")];
        lines.push(format!("  title = {{{}}},", self.title));
        lines.push(format!("  author = {{{authors_str}}},"));
        lines.push(format!("  year = {{{}}},", if self.year > 0 { self.year.to_string() } else { "Unknown".to_owned() }));
        if !venue_field.is_empty() {
            lines.push(venue_field);
        }
        if !self.doi.is_empty() {
            lines.push(format!("  doi = {{{}}},", self.doi));
        }
        if !self.arxiv_id.is_empty() {
            lines.push(format!("  eprint = {{{}}},", self.arxiv_id));
            lines.push("  archiveprefix = {arXiv},".to_owned());
        }
        if !self.url.is_empty() {
            lines.push(format!("  url = {{{}}},", self.url));
        }
        lines.push("}".to_owned());
        lines.join("\n")
    }
}

/// Returns `true` when the string looks like an arXiv category code
/// (e.g. `cs.LG`, `hep-th.AB`).
fn is_arxiv_category(s: &str) -> bool {
    // Matches patterns like cs.LG, hep-th.AB, math.OC
    let prefixes = [
        "cs", "math", "stat", "eess", "physics", "q-bio", "q-fin",
        "astro-ph", "cond-mat", "gr-qc", "hep-ex", "hep-lat", "hep-ph",
        "hep-th", "nlin", "nucl-ex", "nucl-th", "quant-ph",
    ];
    if let Some(dot_pos) = s.find('.') {
        let prefix = &s[..dot_pos];
        let suffix = &s[dot_pos + 1..];
        prefixes.contains(&prefix)
            && suffix.len() == 2
            && suffix.chars().all(|c| c.is_ascii_uppercase())
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_name() {
        let a = Author::new("John Smith");
        assert_eq!(a.last_name(), "smith");
    }

    #[test]
    fn test_cite_key() {
        let p = Paper {
            paper_id: "test".to_owned(),
            title: "Attention Is All You Need".to_owned(),
            authors: vec![Author::new("Ashish Vaswani")],
            year: 2017,
            abstract_text: String::new(),
            venue: String::new(),
            citation_count: 0,
            doi: String::new(),
            arxiv_id: String::new(),
            url: String::new(),
            source: "arxiv".to_owned(),
        };
        assert_eq!(p.cite_key(), "vaswani2017attention");
    }

    #[test]
    fn test_arxiv_category_detection() {
        assert!(is_arxiv_category("cs.LG"));
        assert!(is_arxiv_category("hep-th.AB"));
        assert!(!is_arxiv_category("NeurIPS"));
        assert!(!is_arxiv_category("cs"));
    }
}
