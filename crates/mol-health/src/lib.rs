//! Mol-HEP-Lab: System health checks and dependency validation
//!
//! Provides `run_doctor` to validate that all required external tools,
//! APIs, and environment resources are available before running a pipeline.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sysinfo::System;
use tokio::process::Command;
use tracing::debug;

// ---------------------------------------------------------------------------
// Public re-exports from stub crates (stubs are empty; we define a local
// MolConfig that matches the expected interface).
// ---------------------------------------------------------------------------

/// Minimal configuration surface that health checks need.
///
/// Consumers that have a richer config type should populate this struct from
/// their own config before calling `run_doctor`.
#[derive(Debug, Clone, Default)]
pub struct MolConfig {
    /// Root directory of the knowledge-base.  Empty string means "not set".
    pub kb_dir: String,
    /// LLM endpoint base URL (e.g. `http://localhost:11434`).
    pub llm_base_url: String,
    /// API key for the LLM endpoint.  May be empty for local deployments.
    pub llm_api_key: String,
    /// Workspace / project root directory.
    pub workspace_dir: String,
}

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Outcome of a single health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// The check passed without issues.
    Pass,
    /// The check raised a concern but is non-fatal.
    Warn,
    /// The check failed; the feature it guards is unavailable.
    Fail,
    /// The check was intentionally skipped (e.g. not applicable).
    Skip,
}

impl CheckStatus {
    fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
            CheckStatus::Skip => "skip",
        }
    }
}

/// Result of a single named health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Machine-readable check identifier.
    pub name: String,
    /// Outcome of the check.
    pub status: CheckStatus,
    /// Human-readable summary of the outcome.
    pub message: String,
    /// Optional extended information (e.g. version string, path, error text).
    pub details: Option<String>,
}

impl CheckResult {
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            message: message.into(),
            details: None,
        }
    }

    fn pass_detail(
        name: impl Into<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            message: message.into(),
            details: Some(detail.into()),
        }
    }

    fn warn(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            message: message.into(),
            details: None,
        }
    }

    fn warn_detail(
        name: impl Into<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            message: message.into(),
            details: Some(detail.into()),
        }
    }

    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            message: message.into(),
            details: None,
        }
    }

    fn fail_detail(
        name: impl Into<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            message: message.into(),
            details: Some(detail.into()),
        }
    }

    fn skip(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Skip,
            message: message.into(),
            details: None,
        }
    }
}

/// Aggregated report produced by `run_doctor`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Individual check results in execution order.
    pub checks: Vec<CheckResult>,
    /// UTC timestamp when the report was generated (RFC 3339).
    pub timestamp: String,
    /// Worst-case aggregated status across all non-skipped checks.
    pub overall_status: CheckStatus,
}

// ---------------------------------------------------------------------------
// Individual checks
// ---------------------------------------------------------------------------

/// 1. Config file existence and validity.
fn check_config(config: Option<&MolConfig>) -> CheckResult {
    const NAME: &str = "config";

    let Some(cfg) = config else {
        return CheckResult::warn(NAME, "No config provided; skipping config validation");
    };

    let ws = &cfg.workspace_dir;
    if ws.is_empty() {
        return CheckResult::warn(NAME, "workspace_dir is not set in config");
    }

    let path = Path::new(ws);
    if !path.exists() {
        return CheckResult::fail_detail(
            NAME,
            "Config workspace_dir does not exist",
            format!("path: {ws}"),
        );
    }

    CheckResult::pass_detail(NAME, "Config is present and workspace_dir exists", ws)
}

/// 2. Python availability (checks `python3` then `python`).
fn check_python() -> CheckResult {
    const NAME: &str = "python";

    for candidate in ["python3", "python"] {
        if let Ok(path) = which::which(candidate) {
            return CheckResult::pass_detail(
                NAME,
                format!("{candidate} found on PATH"),
                path.display().to_string(),
            );
        }
    }

    CheckResult::fail_detail(
        NAME,
        "python3 / python not found on PATH",
        "Install Python 3.11+ and ensure it is on PATH",
    )
}

/// 3. LLM API connectivity.
async fn check_llm_connectivity(config: Option<&MolConfig>) -> CheckResult {
    const NAME: &str = "llm_connectivity";

    let Some(cfg) = config else {
        return CheckResult::skip(NAME, "No config — LLM connectivity check skipped");
    };

    if cfg.llm_base_url.is_empty() {
        return CheckResult::warn(NAME, "llm_base_url is not configured");
    }

    let url = format!("{}/models", cfg.llm_base_url.trim_end_matches('/'));
    debug!("LLM connectivity probe: {url}");

    let client = match reqwest_build_client() {
        Ok(c) => c,
        Err(e) => {
            return CheckResult::fail_detail(NAME, "Could not build HTTP client", e);
        }
    };

    match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status < 500 {
                CheckResult::pass_detail(
                    NAME,
                    format!("LLM endpoint reachable (HTTP {status})"),
                    &url,
                )
            } else {
                CheckResult::fail_detail(
                    NAME,
                    format!("LLM endpoint returned HTTP {status}"),
                    &url,
                )
            }
        }
        Err(e) => CheckResult::fail_detail(NAME, "LLM endpoint unreachable", e.to_string()),
    }
}

/// Build a minimal reqwest client without pulling in the full reqwest feature
/// set — we only need a one-shot GET.
fn reqwest_build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()
        .map_err(|e| e.to_string())
}

/// 4. Docker availability (`docker` binary + `docker info`).
async fn check_docker() -> CheckResult {
    const NAME: &str = "docker";

    let docker_bin = match which::which("docker") {
        Ok(p) => p,
        Err(_) => {
            return CheckResult::warn_detail(
                NAME,
                "docker not found on PATH",
                "Install Docker Desktop or Docker Engine",
            );
        }
    };

    match Command::new(&docker_bin)
        .arg("info")
        .arg("--format")
        .arg("{{.ServerVersion}}")
        .output()
        .await
    {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            CheckResult::pass_detail(
                NAME,
                "Docker daemon is running",
                format!("server version: {version}"),
            )
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            CheckResult::warn_detail(
                NAME,
                "docker binary found but daemon is not running",
                stderr,
            )
        }
        Err(e) => CheckResult::fail_detail(NAME, "Failed to run docker info", e.to_string()),
    }
}

/// 5. LaTeX availability (`pdflatex` and `bibtex`).
fn check_latex() -> CheckResult {
    const NAME: &str = "latex";

    let pdflatex = which::which("pdflatex");
    let bibtex = which::which("bibtex");

    match (pdflatex, bibtex) {
        (Ok(pdf), Ok(bib)) => CheckResult::pass_detail(
            NAME,
            "pdflatex and bibtex found",
            format!(
                "pdflatex: {}  bibtex: {}",
                pdf.display(),
                bib.display()
            ),
        ),
        (Ok(pdf), Err(_)) => CheckResult::warn_detail(
            NAME,
            "pdflatex found but bibtex missing",
            format!("pdflatex: {}  — install bibtex for full LaTeX support", pdf.display()),
        ),
        (Err(_), Ok(bib)) => CheckResult::warn_detail(
            NAME,
            "bibtex found but pdflatex missing",
            format!("bibtex: {}  — install pdflatex for full LaTeX support", bib.display()),
        ),
        (Err(_), Err(_)) => CheckResult::warn_detail(
            NAME,
            "LaTeX toolchain not found (pdflatex, bibtex)",
            "Install TeX Live or MiKTeX to enable PDF report generation",
        ),
    }
}

/// 6. OpenCode CLI availability.
fn check_opencode() -> CheckResult {
    const NAME: &str = "opencode";

    match which::which("opencode") {
        Ok(path) => CheckResult::pass_detail(NAME, "opencode CLI found", path.display().to_string()),
        Err(_) => CheckResult::warn_detail(
            NAME,
            "opencode CLI not found on PATH",
            "Install opencode to enable AI-assisted experiment generation",
        ),
    }
}

/// 7. Git availability.
fn check_git() -> CheckResult {
    const NAME: &str = "git";

    match which::which("git") {
        Ok(path) => CheckResult::pass_detail(NAME, "git found", path.display().to_string()),
        Err(_) => CheckResult::fail_detail(
            NAME,
            "git not found on PATH",
            "Install git — it is required for knowledge-base versioning",
        ),
    }
}

/// 8. KB directory structure.
fn check_kb_structure(config: Option<&MolConfig>) -> CheckResult {
    const NAME: &str = "kb_structure";

    let Some(cfg) = config else {
        return CheckResult::skip(NAME, "No config — KB structure check skipped");
    };

    if cfg.kb_dir.is_empty() {
        return CheckResult::warn(NAME, "kb_dir is not configured");
    }

    let base = Path::new(&cfg.kb_dir);
    if !base.exists() {
        return CheckResult::fail_detail(
            NAME,
            "KB root directory does not exist",
            cfg.kb_dir.clone(),
        );
    }

    let required: &[&str] = &["papers", "notes", "experiments"];
    let mut missing: Vec<&str> = Vec::new();

    for sub in required {
        if !base.join(sub).exists() {
            missing.push(sub);
        }
    }

    if missing.is_empty() {
        CheckResult::pass_detail(
            NAME,
            "KB directory structure is valid",
            cfg.kb_dir.clone(),
        )
    } else {
        CheckResult::warn_detail(
            NAME,
            format!("KB subdirectories missing: {}", missing.join(", ")),
            format!(
                "Run `mkdir -p {}` to create them",
                missing
                    .iter()
                    .map(|s| format!("{}/{s}", cfg.kb_dir))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        )
    }
}

/// 9. GPU / NPU availability (via sysinfo + optional npu-smi probe).
async fn check_gpu_npu() -> CheckResult {
    const NAME: &str = "gpu_npu";

    // --- CPU / memory info via sysinfo (always available) ---
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem_gb = sys.total_memory() / 1_073_741_824;

    // --- Try NVIDIA GPU via nvidia-smi ---
    if let Ok(nvidia_path) = which::which("nvidia-smi") {
        if let Ok(out) = Command::new(&nvidia_path)
            .arg("--query-gpu=name,memory.total")
            .arg("--format=csv,noheader,nounits")
            .output()
            .await
        {
            if out.status.success() {
                let info = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                if !info.is_empty() {
                    return CheckResult::pass_detail(
                        NAME,
                        "NVIDIA GPU(s) available",
                        format!("{info}  (host RAM: {total_mem_gb} GiB)"),
                    );
                }
            }
        }
    }

    // --- Try Ascend NPU via npu-smi ---
    if let Ok(npu_path) = which::which("npu-smi") {
        if let Ok(out) = Command::new(&npu_path)
            .arg("info")
            .output()
            .await
        {
            if out.status.success() {
                return CheckResult::pass_detail(
                    NAME,
                    "Ascend NPU available",
                    format!("npu-smi ok  (host RAM: {total_mem_gb} GiB)"),
                );
            }
        }
    }

    // --- No accelerator found ---
    CheckResult::warn_detail(
        NAME,
        "No GPU/NPU accelerator detected — CPU-only execution",
        format!("host RAM: {total_mem_gb} GiB"),
    )
}

/// 10. Disk space check (warn if workspace partition has < 10 GiB free).
fn check_disk_space(config: Option<&MolConfig>) -> CheckResult {
    const NAME: &str = "disk_space";
    const WARN_THRESHOLD_GIB: u64 = 10;
    const FAIL_THRESHOLD_GIB: u64 = 2;

    let path: PathBuf = config
        .map(|c| {
            if c.workspace_dir.is_empty() {
                PathBuf::from(".")
            } else {
                PathBuf::from(&c.workspace_dir)
            }
        })
        .unwrap_or_else(|| PathBuf::from("."));

    let disks = sysinfo::Disks::new_with_refreshed_list();

    // Find the disk whose mount point is the longest prefix of `path`.
    let abs_path = path.canonicalize().unwrap_or(path.clone());
    let best = disks
        .iter()
        .filter(|d| abs_path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len());

    let Some(disk) = best else {
        return CheckResult::warn(NAME, "Could not determine disk info for workspace path");
    };

    let free_gib = disk.available_space() / 1_073_741_824;
    let total_gib = disk.total_space() / 1_073_741_824;
    let detail = format!(
        "{} free / {} total on {}",
        format_gib(free_gib),
        format_gib(total_gib),
        disk.mount_point().display()
    );

    if free_gib < FAIL_THRESHOLD_GIB {
        CheckResult::fail_detail(
            NAME,
            format!("Critical: only {free_gib} GiB free on workspace disk"),
            detail,
        )
    } else if free_gib < WARN_THRESHOLD_GIB {
        CheckResult::warn_detail(
            NAME,
            format!("Low disk space: {free_gib} GiB free"),
            detail,
        )
    } else {
        CheckResult::pass_detail(NAME, format!("{free_gib} GiB free on workspace disk"), detail)
    }
}

fn format_gib(gib: u64) -> String {
    format!("{gib} GiB")
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

/// Run all Mol-HEP-Lab health checks and return a `DoctorReport`.
///
/// Pass `None` for `config` to run environment-only checks (binary detection,
/// disk space, GPU probing) without config-dependent validation.
pub async fn run_doctor(config: Option<&MolConfig>) -> DoctorReport {
    let timestamp: DateTime<Utc> = Utc::now();

    let mut checks = Vec::with_capacity(10);

    // 1. Config
    checks.push(check_config(config));

    // 2. Python
    checks.push(check_python());

    // 3. LLM connectivity
    checks.push(check_llm_connectivity(config).await);

    // 4. Docker
    checks.push(check_docker().await);

    // 5. LaTeX
    checks.push(check_latex());

    // 6. OpenCode CLI
    checks.push(check_opencode());

    // 7. Git
    checks.push(check_git());

    // 8. KB directory structure
    checks.push(check_kb_structure(config));

    // 9. GPU / NPU
    checks.push(check_gpu_npu().await);

    // 10. Disk space
    checks.push(check_disk_space(config));

    let overall_status = aggregate_status(&checks);

    DoctorReport {
        checks,
        timestamp: timestamp.to_rfc3339(),
        overall_status,
    }
}

/// Derive the worst-case status from a slice of results (ignoring Skip).
fn aggregate_status(checks: &[CheckResult]) -> CheckStatus {
    let mut has_warn = false;
    for c in checks {
        match c.status {
            CheckStatus::Fail => return CheckStatus::Fail,
            CheckStatus::Warn => has_warn = true,
            CheckStatus::Pass | CheckStatus::Skip => {}
        }
    }
    if has_warn {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Print a human-readable doctor report to stdout with colored indicators.
///
/// Uses ANSI escape codes for terminal color.  Falls back gracefully if the
/// terminal does not support color (indicators remain readable as plain text).
pub fn print_report(report: &DoctorReport) {
    println!();
    println!(
        "{}Mol-HEP-Lab Doctor Report{}  ({})",
        bold(),
        reset(),
        report.timestamp
    );
    println!("{}", "─".repeat(60));

    for check in &report.checks {
        let (icon, color) = status_icon_color(&check.status);
        print!("  {color}{icon}{reset} {bold}{name}{reset}",
            color = color,
            icon = icon,
            reset = reset(),
            bold = bold(),
            name = check.name,
        );
        println!("  {}", check.message);
        if let Some(ref detail) = check.details {
            println!("       {dim}{detail}{reset}", dim = dim(), reset = reset());
        }
    }

    println!("{}", "─".repeat(60));

    let fail_count = report.checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let warn_count = report.checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let pass_count = report.checks.iter().filter(|c| c.status == CheckStatus::Pass).count();

    let (icon, color) = status_icon_color(&report.overall_status);
    println!(
        "  Overall: {color}{icon} {status}{reset}  \
         ({pass_count} passed, {warn_count} warnings, {fail_count} failed)",
        color = color,
        icon = icon,
        status = report.overall_status.as_str().to_uppercase(),
        reset = reset(),
    );
    println!();
}

fn status_icon_color(status: &CheckStatus) -> (&'static str, &'static str) {
    match status {
        CheckStatus::Pass => ("✔", "\x1b[32m"),   // green
        CheckStatus::Warn => ("⚠", "\x1b[33m"),   // yellow
        CheckStatus::Fail => ("✘", "\x1b[31m"),   // red
        CheckStatus::Skip => ("–", "\x1b[90m"),   // dark gray
    }
}

fn bold() -> &'static str {
    "\x1b[1m"
}

fn dim() -> &'static str {
    "\x1b[2m"
}

fn reset() -> &'static str {
    "\x1b[0m"
}

// ---------------------------------------------------------------------------
// JSON export
// ---------------------------------------------------------------------------

/// Serialize a `DoctorReport` to a pretty-printed JSON string.
pub fn report_to_json(report: &DoctorReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|e| {
        format!(r#"{{"error": "serialization failed: {e}"}}"#)
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_result_constructors() {
        let r = CheckResult::pass("foo", "all good");
        assert_eq!(r.status, CheckStatus::Pass);
        assert!(r.details.is_none());

        let r = CheckResult::warn_detail("bar", "watch out", "extra");
        assert_eq!(r.status, CheckStatus::Warn);
        assert_eq!(r.details.as_deref(), Some("extra"));

        let r = CheckResult::fail("baz", "broken");
        assert_eq!(r.status, CheckStatus::Fail);
    }

    #[test]
    fn aggregate_status_all_pass() {
        let checks = vec![
            CheckResult::pass("a", ""),
            CheckResult::pass("b", ""),
        ];
        assert_eq!(aggregate_status(&checks), CheckStatus::Pass);
    }

    #[test]
    fn aggregate_status_with_warn() {
        let checks = vec![
            CheckResult::pass("a", ""),
            CheckResult::warn("b", ""),
        ];
        assert_eq!(aggregate_status(&checks), CheckStatus::Warn);
    }

    #[test]
    fn aggregate_status_with_fail() {
        let checks = vec![
            CheckResult::pass("a", ""),
            CheckResult::warn("b", ""),
            CheckResult::fail("c", ""),
        ];
        assert_eq!(aggregate_status(&checks), CheckStatus::Fail);
    }

    #[test]
    fn aggregate_status_skip_ignored() {
        let checks = vec![
            CheckResult::pass("a", ""),
            CheckResult::skip("b", ""),
        ];
        assert_eq!(aggregate_status(&checks), CheckStatus::Pass);
    }

    #[test]
    fn report_to_json_roundtrip() {
        let report = DoctorReport {
            checks: vec![CheckResult::pass("test", "ok")],
            timestamp: "2026-04-06T00:00:00Z".to_string(),
            overall_status: CheckStatus::Pass,
        };
        let json = report_to_json(&report);
        assert!(json.contains("\"pass\""));
        assert!(json.contains("test"));
    }

    #[test]
    fn check_status_serialization() {
        let json = serde_json::to_string(&CheckStatus::Fail).unwrap();
        assert_eq!(json, r#""fail""#);
        let json = serde_json::to_string(&CheckStatus::Skip).unwrap();
        assert_eq!(json, r#""skip""#);
    }

    #[test]
    fn check_config_no_config() {
        let r = check_config(None);
        assert_eq!(r.status, CheckStatus::Warn);
    }

    #[test]
    fn check_kb_structure_no_config() {
        let r = check_kb_structure(None);
        assert_eq!(r.status, CheckStatus::Skip);
    }

    #[test]
    fn check_disk_space_no_config() {
        // Should complete without panicking and return a non-fail result on
        // a normal developer machine.
        let r = check_disk_space(None);
        // We only assert it ran; the actual status depends on the machine.
        assert!(!r.name.is_empty());
    }

    #[tokio::test]
    async fn run_doctor_no_config_completes() {
        let report = run_doctor(None).await;
        assert_eq!(report.checks.len(), 10);
        assert!(!report.timestamp.is_empty());
    }
}
