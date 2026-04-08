//! Agent runtime modules for pipeline stage execution.
//!
//! Each module encapsulates workspace preparation, prompt building,
//! result checking, and file management for a family of pipeline stages.
//! The actual LLM turn loop is wired in Phase 4 when the executor
//! integrates these runtimes.

pub mod experiment_run;
pub mod iterative_refine;
pub mod result_analysis;
pub mod sanity_check;
