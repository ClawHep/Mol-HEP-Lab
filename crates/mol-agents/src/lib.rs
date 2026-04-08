//! Mol-HEP-Lab: Autonomous agent definitions and coordination
//!
//! Provides:
//! - [`base`] — core [`BaseAgent`] trait, [`AgentContext`], [`AgentPlan`],
//!   [`AgentStepResult`], [`ReviewOutcome`], and [`AgentOrchestrator`].
//! - [`benchmark`] — benchmark discovery, selection, acquisition, and
//!   validation pipeline ([`BenchmarkOrchestrator`]).
//! - [`code_searcher`] — GitHub search, pattern extraction, and caching
//!   ([`CodeSearchAgent`]).
//! - [`figure`] — figure planning, code generation, rendering, and quality
//!   review pipeline ([`FigureOrchestrator`]).

pub mod base;
pub mod benchmark;
pub mod code_searcher;
pub mod figure;

// Re-export the most commonly used items at crate root.
pub use base::{
    AgentContext, AgentOrchestrator, AgentPlan, AgentStepResult, BaseAgent, ReviewOutcome,
};
pub use benchmark::{BenchmarkAgentConfig, BenchmarkOrchestrator, BenchmarkPlan};
pub use code_searcher::{CodeSearchAgent, CodeSearchResult};
pub use figure::{FigureAgentConfig, FigureOrchestrator, FigurePlan};
