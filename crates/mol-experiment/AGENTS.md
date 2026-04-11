<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-experiment

## Purpose
Experiment scheduling, sandbox execution, and result evaluation for AI-generated Python code. Supports multiple backends (Local, Docker, SSH, Colab) with GPU passthrough, metric extraction, convergence checking, and visualization. Enables safe isolated execution of research experiments across heterogeneous hardware.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/sandbox.rs` | LocalSandbox trait and execution interface |
| `src/runner.rs` | ExperimentRunner and factory functions |
| `src/docker.rs` | Docker backend via bollard |
| `src/ssh.rs` | SSH remote execution backend |
| `src/colab.rs` | Google Colab backend via Drive file polling |
| `src/metrics.rs` | Metric extraction from stdout/files |
| `src/evaluators/convergence.rs` | Convergence detection for training |
| `src/validation.rs` | Code validation and safety checks |
| `src/visualize.rs` | Result visualization generation |
| `src/harness.rs` | Experiment execution harness |
| `src/git_manager.rs` | Git repo management for experiments |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/evaluators/` | Metric evaluators (convergence checking, etc.) |

## For AI Agents

### Working In This Directory
- This is a library crate for experiment execution
- Run `cargo test -p mol-experiment` to test sandbox logic and metric parsing
- Run `cargo clippy -p mol-experiment` for linting
- Factory: use `build_runner(config, workspace)` to create appropriate backend
- Main interface: `ExperimentRunner::run_experiment(code, env)` returns `ExperimentResult`

### Testing Requirements
- Local sandbox tests: verify subprocess execution and output capture
- Docker backend tests: mock Docker API responses (use testcontainers or mockito)
- SSH backend tests: mock SSH connections (use fake SSH server)
- Metric extraction tests: parse various output formats (loss values, accuracy, etc.)
- Convergence tests: verify detection of training convergence
- Git manager tests: verify repo setup and cleanup

### Common Patterns
- All backends implement `Sandbox` trait with `execute()` method
- Metrics extracted via regex patterns or file parsing
- Results include stdout, stderr, return code, and structured metrics
- Convergence detection uses moving averages over training iterations
- Docker uses GPU passthrough via `--gpus all`
- SSH supports key-based and password authentication

## Dependencies
### Internal
- mol-common (types, hardware detection)
- mol-config (experiment configuration)
- mol-llm (LLM client for code validation)

### External
- `serde` + `serde_json` (result serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `async-trait` (trait definitions)
- `bollard` (Docker API)
- `ssh2` (SSH connections)
- `tempfile` (test fixtures)
- `regex` (metric parsing)
- `uuid` (experiment IDs)
- `chrono` (timestamps)
- `futures-util` (async utilities)
- `shellexpand` (environment variable expansion)
- `git2` (Git operations)
- `plotters` (visualization)
