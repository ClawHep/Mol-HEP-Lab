<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-web

## Purpose
Web scraping, PDF extraction, and URL handling for Mol-HEP-Lab. Provides Tavily-powered web search with DuckDuckGo fallback, HTML crawling and text extraction, PDF text/metadata extraction, Google Scholar scraping, and network connectivity pre-checks to ensure external service availability.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/search.rs` | Web search client with Tavily and fallback providers |
| `src/crawler.rs` | HTML crawling and text extraction |
| `src/pdf.rs` | PDF text and metadata extraction via lopdf |
| `src/scholar.rs` | Google Scholar scraping with rate limiting |
| `src/connectivity.rs` | Network connectivity checks for external services |
| `src/agent.rs` | WebSearchAgent: unified search orchestration |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate for web access
- Run `cargo test -p mol-web` to test crawling and extraction with fixtures
- Run `cargo clippy -p mol-web` for linting
- Main entry point: `search(query)` for web search or `crawl(url)` for page crawling
- Connectivity: `check_connectivity()` validates external services before use

### Testing Requirements
- Mock HTTP responses for search providers (Tavily, DuckDuckGo)
- HTML parsing tests: verify text extraction from various page structures
- PDF extraction tests: handle various PDF versions and encodings
- Scholar scraping tests: mock Google Scholar responses
- Connectivity tests: mock DNS failures, timeouts, HTTP errors
- Rate limiting tests: verify Scholar rate limiting behavior

### Common Patterns
- All HTTP clients use `reqwest` with timeout settings
- HTML parsed via `scraper` (CSS selectors)
- PDF parsing uses `lopdf` library
- Web search tries Tavily first, falls back to DuckDuckGo
- Scholar scraping uses regex to parse HTML (no JSON API available)
- Connectivity checks performed on startup and cached

## Dependencies
### Internal
- mol-common (types)
- mol-llm (LLM for search result ranking)

### External
- `reqwest` (HTTP client)
- `serde` + `serde_json` (JSON serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `scraper` (HTML parsing)
- `lopdf` (PDF text extraction)
- `url` (URL parsing/validation)
