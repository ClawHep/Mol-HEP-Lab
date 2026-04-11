<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-health

## Purpose
System health checks and dependency validation for Mol-HEP-Lab. Validates that all required external tools (LLMs, databases, sandboxes), APIs, environment variables, and system resources are available before running a pipeline. Provides detailed diagnostic reports for troubleshooting missing dependencies.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Health check types, MolConfig wrapper, and public API |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Monolithic lib.rs module |

## For AI Agents

### Working In This Directory
- This is a library crate for health checking
- Run `cargo test -p mol-health` to test health check logic
- Run `cargo clippy -p mol-health` for linting
- Main entry point: `run_doctor(config)` returns `DoctorReport` with all checks
- Check status: Pass, Warn, Fail, Skip — used for early exit decisions

### Testing Requirements
- Tool availability tests: mock PATH resolution
- LLM endpoint tests: mock HTTP responses
- Database/KB tests: filesystem checks
- API key validation tests: format validation without actual API calls
- Resource tests: verify CPU/memory detection
- Error handling: graceful degradation when checks fail

### Common Patterns
- Health checks organized by category (LLM, Storage, Tools, Resources)
- Each check returns CheckStatus (Pass/Warn/Fail/Skip)
- Checks are non-blocking (warn) or blocking (fail) based on severity
- Tool discovery uses `which` crate for PATH resolution
- Results timestamped and serializable to JSON

## Dependencies
### Internal
- mol-common (types, hardware detection)
- mol-config (configuration)

### External
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `which` (tool discovery)
- `serde` + `serde_json` (report serialization)
- `chrono` (timestamps)
- `sysinfo` (system resource checks)
- `reqwest` (LLM endpoint health checks)
