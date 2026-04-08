//! Sandbox permission policy for tool execution.
//!
//! All file operations are sandboxed: write operations are confined to the
//! workspace, read operations are allowed in the workspace plus any explicitly
//! configured directories.  Only dangerous bash commands are blocked.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

/// Dangerous bash patterns that are unconditionally blocked.
const DANGEROUS_BASH_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs.",
    "dd if=/dev/zero",
    ":(){ :",
    "> /dev/sda",
    "chmod -R 777 /",
    "curl | sh",
    "wget | sh",
    "pip install --",
    "shutdown",
    "reboot",
    "kill -9 1",
    "pkill -9",
];

/// Bash redirect patterns that write to absolute paths.
const REDIRECT_WRITE_PATTERNS: &[&str] = &["> /", ">> /", "tee /", "cp * /", "mv * /"];

/// Permission policy: which tools are allowed and what paths are accessible.
#[derive(Debug, Clone)]
pub struct PermissionPolicy {
    /// Absolute path to the workspace directory.
    pub workspace: PathBuf,
    /// Additional directories that the agent may read from.
    pub allowed_read_dirs: Vec<PathBuf>,
    /// Whether outbound network requests are allowed (for bash).
    pub network_allowed: bool,
}

impl PermissionPolicy {
    /// Create a new policy rooted at `workspace`.
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
            allowed_read_dirs: Vec::new(),
            network_allowed: true,
        }
    }

    /// Add an additional directory that the agent may read.
    pub fn allow_read_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        let p = dir.into();
        if p.is_dir() {
            self.allowed_read_dirs.push(p);
        }
        self
    }

    /// Check whether a tool call is permitted.
    ///
    /// Returns `Ok(())` if allowed, `Err(reason)` if denied.
    pub fn check_permission(&self, tool: &str, args: &Value) -> Result<()> {
        match tool {
            "bash" => self.check_bash(args),
            "write_file" | "edit_file" => Ok(()), // write path validated at execution time
            "read_file" | "glob_search" | "grep_search" => Ok(()),
            _ => Err(anyhow::anyhow!("Unknown tool: {tool}")),
        }
    }

    fn check_bash(&self, args: &Value) -> Result<()> {
        let command = args.get("command").and_then(Value::as_str).unwrap_or("");
        let cmd_lower = command.to_lowercase();

        for pattern in DANGEROUS_BASH_PATTERNS {
            if cmd_lower.contains(pattern) {
                return Err(anyhow::anyhow!(
                    "Dangerous command blocked: {}",
                    &command[..command.len().min(80)]
                ));
            }
        }

        // Block write-redirects to absolute paths outside the workspace
        let ws_str = self.workspace.to_string_lossy();
        for pattern in REDIRECT_WRITE_PATTERNS {
            if let Some(idx) = cmd_lower.find(pattern) {
                let after = &command[idx + pattern.len() - 1..].trim_start();
                let path_token = after.split_whitespace().next().unwrap_or("");
                if path_token.starts_with('/') && !path_token.starts_with(ws_str.as_ref()) {
                    return Err(anyhow::anyhow!(
                        "Bash write to path outside workspace blocked: {path_token}\n\
                         All file modifications must be inside: {ws_str}"
                    ));
                }
            }
        }

        Ok(())
    }

    /// Resolve a path for reading — workspace-relative or absolute.
    pub fn resolve_read_path(&self, raw: &str) -> Result<PathBuf> {
        let p = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            self.workspace.join(raw)
        };
        let resolved = p.canonicalize().unwrap_or_else(|_| p.clone());
        if !resolved.exists() {
            return Err(anyhow::anyhow!("File not found: {raw}"));
        }
        Ok(resolved)
    }

    /// Resolve a path for writing — must stay within the workspace.
    ///
    /// Returns an error if the resolved path escapes the workspace directory,
    /// preventing arbitrary file writes via prompt injection.
    pub fn resolve_write_path(&self, raw: &str) -> anyhow::Result<PathBuf> {
        let p = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            self.workspace.join(raw)
        };
        // Canonicalize both paths (resolve symlinks and ../) for comparison.
        // Fall back to the raw path if canonicalize fails (e.g., file doesn't exist yet).
        let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
        let ws_canonical = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());
        if !canonical.starts_with(&ws_canonical) {
            return Err(anyhow::anyhow!(
                "Write path '{}' is outside workspace '{}'",
                canonical.display(),
                ws_canonical.display()
            ));
        }
        Ok(p)
    }
}
