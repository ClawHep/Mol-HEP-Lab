<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-pipeline

## Purpose
Pipeline execution and stage orchestration for Mol-HEP-Lab's 5-phase/26-step research workflow. Implements state machine for advancing stages, checkpoint I/O for resumability, per-stage input/output contracts, and high-level orchestration of the entire pipeline including iterative refinement and rollback mechanisms.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and public API |
| `src/stages.rs` | Stage/Phase enums, state machine (advance function), transitions |
| `src/checkpoint.rs` | Checkpoint write/read and heartbeat I/O |
| `src/contracts.rs` | Per-stage artifact I/O contracts and validation |
| `src/executor.rs` | Stage dispatch and execution context |
| `src/runner.rs` | High-level pipeline orchestration |
| `src/knowledge.rs` | Agent mapping and dataset configuration |
| `src/stages_impl/` | Phase-specific implementations (phase1.rs through phase5.rs) |
| `src/runtimes/` | Runtime backends: experiment_run, iterative_refine, result_analysis, sanity_check |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/stages_impl/` | Implementation of each phase's stages |
| `src/runtimes/` | High-level execution runtimes |

## For AI Agents

### Working In This Directory
- This is a library crate for pipeline orchestration
- Run `cargo test -p mol-pipeline` to test state machine and contracts
- Run `cargo clippy -p mol-pipeline` for linting
- Main entry point: `execute_pipeline()` or `execute_iterative_pipeline()`
- Stages are immutable; state advances via `advance()` function returning `TransitionOutcome`

### Testing Requirements
- State machine tests: verify all valid transitions and reject invalid ones
- Checkpoint tests: write/resume with various stage states
- Contract validation: verify inputs/outputs at each stage boundary
- Integration: test across all 5 phases with mocked stage execution
- Rollback tests: verify decision rollback mechanisms and gate logic

### Common Patterns
- Enum-based state machine: `Stage` enum with variants for each of 26 steps
- Checkpoints use JSON serialization for resumability
- Each stage has input/output artifact contracts
- Transitions validated via `TransitionEvent` enum
- Rollback triggered by `TransitionOutcome::Rollback`
- Gates define required/optional stage execution

## Dependencies
### Internal
- mol-common (types, artifact definitions)
- mol-config (pipeline configuration)
- mol-llm (LLM integration for stage logic)

### External
- `serde` + `serde_json` + `serde_yaml` (checkpoint/config serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `chrono` (timestamps)
- `num_enum` (stage enums)
- `tempfile` (test fixtures)
- `regex` (log parsing)
