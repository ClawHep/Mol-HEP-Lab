//! LLM response types.

use serde::{Deserialize, Serialize};

/// Parsed response from an LLM API call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    /// The text content returned by the model.
    pub content: String,

    /// The model name as reported by the API.
    pub model: String,

    /// Number of prompt (input) tokens consumed.
    pub prompt_tokens: u32,

    /// Number of completion (output) tokens generated.
    pub completion_tokens: u32,

    /// Total tokens used (prompt + completion).
    pub total_tokens: u32,

    /// Why the model stopped generating (e.g. "stop", "length", "tool_calls").
    pub finish_reason: Option<String>,

    /// `true` when the finish reason was "length" (output was cut off).
    pub truncated: bool,

    /// The raw JSON response body from the API, when available.
    pub raw: Option<serde_json::Value>,
}

impl LLMResponse {
    /// Create a minimal response with only content and model set.
    pub fn simple(content: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            model: model.into(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            finish_reason: None,
            truncated: false,
            raw: None,
        }
    }
}

/// Result of a preflight connectivity check.
#[derive(Debug, Clone)]
pub struct PreflightResult {
    pub success: bool,
    pub message: String,
}

impl PreflightResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self { success: true, message: message.into() }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        Self { success: false, message: message.into() }
    }
}
