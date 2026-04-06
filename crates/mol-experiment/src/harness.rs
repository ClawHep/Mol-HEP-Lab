//! Immutable experiment infrastructure for time and metric management.
//!
//! [`ExperimentHarness`] is injected into sandbox environments to provide
//! consistent timing, metric reporting, and result serialization.

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

/// Global singleton harness instance.
static DEFAULT_HARNESS: OnceLock<std::sync::Mutex<ExperimentHarness>> = OnceLock::new();

/// Experiment harness for time budget management, metric collection, and
/// result serialization.  Ported from the Python `ExperimentHarness` class.
pub struct ExperimentHarness {
    start: Instant,
    time_budget: u64,
    metrics: HashMap<String, f64>,
    partial_results: Vec<Value>,
    step_count: u64,
    nan_count: u64,
    output_dir: PathBuf,
}

impl ExperimentHarness {
    /// Create a new harness with the given time budget (in seconds).
    /// Results will be written to the current directory.
    pub fn new(time_budget: u64) -> Self {
        Self::with_output_dir(time_budget, Path::new("."))
    }

    /// Create a new harness that writes `results.json` into `output_dir`.
    pub fn with_output_dir(time_budget: u64, output_dir: &Path) -> Self {
        Self {
            start: Instant::now(),
            time_budget: time_budget.max(1),
            metrics: HashMap::new(),
            partial_results: Vec::new(),
            step_count: 0,
            nan_count: 0,
            output_dir: output_dir.to_path_buf(),
        }
    }

    // -- Accessors -----------------------------------------------------------

    /// Seconds elapsed since the harness was created.
    pub fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Fraction of the time budget consumed, clamped to `[0.0, 1.0]`.
    pub fn progress(&self) -> f64 {
        (self.elapsed() / self.time_budget as f64).min(1.0)
    }

    /// Number of non-finite values encountered so far.
    pub fn nan_count(&self) -> u64 {
        self.nan_count
    }

    /// Number of steps completed so far.
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    // -- Control flow --------------------------------------------------------

    /// Returns `true` when 80 % of the time budget has been consumed.
    pub fn should_stop(&self) -> bool {
        self.elapsed() >= self.time_budget as f64 * 0.8
    }

    /// Validate that `value` is finite.  Returns `false` (and increments the
    /// NaN counter) when the value is NaN or infinite.
    ///
    /// When the NaN counter reaches 5, [`finalize`](Self::finalize) is called
    /// and a message is printed to stderr.  In production the Python version
    /// calls `sys.exit(1)` at that point; here we return `false` and let the
    /// caller decide.
    pub fn check_value(&mut self, value: f64, name: &str) -> bool {
        if value.is_nan() || value.is_infinite() {
            self.nan_count += 1;
            eprintln!("WARNING: {name} = {value} (non-finite, skipped)");
            if self.nan_count >= 5 {
                eprintln!("FAIL: Too many NaN/Inf values detected. Stopping early.");
                self.finalize();
            }
            return false;
        }
        true
    }

    // -- Reporting -----------------------------------------------------------

    /// Record a named metric.  Non-finite values are rejected via
    /// [`check_value`](Self::check_value).
    pub fn report_metric(&mut self, name: &str, value: f64) {
        if !self.check_value(value, name) {
            return;
        }
        self.metrics.insert(name.to_string(), value);
        println!("{name}: {value}");
    }

    /// Append an arbitrary JSON value to the partial results list.
    pub fn log_result(&mut self, result: Value) {
        self.partial_results.push(result);
    }

    /// Increment the step counter.
    pub fn step(&mut self) {
        self.step_count += 1;
    }

    // -- Serialization -------------------------------------------------------

    /// Write `results.json` to the configured output directory.
    pub fn finalize(&self) {
        let elapsed = (self.elapsed() * 100.0).round() / 100.0; // round to 2 decimals
        let mut output = serde_json::json!({
            "metrics": self.metrics,
            "elapsed_sec": elapsed,
            "time_budget_sec": self.time_budget,
            "steps_completed": self.step_count,
            "nan_count": self.nan_count,
        });
        if !self.partial_results.is_empty() {
            output["results"] = Value::Array(self.partial_results.clone());
        }

        let path = self.output_dir.join("results.json");
        if let Ok(json) = serde_json::to_string_pretty(&output) {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("WARNING: failed to write {}: {e}", path.display());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Singleton accessor (mirrors Python `get_harness`)
// ---------------------------------------------------------------------------

/// Return (or lazily create) the global default harness with the given time
/// budget.  Subsequent calls ignore `time_budget` and return the existing
/// instance.
pub fn get_harness(time_budget: u64) -> &'static std::sync::Mutex<ExperimentHarness> {
    DEFAULT_HARNESS.get_or_init(|| std::sync::Mutex::new(ExperimentHarness::new(time_budget)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn harness_initial_progress_near_zero() {
        let h = ExperimentHarness::new(120);
        assert!(h.progress() < 0.01);
        assert!(!h.should_stop());
    }

    #[test]
    fn harness_rejects_nan() {
        let mut h = ExperimentHarness::new(120);
        assert!(!h.check_value(f64::NAN, "x"));
        assert_eq!(h.nan_count(), 1);
    }

    #[test]
    fn harness_rejects_infinity() {
        let mut h = ExperimentHarness::new(120);
        assert!(!h.check_value(f64::INFINITY, "x"));
        assert_eq!(h.nan_count(), 1);
    }

    #[test]
    fn harness_accepts_finite() {
        let mut h = ExperimentHarness::new(120);
        assert!(h.check_value(42.0, "x"));
        assert_eq!(h.nan_count(), 0);
    }

    #[test]
    fn harness_step_increments() {
        let mut h = ExperimentHarness::new(120);
        h.step();
        h.step();
        assert_eq!(h.step_count(), 2);
    }

    #[test]
    fn harness_finalize_writes_json() {
        let tmp = tempdir().unwrap();
        let mut h = ExperimentHarness::with_output_dir(120, tmp.path());
        h.report_metric("accuracy", 0.95);
        h.step();
        h.finalize();
        let content = std::fs::read_to_string(tmp.path().join("results.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["metrics"]["accuracy"], 0.95);
        assert_eq!(v["steps_completed"], 1);
    }
}
