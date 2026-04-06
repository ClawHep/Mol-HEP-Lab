//! Mol-HEP-Lab: Knowledge base management and retrieval
//!
//! Provides a structured knowledge base for storing and retrieving research
//! artifacts produced by the Mol-HEP-Lab pipeline.  Two storage backends are
//! supported:
//!
//! - [`KBBackend::Markdown`] – plain Markdown files with YAML frontmatter
//! - [`KBBackend::Obsidian`] – Markdown with Obsidian-compatible wikilinks,
//!   inline tags, and frontmatter

pub mod kb;

pub use kb::{KBBackend, KBCategory, KBEntry, KnowledgeBase};
