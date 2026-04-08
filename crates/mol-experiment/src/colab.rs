//! Google Colab / Drive sandbox.
//!
//! Execution model:
//!   1. Write experiment code to a JSON "job" file on a mounted Google Drive.
//!   2. A Colab notebook (worker) watches that Drive folder and picks up jobs.
//!   3. Poll the Drive folder for a result file until it appears or times out.
//!   4. Read and parse the result.
//!
//! The user must have the Google Drive folder mounted locally (e.g. via
//! `google-drive-ocamlfuse` or `rclone`) at `drive_root`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use mol_config::{ColabDriveConfig, ExperimentConfig};

use crate::metrics::{parse_metrics_from_stdout, MetricValue};
use crate::sandbox::{ExecutionResult, Sandbox};

// ---------------------------------------------------------------------------
// Wire format: job / result JSON
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct ColabJob {
    job_id: String,
    code: String,
    timeout_sec: u64,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct ColabResult {
    #[allow(dead_code)]
    job_id: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
    #[allow(dead_code)]
    elapsed_sec: f64,
    #[serde(default)]
    metrics: serde_json::Value,
}

// ---------------------------------------------------------------------------
// ColabSandbox
// ---------------------------------------------------------------------------

/// Async sandbox that delegates execution to a Google Colab notebook via Drive.
pub struct ColabSandbox {
    #[allow(dead_code)]
    config: ColabDriveConfig,
    workdir: PathBuf,
    drive_path: PathBuf,
    poll_interval: Duration,
    timeout: Duration,
    #[allow(dead_code)]
    run_counter: std::sync::atomic::AtomicU64,
}

impl ColabSandbox {
    /// Create a new `ColabSandbox`.
    pub fn new(config: ColabDriveConfig, workdir: PathBuf) -> Self {
        let poll_interval = Duration::from_secs(config.poll_interval_sec as u64);
        let timeout = Duration::from_secs(config.timeout_sec as u64);
        let drive_path = PathBuf::from(shellexpand::tilde(&config.drive_root).to_string());
        Self {
            config,
            workdir,
            drive_path,
            poll_interval,
            timeout,
            run_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    // ------------------------------------------------------------------
    // Drive path helpers
    // ------------------------------------------------------------------

    fn jobs_dir(&self) -> PathBuf {
        self.drive_path.join("jobs")
    }

    fn results_dir(&self) -> PathBuf {
        self.drive_path.join("results")
    }

    fn job_path(&self, job_id: &str) -> PathBuf {
        self.jobs_dir().join(format!("{job_id}.json"))
    }

    fn result_path(&self, job_id: &str) -> PathBuf {
        self.results_dir().join(format!("{job_id}.json"))
    }

    // ------------------------------------------------------------------
    // Write + poll
    // ------------------------------------------------------------------

    /// Write the job JSON to the Drive jobs directory.
    async fn write_job(&self, job_id: &str, code: &str, timeout_sec: u64) -> Result<()> {
        tokio::fs::create_dir_all(self.jobs_dir()).await?;
        let job = ColabJob {
            job_id: job_id.to_string(),
            code: code.to_string(),
            timeout_sec,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string_pretty(&job)?;
        tokio::fs::write(self.job_path(job_id), json).await?;
        debug!("ColabSandbox: wrote job {job_id} to Drive");
        Ok(())
    }

    /// Poll for the result file, returning its path when it appears.
    async fn poll_for_result(
        &self,
        job_id: &str,
        deadline: Instant,
    ) -> Result<PathBuf> {
        let result_path = self.result_path(job_id);
        loop {
            if result_path.exists() {
                return Ok(result_path);
            }

            if Instant::now() >= deadline {
                bail!(
                    "Colab job {job_id} timed out after {}s",
                    self.timeout.as_secs()
                );
            }

            tokio::time::sleep(self.poll_interval).await;
            debug!("ColabSandbox: polling for result {job_id}...");
        }
    }

    /// Parse the result JSON file into an `ExecutionResult`.
    async fn read_result(
        &self,
        result_path: &Path,
        start: Instant,
    ) -> Result<ExecutionResult> {
        let text = tokio::fs::read_to_string(result_path).await?;
        let colab_result: ColabResult = serde_json::from_str(&text)
            .context("parsing Colab result JSON")?;

        // Parse metrics: try JSON field first, then stdout.
        let mut metrics: HashMap<String, MetricValue> =
            parse_metrics_from_stdout(&colab_result.stdout);

        // Supplement with any metrics in the result JSON itself.
        if let serde_json::Value::Object(map) = &colab_result.metrics {
            for (k, v) in map {
                if let Some(f) = v.as_f64() {
                    if f.is_finite() {
                        metrics
                            .entry(k.clone())
                            .or_insert(MetricValue::Float(f));
                    }
                }
            }
        }

        // Remove the job and result files after reading.
        tokio::fs::remove_file(result_path).await.ok();

        Ok(ExecutionResult {
            stdout: colab_result.stdout,
            stderr: colab_result.stderr,
            exit_code: colab_result.exit_code,
            duration: start.elapsed(),
            metrics,
            timed_out: false,
        })
    }
}

#[async_trait]
impl Sandbox for ColabSandbox {
    async fn setup(&mut self) -> Result<()> {
        tokio::fs::create_dir_all(&self.workdir).await?;
        tokio::fs::create_dir_all(self.jobs_dir()).await?;
        tokio::fs::create_dir_all(self.results_dir()).await?;
        info!(
            "ColabSandbox: Drive root verified at {}",
            self.drive_path.display()
        );
        Ok(())
    }

    async fn execute(&self, code: &str, timeout: Option<Duration>) -> Result<ExecutionResult> {
        let timeout_dur = timeout.unwrap_or(self.timeout);
        let timeout_secs = timeout_dur.as_secs();

        let job_id = format!(
            "mol-{}",
            &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]
        );

        self.write_job(&job_id, code, timeout_secs).await?;

        let deadline = Instant::now() + timeout_dur;
        let start = Instant::now();

        match self.poll_for_result(&job_id, deadline).await {
            Ok(result_path) => self.read_result(&result_path, start).await,
            Err(e) => {
                // Remove the job file since the worker won't get a result back in time.
                tokio::fs::remove_file(self.job_path(&job_id)).await.ok();
                warn!("ColabSandbox: {e}");
                Ok(ExecutionResult {
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: -1,
                    duration: start.elapsed(),
                    metrics: HashMap::new(),
                    timed_out: true,
                })
            }
        }
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Nothing persistent to clean up; Drive files are removed after each run.
        Ok(())
    }

    async fn is_ready(&self) -> bool {
        self.drive_path.exists() && self.drive_path.is_dir()
    }
}

// ---------------------------------------------------------------------------
// Factory helper
// ---------------------------------------------------------------------------

/// Build a `ColabSandbox` from a full `ExperimentConfig`.
pub fn colab_sandbox_from_config(config: &ExperimentConfig, workdir: PathBuf) -> ColabSandbox {
    ColabSandbox::new(config.colab_drive.clone(), workdir)
}

// ---------------------------------------------------------------------------
// Worker notebook template writer
// ---------------------------------------------------------------------------

/// Write a minimal Python worker script that the Colab notebook should run.
///
/// The worker polls `jobs/` for new JSON files, executes the code, and writes
/// the result to `results/`.
pub fn write_worker_script(path: &Path) -> Result<()> {
    let content = r#"#!/usr/bin/env python3
"""
Mol-HEP-Lab Colab Drive Worker
================================
Run this cell in a Colab notebook that has Google Drive mounted.

This script watches the 'jobs/' folder for new experiment jobs, executes
each one, and writes results to 'results/'.

Usage in Colab:
    from google.colab import drive
    drive.mount('/content/drive')

    import subprocess, sys
    subprocess.run([sys.executable, '/path/to/colab_worker.py'])
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

DRIVE_ROOT = Path(os.environ.get("MOL_DRIVE_ROOT", "/content/drive/MyDrive/mol-experiment"))
JOBS_DIR = DRIVE_ROOT / "jobs"
RESULTS_DIR = DRIVE_ROOT / "results"
POLL_INTERVAL = 5  # seconds

JOBS_DIR.mkdir(parents=True, exist_ok=True)
RESULTS_DIR.mkdir(parents=True, exist_ok=True)

print(f"Mol-HEP-Lab worker started. Watching {JOBS_DIR}")

while True:
    for job_file in sorted(JOBS_DIR.glob("*.json")):
        try:
            job = json.loads(job_file.read_text())
            job_id = job["job_id"]
            code = job["code"]
            timeout = int(job.get("timeout_sec", 300))

            # Write code to a temp file
            script = DRIVE_ROOT / f"_worker_{job_id}.py"
            script.write_text(code)

            start = time.monotonic()
            try:
                cp = subprocess.run(
                    [sys.executable, "-u", str(script)],
                    capture_output=True,
                    text=True,
                    timeout=timeout,
                )
                result = {
                    "job_id": job_id,
                    "stdout": cp.stdout,
                    "stderr": cp.stderr,
                    "exit_code": cp.returncode,
                    "elapsed_sec": time.monotonic() - start,
                    "metrics": {},
                }
            except subprocess.TimeoutExpired as exc:
                result = {
                    "job_id": job_id,
                    "stdout": (exc.stdout or b"").decode("utf-8", errors="replace"),
                    "stderr": f"Timed out after {timeout}s",
                    "exit_code": -1,
                    "elapsed_sec": time.monotonic() - start,
                    "metrics": {},
                }
            finally:
                script.unlink(missing_ok=True)

            result_file = RESULTS_DIR / f"{job_id}.json"
            result_file.write_text(json.dumps(result, indent=2))
            job_file.unlink(missing_ok=True)
            print(f"Completed job {job_id}: exit_code={result['exit_code']}")

        except Exception as e:
            print(f"Error processing {job_file}: {e}")

    time.sleep(POLL_INTERVAL)
"#;
    std::fs::write(path, content).context("write colab worker script")?;
    Ok(())
}
