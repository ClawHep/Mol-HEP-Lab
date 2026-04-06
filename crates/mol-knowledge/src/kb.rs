//! Knowledge base manager for Mol-HEP-Lab.
//!
//! Entries are organised into category sub-directories beneath a configurable
//! root path.  Each entry is a Markdown file with YAML frontmatter.  The
//! optional [`KBBackend::Obsidian`] backend enriches entries with inline
//! `#tags` and `[[wikilinks]]` recognised by Obsidian.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, info};
use walkdir::WalkDir;

// ---------------------------------------------------------------------------
// Stage → category mapping (mirrors Python KB_CATEGORY_MAP)
// ---------------------------------------------------------------------------

fn stage_category(stage: u32) -> KBCategory {
    match stage {
        1 | 2 | 8 => KBCategory::Questions,
        3 | 9 | 11 | 15 | 20 | 21 => KBCategory::Decisions,
        4 | 5 | 6 => KBCategory::Literature,
        7 | 14 => KBCategory::Findings,
        10 | 12 | 13 => KBCategory::Experiments,
        16 | 17 | 18 | 19 | 22 => KBCategory::Reviews,
        _ => KBCategory::Findings,
    }
}

// ---------------------------------------------------------------------------
// KBCategory
// ---------------------------------------------------------------------------

/// Top-level categories for knowledge-base entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KBCategory {
    Questions,
    Literature,
    Experiments,
    Findings,
    Decisions,
    Reviews,
}

impl KBCategory {
    /// Returns the directory name used on disk for this category.
    pub fn dir_name(&self) -> &'static str {
        match self {
            KBCategory::Questions => "questions",
            KBCategory::Literature => "literature",
            KBCategory::Experiments => "experiments",
            KBCategory::Findings => "findings",
            KBCategory::Decisions => "decisions",
            KBCategory::Reviews => "reviews",
        }
    }

    /// Parse from the directory-name string stored in frontmatter.
    pub fn from_dir_name(s: &str) -> Option<Self> {
        match s {
            "questions" => Some(KBCategory::Questions),
            "literature" => Some(KBCategory::Literature),
            "experiments" => Some(KBCategory::Experiments),
            "findings" => Some(KBCategory::Findings),
            "decisions" => Some(KBCategory::Decisions),
            "reviews" => Some(KBCategory::Reviews),
            _ => None,
        }
    }

    /// All variants, used when initialising directory trees.
    pub fn all() -> &'static [KBCategory] {
        &[
            KBCategory::Questions,
            KBCategory::Literature,
            KBCategory::Experiments,
            KBCategory::Findings,
            KBCategory::Decisions,
            KBCategory::Reviews,
        ]
    }
}

impl std::fmt::Display for KBCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.dir_name())
    }
}

// ---------------------------------------------------------------------------
// KBEntry
// ---------------------------------------------------------------------------

/// A single knowledge-base entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KBEntry {
    /// Unique identifier, e.g. `"goal-define-run-abc123"`.
    pub id: String,
    /// Logical category determining the storage sub-directory.
    pub category: KBCategory,
    /// Human-readable title.
    pub title: String,
    /// Markdown body of the entry.
    pub content: String,
    /// Arbitrary searchable tags.
    pub tags: Vec<String>,
    /// Pipeline stage identifier, e.g. `"01-goal_define"`.
    pub stage: String,
    /// UTC creation timestamp.
    pub timestamp: DateTime<Utc>,
    /// Source identifier (run-id, DOI, file path, …).
    pub source: String,
}

impl KBEntry {
    /// Construct a new entry stamped with the current UTC time.
    pub fn new(
        id: impl Into<String>,
        category: KBCategory,
        title: impl Into<String>,
        content: impl Into<String>,
        tags: Vec<String>,
        stage: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            title: title.into(),
            content: content.into(),
            tags,
            stage: stage.into(),
            timestamp: Utc::now(),
            source: source.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// KBBackend
// ---------------------------------------------------------------------------

/// Storage back-end flavour for the knowledge base.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KBBackend {
    /// Plain Markdown files with YAML frontmatter — no tool-specific extras.
    #[default]
    Markdown,
    /// Markdown with Obsidian-compatible wikilinks `[[…]]` and inline `#tags`.
    Obsidian,
}

// ---------------------------------------------------------------------------
// KnowledgeBase
// ---------------------------------------------------------------------------

/// Manages a file-system knowledge base under a configurable root directory.
///
/// # Layout
/// ```text
/// <root>/
///   questions/
///   literature/
///   experiments/
///   findings/
///   decisions/
///   reviews/
/// ```
pub struct KnowledgeBase {
    root: PathBuf,
    backend: KBBackend,
}

impl KnowledgeBase {
    /// Create a new [`KnowledgeBase`] handle.  Does **not** touch the file
    /// system — call [`Self::init_dirs`] to create the directory tree.
    pub fn new(root: PathBuf, backend: KBBackend) -> Self {
        Self { root, backend }
    }

    /// Create sub-directories for every [`KBCategory`] under `root`.
    pub async fn init_dirs(&self) -> Result<()> {
        for cat in KBCategory::all() {
            let dir = self.root.join(cat.dir_name());
            fs::create_dir_all(&dir)
                .await
                .with_context(|| format!("Failed to create KB directory: {}", dir.display()))?;
            debug!(path = %dir.display(), "KB directory ready");
        }
        info!(root = %self.root.display(), "Knowledge base directories initialised");
        Ok(())
    }

    /// Write a single [`KBEntry`] to the appropriate category sub-directory.
    ///
    /// Returns the path of the written file.
    pub async fn write_entry(&self, entry: &KBEntry) -> Result<PathBuf> {
        let dir = self.root.join(entry.category.dir_name());
        fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("Failed to create category dir: {}", dir.display()))?;

        let filename = format!("{}.md", entry.id);
        let filepath = dir.join(&filename);

        let doc = self.render_entry(entry);
        fs::write(&filepath, doc)
            .await
            .with_context(|| format!("Failed to write KB entry: {}", filepath.display()))?;

        debug!(path = %filepath.display(), id = %entry.id, "KB entry written");
        Ok(filepath)
    }

    /// Write a pipeline stage's output directly into the knowledge base.
    ///
    /// The category is determined automatically from `stage` via the same
    /// mapping used in the Python reference implementation.
    pub async fn write_stage_to_kb(
        &self,
        stage: u32,
        stage_name: &str,
        content: &str,
        tags: &[String],
    ) -> Result<()> {
        let category = stage_category(stage);
        let stage_label = format!("{stage:02}-{stage_name}");
        let title = format!(
            "Stage {stage:02}: {}",
            stage_name.replace('_', " ").to_ascii_uppercase()
        );

        let mut all_tags = vec![
            stage_name.to_string(),
            format!("stage-{stage:02}"),
        ];
        all_tags.extend_from_slice(tags);

        let entry = KBEntry::new(
            format!("{stage_name}-stage{stage:02}"),
            category,
            title,
            content,
            all_tags,
            stage_label,
            format!("stage-{stage:02}"),
        );

        self.write_entry(&entry).await?;
        Ok(())
    }

    /// Read all entries from the knowledge base, optionally filtered to a
    /// single [`KBCategory`].
    pub async fn read_entries(&self, category: Option<KBCategory>) -> Result<Vec<KBEntry>> {
        let search_root: PathBuf = match &category {
            Some(cat) => self.root.join(cat.dir_name()),
            None => self.root.clone(),
        };

        if !search_root.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        for dir_entry in WalkDir::new(&search_root)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().and_then(|x| x.to_str()) == Some("md")
            })
        {
            match parse_entry_from_path(dir_entry.path()).await {
                Ok(entry) => entries.push(entry),
                Err(err) => {
                    tracing::warn!(
                        path = %dir_entry.path().display(),
                        error = %err,
                        "Skipping unparseable KB entry"
                    );
                }
            }
        }

        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(entries)
    }

    /// Generate a Markdown weekly report by aggregating all entries in the
    /// `reviews` category and computing per-category counts.
    ///
    /// Returns the report as a `String`; callers are responsible for writing
    /// it to disk (or passing it through [`Self::write_entry`]).
    pub async fn generate_weekly_report(&self) -> Result<String> {
        let week_label = Utc::now().format("%Y-W%V").to_string();
        let all_entries = self.read_entries(None).await?;

        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in &all_entries {
            *counts.entry(e.category.dir_name().to_string()).or_default() += 1;
        }

        let total = all_entries.len();

        let mut lines: Vec<String> = vec![
            format!("# Weekly Knowledge-Base Report — {week_label}"),
            String::new(),
            "## Summary".to_string(),
            format!("- Week: {week_label}"),
            format!("- Total entries: {total}"),
            String::new(),
            "## Entries by Category".to_string(),
        ];

        for cat in KBCategory::all() {
            let n = counts.get(cat.dir_name()).copied().unwrap_or(0);
            lines.push(format!("- **{}**: {n}", cat.dir_name()));
        }

        lines.push(String::new());
        lines.push("## Recent Entries".to_string());

        // Show up to 20 most-recent entries
        let recent = all_entries.iter().rev().take(20);
        for e in recent {
            if self.backend == KBBackend::Obsidian {
                lines.push(format!("- [[{}]] — {}", e.id, e.title));
            } else {
                lines.push(format!("- **{}** — {}", e.id, e.title));
            }
        }

        if total == 0 {
            lines.push(String::new());
            lines.push("_No entries found in knowledge base._".to_string());
        }

        Ok(lines.join("\n"))
    }

    // -----------------------------------------------------------------------
    // Internal rendering
    // -----------------------------------------------------------------------

    fn render_entry(&self, entry: &KBEntry) -> String {
        let frontmatter = self.render_frontmatter(entry);
        let heading = format!("# {}\n", entry.title);
        let body = entry.content.trim_end().to_string();

        let mut parts = vec![frontmatter, heading, body];

        if self.backend == KBBackend::Obsidian {
            let extras = self.render_obsidian_extras(entry);
            if !extras.is_empty() {
                parts.push(extras);
            }
        }

        parts.join("\n")
    }

    fn render_frontmatter(&self, entry: &KBEntry) -> String {
        // Build a minimal, deterministic YAML block without pulling in a
        // full YAML serialisation library (serde_yaml is not in the workspace).
        let ts = entry.timestamp.to_rfc3339();
        let tags_yaml = if entry.tags.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = entry.tags.iter().map(|t| format!("  - {t}")).collect();
            format!("tags:\n{}\n", items.join("\n"))
        };

        format!(
            "---\nid: {id}\ntitle: \"{title}\"\ncategory: {cat}\nstage: {stage}\nsource: {source}\ncreated: {ts}\n{tags}---\n",
            id = entry.id,
            title = entry.title.replace('"', "\\\""),
            cat = entry.category.dir_name(),
            stage = entry.stage,
            source = entry.source,
            tags = tags_yaml,
        )
    }

    fn render_obsidian_extras(&self, entry: &KBEntry) -> String {
        let mut extras: Vec<String> = Vec::new();

        if !entry.tags.is_empty() {
            let tag_line: Vec<String> = entry.tags.iter().map(|t| format!("#{t}")).collect();
            extras.push(tag_line.join(" "));
        }

        // Cross-reference to the source (run-id style) as a wikilink
        if !entry.source.is_empty() {
            extras.push(format!("Related: [[{}]]", entry.source));
        }

        extras.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a Markdown file back into a [`KBEntry`].
///
/// Reads the YAML frontmatter block between the opening and closing `---`
/// delimiters.  The rest of the file (after the closing `---`) is used as
/// the entry content.
async fn parse_entry_from_path(path: &Path) -> Result<KBEntry> {
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("Cannot read {}", path.display()))?;

    parse_entry_from_str(&raw, path)
}

fn parse_entry_from_str(raw: &str, path: &Path) -> Result<KBEntry> {
    // Locate YAML frontmatter delimiters
    let after_first = raw
        .strip_prefix("---\n")
        .with_context(|| format!("No opening frontmatter in {}", path.display()))?;

    let (yaml_block, rest) = after_first
        .split_once("\n---\n")
        .with_context(|| format!("No closing frontmatter in {}", path.display()))?;

    // Parse individual fields from the YAML block with a simple key-value scan
    let fm = parse_yaml_kv(yaml_block);

    let id = fm
        .get("id")
        .cloned()
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());

    let title = fm
        .get("title")
        .cloned()
        .unwrap_or_else(|| id.clone())
        .trim_matches('"')
        .to_string();

    let cat_str = fm.get("category").map(String::as_str).unwrap_or("findings");
    let category = KBCategory::from_dir_name(cat_str).unwrap_or_else(|| {
        // Infer from path component
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .and_then(KBCategory::from_dir_name)
            .unwrap_or(KBCategory::Findings)
    });

    let stage = fm.get("stage").cloned().unwrap_or_default();
    let source = fm.get("source").cloned().unwrap_or_default();

    let timestamp = fm
        .get("created")
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    // Tags: collected from the multi-line block starting after "tags:"
    let tags = parse_yaml_list(yaml_block, "tags");

    // Content: everything after the closing frontmatter delimiter
    let content = rest.trim_start_matches('\n').to_string();

    Ok(KBEntry {
        id,
        category,
        title,
        content,
        tags,
        stage,
        timestamp,
        source,
    })
}

/// Extract simple `key: value` pairs from a YAML block (single-line values only).
fn parse_yaml_kv(yaml: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in yaml.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            if !key.is_empty() && !val.is_empty() && !key.starts_with('-') {
                map.insert(key, val);
            }
        }
    }
    map
}

/// Extract a YAML sequence (list items starting with `  - `) for a given key.
fn parse_yaml_list(yaml: &str, key: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut in_block = false;
    for line in yaml.lines() {
        if line.trim_start() == format!("{key}:").as_str() || line == format!("{key}:") {
            in_block = true;
            continue;
        }
        if in_block {
            if let Some(item) = line.trim().strip_prefix("- ") {
                items.push(item.to_string());
            } else if !line.trim().is_empty() {
                // Another key starts — end of block
                break;
            }
        }
    }
    items
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_entry() -> KBEntry {
        KBEntry::new(
            "test-entry-001",
            KBCategory::Findings,
            "Test Finding",
            "This is the body of the finding.",
            vec!["mol-hep".to_string(), "test".to_string()],
            "07-analysis",
            "run-abc123",
        )
    }

    #[test]
    fn category_dir_names_round_trip() {
        for cat in KBCategory::all() {
            let name = cat.dir_name();
            let recovered = KBCategory::from_dir_name(name).expect("round-trip failed");
            assert_eq!(cat, &recovered);
        }
    }

    #[test]
    fn stage_category_mapping() {
        assert_eq!(stage_category(1), KBCategory::Questions);
        assert_eq!(stage_category(4), KBCategory::Literature);
        assert_eq!(stage_category(7), KBCategory::Findings);
        assert_eq!(stage_category(10), KBCategory::Experiments);
        assert_eq!(stage_category(3), KBCategory::Decisions);
        assert_eq!(stage_category(16), KBCategory::Reviews);
        assert_eq!(stage_category(99), KBCategory::Findings); // default
    }

    #[tokio::test]
    async fn init_dirs_creates_all_categories() {
        let tmp = TempDir::new().unwrap();
        let kb = KnowledgeBase::new(tmp.path().to_path_buf(), KBBackend::Markdown);
        kb.init_dirs().await.unwrap();

        for cat in KBCategory::all() {
            assert!(
                tmp.path().join(cat.dir_name()).is_dir(),
                "Missing directory: {}",
                cat.dir_name()
            );
        }
    }

    #[tokio::test]
    async fn write_and_read_entry_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let kb = KnowledgeBase::new(tmp.path().to_path_buf(), KBBackend::Markdown);
        kb.init_dirs().await.unwrap();

        let entry = sample_entry();
        let path = kb.write_entry(&entry).await.unwrap();
        assert!(path.exists());

        let entries = kb.read_entries(Some(KBCategory::Findings)).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "test-entry-001");
        assert_eq!(entries[0].title, "Test Finding");
        assert!(entries[0].content.contains("body of the finding"));
    }

    #[tokio::test]
    async fn obsidian_backend_adds_wikilinks() {
        let tmp = TempDir::new().unwrap();
        let kb = KnowledgeBase::new(tmp.path().to_path_buf(), KBBackend::Obsidian);
        kb.init_dirs().await.unwrap();

        let entry = sample_entry();
        let path = kb.write_entry(&entry).await.unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[[run-abc123]]"), "Expected wikilink in: {contents}");
        assert!(contents.contains("#mol-hep"), "Expected inline tag in: {contents}");
    }

    #[tokio::test]
    async fn write_stage_to_kb_uses_correct_category() {
        let tmp = TempDir::new().unwrap();
        let kb = KnowledgeBase::new(tmp.path().to_path_buf(), KBBackend::Markdown);
        kb.init_dirs().await.unwrap();

        // Stage 4 → literature
        kb.write_stage_to_kb(4, "lit_search", "Some literature content.", &[])
            .await
            .unwrap();

        let entries = kb.read_entries(Some(KBCategory::Literature)).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].id.starts_with("lit_search-"));
    }

    #[tokio::test]
    async fn weekly_report_contains_category_counts() {
        let tmp = TempDir::new().unwrap();
        let kb = KnowledgeBase::new(tmp.path().to_path_buf(), KBBackend::Markdown);
        kb.init_dirs().await.unwrap();

        kb.write_entry(&sample_entry()).await.unwrap();

        let report = kb.generate_weekly_report().await.unwrap();
        assert!(report.contains("findings"), "Report missing findings count");
        assert!(report.contains("Total entries: 1"));
    }

    #[test]
    fn parse_yaml_kv_extracts_fields() {
        let yaml = "id: entry-01\ntitle: \"My Title\"\ncategory: findings\n";
        let kv = parse_yaml_kv(yaml);
        assert_eq!(kv.get("id").unwrap(), "entry-01");
        assert_eq!(kv.get("category").unwrap(), "findings");
    }

    #[test]
    fn parse_yaml_list_extracts_items() {
        let yaml = "tags:\n  - mol-hep\n  - test\ncreated: 2024-01-01T00:00:00Z\n";
        let items = parse_yaml_list(yaml, "tags");
        assert_eq!(items, vec!["mol-hep", "test"]);
    }
}
