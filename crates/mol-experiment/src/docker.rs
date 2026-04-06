//! Docker-based sandbox using the bollard crate.
//!
//! Lifecycle: create container → start → exec → collect output → stop → remove.
//! Supports GPU passthrough (NVIDIA CUDA and Ascend NPU), network policies,
//! pip pre-install, and memory / SHM limits.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::models::{DeviceRequest, HostConfig, Mount, MountTypeEnum};
use bollard::Docker;
use futures_util::StreamExt;
use tracing::{debug, info, warn};

use mol_config::{DockerSandboxConfig, ExperimentConfig};

use crate::metrics::{parse_metrics_from_file, parse_metrics_from_stdout};
use crate::sandbox::{ExecutionResult, Sandbox};

// ---------------------------------------------------------------------------
// Network policy
// ---------------------------------------------------------------------------

/// Network isolation level for the Docker container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPolicy {
    /// No network at any point.
    None,
    /// Network available for setup phases only (pip + setup.py).
    SetupOnly,
    /// Network for pip install phase only (legacy alias for SetupOnly).
    PipOnly,
    /// Full network throughout all phases.
    Full,
}

impl NetworkPolicy {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => NetworkPolicy::None,
            "setup_only" | "setuponly" => NetworkPolicy::SetupOnly,
            "pip_only" | "piponly" => NetworkPolicy::PipOnly,
            "full" => NetworkPolicy::Full,
            _ => NetworkPolicy::None,
        }
    }
}

// ---------------------------------------------------------------------------
// DockerSandbox
// ---------------------------------------------------------------------------

/// Runs experiment code inside a Docker container via the bollard API.
pub struct DockerSandbox {
    config: DockerSandboxConfig,
    workdir: PathBuf,
    client: Docker,
    /// Active container ID (set after `setup`, cleared after `cleanup`).
    container_id: Option<String>,
    run_counter: std::sync::atomic::AtomicU64,
}

impl DockerSandbox {
    /// Connect to the local Docker daemon and create the sandbox.
    pub async fn new(config: DockerSandboxConfig, workdir: PathBuf) -> Result<Self> {
        let client = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker daemon")?;
        tokio::fs::create_dir_all(&workdir).await?;
        Ok(Self {
            config,
            workdir,
            client,
            container_id: None,
            run_counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Build the `HostConfig` for `docker create` from the sandbox config.
    fn build_host_config(
        &self,
        staging_dir: &std::path::Path,
        network_mode: Option<String>,
    ) -> HostConfig {
        let cfg = &self.config;

        // Volume mounts: staging dir → /workspace
        let workspace_mount = Mount {
            target: Some("/workspace".to_string()),
            source: Some(staging_dir.to_string_lossy().to_string()),
            typ: Some(MountTypeEnum::BIND),
            read_only: Some(false),
            ..Default::default()
        };

        let mut mounts = vec![workspace_mount];

        // Mount pre-cached datasets (read-only)
        let system_datasets = std::path::Path::new("/opt/datasets");
        let user_datasets = dirs_next_home().join(".cache").join("datasets");

        if system_datasets.is_dir() {
            mounts.push(Mount {
                target: Some("/workspace/data".to_string()),
                source: Some(system_datasets.to_string_lossy().to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(true),
                ..Default::default()
            });
        } else if user_datasets.is_dir() {
            mounts.push(Mount {
                target: Some("/workspace/data".to_string()),
                source: Some(user_datasets.to_string_lossy().to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            });
        }

        // HuggingFace cache (read-only)
        let hf_container = "/home/researcher/.cache/huggingface";
        let hf_host = std::env::var("HF_HOME")
            .ok()
            .map(|p| std::path::PathBuf::from(p))
            .unwrap_or_else(|| dirs_next_home().join(".cache").join("huggingface"));
        if hf_host.is_dir() {
            mounts.push(Mount {
                target: Some(hf_container.to_string()),
                source: Some(hf_host.to_string_lossy().to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(true),
                ..Default::default()
            });
        }

        // GPU / NPU device requests
        let mut device_requests: Vec<DeviceRequest> = Vec::new();
        let mut devices: Vec<bollard::models::DeviceMapping> = Vec::new();

        if cfg.gpu_enabled {
            let accel = cfg.accelerator_type.as_str();
            let resolved = if accel == "auto" {
                auto_detect_accelerator()
            } else {
                accel.to_string()
            };

            if resolved == "npu" {
                // Ascend NPU passthrough
                let npu_devices = if cfg.gpu_device_ids.is_empty() {
                    (0u32..)
                        .take_while(|d| {
                            std::path::Path::new(&format!("/dev/davinci{d}")).exists()
                        })
                        .collect::<Vec<_>>()
                } else {
                    cfg.gpu_device_ids.clone()
                };

                for d in &npu_devices {
                    let dev = format!("/dev/davinci{d}");
                    devices.push(bollard::models::DeviceMapping {
                        path_on_host: Some(dev.clone()),
                        path_in_container: Some(dev),
                        cgroup_permissions: Some("rwm".to_string()),
                    });
                }
                for dev_path in &["/dev/davinci_manager", "/dev/devmm_svm", "/dev/hisi_hdc"] {
                    devices.push(bollard::models::DeviceMapping {
                        path_on_host: Some(dev_path.to_string()),
                        path_in_container: Some(dev_path.to_string()),
                        cgroup_permissions: Some("rwm".to_string()),
                    });
                }
            } else if resolved == "cuda" {
                // NVIDIA GPU passthrough
                let caps = vec![vec!["gpu".to_string()]];
                if cfg.gpu_device_ids.is_empty() {
                    device_requests.push(DeviceRequest {
                        driver: Some(String::new()),
                        count: Some(-1), // all GPUs
                        device_ids: None,
                        capabilities: Some(caps),
                        options: None,
                    });
                } else {
                    let ids: Vec<String> = cfg
                        .gpu_device_ids
                        .iter()
                        .map(|d| d.to_string())
                        .collect();
                    device_requests.push(DeviceRequest {
                        driver: Some(String::new()),
                        count: None,
                        device_ids: Some(ids),
                        capabilities: Some(caps),
                        options: None,
                    });
                }
            }
        }

        let memory_bytes = (cfg.memory_limit_mb as i64) * 1024 * 1024;
        let shm_bytes = (cfg.shm_size_mb as i64) * 1024 * 1024;

        HostConfig {
            mounts: Some(mounts),
            memory: Some(memory_bytes),
            shm_size: Some(shm_bytes),
            network_mode,
            device_requests: if device_requests.is_empty() {
                None
            } else {
                Some(device_requests)
            },
            devices: if devices.is_empty() { None } else { Some(devices) },
            cap_add: Some(vec!["NET_ADMIN".to_string()]),
            ..Default::default()
        }
    }

    /// Write `requirements.txt` into `staging_dir` from `pip_pre_install`.
    async fn write_requirements(&self, staging_dir: &std::path::Path) -> Result<()> {
        if self.config.pip_pre_install.is_empty() {
            return Ok(());
        }
        let req_path = staging_dir.join("requirements.txt");
        let content = self.config.pip_pre_install.join("\n") + "\n";
        tokio::fs::write(&req_path, &content).await?;
        Ok(())
    }

    /// Collect all log output from a container into a single `String`.
    async fn collect_logs(&self, container_id: &str) -> (String, String) {
        let opts = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            follow: false,
            ..Default::default()
        };
        let mut stdout = String::new();
        let mut stderr = String::new();

        let mut stream = self.client.logs(container_id, Some(opts));
        while let Some(item) = stream.next().await {
            match item {
                Ok(LogOutput::StdOut { message }) => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(LogOutput::StdErr { message }) => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
        (stdout, stderr)
    }

    /// Create, start, exec, and stop a fresh container for one run.
    async fn run_in_container(
        &self,
        staging_dir: &std::path::Path,
        entry_point: &str,
        timeout: Duration,
    ) -> Result<ExecutionResult> {
        let cfg = &self.config;
        let network_policy = NetworkPolicy::from_str(&cfg.network_policy);

        let network_mode = match &network_policy {
            NetworkPolicy::None => Some("none".to_string()),
            _ => None, // bridge (default) — iptables blocks after setup
        };

        let container_name = format!(
            "mol-exp-{}-{}",
            std::process::id(),
            self.run_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );

        let host_config = self.build_host_config(staging_dir, network_mode);

        // Environment variables
        let mut env: Vec<String> = vec!["PYTHONUNBUFFERED=1".to_string()];

        if matches!(network_policy, NetworkPolicy::SetupOnly | NetworkPolicy::PipOnly) {
            env.push("RC_SETUP_ONLY_NETWORK=1".to_string());
        }

        if let Some(hf_token) = std::env::var("HF_TOKEN")
            .ok()
            .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok())
        {
            env.push(format!("HF_TOKEN={hf_token}"));
        }

        let container_config = Config {
            image: Some(cfg.image.as_str()),
            working_dir: Some("/workspace"),
            env: Some(env.iter().map(|s| s.as_str()).collect()),
            cmd: Some(vec![entry_point]),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Create container
        debug!("DockerSandbox: creating container {container_name}");
        let create_resp = self
            .client
            .create_container(
                Some(CreateContainerOptions {
                    name: &container_name,
                    platform: None,
                }),
                container_config,
            )
            .await
            .context("create container")?;

        let cid = create_resp.id;

        // Start container
        self.client
            .start_container(&cid, None::<StartContainerOptions<String>>)
            .await
            .context("start container")?;

        let start = std::time::Instant::now();

        // Wait for completion with timeout
        let result = tokio::time::timeout(timeout, self.wait_for_container(&cid)).await;

        let duration = start.elapsed();

        let (exit_code, timed_out) = match result {
            Ok(Ok(code)) => (code, false),
            Ok(Err(e)) => {
                warn!("Error waiting for container {cid}: {e}");
                (-1, false)
            }
            Err(_) => {
                warn!("Container {cid} timed out after {}s", timeout.as_secs());
                self.client
                    .stop_container(&cid, Some(StopContainerOptions { t: 5 }))
                    .await
                    .ok();
                (-1, true)
            }
        };

        let (stdout, stderr) = self.collect_logs(&cid).await;

        // Remove container
        if !cfg.keep_containers {
            self.client
                .remove_container(
                    &cid,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .ok();
        }

        // Parse metrics
        let mut metrics = parse_metrics_from_stdout(&stdout);

        // Supplement with structured results.json if present
        let results_json = staging_dir.join("results.json");
        if results_json.exists() {
            if let Ok(file_metrics) = parse_metrics_from_file(&results_json) {
                for (k, v) in file_metrics {
                    metrics.entry(k).or_insert(v);
                }
            }
        }

        Ok(ExecutionResult {
            stdout,
            stderr,
            exit_code,
            duration,
            metrics,
            timed_out,
        })
    }

    /// Wait for a container to reach the "exited" state and return its exit code.
    async fn wait_for_container(&self, container_id: &str) -> Result<i32> {
        use bollard::container::WaitContainerOptions;
        let mut stream = self
            .client
            .wait_container(container_id, None::<WaitContainerOptions<String>>);
        if let Some(result) = stream.next().await {
            let resp = result?;
            return Ok(resp.status_code as i32);
        }
        Ok(-1)
    }
}

#[async_trait]
impl Sandbox for DockerSandbox {
    async fn setup(&mut self) -> Result<()> {
        // Verify that the configured image exists locally.
        self.client
            .inspect_image(&self.config.image)
            .await
            .with_context(|| {
                format!(
                    "Docker image '{}' not found locally. \
                     Build with: docker build -t {} researchmol/docker/",
                    self.config.image, self.config.image
                )
            })?;
        info!("DockerSandbox: image '{}' verified", self.config.image);
        Ok(())
    }

    async fn execute(&self, code: &str, timeout: Option<Duration>) -> Result<ExecutionResult> {
        let n = self
            .run_counter
            .load(std::sync::atomic::Ordering::Relaxed);
        let staging = self.workdir.join(format!("_docker_run_{n}"));
        tokio::fs::create_dir_all(&staging).await?;

        // Write the experiment code.
        tokio::fs::write(staging.join("main.py"), code).await?;

        // Write pip requirements if configured.
        self.write_requirements(&staging).await?;

        let timeout_dur = timeout.unwrap_or(Duration::from_secs(300));
        self.run_in_container(&staging, "main.py", timeout_dur)
            .await
    }

    async fn cleanup(&mut self) -> Result<()> {
        if let Some(cid) = self.container_id.take() {
            self.client
                .remove_container(
                    &cid,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .ok();
        }
        Ok(())
    }

    async fn is_ready(&self) -> bool {
        self.client.ping().await.is_ok()
    }
}

// ---------------------------------------------------------------------------
// Helper: create DockerSandbox from full ExperimentConfig
// ---------------------------------------------------------------------------

/// Build a `DockerSandbox` from a full `ExperimentConfig`.
pub async fn docker_sandbox_from_config(
    config: &ExperimentConfig,
    workdir: PathBuf,
) -> Result<DockerSandbox> {
    DockerSandbox::new(config.docker.clone(), workdir).await
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn dirs_next_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
}

/// Detect the available GPU accelerator type.
fn auto_detect_accelerator() -> String {
    // Prefer nvidia-smi check; fall back to Ascend device files.
    let nv = std::process::Command::new("nvidia-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if nv {
        return "cuda".to_string();
    }
    if std::path::Path::new("/dev/davinci0").exists() {
        return "npu".to_string();
    }
    "none".to_string()
}
