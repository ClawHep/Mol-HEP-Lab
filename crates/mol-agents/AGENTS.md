<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-agents

## Purpose
Autonomous agent definitions and coordination for Mol-HEP-Lab. Provides base agent trait and context, orchestrators for specialized agents (Benchmark, Code Search, Figure), and multi-step planning/execution with LLM-driven decision-making and review cycles.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/base.rs` | BaseAgent trait, AgentContext, AgentOrchestrator, review logic |
| `src/benchmark/mod.rs` | Benchmark orchestrator and pipeline |
| `src/benchmark/surveyor.rs` | Literature survey for benchmarks |
| `src/benchmark/selector.rs` | Benchmark selection via evaluation metrics |
| `src/benchmark/acquirer.rs` | Dataset acquisition and setup |
| `src/benchmark/validator.rs` | Benchmark validation and metrics extraction |
| `src/code_searcher/mod.rs` | Code search agent and GitHub integration |
| `src/code_searcher/github_client.rs` | GitHub API client for code search |
| `src/code_searcher/pattern_extractor.rs` | Code pattern extraction and caching |
| `src/code_searcher/query_gen.rs` | Search query generation from requirements |
| `src/figure/mod.rs` | Figure orchestrator for visualization |
| `src/figure/planner.rs` | Figure planning and style selection |
| `src/figure/codegen.rs` | Code generation for figures |
| `src/figure/renderer.rs` | Rendering and quality checks |
| `src/figure/critic.rs` | LLM-driven figure criticism |
| `src/figure/decision.rs` | Decision logic for figure improvements |
| `src/figure/integrator.rs` | Integration of figures into documents |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/benchmark/` | Benchmark discovery and validation pipeline |
| `src/code_searcher/` | Code search agent with GitHub integration |
| `src/figure/` | Figure generation and improvement pipeline |

## For AI Agents

### Working In This Directory
- This is a library crate for agent orchestration
- Run `cargo test -p mol-agents` to test agent logic and LLM interactions
- Run `cargo clippy -p mol-agents` for linting
- Main entry points: `BenchmarkOrchestrator::run()`, `CodeSearchAgent::search()`, `FigureOrchestrator::run()`
- All orchestrators use LLM for planning, execution, and review

### Testing Requirements
- BaseAgent tests: context creation, plan generation, step execution
- Orchestrator tests: verify multi-step workflows with mocked LLM calls
- Benchmark tests: dataset acquisition, metric extraction, validation
- Code search tests: GitHub API mocking, query generation, pattern extraction
- Figure tests: code generation, rendering, quality assessment
- Review cycle tests: verify feedback incorporation and iteration limits

### Common Patterns
- All agents extend BaseAgent trait with specialized logic
- Multi-step planning via `AgentPlan` struct with steps
- Each step returns `AgentStepResult` with artifacts and status
- LLM calls for planning and review via mol-llm integration
- Results include review feedback for iterative improvement
- Caching for expensive operations (GitHub searches, rendered figures)

## Dependencies
### Internal
- mol-common (types, quality assessment)
- mol-llm (LLM client for planning/review)
- mol-experiment (for benchmark execution)
- mol-web (web scraping for data acquisition)

### External
- `serde` + `serde_json` (result serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `async-trait` (trait definitions)
- `reqwest` (GitHub API calls)
