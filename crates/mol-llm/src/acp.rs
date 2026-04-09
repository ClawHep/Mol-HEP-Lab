//! ACP (Agent Client Protocol) client.
//!
//! Invokes ACP-compatible agent CLIs (Claude Code, Codex, Gemini CLI, etc.)
//! via the `acpx` bridge tool as a subprocess.  A single named session is
//! maintained across multiple [`ACPClient::invoke`] calls so the agent retains
//! full context.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use tokio::process::Command;
use tracing::{info, warn};

use crate::response::LLMResponse;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for an ACP agent connection.
#[derive(Debug, Clone)]
pub struct ACPConfig {
    /// The agent CLI name (e.g. `"claude"`, `"codex"`, `"gemini"`).
    pub agent: String,

    /// Working directory for the agent subprocess.
    pub cwd: PathBuf,

    /// Explicit path to the `acpx` binary.  Empty means auto-detect.
    pub acpx_command: String,

    /// Named acpx session to use (or create) for this client.
    pub session_name: String,

    /// Per-prompt timeout in seconds.
    pub timeout_sec: u64,
}

impl Default for ACPConfig {
    fn default() -> Self {
        Self {
            agent: "claude".to_owned(),
            cwd: PathBuf::from("."),
            acpx_command: String::new(),
            session_name: "researchmol".to_owned(),
            timeout_sec: 1800,
        }
    }
}

// ---------------------------------------------------------------------------
// acpx output line patterns
// ---------------------------------------------------------------------------

fn is_control_line(line: &str) -> bool {
    line.starts_with("[done]")
        || line.starts_with("[client]")
        || line.starts_with("[acpx]")
}

fn is_tool_line(line: &str) -> bool {
    line.starts_with("[tool]")
}

/// Large prompts are written to a temp file to avoid E2BIG.
const MAX_CLI_PROMPT_BYTES: usize = 100_000;

/// Error substrings that indicate a dead/stale session (trigger reconnect).
const RECONNECT_ERRORS: &[&str] = &[
    "agent needs reconnect",
    "session not found",
    "Query closed",
];

// ---------------------------------------------------------------------------
// ACPClient
// ---------------------------------------------------------------------------

/// LLM client that drives ACP agents via `acpx`.
pub struct ACPClient {
    pub config: ACPConfig,
    acpx_path: Option<PathBuf>,
    session_ready: bool,
}

impl ACPClient {
    /// Create a new client from the given [`ACPConfig`].
    pub fn new(config: ACPConfig) -> Self {
        let acpx_path = if config.acpx_command.is_empty() {
            find_acpx()
        } else {
            Some(PathBuf::from(&config.acpx_command))
        };
        Self {
            config,
            acpx_path,
            session_ready: false,
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Send `messages` to the ACP agent and return its response.
    ///
    /// Parameters `model`, `max_tokens`, `temperature`, and `json_mode` are
    /// accepted for interface compatibility with [`crate::client::LLMClient`]
    /// but are not forwarded — the agent manages its own inference parameters.
    pub async fn invoke(
        &mut self,
        messages: &[crate::client::Message],
        system: Option<&str>,
    ) -> Result<LLMResponse> {
        let prompt = messages_to_prompt(messages, system);
        let content = self.send_prompt(&prompt).await?;
        Ok(LLMResponse {
            content,
            model: format!("acp:{}", self.config.agent),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            finish_reason: Some("stop".to_owned()),
            truncated: false,
            raw: None,
        })
    }

    /// Check that `acpx` and the agent CLI are available and that a session
    /// can be established.
    pub async fn preflight(&mut self) -> crate::response::PreflightResult {
        if self.acpx_path.is_none() {
            return crate::response::PreflightResult::fail(
                "acpx not found. Install: npm install -g acpx \
                 or set llm.acp.acpx_command in config.",
            );
        }
        // Check the agent binary is on PATH
        if which::which(self.config.agent.as_str()).is_err() {
            return crate::response::PreflightResult::fail(format!(
                "ACP agent CLI not found: {:?} (not on PATH)",
                self.config.agent
            ));
        }
        match self.ensure_session().await {
            Ok(()) => crate::response::PreflightResult::ok(format!(
                "OK - ACP session ready ({} via acpx)",
                self.config.agent
            )),
            Err(e) => {
                crate::response::PreflightResult::fail(format!("ACP session init failed: {e}"))
            }
        }
    }

    /// Close the named acpx session.
    pub async fn close(&mut self) {
        if !self.session_ready {
            return;
        }
        if let Some(acpx) = &self.acpx_path {
            let _ = Command::new(acpx)
                .args([
                    "--ttl",
                    "0",
                    "--cwd",
                    &self.abs_cwd(),
                    &self.config.agent,
                    "sessions",
                    "close",
                    &self.config.session_name,
                ])
                .output()
                .await;
        }
        self.session_ready = false;
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    fn abs_cwd(&self) -> String {
        self.config
            .cwd
            .canonicalize()
            .unwrap_or_else(|_| self.config.cwd.clone())
            .to_string_lossy()
            .into_owned()
    }

    async fn ensure_session(&mut self) -> Result<()> {
        if self.session_ready {
            return Ok(());
        }
        let acpx = self
            .acpx_path
            .as_ref()
            .ok_or_else(|| anyhow!("acpx not found"))?
            .clone();
        let cwd = self.abs_cwd();

        // Always create a fresh session to avoid context leakage between runs
        // (Friction Fix #3: stale session state produces garbage artifacts).
        // First try closing any existing session with the same name.
        let _ = Command::new(&acpx)
            .args([
                "--ttl",
                "0",
                "--cwd",
                &cwd,
                &self.config.agent,
                "sessions",
                "close",
                &self.config.session_name,
            ])
            .output()
            .await;

        let result = Command::new(&acpx)
            .args([
                "--ttl",
                "0",
                "--cwd",
                &cwd,
                &self.config.agent,
                "sessions",
                "new",
                "--name",
                &self.config.session_name,
            ])
            .output()
            .await?;

        if !result.status.success() {
            // Fall back to `sessions ensure` if `new` fails (e.g. agent doesn't support it)
            let result2 = Command::new(&acpx)
                .args([
                    "--ttl",
                    "0",
                    "--cwd",
                    &cwd,
                    &self.config.agent,
                    "sessions",
                    "ensure",
                    "--name",
                    &self.config.session_name,
                ])
                .output()
                .await?;

            if !result2.status.success() {
                let stderr = String::from_utf8_lossy(&result2.stderr);
                bail!("Failed to create ACP session: {stderr}");
            }
        }

        self.session_ready = true;
        info!(
            session = %self.config.session_name,
            agent = %self.config.agent,
            "ACP session ready (fresh)"
        );
        Ok(())
    }

    async fn send_prompt(&mut self, prompt: &str) -> Result<String> {
        let acpx = self
            .acpx_path
            .as_ref()
            .ok_or_else(|| anyhow!("acpx not found"))?
            .clone();

        let use_file = prompt.len() > MAX_CLI_PROMPT_BYTES;

        let mut last_err: Option<anyhow::Error> = None;
        const MAX_RECONNECTS: u32 = 5;

        for attempt in 0..=MAX_RECONNECTS {
            self.ensure_session().await?;

            let result = if use_file {
                self.send_via_file(&acpx, prompt).await
            } else {
                self.send_cli(&acpx, prompt).await
            };

            match result {
                Ok(content) => return Ok(content),
                Err(e) => {
                    let msg = e.to_string();
                    if RECONNECT_ERRORS.iter().any(|pat| msg.contains(pat)) && attempt < MAX_RECONNECTS {
                        // Exponential backoff: 2s, 4s, 8s, 16s, 32s
                        let backoff = std::time::Duration::from_secs(2u64.pow(attempt + 1));
                        warn!(
                            attempt = attempt + 1,
                            backoff_secs = backoff.as_secs(),
                            error = %e,
                            "ACP session stale, reconnecting after backoff"
                        );
                        tokio::time::sleep(backoff).await;
                        self.force_reconnect().await;
                        last_err = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("ACP send failed after reconnect attempts")))
    }

    async fn send_cli(&self, acpx: &PathBuf, prompt: &str) -> Result<String> {
        // No artificial timeout — research tasks need sufficient time to complete.
        // The runner monitors heartbeat/artifacts for liveness instead.
        let output = Command::new(acpx)
            .args([
                "--approve-all",
                "--ttl",
                "0",
                "--cwd",
                &self.abs_cwd(),
                &self.config.agent,
                "-s",
                &self.config.session_name,
                prompt,
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "ACP prompt failed (exit {}): {stderr}",
                output.status.code().unwrap_or(-1)
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        Ok(extract_response(&stdout))
    }

    async fn send_via_file(&self, acpx: &PathBuf, prompt: &str) -> Result<String> {
        use tokio::io::AsyncWriteExt;

        info!(
            bytes = prompt.len(),
            "Prompt too large for CLI arg; using temp file"
        );

        // Write prompt to a named temp file.
        let mut tmp = tokio::fs::File::from_std(
            tempfile::Builder::new()
                .prefix("mol_prompt_")
                .suffix(".md")
                .tempfile()?
                .into_file(),
        );
        tmp.write_all(prompt.as_bytes()).await?;
        // We need the path — keep the NamedTempFile alive.
        drop(tmp);

        // Rebuild with a known path so we can reference it.
        let tmp_file = tempfile::Builder::new()
            .prefix("mol_prompt_")
            .suffix(".md")
            .tempfile()?;
        let tmp_path = tmp_file.path().to_owned();
        tokio::fs::write(&tmp_path, prompt.as_bytes()).await?;

        let short_prompt = format!(
            "Read the file at {} in its entirety. \
             Follow ALL instructions contained in that file and \
             respond exactly as requested. Do NOT summarize, \
             just produce the requested output.",
            tmp_path.display()
        );

        let result = self.send_cli(acpx, &short_prompt).await;
        // temp file is removed when `tmp_file` drops here
        drop(tmp_file);
        result
    }

    async fn force_reconnect(&mut self) {
        self.close().await;
        self.session_ready = false;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the `acpx` binary on PATH or in the ResearchMol plugin directory.
fn find_acpx() -> Option<PathBuf> {
    if let Ok(p) = which::which("acpx") {
        return Some(p);
    }
    // Check ResearchMol's bundled acpx plugin directory
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    let bundled = home.join(".openmol/extensions/acpx/node_modules/.bin/acpx");
    if bundled.is_file() {
        return Some(bundled);
    }
    None
}

/// Flatten a message list into a single text prompt preserving role labels.
fn messages_to_prompt(messages: &[crate::client::Message], system: Option<&str>) -> String {
    use crate::client::Message;
    let mut parts: Vec<String> = Vec::new();

    if let Some(sys) = system {
        parts.push(format!("[System]\n{sys}"));
    }

    for msg in messages {
        match msg {
            Message::System(content) => parts.push(format!("[System]\n{content}")),
            Message::Assistant(content) => {
                parts.push(format!("[Previous Response]\n{content}"))
            }
            Message::User(content) => parts.push(content.clone()),
        }
    }

    parts.join("\n\n")
}

/// Strip acpx metadata lines from raw subprocess output, returning only the
/// agent's actual response text.
fn extract_response(raw: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_tool_block = false;

    for line in raw.lines() {
        if is_control_line(line) {
            in_tool_block = false;
            continue;
        }
        if is_tool_line(line) {
            in_tool_block = true;
            continue;
        }
        if in_tool_block {
            // Tool block continuation lines are indented or blank
            if line.starts_with("  ") || line.trim().is_empty() {
                continue;
            }
            // Non-indented, non-empty line signals end of tool block
            in_tool_block = false;
        }
        // Skip leading blank lines
        if lines.is_empty() && line.trim().is_empty() {
            continue;
        }
        lines.push(line);
    }

    // Trim trailing blank lines
    while lines.last().map(|l: &&str| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_strips_control_lines() {
        let raw = "[acpx] session info\n\
                   [client] some metadata\n\
                   \n\
                   Hello from the agent.\n\
                   This is the answer.\n\
                   [done]\n";
        assert_eq!(extract_response(raw), "Hello from the agent.\nThis is the answer.");
    }

    #[test]
    fn extract_strips_tool_blocks() {
        // Continuation lines in a tool block are indented with two spaces,
        // matching the Python acpx output format.
        let raw = "[tool] read_file\n  input: foo.txt\n  output: bar\nActual response here.\n";
        assert_eq!(extract_response(raw), "Actual response here.");
    }

    #[test]
    fn messages_to_prompt_basic() {
        use crate::client::Message;
        let msgs = vec![
            Message::System("Be concise.".into()),
            Message::User("What is 2+2?".into()),
        ];
        let prompt = messages_to_prompt(&msgs, None);
        assert!(prompt.contains("[System]\nBe concise."));
        assert!(prompt.contains("What is 2+2?"));
    }
}
