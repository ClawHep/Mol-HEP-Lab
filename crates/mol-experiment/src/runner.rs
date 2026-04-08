//! Experiment orchestration: tie together a sandbox, validation, and metric parsing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use mol_config::ExperimentConfig;

use crate::sandbox::{ExecutionResult, Sandbox};
use crate::validation::validate_code;

// ---------------------------------------------------------------------------
// ExperimentResult
// ---------------------------------------------------------------------------

/// The full outcome of a single experiment run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResult {
    /// Whether the experiment produced a successful (exit code 0) result.
    pub success: bool,
    /// Parsed scalar metrics (name → value).
    pub metrics: HashMap<String, f64>,
    /// The complete stdout log.
    pub logs: String,
    /// The stderr output.
    pub stderr: String,
    /// Paths to any artifact files produced (e.g. checkpoints, plots).
    pub artifacts: Vec<PathBuf>,
    /// Wall-clock duration of the sandbox execution.
    pub duration: Duration,
    /// Whether the run hit the timeout deadline.
    pub timed_out: bool,
    /// Non-fatal errors / warnings produced during the run.
    pub warnings: Vec<String>,
}

impl ExperimentResult {
    /// Return the value of the primary metric, or `None` if not present.
    pub fn primary_metric(&self, key: &str) -> Option<f64> {
        self.metrics.get(key).copied()
    }
}

// ---------------------------------------------------------------------------
// ExperimentRunner
// ---------------------------------------------------------------------------

/// Orchestrates the validate → execute → parse-metrics pipeline for one sandbox.
pub struct ExperimentRunner {
    sandbox: Box<dyn Sandbox>,
    config: ExperimentConfig,
}

impl ExperimentRunner {
    /// Create a new runner backed by the given sandbox.
    pub fn new(sandbox: Box<dyn Sandbox>, config: ExperimentConfig) -> Self {
        Self { sandbox, config }
    }

    /// Validate, execute, and parse an experiment code string.
    pub async fn run_experiment(
        &self,
        code: &str,
        timeout: Option<Duration>,
    ) -> Result<ExperimentResult> {
        let mut warnings: Vec<String> = Vec::new();

        // --- 1. Pre-flight validation ---
        let issues = validate_code(code, &self.config.sandbox.allowed_imports)?;
        let mut has_error = false;
        for issue in &issues {
            if issue.severity == crate::validation::Severity::Error {
                has_error = true;
                warn!("Validation error: {issue}");
            } else {
                warnings.push(issue.to_string());
            }
        }

        if has_error {
            let msgs: Vec<String> = issues
                .iter()
                .filter(|i| i.severity == crate::validation::Severity::Error)
                .map(|i| i.to_string())
                .collect();
            return Ok(ExperimentResult {
                success: false,
                metrics: HashMap::new(),
                logs: String::new(),
                stderr: msgs.join("\n"),
                artifacts: Vec::new(),
                duration: Duration::ZERO,
                timed_out: false,
                warnings,
            });
        }

        // --- 2. Execute ---
        let timeout_dur =
            timeout.unwrap_or(Duration::from_secs(self.config.time_budget_sec as u64));

        info!("ExperimentRunner: executing (timeout {}s)", timeout_dur.as_secs());

        let exec_result: ExecutionResult = self.sandbox.execute(code, Some(timeout_dur)).await?;

        // --- 3. Decompose result before partial moves ---
        let success = exec_result.success();
        let timed_out = exec_result.timed_out;
        let duration = exec_result.duration;
        let logs = exec_result.stdout;
        let stderr = exec_result.stderr;

        let metrics: HashMap<String, f64> = exec_result
            .metrics
            .into_iter()
            .filter_map(|(k, v)| v.as_f64().map(|f| (k, f)))
            .collect();

        Ok(ExperimentResult {
            success,
            metrics,
            logs,
            stderr,
            artifacts: Vec::new(), // populated by callers if needed
            duration,
            timed_out,
            warnings,
        })
    }

    /// Return a reference to the underlying sandbox.
    pub fn sandbox(&self) -> &dyn Sandbox {
        self.sandbox.as_ref()
    }

    /// Return a reference to the experiment configuration.
    pub fn config(&self) -> &ExperimentConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// SandboxFactory
// ---------------------------------------------------------------------------

/// Create the appropriate sandbox backend from an `ExperimentConfig`.
///
/// Selection logic:
///   - `ExperimentMode::Docker`     → `DockerSandbox`
///   - `ExperimentMode::SshRemote`  → `SshSandbox`
///   - `ExperimentMode::ColabDrive` → `ColabSandbox`
///   - `ExperimentMode::Sandbox` / `Simulated` → `LocalSandbox`
pub async fn create_sandbox(
    config: &ExperimentConfig,
    workdir: PathBuf,
) -> Result<Box<dyn Sandbox>> {
    use mol_config::ExperimentMode;

    match &config.mode {
        ExperimentMode::Docker => {
            let mut sandbox = crate::docker::DockerSandbox::new(
                config.docker.clone(),
                workdir,
            )
            .await?;
            sandbox.setup().await?;
            Ok(Box::new(sandbox))
        }

        ExperimentMode::SshRemote => {
            let mut sandbox = crate::ssh::SshSandbox::new(
                config.ssh_remote.clone(),
                workdir,
            );
            sandbox.setup().await?;
            Ok(Box::new(sandbox))
        }

        ExperimentMode::ColabDrive => {
            let mut sandbox = crate::colab::ColabSandbox::new(
                config.colab_drive.clone(),
                workdir,
            );
            sandbox.setup().await?;
            Ok(Box::new(sandbox))
        }

        // Default: local subprocess sandbox.
        ExperimentMode::Sandbox | ExperimentMode::Simulated => {
            use crate::sandbox::{LocalSandbox, LocalSandboxConfig};
            let local_cfg = LocalSandboxConfig {
                workdir,
                python_path: config.sandbox.python_path.clone(),
                allowed_imports: config.sandbox.allowed_imports.clone(),
            };
            let mut sandbox = LocalSandbox::new(local_cfg);
            sandbox.setup().await?;
            Ok(Box::new(sandbox))
        }
    }
}

/// Build an `ExperimentRunner` from a full config, selecting the right sandbox backend.
pub async fn build_runner(
    config: ExperimentConfig,
    workdir: PathBuf,
) -> Result<ExperimentRunner> {
    let sandbox = create_sandbox(&config, workdir).await?;
    Ok(ExperimentRunner::new(sandbox, config))
}
