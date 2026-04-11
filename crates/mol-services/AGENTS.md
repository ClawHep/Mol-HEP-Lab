<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-services

## Purpose
WebSocket service layer for Mol-HEP-Lab. Provides Axum-based async web services including agent orchestration bridge, GPU/CPU resource monitoring, experiment result caching, and multi-round multi-agent discussion engine. Enables real-time frontend communication and resource tracking during pipeline execution.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization, server builder, and CORS setup |
| `src/agent_bridge.rs` | Main agent orchestration WebSocket service |
| `src/resource_monitor.rs` | GPU/CPU resource streaming WebSocket service |
| `src/result_registry.rs` | Shared experiment result cache (file-backed) |
| `src/discussion.rs` | Multi-round multi-agent discussion engine |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate for WebSocket services
- Run `cargo test -p mol-services` to test service logic
- Run `cargo clippy -p mol-services` for linting
- Main entry point: `build_server(state)` returns Axum router with all endpoints
- Endpoints: `GET /ws/agents` (agent bridge), `GET /ws/resources` (monitor)

### Testing Requirements
- Agent bridge tests: WebSocket message handling, agent context flow
- Resource monitor tests: GPU/CPU metrics streaming
- Result registry tests: caching, retrieval, file persistence
- Discussion tests: multi-agent message coordination
- WebSocket tests: connection lifecycle, message ordering
- Concurrency tests: multiple simultaneous connections

### Common Patterns
- All services use Axum for async HTTP/WebSocket handling
- State shared via `Arc<BridgeState>` with thread-safe access (dashmap)
- WebSocket messages serialized as JSON
- Resource monitoring uses sysinfo and nvml_wrapper
- Result registry backed by filesystem (JSON files)
- CORS restricted to localhost connections

## Dependencies
### Internal
- mol-common (types, hardware detection)
- mol-config (service configuration)
- mol-llm (LLM integration for discussion)
- mol-pipeline (pipeline state access)

### External
- `axum` (async web framework)
- `axum-extra` (additional middleware/utilities)
- `tower-http` (CORS middleware)
- `tokio` (async runtime)
- `serde` + `serde_json` (JSON serialization)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `dashmap` (thread-safe caching)
- `uuid` (message/connection IDs)
- `chrono` (timestamps)
- `futures-util` (async utilities)
- `sysinfo` (CPU/memory monitoring)
- `nvml-wrapper` (GPU monitoring)
- `async-trait` (trait definitions)
- `base64` (encoding)
- `regex` (message parsing)
