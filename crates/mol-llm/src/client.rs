//! Main LLM client — OpenAI-compatible with model fallback, retries, and
//! optional Anthropic backend.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::anthropic::AnthropicAdapter;
use crate::response::{LLMResponse, PreflightResult};
use crate::retry::call_with_retry;

// ---------------------------------------------------------------------------
// Model routing constants
// ---------------------------------------------------------------------------

/// Models that use `max_completion_tokens` (OpenAI o-series + gpt-5.4).
///
/// Checked before [`RESPONSES_API_MODELS`] so that `gpt-5.4` does not
/// accidentally match the `gpt-5` prefix in the latter set.
pub const NEW_PARAM_MODELS: &[&str] = &["o3", "o3-mini", "o4-mini", "gpt-5.4"];

/// Models routed through the OpenAI Responses API that use `max_output_tokens`.
pub const RESPONSES_API_MODELS: &[&str] = &["gpt-5", "gpt-5.1", "gpt-5.2"];

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const JSON_MODE_INSTRUCTION: &str =
    "You MUST respond with valid JSON only. \
     Do not include any text outside the JSON object.";

// ---------------------------------------------------------------------------
// Message type
// ---------------------------------------------------------------------------

/// A chat message sent to or received from the LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    System(String),
    User(String),
    Assistant(String),
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::System(content.into())
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::User(content.into())
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant(content.into())
    }

    pub fn role(&self) -> &'static str {
        match self {
            Self::System(_) => "system",
            Self::User(_) => "user",
            Self::Assistant(_) => "assistant",
        }
    }

    pub fn content(&self) -> &str {
        match self {
            Self::System(c) | Self::User(c) | Self::Assistant(c) => c,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for [`LLMClient`].
#[derive(Debug, Clone)]
pub struct LLMClientConfig {
    /// Base URL for the OpenAI-compatible API endpoint.
    pub base_url: String,

    /// API key (Bearer token).
    pub api_key: String,

    /// Primary model name.
    pub primary_model: String,

    /// Fallback models tried in order when the primary model fails.
    pub fallback_models: Vec<String>,

    /// Maximum tokens to generate per response.
    pub max_tokens: u32,

    /// Sampling temperature.
    pub temperature: f64,

    /// Maximum number of retry attempts per model.
    pub max_retries: u32,

    /// Base delay for exponential backoff.
    pub retry_base_delay: Duration,

    /// Per-request timeout.
    pub timeout: Duration,

    /// User-Agent header value (for Cloudflare bypass).
    pub user_agent: String,

    /// Extra headers forwarded with every request (e.g. MetaMol proxy headers).
    pub extra_headers: Vec<(String, String)>,

    /// Fallback base URL used when the primary endpoint is unreachable.
    ///
    /// Set automatically when the MetaMol bridge is enabled.
    pub fallback_url: String,

    /// API key for the fallback endpoint (falls back to [`api_key`] if empty).
    pub fallback_api_key: String,
}

impl Default for LLMClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_owned(),
            api_key: String::new(),
            primary_model: "gpt-4o".to_owned(),
            fallback_models: vec!["gpt-4.1".to_owned(), "gpt-4o-mini".to_owned()],
            max_tokens: 4096,
            temperature: 0.7,
            max_retries: 5,
            retry_base_delay: Duration::from_secs(3),
            timeout: Duration::ZERO,  // no timeout by default; research tasks need unlimited time
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            extra_headers: Vec::new(),
            fallback_url: String::new(),
            fallback_api_key: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// LLMClient
// ---------------------------------------------------------------------------

/// Stateless OpenAI-compatible chat completion client with model fallback,
/// retry, and optional Anthropic backend support.
pub struct LLMClient {
    config: LLMClientConfig,
    /// Full model chain: [primary, fallback0, fallback1, …]
    model_chain: Vec<String>,
    /// Anthropic adapter, present only when provider == "anthropic".
    anthropic: Option<AnthropicAdapter>,
    /// Shared reqwest client (connection pool).
    http: Client,
}

impl LLMClient {
    /// Create a new client from the given configuration.
    pub fn new(config: LLMClientConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent(&config.user_agent);
        // Only set timeout if non-zero; 0 = no timeout (long-running research tasks)
        if !config.timeout.is_zero() {
            builder = builder.timeout(config.timeout);
        }
        let http = builder.build()?;

        let model_chain = std::iter::once(config.primary_model.clone())
            .chain(config.fallback_models.iter().cloned())
            .collect();

        Ok(Self {
            config,
            model_chain,
            anthropic: None,
            http,
        })
    }

    /// Attach an Anthropic adapter (used when provider == "anthropic").
    pub fn with_anthropic(mut self, adapter: AnthropicAdapter) -> Self {
        self.anthropic = Some(adapter);
        self
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Send a chat request with model fallback and retry.
    ///
    /// - `model_override`: if provided, skips the fallback chain and uses only
    ///   this model.
    /// - `json_mode`: request JSON output (via `response_format` for OpenAI,
    ///   or system-prompt injection for incompatible providers).
    pub async fn chat(
        &self,
        messages: &[Message],
        json_mode: bool,
        model_override: Option<&str>,
    ) -> Result<LLMResponse> {
        let models: Vec<String> = if let Some(m) = model_override {
            vec![m.to_owned()]
        } else {
            self.model_chain.clone()
        };

        let mut last_err: Option<anyhow::Error> = None;

        for (idx, model) in models.iter().enumerate() {
            match self.call_single_model(model, messages, json_mode, self.config.max_tokens).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    warn!(model = %model, error = %e, "Model call failed, trying next");
                    let is_rate_or_conn = {
                        let msg = e.to_string();
                        msg.contains("429")
                            || msg.contains("connection")
                            || msg.contains("timed out")
                    };
                    if is_rate_or_conn && idx + 1 < models.len() {
                        let delay = self.config.retry_base_delay
                            * 2u32.saturating_pow(idx as u32);
                        info!(
                            model = %model,
                            next = %models[idx + 1],
                            delay_ms = delay.as_millis(),
                            "Rate-limit/connection error; waiting before next model"
                        );
                        tokio::time::sleep(delay).await;
                    }
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("All models failed")))
    }

    /// Strip `<think>…</think>` blocks from a response content string.
    pub fn strip_thinking(content: &str) -> String {
        let re = regex::Regex::new(r"(?is)<think>.*?</think>").unwrap();
        re.replace_all(content, "").trim().to_owned()
    }

    /// Quick connectivity check — one minimal chat call.
    ///
    /// Distinguishes: 401 (bad key), 403 (model forbidden), 404 (bad
    /// endpoint), 429 (rate limited), timeout.
    pub async fn preflight(&self) -> PreflightResult {
        let is_reasoning = NEW_PARAM_MODELS
            .iter()
            .any(|p| self.config.primary_model.starts_with(p));
        let min_tokens: u32 = if is_reasoning { 64 } else { 1 };

        let ping = vec![Message::user("ping")];
        match self.call_single_model(
            &self.config.primary_model,
            &ping,
            false,
            min_tokens,
        )
        .await
        {
            Ok(_) => PreflightResult::ok(format!(
                "OK - model {} responding",
                self.config.primary_model
            )),
            Err(e) => {
                let msg = e.to_string();
                let friendly = if msg.contains("401") {
                    "Invalid API key".to_owned()
                } else if msg.contains("403") {
                    format!(
                        "Model {} not allowed for this key",
                        self.config.primary_model
                    )
                } else if msg.contains("404") {
                    format!("Endpoint not found: {}", self.config.base_url)
                } else if msg.contains("429") {
                    "Rate limited - try again in a moment".to_owned()
                } else {
                    format!("Connection failed: {msg}")
                };
                PreflightResult::fail(friendly)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    /// Call a single model with retry, dispatching to Anthropic or OpenAI path.
    async fn call_single_model(
        &self,
        model: &str,
        messages: &[Message],
        json_mode: bool,
        max_tokens: u32,
    ) -> Result<LLMResponse> {
        if let Some(anthropic) = &self.anthropic {
            // Anthropic path: direct call (no reqwest::Response wrapping needed).
            return anthropic
                .chat_completion(model, messages, max_tokens, self.config.temperature, json_mode)
                .await;
        }

        // OpenAI-compatible path: use call_with_retry over reqwest::Response.
        let response = call_with_retry(
            |_attempt| self.build_and_send(model, messages, max_tokens, json_mode),
            self.config.max_retries,
            self.config.retry_base_delay,
        )
        .await?;

        parse_openai_response(response, model).await
    }

    /// Build the request body and POST it, returning the raw reqwest Response.
    async fn build_and_send(
        &self,
        model: &str,
        messages: &[Message],
        max_tokens: u32,
        json_mode: bool,
    ) -> Result<reqwest::Response> {
        // Copy messages to avoid mutating the caller's list across retries.
        let mut msgs: Vec<Value> = messages
            .iter()
            .map(|m| json!({"role": m.role(), "content": m.content()}))
            .collect();

        // JSON mode prompt injection for non-OpenAI providers.
        if json_mode {
            let use_prompt_injection = model.starts_with("claude")
                || model.starts_with("deepseek")
                || self.config.base_url.to_lowercase().contains("deepseek");

            if use_prompt_injection {
                if let Some(first) = msgs.first_mut() {
                    if first["role"].as_str() == Some("system") {
                        let prev = first["content"].as_str().unwrap_or("").to_owned();
                        first["content"] =
                            json!(format!("{JSON_MODE_INSTRUCTION}\n\n{prev}"));
                    } else {
                        msgs.insert(
                            0,
                            json!({"role": "system", "content": JSON_MODE_INSTRUCTION}),
                        );
                    }
                } else {
                    msgs.insert(
                        0,
                        json!({"role": "system", "content": JSON_MODE_INSTRUCTION}),
                    );
                }
            }
        }

        let mut body = json!({
            "model": model,
            "messages": msgs,
            "temperature": self.config.temperature,
            "stream": true,
        });

        // Select the correct max-tokens parameter for the model.
        // Check NEW_PARAM_MODELS first — "gpt-5.4" must NOT fall through to
        // RESPONSES_API_MODELS whose "gpt-5" prefix would also match.
        if NEW_PARAM_MODELS.iter().any(|p| model.starts_with(p)) {
            let reasoning_min = 32768u32;
            body["max_completion_tokens"] = json!(max_tokens.max(reasoning_min));
        } else if RESPONSES_API_MODELS.iter().any(|p| model.starts_with(p)) {
            body["max_output_tokens"] = json!(max_tokens.max(32768));
        } else {
            body["max_tokens"] = json!(max_tokens);
        }

        if json_mode {
            let use_prompt_injection = model.starts_with("claude")
                || model.starts_with("deepseek")
                || self.config.base_url.to_lowercase().contains("deepseek");
            if !use_prompt_injection {
                body["response_format"] = json!({"type": "json_object"});
            }
        }

        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        let mut req = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json");

        for (k, v) in &self.config.extra_headers {
            req = req.header(k, v);
        }

        req = req.json(&body);

        match req.send().await {
            Ok(r) => Ok(r),
            Err(e) if !self.config.fallback_url.is_empty() => {
                warn!(
                    error = %e,
                    fallback = %self.config.fallback_url,
                    "Primary endpoint unreachable, using fallback"
                );
                self.send_to_fallback(body).await
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn send_to_fallback(&self, body: Value) -> Result<reqwest::Response> {
        let url = format!(
            "{}/chat/completions",
            self.config.fallback_url.trim_end_matches('/')
        );
        let key = if self.config.fallback_api_key.is_empty() {
            &self.config.api_key
        } else {
            &self.config.fallback_api_key
        };

        Ok(self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?)
    }
}

// ---------------------------------------------------------------------------
// OpenAI response parsing
// ---------------------------------------------------------------------------

/// Consume a streaming SSE or plain-JSON response and parse it into an
/// [`LLMResponse`].
async fn parse_openai_response(response: reqwest::Response, model: &str) -> Result<LLMResponse> {
    let mut chunks: Vec<String> = Vec::new();
    let mut finish_reason: Option<String> = None;
    let mut model_name = model.to_owned();
    let mut usage_json: Value = json!({});
    let mut raw_body: Option<Value> = None;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    if content_type.contains("text/event-stream") {
        // SSE streaming path
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk_bytes = chunk_result?;
            buffer.push_str(&String::from_utf8_lossy(&chunk_bytes));

            loop {
                match buffer.find('\n') {
                    None => break,
                    Some(pos) => {
                        let line = buffer[..pos].trim().to_owned();
                        buffer = buffer[pos + 1..].to_owned();

                        if line.is_empty() || !line.starts_with("data: ") {
                            continue;
                        }
                        let payload = &line[6..];
                        if payload == "[DONE]" {
                            break;
                        }
                        if let Ok(event) = serde_json::from_str::<Value>(payload) {
                            if let Some(m) = event.get("model").and_then(Value::as_str) {
                                if model_name == model {
                                    model_name = m.to_owned();
                                }
                            }
                            if let Some(u) = event.get("usage") {
                                usage_json = u.clone();
                            }
                            if let Some(choices) =
                                event.get("choices").and_then(Value::as_array)
                            {
                                for choice in choices {
                                    if let Some(c) = choice
                                        .get("delta")
                                        .and_then(|d| d.get("content"))
                                        .and_then(Value::as_str)
                                    {
                                        chunks.push(c.to_owned());
                                    }
                                    if finish_reason.is_none() {
                                        if let Some(fr) = choice
                                            .get("finish_reason")
                                            .and_then(Value::as_str)
                                        {
                                            if !fr.is_empty() {
                                                finish_reason = Some(fr.to_owned());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Non-streaming JSON response
        let data: Value = response.json().await?;

        if let Some(err) = data.get("error") {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            let typ = err
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("api_error");
            bail!("{typ}: {msg}");
        }

        if let Some(m) = data.get("model").and_then(Value::as_str) {
            model_name = m.to_owned();
        }
        if let Some(u) = data.get("usage") {
            usage_json = u.clone();
        }
        if let Some(choices) = data.get("choices").and_then(Value::as_array) {
            if let Some(choice) = choices.first() {
                if let Some(c) = choice
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(Value::as_str)
                {
                    chunks.push(c.to_owned());
                }
                if let Some(fr) = choice.get("finish_reason").and_then(Value::as_str) {
                    if !fr.is_empty() {
                        finish_reason = Some(fr.to_owned());
                    }
                }
            }
        }
        raw_body = Some(data);
    }

    let content = chunks.join("");

    let raw = raw_body.unwrap_or_else(|| {
        json!({
            "choices": [{"message": {"content": &content}, "finish_reason": &finish_reason}],
            "model": &model_name,
            "usage": &usage_json,
        })
    });

    let prompt_tokens = usage_json
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let completion_tokens = usage_json
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    let truncated = finish_reason.as_deref() == Some("length");

    Ok(LLMResponse {
        content,
        model: model_name,
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        finish_reason,
        truncated,
        raw: Some(raw),
    })
}
