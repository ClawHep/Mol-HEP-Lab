//! Session state for a single agentic stage execution.
//!
//! Ported from `claw_engine/session.py`. Provides timestamped logging,
//! artifact tracking, and auto-persist to JSON for live debugging.

use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

/// Mutable state accumulated across an agentic stage execution.
///
/// Provides timestamped logging with auto-persist to a JSON snapshot
/// and a streaming text log for live `tail -f` debugging.
#[derive(Debug)]
pub struct StageSession {
    /// Directory where session artifacts are written.
    pub stage_dir: PathBuf,
    /// Short name for this stage (used in log file names).
    pub stage_name: String,
    /// Ordered log entries with elapsed timestamps.
    pub phase_log: Vec<String>,
    /// Named artifacts produced during this session.
    pub artifacts: Vec<String>,
    /// Total LLM API calls made.
    pub llm_calls: u64,
    /// Total sandbox/bash runs.
    pub sandbox_runs: u64,
    /// The currently active phase label.
    pub current_phase: String,
    /// Error messages accumulated.
    pub errors: Vec<String>,
    /// Free-form key-value metadata.
    pub metadata: serde_json::Map<String, Value>,
    /// Whether to auto-persist after each log call.
    pub auto_persist: bool,
    start: Instant,
}

impl StageSession {
    /// Create a new session rooted at `stage_dir`.
    pub fn new(stage_dir: impl Into<PathBuf>) -> Self {
        Self {
            stage_dir: stage_dir.into(),
            stage_name: String::new(),
            phase_log: Vec::new(),
            artifacts: Vec::new(),
            llm_calls: 0,
            sandbox_runs: 0,
            current_phase: String::new(),
            errors: Vec::new(),
            metadata: serde_json::Map::new(),
            auto_persist: true,
            start: Instant::now(),
        }
    }

    /// Set the stage name (used in file names and log output).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.stage_name = name.into();
        self
    }

    /// Elapsed time since session creation.
    pub fn elapsed_sec(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Log a message with the current elapsed time.
    pub fn log(&mut self, phase: &str, message: &str) {
        let elapsed = self.elapsed_sec();
        self.current_phase = phase.to_owned();
        let entry = format!("[{elapsed:7.1}s] [{phase}] {message}");
        self.phase_log.push(entry.clone());
        info!("[{}] {}", self.stage_name_or_default(), entry);
        self.append_live_log(&entry);
        if self.auto_persist {
            let _ = self.persist();
        }
    }

    /// Log an error with optional exception context.
    pub fn log_error(&mut self, phase: &str, message: &str, error: Option<&anyhow::Error>) {
        let error_msg = match error {
            Some(e) => format!("{message}: {e}"),
            None => message.to_owned(),
        };
        self.errors.push(error_msg.clone());
        self.log(phase, &format!("ERROR: {error_msg}"));
    }

    /// Record a named artifact.
    pub fn add_artifact(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.artifacts.contains(&name) {
            self.artifacts.push(name);
        }
    }

    /// Set a metadata key.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Persist the session snapshot to `{stage_dir}/{stage_name}_session.json`.
    ///
    /// Returns the path written.
    pub fn persist(&self) -> anyhow::Result<PathBuf> {
        let file_name = format!("{}_session.json", self.stage_name_or_default());
        let path = self.stage_dir.join(&file_name);

        let payload = SessionSnapshot {
            stage_name: self.stage_name.clone(),
            current_phase: self.current_phase.clone(),
            llm_calls: self.llm_calls,
            sandbox_runs: self.sandbox_runs,
            elapsed_sec: (self.elapsed_sec() * 10.0).round() / 10.0,
            artifacts: self.artifacts.clone(),
            errors: self.errors.clone(),
            metadata: self.metadata.clone(),
            phase_log: self.phase_log.clone(),
        };

        let json = serde_json::to_string_pretty(&payload)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn stage_name_or_default(&self) -> &str {
        if self.stage_name.is_empty() { "stage" } else { &self.stage_name }
    }

    fn append_live_log(&self, entry: &str) {
        let file_name = format!("{}_live.log", self.stage_name_or_default());
        let path = self.stage_dir.join(&file_name);
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{entry}");
        }
    }
}

/// JSON-serializable snapshot of [`StageSession`] state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub stage_name: String,
    pub current_phase: String,
    pub llm_calls: u64,
    pub sandbox_runs: u64,
    pub elapsed_sec: f64,
    pub artifacts: Vec<String>,
    pub errors: Vec<String>,
    pub metadata: serde_json::Map<String, Value>,
    pub phase_log: Vec<String>,
}
