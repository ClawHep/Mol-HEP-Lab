//! SSH remote sandbox: upload experiment code, execute remotely, collect results.
//!
//! Uses the `ssh2` crate for channel-based execution and SCP file transfers.
//! Optionally delegates to a Docker container on the remote host for stronger
//! isolation.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use ssh2::Session;
use tracing::{debug, info, warn};

use mol_config::{ExperimentConfig, SshRemoteConfig};

use crate::metrics::parse_metrics_from_stdout;
use crate::sandbox::{ExecutionResult, Sandbox};

// ---------------------------------------------------------------------------
// SshSandbox
// ---------------------------------------------------------------------------

/// Executes experiment code on a remote host via SSH.
///
/// Execution flow:
///   1. Create a unique temporary directory on the remote host.
///   2. Upload experiment code via SCP.
///   3. Run optional setup commands (pip install, conda activate, etc.).
///   4. Execute the experiment (`python3 main.py` or Docker).
///   5. Parse stdout for metrics.
///   6. Clean up the remote directory.
pub struct SshSandbox {
    config: SshRemoteConfig,
    workdir: PathBuf,
    session: Option<Session>,
    #[allow(dead_code)]
    run_counter: std::sync::atomic::AtomicU64,
}

impl SshSandbox {
    /// Create a new `SshSandbox`. Call `setup()` to establish the SSH connection.
    pub fn new(config: SshRemoteConfig, workdir: PathBuf) -> Self {
        Self {
            config,
            workdir,
            session: None,
            run_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    // ------------------------------------------------------------------
    // Connection helpers
    // ------------------------------------------------------------------

    /// Establish an SSH session, authenticating with a key file or agent.
    fn connect(&self) -> Result<Session> {
        let cfg = &self.config;
        if cfg.host.is_empty() {
            bail!("SSH host is not configured");
        }

        let addr = format!("{}:{}", cfg.host, cfg.port);
        debug!("SshSandbox: connecting to {addr}");

        let tcp = TcpStream::connect(&addr)
            .with_context(|| format!("TCP connect to {addr}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

        let mut sess = Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        // Authenticate
        if !cfg.key_path.is_empty() {
            let key = shellexpand::tilde(&cfg.key_path).to_string();
            let key_path = std::path::Path::new(&key);
            if cfg.user.is_empty() {
                bail!("SSH user is required for key authentication");
            }
            sess.userauth_pubkey_file(&cfg.user, None, key_path, None)
                .with_context(|| format!("SSH key auth as '{}' with key {key}", cfg.user))?;
        } else {
            // Try agent authentication
            let user = if cfg.user.is_empty() { "root" } else { &cfg.user };
            let mut agent = sess.agent()?;
            agent.connect()?;
            agent.list_identities()?;
            let identities = agent.identities()?;
            if identities.is_empty() {
                bail!("No SSH identities available in agent and no key_path configured");
            }
            agent.userauth(user, &identities[0])?;
        }

        if !sess.authenticated() {
            bail!("SSH authentication failed for {}", cfg.host);
        }

        info!("SshSandbox: connected to {}", cfg.host);
        Ok(sess)
    }

    // ------------------------------------------------------------------
    // Remote execution helpers
    // ------------------------------------------------------------------

    /// Execute a shell command on the remote host, returning (stdout, stderr, exit_code).
    fn ssh_exec(
        sess: &Session,
        command: &str,
        _timeout_secs: u64,
    ) -> Result<(String, String, i32)> {
        // The TCP-level timeout was set during connect(); channel-level timeout
        // is not directly supported by ssh2 — callers rely on the session
        // timeout set at connection time.
        let mut channel = sess.channel_session()?;
        channel.exec(command)?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        channel.read_to_string(&mut stdout)?;
        channel.stderr().read_to_string(&mut stderr)?;
        channel.wait_close()?;

        let exit_code = channel.exit_status().unwrap_or(-1);
        Ok((stdout, stderr, exit_code))
    }

    /// Upload all files in `local_dir` to `remote_dir` via SCP.
    fn scp_upload(sess: &Session, local_dir: &Path, remote_dir: &str) -> Result<bool> {
        let entries = std::fs::read_dir(local_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                let remote_path = format!("{remote_dir}/{filename}");

                let data = std::fs::read(&path)?;
                let len = data.len() as u64;

                let mut remote_file = sess
                    .scp_send(
                        std::path::Path::new(&remote_path),
                        0o644,
                        len,
                        None,
                    )
                    .with_context(|| format!("SCP send to {remote_path}"))?;

                remote_file.write_all(&data)?;
                remote_file.send_eof()?;
                remote_file.wait_eof()?;
                remote_file.close()?;
                remote_file.wait_close()?;
            }
        }
        Ok(true)
    }

    /// Build the shell command to execute the experiment on the remote host.
    fn build_exec_command(&self, remote_dir: &str, entry_point: &str) -> String {
        let cfg = &self.config;
        let rd = shell_quote(remote_dir);
        let ep = shell_quote(entry_point);
        let py = shell_quote(&cfg.remote_python);

        let gpu_env = if cfg.gpu_ids.is_empty() {
            String::new()
        } else if cfg.accelerator_type == "npu" {
            format!(
                "ASCEND_VISIBLE_DEVICES={} ",
                cfg.gpu_ids
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        } else {
            format!(
                "CUDA_VISIBLE_DEVICES={} ",
                cfg.gpu_ids
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };

        if cfg.use_docker {
            self.build_docker_exec_command(remote_dir, entry_point)
        } else {
            // Bare Python execution with optional network isolation via unshare.
            format!(
                "cd {rd} && \
                 if command -v unshare >/dev/null 2>&1; then \
                   HOME={rd} {gpu_env}unshare --net {py} -u {ep}; \
                 else \
                   echo 'WARNING: unshare not available, running without network isolation' >&2; \
                   HOME={rd} {gpu_env}{py} -u {ep}; \
                 fi"
            )
        }
    }

    /// Build a `docker run` command to execute on the remote host.
    fn build_docker_exec_command(&self, remote_dir: &str, entry_point: &str) -> String {
        let cfg = &self.config;
        let rd = shell_quote(remote_dir);
        let ep = shell_quote(entry_point);
        let img = shell_quote(&cfg.docker_image);

        let mut parts = vec![
            "docker".to_string(),
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            format!("{rd}:/workspace"),
            "-w".to_string(),
            "/workspace".to_string(),
            format!("--memory={}m", cfg.docker_memory_limit_mb),
            format!("--shm-size={}m", cfg.docker_shm_size_mb),
        ];

        if cfg.docker_network_policy == "none" {
            parts.extend(["--network".to_string(), "none".to_string()]);
        }

        // GPU passthrough on remote
        if cfg.accelerator_type == "npu" {
            if cfg.gpu_ids.is_empty() {
                parts.extend(["--device".to_string(), "/dev/davinci0".to_string()]);
            } else {
                for d in &cfg.gpu_ids {
                    parts.extend(["--device".to_string(), format!("/dev/davinci{d}")]);
                }
            }
            parts.extend([
                "--device".to_string(),
                "/dev/davinci_manager".to_string(),
                "--device".to_string(),
                "/dev/devmm_svm".to_string(),
                "--device".to_string(),
                "/dev/hisi_hdc".to_string(),
            ]);
        } else if cfg.gpu_ids.is_empty() {
            parts.extend(["--gpus".to_string(), "all".to_string()]);
        } else {
            let spec = cfg
                .gpu_ids
                .iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(",");
            parts.extend(["--gpus".to_string(), format!("device={spec}")]);
        }

        parts.push(img);
        parts.extend(["python3".to_string(), "-u".to_string(), ep]);

        parts.join(" ")
    }

    /// Run the full execute flow (blocking; intended to be called from tokio `spawn_blocking`).
    fn execute_blocking(
        &self,
        code: &str,
        timeout_secs: u64,
    ) -> Result<ExecutionResult> {
        let sess = self.connect()?;
        let cfg = &self.config;
        let run_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let remote_dir = format!("{}/mol-{run_id}", cfg.remote_workdir);

        // 1. Create remote directory
        let mkdir_cmd = format!("mkdir -p {}", shell_quote(&remote_dir));
        let (_, stderr, rc) = Self::ssh_exec(&sess, &mkdir_cmd, 30)?;
        if rc != 0 {
            bail!("Failed to create remote directory: {stderr}");
        }

        // 2. Write code to a local staging dir and upload
        let staging = self.workdir.join(format!("_ssh_{run_id}"));
        std::fs::create_dir_all(&staging)?;
        std::fs::write(staging.join("main.py"), code)?;

        if !Self::scp_upload(&sess, &staging, &remote_dir)? {
            bail!("SCP upload failed");
        }

        // 3. Run setup commands
        for setup_cmd in &cfg.setup_commands {
            let full_cmd = format!("cd {} && {setup_cmd}", shell_quote(&remote_dir));
            let (_, stderr, rc) = Self::ssh_exec(&sess, &full_cmd, cfg.setup_timeout_sec as u64)?;
            if rc != 0 {
                warn!("Setup command failed (exit {rc}): {stderr}");
            }
        }

        // 4. Execute experiment
        let exec_cmd = self.build_exec_command(&remote_dir, "main.py");
        debug!("SshSandbox remote command: {exec_cmd}");

        let start = std::time::Instant::now();
        let (stdout, stderr, exit_code) = Self::ssh_exec(&sess, &exec_cmd, timeout_secs)?;
        let duration = start.elapsed();

        // 5. Parse metrics
        let metrics = parse_metrics_from_stdout(&stdout);

        // 6. Cleanup remote directory
        let rm_cmd = format!("rm -rf {}", shell_quote(&remote_dir));
        Self::ssh_exec(&sess, &rm_cmd, 15).ok();

        // Cleanup local staging
        std::fs::remove_dir_all(&staging).ok();

        Ok(ExecutionResult {
            stdout,
            stderr,
            exit_code,
            duration,
            metrics,
            timed_out: false,
        })
    }
}

#[async_trait]
impl Sandbox for SshSandbox {
    async fn setup(&mut self) -> Result<()> {
        tokio::fs::create_dir_all(&self.workdir).await?;
        // Test connectivity
        let config = self.config.clone();
        tokio::task::spawn_blocking(move || {
            let sess = {
                let sandbox = SshSandbox::new(config, PathBuf::new());
                sandbox.connect()
            };
            sess.map(|_| ())
        })
        .await??;
        Ok(())
    }

    async fn execute(&self, code: &str, timeout: Option<Duration>) -> Result<ExecutionResult> {
        let timeout_secs = timeout
            .unwrap_or(Duration::from_secs(self.config.timeout_sec as u64))
            .as_secs();

        let code = code.to_owned();
        let workdir = self.workdir.clone();
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || {
            let sandbox = SshSandbox::new(config, workdir);
            sandbox.execute_blocking(&code, timeout_secs)
        })
        .await?
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.session = None;
        Ok(())
    }

    async fn is_ready(&self) -> bool {
        if self.config.host.is_empty() {
            return false;
        }
        let config = self.config.clone();
        tokio::task::spawn_blocking(move || {
            let sandbox = SshSandbox::new(config, PathBuf::new());
            sandbox.connect().is_ok()
        })
        .await
        .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Factory helper
// ---------------------------------------------------------------------------

/// Build an `SshSandbox` from a full `ExperimentConfig`.
pub fn ssh_sandbox_from_config(config: &ExperimentConfig, workdir: PathBuf) -> SshSandbox {
    SshSandbox::new(config.ssh_remote.clone(), workdir)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Shell-quote a string with single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}
