//! Mol-HEP-Lab: Pipeline execution and stage orchestration.
//!
//! This crate implements the 26-stage research pipeline state machine and
//! runner, ported from the Python `mol` backend.
//!
//! # Modules
//!
//! - [`stages`]     — Stage enum, status/event enums, and the core `advance`
//!                    state machine function.
//! - [`checkpoint`] — Atomic checkpoint and heartbeat I/O.
//! - [`contracts`]  — Per-stage input/output artifact contracts.
//! - [`executor`]   — Stage dispatch (`execute_stage`) with stub implementations.
//! - [`runner`]     — High-level pipeline orchestration (`execute_pipeline`).

pub mod checkpoint;
pub mod contracts;
pub mod executor;
pub mod runner;
pub mod runtimes;
pub mod stages;

// ---------------------------------------------------------------------------
// Convenience re-exports for downstream crates
// ---------------------------------------------------------------------------

pub use stages::{
    advance, advance_with_opts, decision_rollback, default_rollback_stage, gate_required,
    gate_rollback, next_stage, phase_map, previous_stage, Stage, StageStatus, TransitionEvent,
    TransitionOutcome, GATE_STAGES, MAX_DECISION_PIVOTS, NONCRITICAL_STAGES, STAGE_SEQUENCE,
};

pub use checkpoint::{read_checkpoint, resume_from_checkpoint, write_checkpoint, write_heartbeat};

pub use contracts::{get_contract, validate_inputs, validate_outputs, StageContract};

pub use executor::{execute_stage, MolConfig, StageContext, StageResult};

pub use runner::{execute_iterative_pipeline, execute_pipeline, PipelineConfig, PipelineSummary};
