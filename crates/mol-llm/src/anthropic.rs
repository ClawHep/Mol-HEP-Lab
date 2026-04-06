//! Anthropic Messages API adapter.
//!
//! Converts OpenAI-format [`Message`] slices to Anthropic's request format,
//! calls the `/v1/messages` endpoint, and maps the response back to
//! [`LLMResponse`].

use anyhow::{Result, anyhow, bail};
use reqwest::Client;
use serde_json::{Value, json};
use tracing::debug;

use crate::client::Message;
use crate::response::LLMResponse;

const JSON_MODE_INSTRUCTION: &str =
    "You MUST respond with valid JSON only. \
     Do not include any text outside the JSON object.";

/// Maps Anthropic `stop_reason` values to OpenAI `finish_reason` strings.
fn map_stop_reason(stop_reason: &str) -> &'static str {
    match stop_reason {
        "end_turn" => "stop",
        "max_tokens" => "length",
        "stop_sequence" => "stop",
        "tool_use" => "tool_calls",
        _ => "stop",
    }
}

/// Anthropic Messages API adapter.
///
/// Constructed with the Anthropic base URL (typically
/// `https://api.anthropic.com`) and an Anthropic API key.  The adapter is
/// intentionally stateless — it holds only config.
pub struct AnthropicAdapter {
    pub base_url: String,
    pub api_key: String,
    pub timeout_sec: u64,
}

impl AnthropicAdapter {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        timeout_sec: u64,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
            timeout_sec,
        }
    }

    /// Call the Anthropic Messages API and return an [`LLMResponse`].
    pub async fn chat_completion(
        &self,
        model: &str,
        messages: &[Message],
        max_tokens: u32,
        temperature: f64,
        json_mode: bool,
    ) -> Result<LLMResponse> {
        // Split system messages from user/assistant messages
        let mut system_parts: Vec<String> = Vec::new();
        let mut chat_messages: Vec<Value> = Vec::new();

        for msg in messages {
            match msg {
                Message::System(content) => {
                    system_parts.push(content.clone());
                }
                Message::User(content) => {
                    chat_messages.push(json!({
                        "role": "user",
                        "content": content,
                    }));
                }
                Message::Assistant(content) => {
                    chat_messages.push(json!({
                        "role": "assistant",
                        "content": content,
                    }));
                }
            }
        }

        // Merge consecutive messages with the same role (Anthropic requires
        // strict user/assistant alternation).
        let chat_messages = merge_consecutive_roles(chat_messages);

        // Ensure there is at least one user message.
        let chat_messages = if chat_messages.is_empty() {
            vec![json!({"role": "user", "content": "Hello."})]
        } else {
            chat_messages
        };

        // Build system string, injecting JSON instruction when requested.
        let mut system_str = system_parts.join("\n\n");
        if json_mode {
            if system_str.is_empty() {
                system_str = JSON_MODE_INSTRUCTION.to_owned();
            } else {
                system_str = format!("{JSON_MODE_INSTRUCTION}\n\n{system_str}");
            }
        }

        // Build request body.
        let mut body = json!({
            "model": model,
            "messages": chat_messages,
            "max_tokens": max_tokens,
            "temperature": temperature,
        });
        if !system_str.is_empty() {
            body["system"] = json!(system_str);
        }

        let url = format!("{}/v1/messages", self.base_url);
        debug!(url, model, "Anthropic API call");

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_sec))
            .build()?;

        let response = client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let body_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error HTTP {status_code}: {body_text}"));
        }

        let data: Value = response.json().await?;

        // Check for Anthropic-level error objects in the response body.
        if data.get("type").and_then(Value::as_str) == Some("error")
            || data.get("error").is_some()
        {
            let err = data.get("error").cloned().unwrap_or(data.clone());
            let err_type = err
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("api_error");
            let err_msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            bail!("Anthropic API error {err_type}: {err_msg}");
        }

        // Extract text content blocks (concatenate all text blocks).
        let content = extract_text_content(&data);

        // Map stop_reason → finish_reason.
        let raw_stop = data
            .get("stop_reason")
            .and_then(Value::as_str)
            .unwrap_or("end_turn");
        let finish_reason = map_stop_reason(raw_stop).to_owned();
        let truncated = finish_reason == "length";

        let usage = data.get("usage").cloned().unwrap_or(json!({}));
        let prompt_tokens = usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let completion_tokens = usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        let reported_model = data
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(model)
            .to_owned();

        Ok(LLMResponse {
            content,
            model: reported_model,
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            finish_reason: Some(finish_reason),
            truncated,
            raw: Some(data),
        })
    }
}

/// Merge consecutive messages that share the same role, joining their content
/// with double newlines.  Anthropic requires strict alternation.
fn merge_consecutive_roles(messages: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for msg in messages {
        let role = msg["role"].as_str().unwrap_or("").to_owned();
        let content = msg["content"].as_str().unwrap_or("").to_owned();
        if let Some(last) = merged.last_mut() {
            if last["role"].as_str() == Some(&role) {
                let prev = last["content"].as_str().unwrap_or("").to_owned();
                last["content"] = json!(format!("{prev}\n\n{content}"));
                continue;
            }
        }
        merged.push(json!({"role": role, "content": content}));
    }
    merged
}

/// Concatenate all `text` content blocks from an Anthropic response.
fn extract_text_content(data: &Value) -> String {
    data.get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
