<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Crates

## Purpose

This directory contains 17 specialized Rust crates that implement the Mol-HEP-Lab backend system. Each crate has a single, focused responsibility within the automated research pipeline. They are linked via a shared workspace and common types defined in `mol-common`.

## Crate Overview

| Crate | Purpose |
|-------|---------|
| `mol-cli` | Command-line entry point. Implements `mol init`, `mol serve`, `mol run`, `mol doctor` commands. Orchestrates startup and configuration loading. |
| `mol-common` | Shared types, prompts, utilities, and constants. Defines PipelineStage, Agent, Experiment, Result types used across all crates. |
| `mol-config` | YAML configuration parsing. Reads `config.mol.yaml`, validates domains, LLM settings, experiment budgets, security gates. |
| `mol-llm` | LLM provider abstraction layer. Supports OpenAI-compatible APIs, Anthropic Messages API, ACP CLI bridge. Handles token counting, streaming, fallback routing. |
| `mol-pipeline` | 18-stage pipeline orchestration and state machine. Manages stage transitions, contracts, gates, checkpoints, rollback on PIVOT/REFINE decisions. |
| `mol-engine` | Core code generation engine. Transforms AI responses into executable Python/bash scripts. Template-driven code generation with domain-specific linting. |
| `mol-experiment` | Experiment framework and sandbox execution. Supports local, Docker, SSH, and Google Colab backends. Manages timeouts, resource limits, artifact capture. |
| `mol-literature` | Literature retrieval and novelty assessment. Integrates OpenAlex, INSPIRE-HEP, arXiv APIs. Deduplication, relevance ranking, citation synthesis. |
| `mol-web` | Axum web server. REST API endpoints, WebSocket handlers for agent status and resource monitoring, static file serving for React frontend. |
| `mol-domains` | Domain detection system. 70+ HEP keywords. Adapters for other domains (ML, chemistry, physics). Domain classification and routing. |
| `mol-knowledge` | Knowledge graph construction and extraction. Semantic synthesis of experiment results. Entity linking, relationship extraction from outputs. |
| `mol-templates` | Tera template engine for dynamic prompt generation. Multi-level knowledge chain (HEP → generic). Template validation and variable injection. |
| `mol-agents` | Agent definitions, benchmarking framework, and code search utilities. Agent state tracking, performance metrics, capability registration. |
| `mol-metamol` | Meta-analysis and cross-experiment coordination. Aggregate results, statistical synthesis, evolutionary improvement suggestions. |
| `mol-services` | Service layer for Docker, SSH, notifications. Docker image caching, SSH key management, email/webhook notification dispatch. |
| `mol-health` | Health monitoring and dependency checks. Verifies LLM connectivity, Docker daemon, SSH keys, GPU availability. Runs in `mol doctor`. |
| `mol-evolution` | Evolutionary and iterative improvement algorithms. Genetic algorithms for hyperparameter search, iterated refinement, candidate ranking. |

## Dependency Graph

```
mol-cli (entrypoint)
    ├── mol-common (shared types)
    ├── mol-config (configuration)
    ├── mol-llm (LLM abstraction)
    ├── mol-pipeline (stage orchestration)
    ├── mol-engine (code generation)
    ├── mol-experiment (sandbox)
    ├── mol-literature (retrieval)
    ├── mol-web (server)
    ├── mol-domains (domain detection)
    ├── mol-knowledge (synthesis)
    ├── mol-templates (Tera engine)
    ├── mol-agents (definitions)
    ├── mol-metamol (meta-analysis)
    ├── mol-services (Docker/SSH)
    ├── mol-health (monitoring)
    └── mol-evolution (iterative)

mol-pipeline
    ├── mol-common
    ├── mol-config
    └── mol-llm

mol-engine
    ├── mol-common
    └── mol-templates

mol-experiment
    ├── mol-common
    ├── mol-services
    └── mol-health

mol-web
    ├── mol-common
    ├── mol-pipeline
    └── mol-services
```

## For AI Agents

### Working In This Directory

1. **Adding a New Crate**: Create `mol-newfeature/Cargo.toml` and `mol-newfeature/src/lib.rs`. Add `crates/mol-newfeature` to workspace members in root `Cargo.toml`.
2. **Shared Types**: If your crate needs types used across multiple crates, add them to `mol-common`. Avoid circular dependencies.
3. **External Dependencies**: Add to `[workspace.dependencies]` in root `Cargo.toml`, not individual `Cargo.toml` files. This ensures consistent versions.
4. **Async Code**: All I/O must be async using Tokio. Use `#[tokio::main]` in tests, `async fn` in library code.
5. **Error Handling**: Return `anyhow::Result<T>` or custom `thiserror` error enums. Don't panic in library code.

### Testing Requirements

**Test each crate independently:**
```bash
# Test a specific crate
cargo test -p mol-pipeline

# Test all crates
cargo test

# With logging
RUST_LOG=debug cargo test -p mol-llm -- --nocapture --test-threads=1

# Verify no clippy warnings
cargo clippy --all -- -D warnings

# Check formatting
cargo fmt -- --check
```

**Integration Testing:**
```bash
# Start backend server
cargo run -p mol-cli -- serve

# In another terminal, verify endpoints
curl http://localhost:3000/health
# Check logs for WebSocket connections from frontend
```

### Common Patterns

- **Service Trait Pattern** (mol-llm, mol-experiment): Define a trait for the service, implement for multiple backends, inject at runtime.
- **Async Request/Response** (mol-web): Use Axum extractors for request parsing, return JSON responses or WebSocket streams.
- **State Machine** (mol-pipeline): Use `num_enum` for stage enumeration, `match` on current state for transitions.
- **Template Rendering** (mol-templates): Precompile Tera templates at startup, render with context HashMap at runtime.
- **Resource Cleanup** (mol-services): Use `Drop` trait or `async_drop` for cleanup; ensure Docker containers/SSH sessions are closed.

## Dependencies

### Internal
- All crates depend on `mol-common` for shared types
- `mol-cli` depends on all other crates for orchestration
- `mol-web` depends on `mol-pipeline` for state inspection
- `mol-experiment` depends on `mol-services` for Docker/SSH

### External (from workspace Cargo.toml)

**Runtime:**
- Tokio 1 — async runtime
- Axum 0.8, tower-http 0.6 — HTTP server with WebSocket support
- Reqwest 0.12 — HTTP client for LLM APIs
- Serde 1, serde_yaml 0.9 — serialization
- Tera 1 — Jinja2-compatible templating

**Infrastructure:**
- Bollard 0.18 — Docker API client
- SSH2 0.9 — SSH remote execution
- Scraper 0.22, quick-xml 0.37 — HTML/XML parsing for literature
- Sysinfo 0.32, nvml-wrapper 0.10 — system and GPU monitoring

**Development:**
- Clap 4 — CLI argument parsing
- Tracing, tracing-subscriber — structured logging
- Thiserror, anyhow — error handling

## Commands

**Build and test:**
```bash
# Build all crates
cargo build

# Build for release (optimized)
cargo build --release

# Run tests for all crates
cargo test

# Check for issues without building
cargo check
```

**Code quality:**
```bash
# Format code
cargo fmt

# Lint
cargo clippy --all -- -D warnings

# Generate documentation
cargo doc --open
```

## Architecture Notes

- **Workspace Resolver**: Uses resolver="2" for optimal dependency resolution in monorepos.
- **Edition**: All crates target Rust 2024 edition.
- **License**: MIT (workspace-level).
- **Features**: Use minimal features by default; enable features only where needed to reduce compilation time.

---

See `../AGENTS.md` for root-level project context.
See individual crate `src/lib.rs` for module-level documentation.
