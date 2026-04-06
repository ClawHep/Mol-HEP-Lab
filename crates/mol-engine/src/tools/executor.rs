//! Tool dispatch and execution.
//!
//! Ported from `claw_engine/tools/executor.py`. Each of the six tools is
//! implemented as an async method. All file operations are sandboxed to the
//! workspace root (enforced by [`PermissionPolicy`]).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use regex::Regex;
use serde_json::Value;
use tokio::fs;
use tokio::process::Command;
use tracing::debug;

use super::permissions::PermissionPolicy;

const MAX_RESULT_CHARS: usize = 16_000;
const MAX_BASH_RESULT_CHARS: usize = 24_000;
const MAX_GLOB_RESULTS: usize = 200;
const MAX_GREP_MATCHES: usize = 200;

/// Result of a single tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// The text result returned to the LLM.
    pub content: String,
    /// True if the tool encountered an error.
    pub is_error: bool,
    /// Wall-clock execution time.
    pub elapsed: Duration,
}

impl ToolResult {
    fn ok(content: impl Into<String>, elapsed: Duration) -> Self {
        Self { content: content.into(), is_error: false, elapsed }
    }

    fn err(content: impl Into<String>, elapsed: Duration) -> Self {
        Self { content: content.into(), is_error: true, elapsed }
    }
}

/// Executes tool calls within a sandboxed workspace.
pub struct ToolExecutor {
    pub workspace: PathBuf,
    pub policy: PermissionPolicy,
    pub bash_timeout_sec: u64,
    pub call_count: u64,
}

impl ToolExecutor {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        let ws: PathBuf = workspace.into();
        let policy = PermissionPolicy::new(ws.clone());
        Self {
            workspace: ws,
            policy,
            bash_timeout_sec: 60,
            call_count: 0,
        }
    }

    pub fn with_timeout(mut self, timeout_sec: u64) -> Self {
        self.bash_timeout_sec = timeout_sec;
        self
    }

    pub fn with_policy(mut self, policy: PermissionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Dispatch a named tool call.
    pub async fn execute(&mut self, tool_name: &str, args: Value) -> Result<ToolResult> {
        self.call_count += 1;
        let t0 = Instant::now();

        let result = match tool_name {
            "bash" => self.exec_bash(&args).await,
            "read_file" => self.exec_read_file(&args).await,
            "write_file" => self.exec_write_file(&args).await,
            "edit_file" => self.exec_edit_file(&args).await,
            "glob_search" => self.exec_glob_search(&args).await,
            "grep_search" => self.exec_grep_search(&args).await,
            other => Err(anyhow::anyhow!("Unknown tool: {other}")),
        };

        let elapsed = t0.elapsed();
        Ok(match result {
            Ok(content) => ToolResult::ok(content, elapsed),
            Err(e) => ToolResult::err(format!("Tool error: {e}"), elapsed),
        })
    }

    // -----------------------------------------------------------------------
    // bash
    // -----------------------------------------------------------------------

    async fn exec_bash(&self, args: &Value) -> Result<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("bash: 'command' is required"))?;

        let timeout_sec = args
            .get("timeout")
            .and_then(Value::as_u64)
            .unwrap_or(self.bash_timeout_sec)
            .min(self.bash_timeout_sec);

        // Permission check (dangerous-command block)
        self.policy.check_permission("bash", args)?;

        debug!("bash: {}", &command[..command.len().min(80)]);

        let output = Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(&self.workspace)
            .env("WORKSPACE", self.workspace.to_string_lossy().as_ref())
            .kill_on_drop(true)
            .output();

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_sec),
            output,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {timeout_sec}s: {}", &command[..command.len().min(100)]))?
        .map_err(|e| anyhow::anyhow!("Failed to spawn bash: {e}"))?;

        let mut parts: Vec<String> = Vec::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            parts.push(stdout.into_owned());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            parts.push(format!("[stderr]\n{stderr}"));
        }
        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            parts.push(format!("[exit_code: {code}]"));
        }

        let raw = if parts.is_empty() { "(no output)".to_owned() } else { parts.join("\n") };
        Ok(truncate_bash(&raw))
    }

    // -----------------------------------------------------------------------
    // read_file
    // -----------------------------------------------------------------------

    async fn exec_read_file(&self, args: &Value) -> Result<String> {
        let raw_path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("read_file: 'path' is required"))?;

        let path = self.policy.resolve_read_path(raw_path)?;

        let text = fs::read_to_string(&path).await.map_err(|e| {
            anyhow::anyhow!("read_file: could not read '{}': {e}", path.display())
        })?;

        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();

        let offset = args.get("offset").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(0);
        let limit = args.get("limit").and_then(Value::as_u64).map(|v| v as usize);
        let end = limit.map(|l| (offset + l).min(total)).unwrap_or(total);

        let numbered: String = lines[offset..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:6} | {}", offset + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        let header = format!(
            "File: {} ({} lines total, showing {}-{})",
            path.file_name().unwrap_or_default().to_string_lossy(),
            total,
            offset + 1,
            end,
        );
        Ok(truncate(&format!("{header}\n{numbered}")))
    }

    // -----------------------------------------------------------------------
    // write_file
    // -----------------------------------------------------------------------

    async fn exec_write_file(&self, args: &Value) -> Result<String> {
        let raw_path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("write_file: 'path' is required"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("write_file: 'content' is required"))?;

        let path = self.policy.resolve_write_path(raw_path)?;

        let existed = path.exists();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                anyhow::anyhow!("write_file: could not create dirs for '{}': {e}", path.display())
            })?;
        }
        fs::write(&path, content).await.map_err(|e| {
            anyhow::anyhow!("write_file: could not write '{}': {e}", path.display())
        })?;

        let kind = if existed { "updated" } else { "created" };
        let line_count = content.lines().count();
        let display = path
            .strip_prefix(&self.workspace)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
        Ok(format!("File {kind}: {display} ({line_count} lines)"))
    }

    // -----------------------------------------------------------------------
    // edit_file
    // -----------------------------------------------------------------------

    async fn exec_edit_file(&self, args: &Value) -> Result<String> {
        let raw_path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("edit_file: 'path' is required"))?;
        let old_string = args["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("edit_file: 'old_string' is required"))?;
        let new_string = args["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("edit_file: 'new_string' is required"))?;
        let replace_all = args.get("replace_all").and_then(Value::as_bool).unwrap_or(false);

        let path = self.policy.resolve_write_path(raw_path)?;
        if !path.exists() {
            return Err(anyhow::anyhow!("edit_file: '{}' does not exist", path.display()));
        }
        if old_string == new_string {
            return Ok("old_string and new_string are identical — no change needed".to_owned());
        }

        let text = fs::read_to_string(&path).await.map_err(|e| {
            anyhow::anyhow!("edit_file: could not read '{}': {e}", path.display())
        })?;

        if !text.contains(old_string) {
            let snippet = &old_string[..old_string.len().min(100)];
            return Ok(format!(
                "old_string not found in {}: '{snippet}...'",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }

        let (new_text, count) = if replace_all {
            let c = text.matches(old_string).count();
            (text.replace(old_string, new_string), c)
        } else {
            (text.replacen(old_string, new_string, 1), 1)
        };

        fs::write(&path, &new_text).await.map_err(|e| {
            anyhow::anyhow!("edit_file: could not write '{}': {e}", path.display())
        })?;

        let display = path
            .strip_prefix(&self.workspace)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
        Ok(format!("Edited {display}: {count} replacement(s)"))
    }

    // -----------------------------------------------------------------------
    // glob_search
    // -----------------------------------------------------------------------

    async fn exec_glob_search(&self, args: &Value) -> Result<String> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("glob_search: 'pattern' is required"))?;

        let base = if let Some(raw) = args.get("path").and_then(Value::as_str) {
            let p = if Path::new(raw).is_absolute() {
                PathBuf::from(raw)
            } else {
                self.workspace.join(raw)
            };
            if !p.is_dir() {
                return Ok(format!("Not a directory: {}", p.display()));
            }
            p
        } else {
            self.workspace.clone()
        };

        // Use the glob crate via std for now (tokio-friendly: spawn_blocking)
        let pattern_owned = format!("{}/{}", base.to_string_lossy(), pattern);
        let base_clone = base.clone();

        let matches = tokio::task::spawn_blocking(move || {
            let mut found: Vec<(u64, PathBuf)> = Vec::new();
            let skip_dirs: &[&str] = &["__pycache__", ".git", "node_modules"];

            if let Ok(paths) = glob::glob(&pattern_owned) {
                for entry in paths.take(50_000).flatten() {
                    if !entry.is_file() {
                        continue;
                    }
                    // Skip hidden/cache dirs
                    let rel = entry.strip_prefix(&base_clone).unwrap_or(&entry);
                    if rel.components().any(|c| {
                        let s = c.as_os_str().to_string_lossy();
                        s.starts_with('.') || skip_dirs.contains(&s.as_ref())
                    }) {
                        continue;
                    }
                    let mtime = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                        .unwrap_or(0);
                    found.push((mtime, entry));
                    if found.len() >= MAX_GLOB_RESULTS * 5 {
                        break;
                    }
                }
            }
            found.sort_by(|a, b| b.0.cmp(&a.0));
            found.truncate(MAX_GLOB_RESULTS);
            found
        })
        .await
        .map_err(|e| anyhow::anyhow!("glob_search: task error: {e}"))?;

        let truncated = matches.len() == MAX_GLOB_RESULTS;
        let lines: Vec<String> = matches
            .iter()
            .map(|(_, p)| {
                p.strip_prefix(&base)
                    .map(|r| r.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| p.to_string_lossy().into_owned())
            })
            .collect();

        let mut header = format!("Found {} file(s)", lines.len());
        if truncated {
            header.push_str(&format!(" (showing first {MAX_GLOB_RESULTS})"));
        }
        if lines.is_empty() {
            Ok("No files matched.".to_owned())
        } else {
            Ok(format!("{header}\n{}", lines.join("\n")))
        }
    }

    // -----------------------------------------------------------------------
    // grep_search
    // -----------------------------------------------------------------------

    async fn exec_grep_search(&self, args: &Value) -> Result<String> {
        let pattern_str = args["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("grep_search: 'pattern' is required"))?;
        let file_glob = args.get("glob").and_then(Value::as_str).map(str::to_owned);
        let context_lines = args.get("context").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(2);

        let base = if let Some(raw) = args.get("path").and_then(Value::as_str) {
            if Path::new(raw).is_absolute() {
                PathBuf::from(raw)
            } else {
                self.workspace.join(raw)
            }
        } else {
            self.workspace.clone()
        };

        // Validate the regex before passing to the blocking task
        Regex::new(pattern_str)
            .map_err(|e| anyhow::anyhow!("grep_search: invalid regex '{pattern_str}': {e}"))?;

        let pattern_owned = pattern_str.to_owned();
        let base_clone = base.clone();

        let result_text = tokio::task::spawn_blocking(move || {
            let regex = Regex::new(&pattern_owned).unwrap();
            let files = collect_text_files(&base_clone, file_glob.as_deref());

            let mut results: Vec<String> = Vec::new();
            let mut total_matches = 0usize;

            for fpath in &files {
                if total_matches >= MAX_GREP_MATCHES {
                    break;
                }
                let text = match std::fs::read_to_string(fpath) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let lines: Vec<&str> = text.lines().collect();
                let mut file_hits: Vec<String> = Vec::new();

                for (i, line) in lines.iter().enumerate() {
                    if regex.is_match(line) {
                        total_matches += 1;
                        let start = i.saturating_sub(context_lines);
                        let end = (i + context_lines + 1).min(lines.len());
                        for j in start..end {
                            let sep = if j == i { ':' } else { '-' };
                            file_hits.push(format!("  {}{sep} {}", j + 1, lines[j]));
                        }
                        if end < lines.len() {
                            file_hits.push("  ---".to_owned());
                        }
                    }
                }
                if !file_hits.is_empty() {
                    let rel = fpath
                        .strip_prefix(&base_clone)
                        .map(|r| r.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| fpath.to_string_lossy().into_owned());
                    results.push(format!("{rel}:\n{}", file_hits.join("\n")));
                }
            }

            if results.is_empty() {
                return format!("No matches for /{pattern_owned}/");
            }
            let mut header = format!("{total_matches} match(es) in {} file(s)", results.len());
            if total_matches >= MAX_GREP_MATCHES {
                header.push_str(" (truncated)");
            }
            truncate(&format!("{header}\n\n{}", results.join("\n\n")))
        })
        .await
        .map_err(|e| anyhow::anyhow!("grep_search: task error: {e}"))?;

        Ok(result_text)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_text_files(base: &Path, glob_filter: Option<&str>) -> Vec<PathBuf> {
    let skip_dirs: &[&str] = &["__pycache__", ".git", "node_modules", ".snapshots"];
    let mut files: Vec<PathBuf> = Vec::new();

    fn walk(dir: &Path, skip: &[&str], files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || skip.contains(&name.as_ref()) {
                continue;
            }
            if path.is_dir() {
                walk(&path, skip, files);
            } else if path.is_file() {
                // Only walk files small enough to grep
                if path.metadata().map(|m| m.len()).unwrap_or(u64::MAX) <= 2 * 1024 * 1024 {
                    files.push(path);
                }
            }
        }
    }

    if base.is_file() {
        files.push(base.to_path_buf());
    } else {
        walk(base, skip_dirs, &mut files);
    }

    if let Some(glob_pat) = glob_filter {
        files.retain(|p| {
            p.file_name()
                .map(|n| {
                    glob::Pattern::new(glob_pat)
                        .map(|pat| pat.matches(&n.to_string_lossy()))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        });
    }

    files.sort();
    files
}

fn truncate(text: &str) -> String {
    if text.len() <= MAX_RESULT_CHARS {
        text.to_owned()
    } else {
        format!(
            "{}\n... [truncated, {} total chars]",
            &text[..MAX_RESULT_CHARS],
            text.len()
        )
    }
}

fn truncate_bash(text: &str) -> String {
    if text.len() <= MAX_BASH_RESULT_CHARS {
        return text.to_owned();
    }
    let head_budget = MAX_BASH_RESULT_CHARS * 3 / 10;
    let tail_budget = MAX_BASH_RESULT_CHARS - head_budget - 200;
    let head = &text[..head_budget];
    let tail = &text[text.len().saturating_sub(tail_budget)..];
    format!(
        "{head}\n\n... [{} total chars, middle truncated] ...\n\n{tail}",
        text.len()
    )
}
