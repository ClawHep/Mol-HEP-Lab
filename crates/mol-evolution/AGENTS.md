<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-evolution

## Purpose
Self-evolution system for adaptive pipeline learning in Mol-HEP-Lab. Records lessons from each pipeline run (failures, slow stages, quality issues) and injects them as prompt overlays into future runs. Inspired by Sibyl's time-weighted evolution mechanism, enabling the pipeline to learn and improve over multiple executions.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Lesson types, EvolutionStore, and overlay generation |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Monolithic lib.rs module |

## For AI Agents

### Working In This Directory
- This is a library crate for self-evolution
- Run `cargo test -p mol-evolution` to test lesson storage and overlay generation
- Run `cargo clippy -p mol-evolution` for linting
- Main interface: `EvolutionStore::new(path)` then `.load()`, `.record_lesson()`, `.get_evolution_overlay()`
- Lessons are classified by category (Configuration, Methodology, DataHandling, etc.)

### Testing Requirements
- Lesson storage tests: write/read from JSON
- Overlay generation tests: verify prompt injection for relevant lessons
- Severity weighting tests: recent lessons weighted higher
- Category filtering tests: correct lesson selection per stage
- Time-decay tests: verify recency weighting
- Concurrent access tests: thread-safe lesson recording

### Common Patterns
- Lessons stored as JSON in `evolution/lessons.json`
- Each lesson includes: category, severity, stage, description, timestamp
- Overlays are injected as prompt prefixes for affected stages
- Time-decay weights recent lessons higher (exponential decay)
- Lessons grouped by stage for targeted injection
- Statistics computed over all stored lessons

## Dependencies
### Internal
- mol-common (types)

### External
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `serde` + `serde_json` (lesson persistence)
- `chrono` (timestamps, time-decay calculation)
- `tempfile` (test fixtures)
