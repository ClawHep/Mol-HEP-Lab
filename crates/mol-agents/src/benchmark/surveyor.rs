//! Surveyor agent — searches HuggingFace and the web for relevant benchmarks.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, warn};

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};

// ---------------------------------------------------------------------------
// Benchmark candidate
// ---------------------------------------------------------------------------

/// A benchmark dataset discovered during the survey phase.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkCandidate {
    /// Short name (e.g. `"QM9"`).
    pub name: String,

    /// Source repository or URL where this benchmark was found.
    pub source_url: String,

    /// Which domain(s) this benchmark targets.
    pub domains: Vec<String>,

    /// Evaluation metrics reported by this benchmark.
    pub metrics: Vec<String>,

    /// How many examples / samples the dataset contains.
    pub size: Option<u64>,

    /// Whether this benchmark is available on HuggingFace.
    pub on_huggingface: bool,

    /// HuggingFace dataset identifier (e.g. `"owner/repo"`).
    pub hf_id: Option<String>,

    /// Star count for associated GitHub repositories.
    pub github_stars: Option<u32>,

    /// Free-form notes gathered during survey.
    pub notes: String,

    /// Raw metadata from the discovery source.
    pub raw: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// SurveyorConfig
// ---------------------------------------------------------------------------

/// Configuration for the [`SurveyorAgent`].
#[derive(Debug, Clone)]
pub struct SurveyorConfig {
    /// Whether to query HuggingFace datasets.
    pub enable_hf_search: bool,

    /// Maximum results to return from HuggingFace.
    pub max_hf_results: usize,

    /// Whether to additionally search the open web.
    pub enable_web_search: bool,

    /// Maximum web search results to consider.
    pub max_web_results: usize,

    /// Minimum number of local HF results before triggering web search fallback.
    pub web_search_min_local: usize,
}

impl Default for SurveyorConfig {
    fn default() -> Self {
        Self {
            enable_hf_search: true,
            max_hf_results: 10,
            enable_web_search: false,
            max_web_results: 5,
            web_search_min_local: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// SurveyorAgent
// ---------------------------------------------------------------------------

/// Searches HuggingFace and, optionally, the open web for benchmark candidates.
pub struct SurveyorAgent {
    config: SurveyorConfig,
}

impl SurveyorAgent {
    /// Create a new surveyor with the given configuration.
    pub fn new(config: SurveyorConfig) -> Self {
        Self { config }
    }

    /// Search HuggingFace datasets API for benchmarks matching `topic`.
    async fn search_huggingface(
        &self,
        topic: &str,
    ) -> Result<Vec<BenchmarkCandidate>> {
        debug!(topic, "Searching HuggingFace datasets");
        let client = reqwest::Client::new();
        let url = format!(
            "https://huggingface.co/api/datasets?search={}&limit={}",
            urlencoding(topic),
            self.config.max_hf_results,
        );

        let resp = client
            .get(&url)
            .header("User-Agent", "mol-agents/0.1")
            .send()
            .await?;

        if !resp.status().is_success() {
            warn!("HuggingFace search returned {}", resp.status());
            return Ok(Vec::new());
        }

        let datasets: Vec<Value> = resp.json().await.unwrap_or_default();
        let candidates = datasets
            .into_iter()
            .filter_map(|d| {
                let id = d.get("id")?.as_str()?.to_owned();
                let description = d
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                Some(BenchmarkCandidate {
                    name: id.split('/').last().unwrap_or(&id).to_owned(),
                    hf_id: Some(id.clone()),
                    source_url: format!("https://huggingface.co/datasets/{id}"),
                    on_huggingface: true,
                    notes: description,
                    ..Default::default()
                })
            })
            .collect();

        Ok(candidates)
    }
}

/// Minimal percent-encode for use in URL query parameters.
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => '+'.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

#[async_trait]
impl BaseAgent for SurveyorAgent {
    fn name(&self) -> &str {
        "surveyor"
    }

    async fn plan(&self, context: &AgentContext) -> Result<AgentPlan> {
        let mut steps = Vec::new();
        if self.config.enable_hf_search {
            steps.push(format!(
                "Search HuggingFace for benchmarks related to '{}'",
                context.topic
            ));
        }
        if self.config.enable_web_search {
            steps.push("Augment with open-web benchmark search".to_owned());
        }
        steps.push("Deduplicate and rank candidates".to_owned());
        Ok(AgentPlan::new("Benchmark survey", steps))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(steps = plan.steps.len(), "SurveyorAgent running");

        // We don't have access to the topic from `plan` alone, so parse from notes.
        // In real use the orchestrator passes the topic via the context; here we use
        // the plan title as a hint.
        let topic = plan.title.replace("Benchmark survey", "").trim().to_owned();
        let topic = if topic.is_empty() { "molecular property prediction".to_owned() } else { topic };

        let mut candidates: Vec<BenchmarkCandidate> = Vec::new();

        if self.config.enable_hf_search {
            match self.search_huggingface(&topic).await {
                Ok(mut found) => {
                    debug!(count = found.len(), "HuggingFace results");
                    candidates.append(&mut found);
                }
                Err(e) => {
                    warn!("HuggingFace search failed: {e}");
                }
            }
        }

        let output = format!(
            "Surveyor found {} benchmark candidates for topic '{topic}'.",
            candidates.len()
        );

        let result = AgentStepResult::ok(output)
            .with_artifact("candidates", &candidates);

        Ok(result)
    }
}
