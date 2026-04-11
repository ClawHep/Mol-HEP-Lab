<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-engine

## Purpose
Core agentic LLM tool-use loop and code generation engine for Mol-HEP-Lab. Implements iterative agent-LLM interactions, tool execution, verification hooks, and code generation strategies. Port of Python `mol/pipeline/mol_engine/` with added bridges to external code environments (OpenCode, OpenHands).

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/turn_loop.rs` | AgentTurnLoop: core LLM interaction loop with tool dispatch |
| `src/codegen/mod.rs` | Codegen module organization |
| `src/codegen/runtime.rs` | CodegenRuntime: filesystem discovery and strategy selection |
| `src/codegen/turn_loop.rs` | CodegenTurnLoop: turn execution within codegen |
| `src/codegen/strategies.rs` | CodegenStrategy trait and implementations |
| `src/codegen/code_agent.rs` | Code agent implementation |
| `src/tools/executor.rs` | Tool execution dispatcher |
| `src/tools/definitions.rs` | Tool definitions and schemas |
| `src/tools/permissions.rs` | Execution permission checks |
| `src/session.rs` | Session lifecycle management |
| `src/bridges/opencode.rs` | OpenCode bridge for external code execution |
| `src/bridges/openhands.rs` | OpenHands bridge for code environments |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/codegen/` | Code generation runtime and strategies |
| `src/tools/` | Tool definitions and execution |
| `src/bridges/` | External code environment bridges |

## For AI Agents

### Working In This Directory
- This is a library crate for code generation and agent loops
- Run `cargo test -p mol-engine` to test agent loops and tool execution
- Run `cargo clippy -p mol-engine` for linting
- Main entry point: `AgentTurnLoop::new()` then `.run(prompt)` to execute a turn
- Code generation: `CodegenRuntime::new()` then `.execute()` for full pipeline

### Testing Requirements
- Agent turn loop tests: verify LLM message passing and tool call handling
- Tool execution tests: mock tool calls and verify result injection
- Strategy tests: different codegen approaches (mol-agent vs fallback)
- Verification hook tests: ensure one-shot verification is applied
- Session tests: workspace discovery and checkpoint management

### Common Patterns
- Turn loops use request/response cycles with tool call injection
- Each turn produces `TurnResult { artifacts, tool_calls, iterations }`
- CodegenStrategy trait allows pluggable generation strategies
- Tools have schemas defined via JSON Schema
- Verification hooks inject quality gates after code generation
- Sessions track workspace state and filesystem artifacts

## Dependencies
### Internal
- mol-common (types, hardware detection, quality assessment)
- mol-config (configuration)
- mol-llm (LLM client)

### External
- `serde` + `serde_json` (schema serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `async-trait` (trait definitions)
- `regex` (text parsing)
- `tempfile` (test fixtures)
- `chrono` (timestamps)
- `reqwest` (HTTP for bridges)
- `glob` (file pattern matching)
- `serde_yaml` (config loading)
- `md-5` (checksums)
- `walkdir` (directory traversal)
