//! Unified LLM provider trait — abstracts API-based and CLI-based backends.
//!
//! The [`LlmProvider`] trait provides a common interface for both the
//! OpenAI-compatible [`LLMClient`] and the CLI-based [`ACPClient`].
//! The [`create_provider`] factory function picks the best available backend
//! based on the configuration.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::info;

use crate::acp::{ACPClient, ACPConfig};
use crate::client::{LLMClient, LLMClientConfig, Message};
use crate::response::LLMResponse;
use mol_config::MolConfig;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Unified LLM provider trait — abstracts API-based and CLI-based backends.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send chat messages and get a response.
    async fn chat(&self, messages: &[Message], json_mode: bool) -> Result<LLMResponse>;

    /// Check if the provider is available and configured.
    async fn preflight(&self) -> Result<String>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// API provider (LLMClient)
// ---------------------------------------------------------------------------

#[async_trait]
impl LlmProvider for LLMClient {
    async fn chat(&self, messages: &[Message], json_mode: bool) -> Result<LLMResponse> {
        self.chat(messages, json_mode, None).await
    }

    async fn preflight(&self) -> Result<String> {
        let result = LLMClient::preflight(self).await;
        if result.success {
            Ok(result.message)
        } else {
            anyhow::bail!("API preflight failed: {}", result.message)
        }
    }

    fn name(&self) -> &str {
        "api"
    }
}

// ---------------------------------------------------------------------------
// CLI provider (ACPClient wrapper)
// ---------------------------------------------------------------------------

/// Wrapper around [`ACPClient`] to satisfy the [`LlmProvider`] trait.
///
/// `ACPClient` requires `&mut self` for invoke/preflight, so we use a
/// `Mutex` for interior mutability.
pub struct CliProvider {
    inner: Mutex<ACPClient>,
}

impl CliProvider {
    pub fn new(client: ACPClient) -> Self {
        Self {
            inner: Mutex::new(client),
        }
    }
}

#[async_trait]
impl LlmProvider for CliProvider {
    async fn chat(&self, messages: &[Message], _json_mode: bool) -> Result<LLMResponse> {
        let mut client = self.inner.lock().await;
        client.invoke(messages, None).await
    }

    async fn preflight(&self) -> Result<String> {
        let mut client = self.inner.lock().await;
        let result = client.preflight().await;
        if result.success {
            Ok(result.message)
        } else {
            anyhow::bail!("CLI preflight failed: {}", result.message)
        }
    }

    fn name(&self) -> &str {
        "cli"
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create the best available LLM provider based on config.
///
/// Priority:
/// 1. Explicit `provider` setting in config (respected unconditionally)
///    - `"acp"` → CLI provider via acpx
///    - `"api"` → API provider (requires key)
///    - `"none"` → no provider
/// 2. Auto-detect: API key in env → API; CLI tool on PATH → CLI
/// 3. Neither → returns `None`
pub fn create_provider(config: &MolConfig) -> Option<Arc<dyn LlmProvider>> {
    let llm = &config.llm;

    // Respect explicit provider choice first (Friction Fix #1: user config > auto-detect)
    match llm.provider.as_str() {
        "acp" => {
            return create_cli_provider(llm);
        }
        "none" => {
            info!("LLM provider: none (disabled by config)");
            return None;
        }
        "api" => {
            // Fall through to API key resolution below
        }
        _ => {
            // Empty or unrecognized → auto-detect (legacy behavior)
        }
    }

    // Try API mode — resolve key from env var or direct config
    let is_placeholder = |k: &str| k.is_empty() || k.starts_with("__") || k == "sk-placeholder";
    let api_key = if !is_placeholder(&llm.api_key) {
        Some(llm.api_key.clone())
    } else if !llm.api_key_env.is_empty() {
        std::env::var(&llm.api_key_env).ok().filter(|k| !k.is_empty())
    } else {
        // Common env vars
        std::env::var("OPENAI_API_KEY")
            .ok()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .filter(|k| !k.is_empty())
    };

    if let Some(key) = api_key {
        let client_config = LLMClientConfig {
            base_url: if llm.base_url.is_empty() {
                "https://api.openai.com/v1".to_owned()
            } else {
                llm.base_url.clone()
            },
            api_key: key,
            primary_model: if llm.primary_model.is_empty() {
                "gpt-4o".to_owned()
            } else {
                llm.primary_model.clone()
            },
            fallback_models: llm.fallback_models.clone(),
            ..Default::default()
        };
        match LLMClient::new(client_config) {
            Ok(client) => {
                info!(provider = "api", model = %llm.primary_model, "LLM provider: API");
                return Some(Arc::new(client));
            }
            Err(e) => {
                tracing::warn!("Failed to create API client: {e}");
            }
        }
    }

    // Auto-detect CLI tool on PATH
    create_cli_provider(llm)
        .or_else(|| {
            info!("LLM provider: none (no API key or CLI tool found)");
            None
        })
}

/// Try to create a CLI (ACP) provider from config or auto-detected CLI tools.
fn create_cli_provider(llm: &mol_config::LlmConfig) -> Option<Arc<dyn LlmProvider>> {
    let agent = if !llm.acp.agent.is_empty() {
        Some(llm.acp.agent.clone())
    } else if which::which("claude").is_ok() {
        Some("claude".to_string())
    } else if which::which("codex").is_ok() {
        Some("codex".to_string())
    } else if which::which("opencode").is_ok() {
        Some("opencode".to_string())
    } else {
        None
    };

    agent.map(|agent| {
        let acp_config = ACPConfig {
            agent: agent.clone(),
            cwd: PathBuf::from(&llm.acp.cwd),
            acpx_command: llm.acp.acpx_command.clone(),
            session_name: llm.acp.session_name.clone(),
            timeout_sec: llm.acp.timeout_sec as u64,
            ..Default::default()
        };
        let client = ACPClient::new(acp_config);
        info!(provider = "cli", agent = %agent, "LLM provider: CLI");
        Arc::new(CliProvider::new(client)) as Arc<dyn LlmProvider>
    })
}
