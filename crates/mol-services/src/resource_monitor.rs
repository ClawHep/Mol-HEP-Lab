//! Resource Monitor — streams CPU/RAM/GPU stats over WebSocket.
//!
//! Ports `resource_monitor.py` to Rust using sysinfo + nvml-wrapper.

use axum::{
    Router,
    extract::WebSocketUpgrade,
    extract::ws::{Message, WebSocket},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::System;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Statistics for one GPU device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStats {
    pub id: u32,
    pub name: String,
    /// Core utilisation 0–100 %.
    pub utilization: f32,
    /// Used VRAM in GiB.
    pub mem_used: f32,
    /// Total VRAM in GiB.
    pub mem_total: f32,
    /// Temperature in °C.
    pub temperature: f32,
}

/// Full resource snapshot sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceStats {
    pub cpu_percent: f32,
    pub mem_used: f32,
    pub mem_total: f32,
    pub gpus: Vec<GpuStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accelerator_label: Option<String>,
    pub timestamp: i64,
}

// ---------------------------------------------------------------------------
// GPU / NPU detection
// ---------------------------------------------------------------------------

/// Query GPUs via nvml-wrapper (NVIDIA only).
#[allow(unexpected_cfgs)]
fn get_gpu_stats_nvml() -> Vec<GpuStats> {
    #[cfg(feature = "nvml")]
    {
        use nvml_wrapper::Nvml;
        let Ok(nvml) = Nvml::init() else { return Vec::new() };
        let Ok(count) = nvml.device_count() else { return Vec::new() };
        let mut out = Vec::new();
        for i in 0..count {
            let Ok(dev) = nvml.device_by_index(i) else { continue };
            let name = dev.name().unwrap_or_else(|_| format!("GPU {i}"));
            let util = dev.utilization_rates().map(|u| u.gpu as f32).unwrap_or(0.0);
            let mem = dev.memory_info().ok();
            let mem_used = mem.as_ref().map(|m| m.used as f32 / (1024.0 * 1024.0 * 1024.0)).unwrap_or(0.0);
            let mem_total = mem.as_ref().map(|m| m.total as f32 / (1024.0 * 1024.0 * 1024.0)).unwrap_or(0.0);
            let temp = dev.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .map(|t| t as f32)
                .unwrap_or(0.0);
            out.push(GpuStats { id: i, name, utilization: util, mem_used, mem_total, temperature: temp });
        }
        return out;
    }
    #[allow(unreachable_code)]
    Vec::new()
}

/// Fall back to nvidia-smi subprocess when nvml feature is off or unavailable.
fn get_gpu_stats_smi() -> Vec<GpuStats> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let Ok(output) = output else { return Vec::new() };
    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 6 {
            continue;
        }
        let id: u32 = parts[0].parse().unwrap_or(0);
        let name = parts[1].to_string();
        let utilization: f32 = parts[2].parse().unwrap_or(0.0);
        let mem_used: f32 = parts[3].parse::<f32>().unwrap_or(0.0) / 1024.0; // MiB → GiB
        let mem_total: f32 = parts[4].parse::<f32>().unwrap_or(0.0) / 1024.0;
        let temperature: f32 = parts[5].parse().unwrap_or(0.0);
        gpus.push(GpuStats { id, name, utilization, mem_used, mem_total, temperature });
    }
    gpus
}

fn get_gpu_stats() -> Vec<GpuStats> {
    let nvml_stats = get_gpu_stats_nvml();
    if !nvml_stats.is_empty() {
        return nvml_stats;
    }
    get_gpu_stats_smi()
}

fn summarize_accelerator_names(gpus: &[GpuStats]) -> Option<String> {
    if gpus.is_empty() {
        return None;
    }
    use std::collections::HashMap;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for g in gpus {
        *counts.entry(g.name.as_str()).or_default() += 1;
    }
    let label: Vec<String> = counts
        .iter()
        .map(|(name, count)| {
            if *count > 1 {
                format!("{count}x {name}")
            } else {
                name.to_string()
            }
        })
        .collect();
    Some(label.join(" + "))
}

// ---------------------------------------------------------------------------
// Resource snapshot
// ---------------------------------------------------------------------------

pub fn get_resource_stats() -> ResourceStats {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_percent = sys.global_cpu_usage();
    let mem_used = (sys.used_memory() as f32) / (1024.0 * 1024.0 * 1024.0); // bytes → GiB
    let mem_total = (sys.total_memory() as f32) / (1024.0 * 1024.0 * 1024.0);

    let gpus = get_gpu_stats();
    let accelerator_label = summarize_accelerator_names(&gpus);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    ResourceStats {
        cpu_percent,
        mem_used,
        mem_total,
        gpus,
        accelerator_label,
        timestamp,
    }
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// Interval between resource stat broadcasts (seconds).
const BROADCAST_INTERVAL_SECS: u64 = 2;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    info!("Resource monitor client connected");

    // Drain inbound messages (client sends nothing but we need to drive the receiver
    // so that disconnect is detected)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {}
    });

    let mut send_task = tokio::spawn(async move {
        // Prime the first CPU sample (first call returns 0)
        let _ = get_resource_stats();
        let mut interval = tokio::time::interval(Duration::from_secs(BROADCAST_INTERVAL_SECS));
        loop {
            interval.tick().await;
            let stats = get_resource_stats();
            let msg = serde_json::json!({
                "type": "resource_stats",
                "payload": stats,
            });
            match serde_json::to_string(&msg) {
                Ok(s) => {
                    if sender.send(Message::Text(s.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Failed to serialize resource stats: {e}");
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
    info!("Resource monitor client disconnected");
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router() -> Router {
    Router::new().route("/res", get(ws_handler))
}
