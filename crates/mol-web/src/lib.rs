//! Mol-HEP-Lab: Web scraping, PDF extraction, and URL handling
//!
//! This crate provides:
//! - Tavily-powered web search with DuckDuckGo fallback
//! - HTML crawling and text extraction
//! - PDF text and metadata extraction via lopdf
//! - Google Scholar scraping with rate limiting
//! - Network connectivity pre-checks

pub mod connectivity;
pub mod crawler;
pub mod pdf;
pub mod scholar;
pub mod search;

pub use connectivity::{check_connectivity, ConnectivityReport, EndpointStatus};
pub use crawler::{crawl, CrawlResult};
pub use pdf::{extract_text, PdfContent};
pub use scholar::{ScholarClient, ScholarPaper};
pub use search::{search, WebSearchClient, WebSearchResult};
