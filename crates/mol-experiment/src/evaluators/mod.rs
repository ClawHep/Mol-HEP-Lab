//! Experiment evaluation utilities (convergence analysis, etc.)

pub mod convergence;

pub use convergence::{
    analyze_convergence, compute_convergence_order, ConvergencePoint, ConvergenceReport,
    ConvergenceResult,
};
