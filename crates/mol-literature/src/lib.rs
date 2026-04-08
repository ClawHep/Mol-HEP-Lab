//! Mol-HEP-Lab: Scientific literature retrieval and parsing.
//!
//! This crate provides async clients for arXiv, Semantic Scholar, and OpenAlex,
//! plus utilities for novelty assessment and citation verification.
//!
//! # Quick start
//! ```no_run
//! use mol_literature::search::{search_papers, SearchOptions};
//!
//! #[tokio::main]
//! async fn main() {
//!     let papers = search_papers("quantum machine learning", &SearchOptions {
//!         limit: 10,
//!         year_min: 2020,
//!         ..Default::default()
//!     }).await.unwrap();
//!
//!     for p in &papers {
//!         println!("{} ({})", p.title, p.year);
//!     }
//! }
//! ```

pub mod arxiv;
pub mod cache;
pub mod citation;
pub mod models;
pub mod novelty;
pub mod openalex;
pub mod search;
pub mod semantic_scholar;

// Re-export the most commonly used types at the crate root.
pub use models::{Author, Paper};
pub use search::{Provider, SearchOptions};
pub use citation::{CitationResult, VerificationReport, VerifyStatus};
pub use novelty::{NoveltyReport, SimilarPaper};
pub use cache::FileCache;
