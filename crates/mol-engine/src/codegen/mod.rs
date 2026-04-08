//! Code generation subsystem.
//!
//! Provides the `CodegenRuntime` orchestrator, strategy trait and built-in
//! implementations, the extended turn loop with compliance hooks, and the
//! multi-phase `CodeAgent`.

pub mod code_agent;
pub mod runtime;
pub mod strategies;
pub mod turn_loop;
pub mod types;

pub use code_agent::{
    Blueprint, BlueprintFile, ChatMessage, ClassSummary, CodeAgent, CodeAgentConfig,
    CodeAgentResult, CodeSummary, FunctionSummary, LlmClient, LlmResponse, MethodSummary,
    SandboxLike, SandboxResult, SolutionNode,
};
pub use runtime::{CodegenRuntime, discover_data};
pub use strategies::{CodegenStrategy, FallbackStrategy, MolAgentStrategy};
pub use turn_loop::{CodegenTurnLoop, ComplianceConfig};
pub use types::{
    CodegenContext, CodegenPhase, CodegenResult, DiscoveredData, GeneratedFiles, HardwareProfile,
};
