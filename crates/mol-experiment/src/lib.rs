//! Mol-HEP-Lab: Experiment scheduling, Docker, and SSH execution
//!
//! This crate provides multiple sandbox backends for running AI-generated
//! Python experiment code safely:
//!
//! | Backend     | Type            | Notes                               |
//! |-------------|-----------------|-------------------------------------|
//! | `Local`     | subprocess      | Fastest; no isolation               |
//! | `Docker`    | bollard         | Isolated container; GPU passthrough |
//! | `Ssh`       | ssh2            | Remote execution on GPU servers     |
//! | `Colab`     | Drive poll      | Google Colab via Drive file I/O     |
//!
//! # Quick start
//!
//! ```rust,no_run
//! use mol_experiment::runner::build_runner;
//! use mol_config::ExperimentConfig;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = ExperimentConfig::default();
//! let runner = build_runner(config, PathBuf::from("/tmp/mol-exp")).await?;
//! let result = runner.run_experiment("print('accuracy: 0.95')", None).await?;
//! println!("success={} metrics={:?}", result.success, result.metrics);
//! # Ok(())
//! # }
//! ```

pub mod colab;
pub mod docker;
pub mod evaluators;
pub mod harness;
pub mod metrics;
pub mod runner;
pub mod sandbox;
pub mod ssh;
pub mod validation;

// ---------------------------------------------------------------------------
// Top-level re-exports
// ---------------------------------------------------------------------------

pub use metrics::{
    extract_loss_curves, parse_metrics_from_file, parse_metrics_from_stdout, MetricValue,
};
pub use runner::{build_runner, create_sandbox, ExperimentResult, ExperimentRunner};
pub use sandbox::{ExecutionResult, LocalSandbox, LocalSandboxConfig, Sandbox};
pub use validation::{
    check_import_whitelist, check_security, validate_code, validate_python_syntax,
    Category, Severity, ValidationIssue,
};
