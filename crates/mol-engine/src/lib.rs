//! mol-engine: Agentic LLM tool-use loop and code generation engine.
//!
//! This crate is the Rust port of `researchclaw/pipeline/claw_engine/` and
//! `researchclaw/pipeline/codegen/`, re-branded for Mol-HEP-Lab as `mol_engine`.
//!
//! # Architecture
//!
//! ```text
//! AgentTurnLoop  ─────────────────────────────────────────────────────────┐
//!   │  sends messages to LLM API                                           │
//!   │  dispatches tool calls via ToolExecutor                              │
//!   │  injects verification hooks (one-shot)                               │
//!   └─ returns TurnResult { artifacts_produced, tool_calls, iterations }   │
//!                                                                           │
//! CodegenRuntime ──────────────────────────────────────────────────────────┘
//!   │  discovers filesystem context (checkpoints, datasets, codebases)
//!   │  selects CodegenStrategy (MolAgentStrategy or FallbackStrategy)
//!   │  runs AgentTurnLoop via the strategy
//!   └─ writes experiment/ dir + experiment_spec.md
//! ```
//!
//! # Quick start
//!
//! ```no_run
//! use mol_engine::turn_loop::{AgentTurnLoop, LlmConfig};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = LlmConfig::new(
//!     "https://api.openai.com/v1",
//!     "sk-...",
//!     "gpt-4o",
//! );
//! let mut loop_ = AgentTurnLoop::new(config, "/tmp/workspace", "You are a coding assistant.");
//! let result = loop_.run("Write a hello world Python script.").await?;
//! println!("Files produced: {:?}", result.artifacts_produced.keys().collect::<Vec<_>>());
//! # Ok(())
//! # }
//! ```

pub mod codegen;
pub mod session;
pub mod tools;
pub mod turn_loop;

// Convenience re-exports for the most commonly used types
pub use codegen::{
    CodegenContext, CodegenPhase, CodegenResult, CodegenRuntime, CodegenStrategy,
    CodegenTurnLoop, ComplianceConfig, DiscoveredData, FallbackStrategy, GeneratedFiles,
    HardwareProfile, MolAgentStrategy,
};
pub use session::StageSession;
pub use tools::{PermissionPolicy, ToolExecutor, ToolResult, ToolSpec};
pub use turn_loop::{AgentTurnLoop, LlmConfig, TurnResult, VerificationHook, MAX_ITERATIONS};
