//! Hardware detection for GPU-aware experiment execution.
//!
//! Detects NVIDIA CUDA GPUs (via `nvml-wrapper`), Apple Silicon MPS (via
//! `sysinfo` + `uname`), and falls back to CPU-only.

use serde::{Deserialize, Serialize};
use std::process::Command;
use sysinfo::System;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// GPUs with fewer MiB of VRAM than this threshold are classified as "limited".
const HIGH_VRAM_THRESHOLD_MB: u64 = 8192;

/// Words indicating a log/status line rather than a metric name.
const LOG_WORDS: &[&str] = &[
    "running",
    "loading",
    "saving",
    "processing",
    "starting",
    "finished",
    "completed",
    "initializing",
    "downloading",
    "training",
    "evaluating",
    "epoch",
    "step",
    "iteration",
    "experiment",
    "warning",
    "error",
    "info",
    "debug",
    "experiments",
    "using",
    "setting",
    "creating",
    "building",
    "computing",
    "reading",
    "writing",
    "opening",
    "closing",
];

/// Maximum word count for a plausible metric name.
const MAX_METRIC_NAME_WORDS: usize = 6;

// ---------------------------------------------------------------------------
// HardwareProfile
// ---------------------------------------------------------------------------

/// Detected hardware capabilities of the local machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProfile {
    /// Whether any GPU/accelerator is available.
    pub has_gpu: bool,
    /// Accelerator type: `"cuda"` | `"npu"` | `"mps"` | `"cpu"`.
    pub gpu_type: String,
    /// Human-readable GPU name, e.g. `"NVIDIA RTX 4090"` / `"Apple M3 Pro"` /
    /// `"CPU only"`.
    pub gpu_name: String,
    /// VRAM in MiB. `None` for MPS (unified memory) and CPU-only.
    pub vram_mb: Option<u64>,
    /// Capability tier: `"high"` | `"limited"` | `"cpu_only"`.
    pub tier: String,
    /// User-facing warning message; empty when `tier == "high"`.
    pub warning: String,
}

impl HardwareProfile {
    /// Returns `true` if this profile represents high-capability hardware.
    pub fn is_high_tier(&self) -> bool {
        self.tier == "high"
    }

    /// Returns `true` if deep-learning experiments are supported at all.
    pub fn supports_deep_learning(&self) -> bool {
        self.has_gpu
    }
}

// ---------------------------------------------------------------------------
// Detection entry point
// ---------------------------------------------------------------------------

/// Detect local hardware and return a [`HardwareProfile`].
///
/// Detection order:
/// 1. NVIDIA GPU via `nvml-wrapper`
/// 2. NVIDIA GPU via `nvidia-smi` subprocess (fallback)
/// 3. Huawei Ascend NPU via `npu-smi` subprocess
/// 4. macOS Apple Silicon (MPS) via platform detection
/// 5. CPU-only fallback
pub fn detect_hardware() -> HardwareProfile {
    if let Some(p) = detect_nvidia_nvml() {
        return p;
    }
    if let Some(p) = detect_nvidia_smi() {
        return p;
    }
    if let Some(p) = detect_ascend() {
        return p;
    }
    if let Some(p) = detect_mps() {
        return p;
    }
    HardwareProfile {
        has_gpu: false,
        gpu_type: "cpu".to_owned(),
        gpu_name: "CPU only".to_owned(),
        vram_mb: None,
        tier: "cpu_only".to_owned(),
        warning: concat!(
            "No GPU detected. Only CPU-based experiments (NumPy, sklearn) are supported. ",
            "For deep learning research ideas, please use a machine with a GPU or a remote GPU server."
        )
        .to_owned(),
    }
}

// ---------------------------------------------------------------------------
// NVIDIA via nvml-wrapper
// ---------------------------------------------------------------------------

fn detect_nvidia_nvml() -> Option<HardwareProfile> {
    use nvml_wrapper::Nvml;
    let nvml = Nvml::init().ok()?;
    let device = nvml.device_by_index(0).ok()?;
    let name = device.name().ok()?;
    let mem = device.memory_info().ok()?;
    let vram_mb = mem.total / (1024 * 1024);
    let (tier, warning) = tier_for_vram(&name, vram_mb);
    Some(HardwareProfile {
        has_gpu: true,
        gpu_type: "cuda".to_owned(),
        gpu_name: name,
        vram_mb: Some(vram_mb),
        tier,
        warning,
    })
}

// ---------------------------------------------------------------------------
// NVIDIA via nvidia-smi subprocess
// ---------------------------------------------------------------------------

fn detect_nvidia_smi() -> Option<HardwareProfile> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();
    let mut parts = line.splitn(2, ',');
    let gpu_name = parts.next()?.trim().to_owned();
    let vram_mb: u64 = parts
        .next()?
        .trim()
        .parse::<f64>()
        .ok()
        .map(|v| v as u64)
        .unwrap_or(0);

    let (tier, warning) = tier_for_vram(&gpu_name, vram_mb);

    Some(HardwareProfile {
        has_gpu: true,
        gpu_type: "cuda".to_owned(),
        gpu_name,
        vram_mb: Some(vram_mb),
        tier,
        warning,
    })
}

fn tier_for_vram(gpu_name: &str, vram_mb: u64) -> (String, String) {
    if vram_mb >= HIGH_VRAM_THRESHOLD_MB {
        ("high".to_owned(), String::new())
    } else {
        (
            "limited".to_owned(),
            format!(
                "Local GPU ({gpu_name}, {vram_mb} MB VRAM) has limited memory. \
                 Complex deep learning experiments may be slow or run out of memory. \
                 Consider using a remote GPU server for best results."
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Huawei Ascend NPU
// ---------------------------------------------------------------------------

fn detect_ascend() -> Option<HardwareProfile> {
    let output = Command::new("npu-smi")
        .arg("info")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);

    // Try to extract NPU chip name (e.g. "910B2" from "| 0  910B2 |")
    let gpu_name = {
        let re = regex::Regex::new(r"\|\s*\d+\s+(\w+)").ok()?;
        re.captures(&text)
            .map(|c| format!("Ascend {}", &c[1]))
            .unwrap_or_else(|| "Ascend NPU".to_owned())
    };

    // Try to extract HBM total (MiB)
    let vram_mb: u64 = {
        let re = regex::Regex::new(r"(\d+)\s*/\s*(\d+)\s*\|?\s*$").ok()?;
        re.captures_iter(&text)
            .filter_map(|c| c[2].parse::<u64>().ok().filter(|&v| v > 0))
            .next()
            .unwrap_or(0)
    };

    let (tier, warning) = if vram_mb >= HIGH_VRAM_THRESHOLD_MB {
        ("high".to_owned(), String::new())
    } else if vram_mb > 0 {
        (
            "limited".to_owned(),
            format!("Ascend NPU ({gpu_name}, {vram_mb} MB HBM) has limited memory."),
        )
    } else {
        (
            "high".to_owned(),
            format!("Ascend NPU ({gpu_name}) detected but could not determine HBM size."),
        )
    };

    Some(HardwareProfile {
        has_gpu: true,
        gpu_type: "npu".to_owned(),
        gpu_name,
        vram_mb: if vram_mb > 0 { Some(vram_mb) } else { None },
        tier,
        warning,
    })
}

// ---------------------------------------------------------------------------
// Apple Silicon MPS
// ---------------------------------------------------------------------------

fn detect_mps() -> Option<HardwareProfile> {
    // Only relevant on macOS arm64
    if std::env::consts::OS != "macos" {
        return None;
    }
    if std::env::consts::ARCH != "aarch64" {
        return None;
    }

    // Get chip name via sysctl
    let gpu_name = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Apple Silicon GPU".to_owned());

    Some(HardwareProfile {
        has_gpu: true,
        gpu_type: "mps".to_owned(),
        gpu_name: gpu_name.clone(),
        vram_mb: None, // MPS uses unified system memory
        tier: "limited".to_owned(),
        warning: format!(
            "macOS GPU detected ({gpu_name}). PyTorch MPS backend is available \
             but has limited performance compared to NVIDIA CUDA GPUs. \
             For large-scale experiments, consider using a remote GPU server."
        ),
    })
}

// ---------------------------------------------------------------------------
// System memory snapshot
// ---------------------------------------------------------------------------

/// Collect current CPU and RAM statistics using `sysinfo`.
pub fn collect_system_stats() -> SystemStats {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_percent = sys.global_cpu_usage();
    let mem_used = sys.used_memory() / (1024 * 1024);
    let mem_total = sys.total_memory() / (1024 * 1024);

    SystemStats {
        cpu_percent,
        mem_used_mb: mem_used,
        mem_total_mb: mem_total,
    }
}

/// Lightweight CPU + RAM snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// Overall CPU utilization 0–100 %.
    pub cpu_percent: f32,
    /// Used RAM in MiB.
    pub mem_used_mb: u64,
    /// Total RAM in MiB.
    pub mem_total_mb: u64,
}

// ---------------------------------------------------------------------------
// Metric name heuristic
// ---------------------------------------------------------------------------

/// Return `true` if `name` looks like a metric key rather than a log line.
///
/// Used when parsing `name: value` metric output from experiment stdout.
pub fn is_metric_name(name: &str) -> bool {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() > MAX_METRIC_NAME_WORDS {
        return false;
    }
    let lower = name.to_lowercase();
    !LOG_WORDS
        .iter()
        .any(|&w| lower.split_whitespace().any(|word| word == w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_name_short_passes() {
        assert!(is_metric_name("train_loss"));
        assert!(is_metric_name("val acc"));
        assert!(is_metric_name("AUC"));
    }

    #[test]
    fn metric_name_log_word_fails() {
        assert!(!is_metric_name("training completed"));
        assert!(!is_metric_name("epoch 5 running"));
        assert!(!is_metric_name("error in loading model weights"));
    }

    #[test]
    fn metric_name_too_many_words_fails() {
        assert!(!is_metric_name("this is way too many words for a metric name here"));
    }

    #[test]
    fn detect_hardware_returns_profile() {
        // Just ensure it doesn't panic; the actual profile depends on the CI machine.
        let profile = detect_hardware();
        assert!(!profile.gpu_type.is_empty());
        assert!(!profile.tier.is_empty());
    }
}
