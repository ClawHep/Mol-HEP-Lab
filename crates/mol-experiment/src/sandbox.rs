//! Base sandbox trait and local (subprocess) sandbox implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::metrics::MetricValue;

// ---------------------------------------------------------------------------
// ExecutionResult
// ---------------------------------------------------------------------------

/// The outcome of running code inside a sandbox.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
    /// Parsed metrics extracted from stdout / results files.
    pub metrics: HashMap<String, MetricValue>,
    /// Whether the execution hit the timeout deadline.
    pub timed_out: bool,
}

impl ExecutionResult {
    /// Returns `true` when the process exited cleanly (exit code 0, no timeout).
    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.timed_out
    }
}

// ---------------------------------------------------------------------------
// Sandbox trait
// ---------------------------------------------------------------------------

/// A sandboxed execution environment that can run Python experiment code.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Prepare the sandbox (create workdir, pull image, establish connection).
    async fn setup(&mut self) -> Result<()>;

    /// Execute the given Python `code` string with an optional timeout.
    async fn execute(&self, code: &str, timeout: Option<Duration>) -> Result<ExecutionResult>;

    /// Release all resources held by this sandbox (containers, SSH sessions).
    async fn cleanup(&mut self) -> Result<()>;

    /// Returns `true` when the sandbox is ready to accept `execute` calls.
    async fn is_ready(&self) -> bool;
}

// ---------------------------------------------------------------------------
// LocalSandbox
// ---------------------------------------------------------------------------

/// Configuration for the local subprocess sandbox.
#[derive(Debug, Clone)]
pub struct LocalSandboxConfig {
    /// Directory where temporary scripts are written and executed.
    pub workdir: PathBuf,
    /// Path to the Python interpreter (may be relative to a venv).
    pub python_path: String,
    /// Import names that are explicitly allowed (empty = allow all).
    pub allowed_imports: Vec<String>,
}

impl Default for LocalSandboxConfig {
    fn default() -> Self {
        Self {
            workdir: std::env::temp_dir().join("mol-experiment"),
            python_path: "python3".to_string(),
            allowed_imports: Vec::new(),
        }
    }
}

/// Runs experiment code as a subprocess on the local machine.
///
/// Each `execute` call writes a temporary `.py` file under `workdir`,
/// spawns it via the configured Python interpreter, and waits for completion
/// (subject to the supplied timeout).
pub struct LocalSandbox {
    pub workdir: PathBuf,
    pub python_path: String,
    pub allowed_imports: Vec<String>,
    run_counter: std::sync::atomic::AtomicU64,
}

impl LocalSandbox {
    /// Create a new `LocalSandbox` with the given configuration.
    pub fn new(config: LocalSandboxConfig) -> Self {
        Self {
            workdir: config.workdir,
            python_path: config.python_path,
            allowed_imports: config.allowed_imports,
            run_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn next_script_path(&self) -> PathBuf {
        let n = self
            .run_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.workdir.join(format!("_experiment_{n}.py"))
    }

    /// Resolve the Python path to an absolute path without following symlinks
    /// (following symlinks would lose venv context).
    fn resolved_python(&self) -> PathBuf {
        let p = Path::new(&self.python_path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(p)
        }
    }
}

#[async_trait]
impl Sandbox for LocalSandbox {
    async fn setup(&mut self) -> Result<()> {
        tokio::fs::create_dir_all(&self.workdir).await?;
        Ok(())
    }

    async fn execute(&self, code: &str, timeout: Option<Duration>) -> Result<ExecutionResult> {
        let script_path = self.next_script_path();
        tokio::fs::write(&script_path, code).await?;

        let python = self.resolved_python();
        debug!("LocalSandbox: running {:?} with {:?}", script_path, python);

        let start = Instant::now();

        // Build child process with unbuffered stdout/stderr.
        let mut child = Command::new(&python)
            .arg("-u")
            .arg(&script_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(&self.workdir)
            .env("PYTHONUNBUFFERED", "1")
            .spawn()?;

        let timeout_dur = timeout.unwrap_or(Duration::from_secs(300));

        // Race the child against the timeout.
        let result = tokio::time::timeout(timeout_dur, async {
            let stdout_handle = child.stdout.take();
            let stderr_handle = child.stderr.take();

            let (mut stdout_buf, mut stderr_buf) = (Vec::new(), Vec::new());

            if let Some(mut out) = stdout_handle {
                out.read_to_end(&mut stdout_buf).await.ok();
            }
            if let Some(mut err) = stderr_handle {
                err.read_to_end(&mut stderr_buf).await.ok();
            }

            let status = child.wait().await?;
            Ok::<(Vec<u8>, Vec<u8>, std::process::ExitStatus), anyhow::Error>((
                stdout_buf,
                stderr_buf,
                status,
            ))
        })
        .await;

        let duration = start.elapsed();

        let exec_result = match result {
            Ok(Ok((stdout_bytes, stderr_bytes, status))) => {
                let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
                let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
                let exit_code = status.code().unwrap_or(-1);

                // Parse metrics from stdout.
                let metrics = crate::metrics::parse_metrics_from_stdout(&stdout);

                // Cleanup on success.
                if exit_code == 0 {
                    tokio::fs::remove_file(&script_path).await.ok();
                }

                ExecutionResult {
                    stdout,
                    stderr,
                    exit_code,
                    duration,
                    metrics,
                    timed_out: false,
                }
            }
            Ok(Err(e)) => {
                warn!("LocalSandbox execution error: {e}");
                ExecutionResult {
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: -1,
                    duration,
                    metrics: HashMap::new(),
                    timed_out: false,
                }
            }
            Err(_elapsed) => {
                // Kill the process on timeout.
                child.kill().await.ok();
                warn!("LocalSandbox timed out after {}s", timeout_dur.as_secs());
                ExecutionResult {
                    stdout: String::new(),
                    stderr: format!(
                        "Execution timed out after {}s",
                        timeout_dur.as_secs()
                    ),
                    exit_code: -1,
                    duration,
                    metrics: HashMap::new(),
                    timed_out: true,
                }
            }
        };

        Ok(exec_result)
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Remove any leftover experiment scripts; ignore errors.
        if self.workdir.exists() {
            let mut entries = tokio::fs::read_dir(&self.workdir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("_experiment_"))
                    .unwrap_or(false)
                {
                    tokio::fs::remove_file(&path).await.ok();
                }
            }
        }
        Ok(())
    }

    async fn is_ready(&self) -> bool {
        // The local sandbox is ready whenever the workdir is accessible.
        Path::new(&self.python_path).exists()
            || which_python(&self.python_path)
    }
}

/// Returns `true` when `name` resolves to an executable on `PATH`.
fn which_python(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
