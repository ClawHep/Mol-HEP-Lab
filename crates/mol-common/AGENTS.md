<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-common

## Purpose
Shared types, utilities, and infrastructure used across the Mol-HEP-Lab system. Provides core pipeline types (AgentLayer, Artifact, StageResult), prompt templates, content quality assessment, hardware detection, text sanitization, and adapter traits for testing and recording.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/types.rs` | Core types: AgentStatus, Artifact, StageResult, ResourceStats, etc. |
| `src/prompts.rs` | Tera-based prompt template engine |
| `src/quality.rs` | Content quality assessment and template-content detection |
| `src/hardware.rs` | GPU/MPS/CPU detection via nvml-wrapper and sysinfo |
| `src/sanitize.rs` | API key redaction, thinking-tag stripping, filename cleaning |
| `src/adapters.rs` | Typed adapter traits for testing and deterministic recording |
| `src/codebase_manifest.rs` | Codebase discovery and manifest generation |
| `src/data.rs` | Data model types and utilities |
| `src/hep.rs` | High Energy Physics domain types |
| `src/knowledge.rs` | Knowledge base and document types |
| `src/writing_guide.rs` | Static conference writing guidelines |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate used by all other mol-* crates
- Run `cargo test -p mol-common` to run unit tests
- Run `cargo clippy -p mol-common` for linting
- Exported types and functions are re-exported at crate root for convenience
- When modifying types, verify all downstream crates still compile

### Testing Requirements
- Unit tests exist for quality assessment, hardware detection, and sanitization
- Mock hardware detection in tests using adapter traits
- Use `tempfile` for testing file-based utilities
- Quality assessment tests should cover edge cases (empty input, only templates, mixed content)

### Common Patterns
- Heavy use of Serde for serialization (JSON/YAML)
- Async utilities use `async-trait` for trait definitions
- Error handling uses `thiserror` for custom error types
- Hardware info uses `sysinfo::System` to query CPU/memory and nvml_wrapper for GPU stats

## Dependencies
### Internal
- None (this is a foundational crate)

### External
- `serde` + `serde_json` + `serde_yaml` (serialization)
- `thiserror` (error types)
- `tracing` (logging)
- `anyhow` (error handling)
- `chrono` (timestamps)
- `sysinfo` (system resource monitoring)
- `nvml-wrapper` (NVIDIA GPU queries)
- `regex` (text parsing)
- `tera` (template engine)
- `async-trait` (async trait definitions)
- `tokio` (async runtime for some utilities)
