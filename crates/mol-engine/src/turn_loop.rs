//! Generic agentic LLM tool-use turn loop.
//!
//! The loop: user_message → (LLM call → tool execution →)* → done
//!
//! Stage-specific behaviour (verification gates, custom prompts) is injected
//! via the `verification_hooks` field. Hooks are one-shot: they fire once and
//! are then removed from the list.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tracing::{debug, info, warn};

use crate::tools::definitions::{ToolSpec, default_tool_specs};
use crate::tools::executor::{ToolExecutor, ToolResult};
use crate::tools::permissions::PermissionPolicy;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of LLM→tool→LLM iterations before the loop exits.
pub const MAX_ITERATIONS: usize = 40;

/// Maximum LLM retries per iteration on transient failures.
const MAX_LLM_RETRIES: usize = 5;

/// File extensions collected from the workspace at loop end.
const COLLECT_EXTENSIONS: &[&str] = &[
    ".py", ".yaml", ".yml", ".json", ".txt", ".csv", ".tsv", ".cfg", ".ini", ".toml", ".rs",
    ".md",
];

/// Directories skipped when collecting workspace files.
const SKIP_DIRS: &[&str] = &["__pycache__", "codebases", "datasets", "checkpoints", ".git", "node_modules", ".snapshots"];

// ---------------------------------------------------------------------------
// TurnResult
// ---------------------------------------------------------------------------

/// Result of a complete turn loop execution.
#[derive(Debug, Default)]
pub struct TurnResult {
    /// The final text response from the LLM (last non-tool-call message).
    pub response: String,
    /// Total number of tool calls dispatched.
    pub tool_calls: u64,
    /// Files produced in the workspace, mapping relative path → content.
    pub artifacts_produced: HashMap<String, String>,
    /// Number of LLM→tool iterations completed.
    pub iterations: usize,
    /// Non-fatal errors encountered.
    pub errors: Vec<String>,
    /// Wall-clock seconds for the complete run.
    pub elapsed_sec: f64,
}

// ---------------------------------------------------------------------------
// VerificationHook
// ---------------------------------------------------------------------------

/// A one-shot callback that may inject a message after each tool round.
///
/// Receives `(workspace, tool_uses_this_iteration, workspace_files)` and
/// returns `Some(inject_message)` to steer the LLM, or `None` to skip.
pub type VerificationHook = Box<
    dyn Fn(&Path, &[Value], &[String]) -> Option<String> + Send + Sync,
>;

// ---------------------------------------------------------------------------
// LLM configuration (provider-agnostic)
// ---------------------------------------------------------------------------

/// Minimal LLM configuration needed by the turn loop.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// OpenAI-compatible base URL (e.g. `https://api.openai.com/v1`).
    pub base_url: String,
    /// API key for the provider.
    pub api_key: String,
    /// Model identifier (e.g. `gpt-4o`, `claude-3-5-sonnet-20241022`).
    pub model: String,
    /// Request timeout in seconds.
    pub timeout_sec: u64,
    /// Maximum output tokens.
    pub max_tokens: u32,
}

impl LlmConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            timeout_sec: 600,
            max_tokens: 8192,
        }
    }
}

// ---------------------------------------------------------------------------
// TraceLog
// ---------------------------------------------------------------------------

/// Step-by-step markdown trace logger for debugging agentic loops.
struct TraceLog {
    path: PathBuf,
    step: usize,
}

impl TraceLog {
    fn new(trace_dir: &Path, prefix: &str) -> Self {
        let path = trace_dir.join(format!("{prefix}_trace.md"));
        let now = chrono::Utc::now().to_rfc3339();
        let _ = std::fs::write(&path, format!("# {prefix} Trace\n\nStarted: {now}\n"));
        Self { path, step: 0 }
    }

    fn write(&self, text: &str) {
        use std::io::Write;
        if let Ok(mut f) =
            std::fs::OpenOptions::new().create(true).append(true).open(&self.path)
        {
            let _ = write!(f, "{text}");
        }
    }

    fn iteration_start(&mut self, i: usize, total: usize) {
        self.step += 1;
        self.write(&format!("\n---\n## Iteration {i}/{total}  (step {})\n", self.step));
    }

    fn llm_request(&self, n_messages: usize, n_tools: usize, model: &str) {
        self.write(&format!(
            "### LLM Request\n- Model: `{model}`\n- Messages in context: {n_messages}\n- Tools available: {n_tools}\n"
        ));
    }

    fn llm_response(&self, text: &str, tool_calls: &[Value]) {
        self.write("### LLM Response\n");
        if !text.is_empty() {
            let preview = if text.len() > 500 {
                format!("{}...", &text[..500])
            } else {
                text.to_owned()
            };
            self.write(&format!("**Text** ({} chars):\n```\n{preview}\n```\n", text.len()));
        }
        if !tool_calls.is_empty() {
            self.write(&format!("**Tool calls**: {}\n", tool_calls.len()));
        } else {
            self.write("**No tool calls** — generation complete.\n");
        }
    }

    fn tool_call(&self, name: &str, _args: &Value, result: &ToolResult) {
        let status = if result.is_error { "ERROR" } else { "OK" };
        let ms = result.elapsed.as_millis();
        self.write(&format!("\n#### Tool: `{name}` [{status}] ({ms}ms)\n"));
        let preview = if result.content.len() > 1000 {
            format!("{}\n... [truncated]", &result.content[..1000])
        } else {
            result.content.clone()
        };
        self.write(&format!("**Result:**\n```\n{preview}\n```\n"));
    }

    fn permission_denied(&self, name: &str, reason: &str) {
        self.write(&format!("\n#### Tool: `{name}` [DENIED]\n**Reason:** {reason}\n"));
    }

    fn iteration_end(&self, files: &[String]) {
        if !files.is_empty() {
            let list: Vec<String> = files[..files.len().min(30)]
                .iter()
                .map(|f| format!("`{f}`"))
                .collect();
            self.write(&format!("\n**Workspace files:** {}\n", list.join(", ")));
        }
    }

    fn loop_end(&self, result: &TurnResult) {
        let now = chrono::Utc::now().to_rfc3339();
        self.write(&format!(
            "\n---\n## Summary\n\
             - Iterations: {}\n\
             - Tool calls: {}\n\
             - Files produced: {:?}\n\
             - Errors: {}\n\
             - Elapsed: {:.1}s\n\
             \nCompleted: {now}\n",
            result.iterations,
            result.tool_calls,
            {
                let mut keys: Vec<_> = result.artifacts_produced.keys().collect();
                keys.sort();
                keys
            },
            result.errors.len(),
            result.elapsed_sec,
        ));
    }
}

// ---------------------------------------------------------------------------
// AgentTurnLoop
// ---------------------------------------------------------------------------

/// Generic agentic turn loop: LLM iteratively calls tools until done.
pub struct AgentTurnLoop {
    llm_config: LlmConfig,
    workspace: PathBuf,
    system_prompt: String,
    max_iterations: usize,
    messages: Vec<Value>,
    tool_specs: Vec<ToolSpec>,
    verification_hooks: Vec<VerificationHook>,
    executor: ToolExecutor,
    http_client: reqwest::Client,
    trace: Option<TraceLog>,
}

impl AgentTurnLoop {
    /// Create a new turn loop.
    ///
    /// `workspace` is the working directory for tool execution.
    /// `system_prompt` is prepended to every LLM call.
    pub fn new(
        llm_config: LlmConfig,
        workspace: impl Into<PathBuf>,
        system_prompt: impl Into<String>,
    ) -> Self {
        let ws: PathBuf = workspace.into();
        let executor = ToolExecutor::new(ws.clone());
        Self {
            llm_config,
            workspace: ws,
            system_prompt: system_prompt.into(),
            max_iterations: MAX_ITERATIONS,
            messages: Vec::new(),
            tool_specs: default_tool_specs(),
            verification_hooks: Vec::new(),
            executor,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .unwrap_or_default(),
            trace: None,
        }
    }

    /// Override max iterations (default: 40).
    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Override tool specs.
    pub fn with_tool_specs(mut self, specs: Vec<ToolSpec>) -> Self {
        self.tool_specs = specs;
        self
    }

    /// Override the permission policy on the executor.
    pub fn with_policy(mut self, policy: PermissionPolicy) -> Self {
        self.executor = self.executor.with_policy(policy);
        self
    }

    /// Override bash timeout.
    pub fn with_bash_timeout(mut self, secs: u64) -> Self {
        self.executor = self.executor.with_timeout(secs);
        self
    }

    /// Add a verification hook (one-shot, fires once then is removed).
    pub fn add_hook(mut self, hook: VerificationHook) -> Self {
        self.verification_hooks.push(hook);
        self
    }

    /// Enable trace logging to `{workspace_parent}/{prefix}_trace.md`.
    pub fn with_trace(mut self, prefix: &str) -> Self {
        let trace_dir = self
            .workspace
            .parent()
            .filter(|p| p.is_dir())
            .unwrap_or(&self.workspace)
            .to_path_buf();
        self.trace = Some(TraceLog::new(&trace_dir, prefix));
        self
    }

    /// The workspace directory used by this loop.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Run the agentic turn loop from a single user message.
    ///
    /// Returns [`TurnResult`] with all files produced and loop statistics.
    pub async fn run(
        &mut self,
        initial_message: &str,
    ) -> Result<TurnResult> {
        let t0 = Instant::now();
        info!("[mol_engine] Turn loop started, workspace={}", self.workspace.display());

        self.messages.push(json!({ "role": "user", "content": initial_message }));

        let mut result = TurnResult::default();

        // Build API tool list once
        let api_tools: Vec<Value> = self.tool_specs.iter().map(|s| s.to_api_tool()).collect();

        'outer: for iteration in 0..self.max_iterations {
            let iter_num = iteration + 1;

            if let Some(t) = &mut self.trace {
                t.iteration_start(iter_num, self.max_iterations);
            }
            if let Some(t) = &self.trace {
                t.llm_request(self.messages.len(), api_tools.len(), &self.llm_config.model);
            }

            info!(
                "[mol_engine] Iteration {iter_num}/{}: calling LLM ({} messages)...",
                self.max_iterations,
                self.messages.len()
            );

            // LLM call with retries
            let response = {
                let mut resp_opt: Option<Value> = None;
                for retry in 0..MAX_LLM_RETRIES {
                    match self.call_llm(&api_tools).await {
                        Ok(r) => {
                            let comp = r
                                .get("usage")
                                .and_then(|u| u.get("completion_tokens"))
                                .and_then(Value::as_u64)
                                .unwrap_or(u64::MAX);
                            if comp == 0 {
                                warn!("[mol_engine] Empty completion (attempt {}/{})", retry + 1, MAX_LLM_RETRIES);
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    2u64.pow(retry.min(3) as u32),
                                ))
                                .await;
                                continue;
                            }
                            resp_opt = Some(r);
                            break;
                        }
                        Err(e) => {
                            warn!("[mol_engine] LLM attempt {}/{} failed: {e}", retry + 1, MAX_LLM_RETRIES);
                            if retry + 1 < MAX_LLM_RETRIES {
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    2u64.pow(retry.min(3) as u32),
                                ))
                                .await;
                            } else {
                                let msg = format!("LLM failed after {MAX_LLM_RETRIES} retries at iter {iter_num}: {e}");
                                result.errors.push(msg);
                                break 'outer;
                            }
                        }
                    }
                }
                match resp_opt {
                    Some(r) => r,
                    None => break,
                }
            };

            result.iterations = iter_num;

            let (assistant_text, tool_uses) = parse_response(&response, &self.tool_specs);

            if let Some(t) = &self.trace {
                t.llm_response(&assistant_text, &tool_uses);
            }
            if !assistant_text.is_empty() {
                result.response = assistant_text.clone();
                debug!("[mol_engine] Iter {iter_num}: text {} chars", assistant_text.len());
            }

            // Append assistant message to history
            let assistant_msg = response
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .cloned()
                .unwrap_or_else(|| json!({ "role": "assistant", "content": assistant_text }));
            self.messages.push(assistant_msg);

            if tool_uses.is_empty() {
                info!("[mol_engine] Iter {iter_num}: no tool calls — loop complete");
                break;
            }

            info!(
                "[mol_engine] Iter {iter_num}: {} tool call(s): {:?}",
                tool_uses.len(),
                tool_uses
                    .iter()
                    .filter_map(|tu| tu.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
                    .collect::<Vec<_>>()
            );

            // Execute each tool call
            for tu in &tool_uses {
                let tool_name = tu
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let tool_id = tu.get("id").and_then(Value::as_str).unwrap_or("call_0");
                let args: Value = tu
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(Value::as_str)
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_else(|| json!({}));

                result.tool_calls += 1;

                // Permission check
                if let Err(e) = self.executor.policy.check_permission(tool_name, &args) {
                    let reason = e.to_string();
                    info!("[mol_engine]   DENIED {tool_name}: {reason}");
                    if let Some(t) = &self.trace {
                        t.permission_denied(tool_name, &reason);
                    }
                    self.messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_id,
                        "content": format!("PERMISSION DENIED: {reason}"),
                    }));
                    continue;
                }

                // Execute
                info!("[mol_engine]   Executing {tool_name}...");
                let tool_result = self.executor.execute(tool_name, args).await?;

                if let Some(t) = &self.trace {
                    t.tool_call(tool_name, tu, &tool_result);
                }

                let level = if tool_result.is_error { "ERROR" } else { "OK" };
                info!(
                    "[mol_engine]   {tool_name} {level} ({}ms, {} chars)",
                    tool_result.elapsed.as_millis(),
                    tool_result.content.len()
                );

                self.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_id,
                    "content": tool_result.content,
                }));
            }

            // List workspace files for hooks and trace
            let ws_files = list_workspace_files(&self.workspace);
            if let Some(t) = &self.trace {
                t.iteration_end(&ws_files);
            }

            // Run verification hooks (one-shot)
            let mut fired: Vec<usize> = Vec::new();
            for (idx, hook) in self.verification_hooks.iter().enumerate() {
                let inject = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    hook(&self.workspace, &tool_uses, &ws_files)
                }))
                .ok()
                .flatten();

                if let Some(msg) = inject {
                    info!("[mol_engine] Hook {idx} fired, injecting message");
                    self.messages.push(json!({ "role": "user", "content": msg }));
                    fired.push(idx);
                }
            }
            // Remove fired hooks (in reverse order to preserve indices)
            for idx in fired.iter().rev() {
                drop(self.verification_hooks.remove(*idx));
            }
        }

        // Collect workspace files
        result.artifacts_produced = collect_workspace_files(&self.workspace);
        result.elapsed_sec = t0.elapsed().as_secs_f64();

        if let Some(t) = &self.trace {
            t.loop_end(&result);
        }

        info!(
            "[mol_engine] Turn loop done: {} iters, {} tool calls, {} files, {:.1}s",
            result.iterations,
            result.tool_calls,
            result.artifacts_produced.len(),
            result.elapsed_sec,
        );

        // Persist conversation log
        let _ = self.save_conversation_log();

        Ok(result)
    }

    // -----------------------------------------------------------------------
    // LLM API call
    // -----------------------------------------------------------------------

    async fn call_llm(&self, api_tools: &[Value]) -> Result<Value> {
        let base_url = self.llm_config.base_url.trim_end_matches('/');
        let url = format!("{base_url}/chat/completions");

        let mut messages = vec![
            json!({ "role": "system", "content": self.system_prompt }),
        ];
        messages.extend_from_slice(&self.messages);

        let body = json!({
            "model": self.llm_config.model,
            "messages": messages,
            "max_tokens": self.llm_config.max_tokens,
            "tools": api_tools,
            "tool_choice": "auto",
        });

        let resp = self
            .http_client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.llm_config.timeout_sec))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.llm_config.api_key))
            .json(&body)
            .send()
            .await
            .context("HTTP request to LLM API failed")?;

        let status = resp.status();
        let bytes = resp.bytes().await.context("Failed to read LLM response body")?;

        if !status.is_success() {
            let text = String::from_utf8_lossy(&bytes);
            return Err(anyhow::anyhow!("LLM API error {status}: {text}"));
        }

        let data: Value = serde_json::from_slice(&bytes).context("Failed to parse LLM JSON")?;
        Ok(data)
    }

    // -----------------------------------------------------------------------
    // Conversation log
    // -----------------------------------------------------------------------

    fn save_conversation_log(&self) -> Result<()> {
        let trace_dir = self
            .workspace
            .parent()
            .filter(|p| p.is_dir())
            .unwrap_or(&self.workspace)
            .to_path_buf();

        // Full log
        let full_path = trace_dir.join("turn_loop_conversation_full.json");
        let full_json = serde_json::to_string_pretty(&self.messages)?;
        std::fs::write(full_path, full_json)?;

        // Truncated log (safe for review)
        let truncated: Vec<Value> = self
            .messages
            .iter()
            .map(|msg| {
                let mut m = msg.clone();
                if let Some(content) = m.get("content").and_then(Value::as_str) {
                    if content.len() > 3000 {
                        m["content"] = json!(format!("{}... [{} total chars]", &content[..3000], content.len()));
                    }
                }
                m
            })
            .collect();
        let log_path = trace_dir.join("turn_loop_conversation.json");
        let log_json = serde_json::to_string_pretty(&truncated)?;
        std::fs::write(log_path, log_json)?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

/// Parse the LLM response into `(assistant_text, tool_uses)`.
///
/// If the API returns no structured tool calls but the text contains a
/// JSON tool call blob, we attempt to recover it (text-based fallback).
fn parse_response(data: &Value, specs: &[ToolSpec]) -> (String, Vec<Value>) {
    let choices = data.get("choices").and_then(Value::as_array);
    let message = choices
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"));

    let text = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    let mut tool_calls: Vec<Value> = message
        .and_then(|m| m.get("tool_calls"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Fallback: try to recover tool calls embedded in text
    if tool_calls.is_empty() && !text.trim().is_empty() {
        if let Some(recovered) = try_recover_tool_calls(&text, specs) {
            if !recovered.is_empty() {
                debug!("[mol_engine] Recovered {} tool call(s) from text", recovered.len());
                tool_calls = recovered;
            }
        }
    }

    (text, tool_calls)
}

/// Attempt to parse JSON tool call objects embedded in assistant text.
fn try_recover_tool_calls(text: &str, specs: &[ToolSpec]) -> Option<Vec<Value>> {
    let stripped = text.trim();

    // Try top-level JSON parse first
    let blobs: Vec<Value> = if let Ok(v) = serde_json::from_str::<Value>(stripped) {
        match v {
            Value::Array(arr) => arr,
            obj @ Value::Object(_) => vec![obj],
            _ => Vec::new(),
        }
    } else {
        // Try to find individual JSON objects with a "tool" key
        let mut found = Vec::new();
        let re = regex::Regex::new(r#"\{[^{}]*"tool"\s*:\s*"[^"]+?"[^{}]*\}"#).ok()?;
        for m in re.find_iter(stripped) {
            if let Ok(v) = serde_json::from_str::<Value>(m.as_str()) {
                found.push(v);
            }
        }
        found
    };

    let mut results = Vec::new();
    for blob in blobs {
        if let Value::Object(obj) = &blob {
            let tool_name = obj
                .get("tool")
                .or_else(|| obj.get("name"))
                .and_then(|v| v.as_str())?;

            if !specs.iter().any(|s| s.name == tool_name) {
                continue;
            }

            let params = obj
                .get("parameters")
                .or_else(|| obj.get("arguments"))
                .or_else(|| obj.get("input"))
                .cloned()
                .unwrap_or_else(|| json!({}));

            let args_str = serde_json::to_string(&params).unwrap_or_else(|_| "{}".to_owned());
            let id = format!("call_{:x}", uuid_hex());

            results.push(json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": tool_name,
                    "arguments": args_str,
                }
            }));
        }
    }

    if results.is_empty() { None } else { Some(results) }
}

fn uuid_hex() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    h.finish()
}

// ---------------------------------------------------------------------------
// Workspace helpers
// ---------------------------------------------------------------------------

fn list_workspace_files(workspace: &Path) -> Vec<String> {
    let mut result = Vec::new();
    let skip: &[&str] = SKIP_DIRS;
    fn walk(dir: &Path, skip: &[&str], out: &mut Vec<String>, ws: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if name.starts_with('.') || skip.contains(&name.as_ref()) {
                continue;
            }
            if path.is_dir() {
                walk(&path, skip, out, ws);
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(ws) {
                    out.push(rel.to_string_lossy().into_owned());
                }
            }
        }
    }
    walk(workspace, skip, &mut result, workspace);
    result.sort();
    result
}

fn collect_workspace_files(workspace: &Path) -> HashMap<String, String> {
    let mut files = HashMap::new();
    let extensions: std::collections::HashSet<&str> = COLLECT_EXTENSIONS.iter().copied().collect();
    let skip: &[&str] = SKIP_DIRS;

    fn walk(
        dir: &Path,
        skip: &[&str],
        exts: &std::collections::HashSet<&str>,
        out: &mut HashMap<String, String>,
        ws: &Path,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if name.starts_with('.') || skip.contains(&name.as_ref()) {
                continue;
            }
            if path.is_dir() {
                walk(&path, skip, exts, out, ws);
            } else if path.is_file() {
                let ext = path
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                if !exts.contains(ext.as_str()) {
                    continue;
                }
                if path.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > 2 * 1024 * 1024 {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(rel) = path.strip_prefix(ws) {
                        out.insert(rel.to_string_lossy().into_owned(), content);
                    }
                }
            }
        }
    }

    walk(workspace, skip, &extensions, &mut files, workspace);
    files
}
