//! MetaMol session lifecycle management.
//!
//! Manages a MetaMol proxy session that spans a single pipeline run.  The
//! session carries a unique identifier and can be used to inject stage-specific
//! skills into LLM requests via HTTP headers.

use anyhow::{Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::stage_skill_map::get_skills_for_stage;

/// Configuration for connecting to a MetaMol proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaMolConfig {
    /// Base URL of the MetaMol proxy (e.g. `http://localhost:30000`).
    pub proxy_url: String,

    /// API key forwarded to the MetaMol proxy.
    pub api_key: String,

    /// Path to the MetaMol skills directory (default: `~/.metamol/skills`).
    pub skills_dir: String,
}

impl Default for MetaMolConfig {
    fn default() -> Self {
        Self {
            proxy_url: "http://localhost:30000".to_owned(),
            api_key: String::new(),
            skills_dir: "~/.metamol/skills".to_owned(),
        }
    }
}

/// A MetaMol session spanning one pipeline run.
///
/// Create with [`MetaMolSession::start`] and terminate with [`MetaMolSession::end`].
#[derive(Debug, Clone)]
pub struct MetaMolSession {
    /// Base URL of the MetaMol proxy.
    pub proxy_url: String,

    /// API key for MetaMol proxy authentication.
    pub api_key: String,

    /// Unique identifier for this session (e.g. `"mol-<uuid>"`).
    pub session_id: String,

    /// Whether the session is still active.
    pub active: bool,

    http: Client,
}

impl MetaMolSession {
    /// Start a new MetaMol session.
    ///
    /// Generates a unique `session_id`, notifies the MetaMol proxy that a
    /// new session has begun (best-effort), and returns the ready session.
    pub async fn start(config: &MetaMolConfig) -> Result<Self> {
        let session_id = format!("mol-{}", Uuid::new_v4());
        let http = Client::new();

        let session = Self {
            proxy_url: config.proxy_url.trim_end_matches('/').to_owned(),
            api_key: config.api_key.clone(),
            session_id: session_id.clone(),
            active: true,
            http,
        };

        // Best-effort: notify proxy of session start.
        if let Err(e) = session.notify_lifecycle("start").await {
            warn!(error = %e, session_id, "MetaMol session start notification failed (non-fatal)");
        }

        info!(session_id, "MetaMol session started");
        Ok(session)
    }

    /// End the session and notify the MetaMol proxy.
    ///
    /// After calling this method `active` is set to `false`.  The proxy
    /// notification is best-effort; failure does not cause an error.
    pub async fn end(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;

        if let Err(e) = self.notify_lifecycle("end").await {
            warn!(
                error = %e,
                session_id = self.session_id,
                "MetaMol session end notification failed (non-fatal)"
            );
        }

        info!(session_id = self.session_id, "MetaMol session ended");
        Ok(())
    }

    /// Retrieve the skills applicable to `stage` from the MetaMol proxy.
    ///
    /// Falls back to the static [`get_skills_for_stage`] mapping if the proxy
    /// is unavailable or returns an empty list.
    pub async fn inject_skills(&self, stage: u32) -> Result<Vec<String>> {
        if !self.active {
            bail!("Cannot inject skills on an inactive MetaMol session");
        }

        // Try the proxy first.
        match self.fetch_skills_from_proxy(stage).await {
            Ok(skills) if !skills.is_empty() => {
                info!(stage, count = skills.len(), "MetaMol skills injected from proxy");
                return Ok(skills);
            }
            Ok(_) => {
                info!(stage, "MetaMol proxy returned no skills; using static map");
            }
            Err(e) => {
                warn!(error = %e, stage, "MetaMol proxy skill fetch failed; using static map");
            }
        }

        // Fallback: static mapping.
        let skills = get_skills_for_stage(stage);
        Ok(skills)
    }

    /// Build the HTTP headers that should be forwarded with LLM API requests
    /// so that the MetaMol proxy can track the stage.
    pub fn stage_headers(&self, stage: u32) -> std::collections::HashMap<String, String> {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Session-Id".to_owned(), self.session_id.clone());
        headers.insert("X-Turn-Type".to_owned(), "main".to_owned());
        headers.insert("X-MetaMol-Stage".to_owned(), stage.to_string());
        headers
    }

    // --- private helpers ---

    async fn notify_lifecycle(&self, event: &str) -> Result<()> {
        let url = format!("{}/metamol/session/{}", self.proxy_url, event);
        self.http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "session_id": self.session_id,
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn fetch_skills_from_proxy(&self, stage: u32) -> Result<Vec<String>> {
        let url = format!("{}/metamol/skills", self.proxy_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.api_key)
            .query(&[
                ("session_id", self.session_id.as_str()),
                ("stage", &stage.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?;

        #[derive(Deserialize)]
        struct SkillsResponse {
            skills: Vec<String>,
        }

        let body: SkillsResponse = resp.json().await?;
        Ok(body.skills)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_headers_contains_session_id() {
        let session = MetaMolSession {
            proxy_url: "http://localhost:30000".to_owned(),
            api_key: "key".to_owned(),
            session_id: "mol-test-123".to_owned(),
            active: true,
            http: Client::new(),
        };
        let headers = session.stage_headers(5);
        assert_eq!(headers.get("X-Session-Id").unwrap(), "mol-test-123");
        assert_eq!(headers.get("X-MetaMol-Stage").unwrap(), "5");
    }
}
