<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-llm

## Purpose
Unified async LLM client supporting OpenAI-compatible APIs (GPT-4o, o3, gpt-5.x, DeepSeek), Anthropic Claude family, and ACP (Agent Client Protocol) via subprocess bridges. Abstracts provider differences and provides retry logic, request/response serialization, and preflight validation.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and factory functions |
| `src/client.rs` | Main LLMClient struct and message types |
| `src/provider.rs` | Provider enum and factory logic |
| `src/response.rs` | Response parsing and preflight validation |
| `src/anthropic.rs` | Anthropic Messages API adapter |
| `src/acp.rs` | Agent Client Protocol (subprocess) adapter |
| `src/retry.rs` | Exponential backoff and error handling |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate for LLM access throughout Mol-HEP-Lab
- Run `cargo test -p mol-llm` to test client logic and mocking
- Run `cargo clippy -p mol-llm` for linting
- Factory: use `create_llm_client(config)` to build appropriate provider
- All chat methods are async and use `Message` enum for input

### Testing Requirements
- Mocked HTTP responses for OpenAI/Anthropic providers
- ACP subprocess spawning tests (use temporary scripts)
- Retry logic verification: exponential backoff on transient failures
- Response parsing tests: verify structured outputs and streaming
- Error handling: API errors, timeouts, authentication failures

### Common Patterns
- All providers implement same async `chat()` interface
- Models with structured output support use OpenAI's `response_format`
- Streaming responses available via iterative `chunk()` calls
- Retry policy: up to 3 attempts with exponential backoff
- Request/response serialization uses Serde with custom error handling

## Dependencies
### Internal
- mol-common (types, hardware detection)
- mol-config (LLM configuration)

### External
- `reqwest` (HTTP client for OpenAI/Anthropic)
- `serde` + `serde_json` (request/response serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `chrono` (timestamps)
- `url` (URL parsing)
- `futures-util` (async utilities)
- `regex` (response parsing)
- `tempfile` (test fixtures)
- `which` (ACP tool discovery)
- `async-trait` (trait definitions)
- `bytes` (binary data handling)
