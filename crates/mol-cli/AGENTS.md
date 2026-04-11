<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-cli

## Purpose
Binary crate providing the `mol` command-line interface for Mol-HEP-Lab. Serves as the main entry point for the autonomous multi-agent research platform, coordinating all subsystems including pipeline execution, configuration management, health checks, and service operations.

## Key Files
| File | Description |
|------|-------------|
| `src/main.rs` | CLI parser and dispatcher using Clap |
| `src/commands/run.rs` | Run the full research pipeline |
| `src/commands/validate.rs` | Validate config.mol.yaml files |
| `src/commands/doctor.rs` | System health and dependency checks |
| `src/commands/init.rs` | Initialize config.mol.yaml from template |
| `src/commands/setup.rs` | Install/verify optional tools (OpenCode) |
| `src/commands/report.rs` | Generate human-readable run reports |
| `src/commands/serve.rs` | Start all WebSocket services |
| `src/server.rs` | Server startup and route configuration |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/commands/` | Subcommand implementations |

## For AI Agents

### Working In This Directory
- This is the binary entrypoint; it depends on all 16 other mol-* crates
- Run `cargo build -p mol-cli` to build the `mol` binary
- Run `cargo test -p mol-cli` to test CLI parsing and command dispatch
- The crate has no library interface; modify via `src/commands/` modules
- When adding a new command, add a variant to `Commands` enum in `main.rs` and create `src/commands/new_command.rs`

### Testing Requirements
- Test each subcommand's argument parsing via Clap
- Verify integration between `run` and all dependent crates (pipeline, engine, services)
- Integration tests should mock LLM calls and file I/O

### Common Patterns
- All commands use `#[tokio::main]` for async execution
- Commands accept `--config` to override config path
- Logging is initialized once in `main()` via `tracing_subscriber`
- Error handling uses `anyhow::Result` for user-friendly error messages

## Dependencies
### Internal
- mol-common (types, hardware, quality assessment)
- mol-config (configuration loading)
- mol-llm (LLM client factory)
- mol-pipeline (pipeline orchestration)
- mol-engine (code generation and agent loops)
- mol-experiment (sandbox execution)
- mol-literature (paper retrieval)
- mol-web (web scraping and search)
- mol-domains (domain detection)
- mol-knowledge (KB storage)
- mol-templates (LaTeX compilation)
- mol-agents (agent framework)
- mol-metamol (meta-learning)
- mol-services (WebSocket servers)
- mol-health (health checks)
- mol-evolution (self-evolution)

### External
- `clap` (CLI argument parsing)
- `tokio` (async runtime)
- `tracing` + `tracing-subscriber` (logging)
- `serde` + `serde_json` + `serde_yaml` (config/data serialization)
- `axum` + `tower-http` (web server for Serve command)
- `chrono` (timestamps)
- `walkdir` (directory traversal)
- `which` (tool discovery in PATH)
