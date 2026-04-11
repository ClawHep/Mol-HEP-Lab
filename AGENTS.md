<!-- Generated: 2026-04-12 -->

# Mol-HEP-Lab

## Purpose

Mol-HEP-Lab is an autonomous multi-agent research platform for high energy physics. It orchestrates 17 specialized AI agents through a unified pipeline to automate the scientific research workflow: from topic exploration through literature review, hypothesis generation, experiment design, code implementation, sandbox execution, result analysis, and paper authoring. The system operates via 5 phases across 18 pipeline stages with human-in-the-loop review gates at critical decision points.

## Key Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Rust workspace manifest defining 17 member crates |
| `Cargo.lock` | Workspace dependency lock file |
| `config.mol.yaml` | Runtime configuration for pipeline, LLM providers, experiment budgets, domains, security |
| `rust-toolchain.toml` | Rust edition specification (2024) |
| `start.sh` | Launch script to initialize server and frontend |
| `references.bib` | Bibliography in BibTeX format |
| `frontend/package.json` | React 19 + TypeScript + Vite dependencies |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `crates/` | 17 Rust crates implementing the pipeline engine, agent system, LLM layer, web server, and domain knowledge |
| `frontend/` | React 19 web UI with TypeScript and Vite; real-time WebSocket monitoring dashboard |
| `hep/` | High energy physics domain vertical: agent definitions, methodology, conventions, prompt templates |
| `generic/` | Domain-agnostic agent templates, common patterns, reusable conventions |
| `data/` | Static YAML data: prompt templates, benchmark datasets, Docker profiles, seminal papers, knowledge registry |
| `docs/` | Architecture deep-dive, handoff procedures, E2E friction logs, planning specs |
| `assets/` | Project assets: logos, UI screenshots, showcase directory with research project outputs |
| `examples/` | Configuration templates (config_template.yaml) for new projects |
| `backend/` | Runtime execution artifacts: pipeline run queues and checkpoints (do not modify directly) |
| `target/` | Cargo build artifacts (ignored) |

## For AI Agents

### Working In This Directory

1. **Backend Development** — All backend work happens in `crates/`. The workspace is unified: edit source code, run tests, and build from the repository root.
2. **Frontend Development** — Frontend changes are in `frontend/`. Use `cd frontend && npm install && npm run dev` to start the Vite dev server on port 5173.
3. **Configuration** — Copy `examples/config_template.yaml` to create project configs. Customize LLM provider, experiment budgets, domain keywords, and stage behavior in `config.mol.yaml`.
4. **Runtime Data** — DO NOT modify files under `backend/runs/` — these are pipeline execution artifacts. Read them for debugging; all mutations happen through the pipeline state machine.
5. **Documentation** — Update docs in `docs/`. Keep `AGENTS.md` files synchronized when adding new directories or crates.

### Testing Requirements

**Backend:**
```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p mol-pipeline

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

**Frontend:**
```bash
cd frontend
npm test
npm run build
```

**Integration:**
- Start backend: `cargo run -p mol-cli -- serve`
- In another terminal: `cd frontend && npm run dev`
- Navigate to `http://localhost:5173` to access the dashboard
- WebSocket connections tested via browser DevTools

### Common Patterns

- **Pipeline Stages**: Defined in `crates/mol-pipeline`. Each stage is a state in a state machine with input/output contracts.
- **LLM Integration**: All LLM calls go through `crates/mol-llm`, which abstracts OpenAI-compatible APIs, Anthropic, and ACP bridge.
- **Prompts**: Templates are in `data/prompts.default.yaml` and domain-specific directories. Use Tera template syntax `{var_name}` for injection.
- **Domain Knowledge**: HEP-specific knowledge in `hep/`, generic patterns in `generic/`. Domain detection uses 70+ HEP keywords.
- **Experiment Execution**: Sandboxed in `crates/mol-experiment` with multiple backends: local, Docker, SSH, Colab.
- **Agent Definitions**: Benchmarking and agent metadata in `crates/mol-agents`. Code search utilities for literature integration in `crates/mol-literature`.

## Dependencies

### Internal
- 17 Rust crates in workspace, all interconnected via shared types in `mol-common`
- React frontend communicates via WebSocket to Axum server in `mol-web`

### External

**Backend (Rust):**
- Tokio 1 (async runtime)
- Axum 0.8 (HTTP + WebSocket server)
- Serde 1 + serde_yaml (config serialization)
- Reqwest 0.12 (HTTP client for LLM APIs)
- Tera 1 (Jinja2-compatible templating)
- Bollard 0.18 (Docker integration)
- SSH2 0.9 (remote execution)
- Scraper 0.22, quick-xml 0.37 (literature parsing)

**Frontend (Node):**
- React 19
- TypeScript
- Vite 8.0.8
- Axios or Fetch API for REST calls
- WebSocket client for real-time updates

## Commands

```bash
# Initialize a new research project
mol init --project-id my-project --topic "quantum entanglement"

# Start the pipeline server
mol serve

# Run pipeline to completion (or next gate)
mol run

# Health check all dependencies
mol doctor

# Build backend
cargo build --release

# Build frontend
cd frontend && npm run build
```

## Architecture Overview

```
┌─────────────────────────────────────┐
│      React Dashboard (Port 5173)    │
│  5-Phase Pipeline Visualization     │
│  Agent Status + Resource Monitor    │
└──────────────┬──────────────────────┘
               │ WebSocket (/ws/agents, /ws/resources)
┌──────────────▼──────────────────────┐
│    Axum Web Server (Port 3000)      │
│  REST API + Static Files            │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────────────┐
│   Pipeline Runner (18-Stage FSM)            │
│  - State machine with contracts             │
│  - Checkpoint/restore support               │
│  - PIVOT/REFINE decision rollback           │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼────────────────────────────────────────┐
│  Execution Layer (Engine, Agents, LLM, Experiment)   │
│  - Code generation (mol-engine)                       │
│  - Agent definitions (mol-agents)                     │
│  - LLM abstraction (mol-llm)                          │
│  - Sandbox execution (mol-experiment)                 │
└─────────────────────────────────────────────────────────┘
```

---

See `docs/architecture-deep-dive.md` for comprehensive system design details.
See `crates/AGENTS.md` for individual crate responsibilities.
