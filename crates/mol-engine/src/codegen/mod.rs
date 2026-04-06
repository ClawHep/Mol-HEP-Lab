//! Code generation subsystem.
//!
//! Provides the `CodegenRuntime` orchestrator, strategy trait and built-in
//! implementations, and the extended turn loop with compliance hooks.

pub mod runtime;
pub mod strategies;
pub mod turn_loop;
pub mod types;

pub use runtime::{CodegenRuntime, discover_data};
pub use strategies::{CodegenStrategy, FallbackStrategy, MolAgentStrategy};
pub use turn_loop::{CodegenTurnLoop, ComplianceConfig};
pub use types::{
    CodegenContext, CodegenPhase, CodegenResult, DiscoveredData, GeneratedFiles, HardwareProfile,
};
