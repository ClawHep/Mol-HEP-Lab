//! Mol-HEP-Lab: LLM provider clients and inference abstractions.
//!
//! # Overview
//!
//! This crate provides a unified async LLM client that supports:
//!
//! - OpenAI-compatible APIs (GPT-4o, o3, gpt-5.x, DeepSeek, …)
//! - Anthropic Messages API (Claude family)
//! - ACP (Agent Client Protocol) via the `acpx` subprocess bridge
//!
//! # Quick start
//!
//! ```rust,no_run
//! use mol_llm::{create_llm_client, LLMClientConfig, Message};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = LLMClientConfig {
//!         base_url: "https://api.openai.com/v1".into(),
//!         api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
//!         ..Default::default()
//!     };
//!     let client = create_llm_client(config)?;
//!     let messages = vec![Message::user("What is 2 + 2?")];
//!     let response = client.chat(&messages, false, None).await?;
//!     println!("{}", response.content);
//!     Ok(())
//! }
//! ```

pub mod acp;
pub mod anthropic;
pub mod client;
pub mod provider;
pub mod response;
pub mod retry;

// ---------------------------------------------------------------------------
// Flat re-exports for the most commonly used types
// ---------------------------------------------------------------------------

pub use acp::{ACPClient, ACPConfig};
pub use anthropic::AnthropicAdapter;
pub use client::{LLMClient, LLMClientConfig, Message, NEW_PARAM_MODELS, RESPONSES_API_MODELS};
pub use provider::{create_provider, CliProvider, LlmProvider};
pub use response::{LLMResponse, PreflightResult};

// ---------------------------------------------------------------------------
// Factory function
// ---------------------------------------------------------------------------

/// Create an [`LLMClient`] from a [`LLMClientConfig`].
///
/// This is the primary entry point for the crate.  For Anthropic-provider
/// configurations, attach an [`AnthropicAdapter`] via
/// [`LLMClient::with_anthropic`] after calling this function.
pub fn create_llm_client(config: LLMClientConfig) -> anyhow::Result<LLMClient> {
    LLMClient::new(config)
}

/// Convenience constructor: create an Anthropic-backed [`LLMClient`].
///
/// The OpenAI-compatible `config` is still used for model names, fallback
/// chain, and retry settings.  HTTP calls are routed through the Anthropic
/// Messages API adapter using `anthropic_base_url` and `anthropic_api_key`.
pub fn create_anthropic_client(
    config: LLMClientConfig,
    anthropic_base_url: impl Into<String>,
    anthropic_api_key: impl Into<String>,
) -> anyhow::Result<LLMClient> {
    let timeout_sec = config.timeout.as_secs();
    let adapter = AnthropicAdapter::new(anthropic_base_url, anthropic_api_key, timeout_sec);
    Ok(LLMClient::new(config)?.with_anthropic(adapter))
}
