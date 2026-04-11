<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-knowledge

## Purpose
Knowledge base management and retrieval for Mol-HEP-Lab. Provides structured storage and retrieval of research artifacts (papers, code, results) with support for multiple backends (Markdown with YAML frontmatter, Obsidian with wikilinks and tags). Enables organizing and cross-referencing pipeline outputs.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/kb.rs` | KnowledgeBase struct, KBEntry, KBBackend enum, and I/O operations |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Monolithic kb.rs module |

## For AI Agents

### Working In This Directory
- This is a library crate for knowledge base operations
- Run `cargo test -p mol-knowledge` to test KB storage and retrieval
- Run `cargo clippy -p mol-knowledge` for linting
- Main interface: `KnowledgeBase::new(backend)` then `.store(entry)` / `.retrieve(id)`
- Supported backends: Markdown (simple) and Obsidian (with wikilinks)

### Testing Requirements
- Backend tests for both Markdown and Obsidian formats
- Entry storage and retrieval tests
- Frontmatter parsing tests (YAML)
- Wikilink and tag parsing tests (Obsidian)
- Concurrent access tests (if applicable)
- File encoding and special character handling

### Common Patterns
- Each KB entry has metadata (id, title, category, tags, frontmatter)
- Markdown backend stores entries as individual .md files
- Obsidian backend adds [[wikilinks]] and #tags for cross-referencing
- Entries indexed by category and tags for quick retrieval
- Async I/O via tokio for file operations

## Dependencies
### Internal
- mol-common (types)

### External
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `walkdir` (directory traversal)
- `chrono` (timestamps)
- `serde` + `serde_json` (JSON serialization)
- `tempfile` (test fixtures)
