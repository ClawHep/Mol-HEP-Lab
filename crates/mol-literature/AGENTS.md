<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-literature

## Purpose
Scientific literature retrieval and parsing for Mol-HEP-Lab. Provides async clients for arXiv, Semantic Scholar, and OpenAlex APIs, plus utilities for novelty assessment, citation verification, and cache management. Enables rapid literature searches and cross-referencing for HEP research.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/search.rs` | Unified search interface and provider enum |
| `src/arxiv.rs` | ArXiv API client with OAI-PMH queries |
| `src/semantic_scholar.rs` | Semantic Scholar API client |
| `src/openalex.rs` | OpenAlex API client |
| `src/models.rs` | Paper, Author, and shared data structures |
| `src/citation.rs` | Citation verification and report generation |
| `src/novelty.rs` | Novelty assessment and similar paper detection |
| `src/cache.rs` | File-based caching for API responses |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate for literature search
- Run `cargo test -p mol-literature` to test API clients with mock responses
- Run `cargo clippy -p mol-literature` for linting
- Main entry point: `search_papers(query, options)` searches across providers
- Caching: `FileCache` stores responses to reduce API load

### Testing Requirements
- Mock HTTP responses for each API (arXiv XML, Semantic Scholar JSON, OpenAlex JSON)
- Search tests: verify query encoding and response parsing
- Citation verification tests: check reference resolution
- Novelty assessment tests: verify similarity calculation
- Cache tests: verify storage, retrieval, and expiration
- Error handling: timeouts, 404s, malformed responses

### Common Patterns
- All providers use async `reqwest::Client`
- Queries built with builder pattern via `SearchOptions`
- Responses parsed into unified `Paper` struct
- Cache uses filesystem with JSON serialization
- ArXiv uses OAI-PMH (XML) format; others use JSON
- Pagination supported via limit/offset
- Year filtering available for most providers

## Dependencies
### Internal
- mol-common (types)
- mol-llm (LLM for novelty assessment via embedding)

### External
- `reqwest` (HTTP client)
- `serde` + `serde_json` (JSON parsing)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `quick-xml` (ArXiv XML parsing)
- `regex` (text extraction)
- `chrono` (date/time)
- `tempfile` (test fixtures)
