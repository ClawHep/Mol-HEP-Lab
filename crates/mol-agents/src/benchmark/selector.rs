//! Selector agent — ranks and selects benchmarks and baselines by relevance.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::benchmark::surveyor::BenchmarkCandidate;

// ---------------------------------------------------------------------------
// Selection result
// ---------------------------------------------------------------------------

/// Tier classification for a selected benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkTier {
    /// Primary benchmark — must be evaluated.
    Primary,
    /// Secondary benchmark — evaluated if resources allow.
    Secondary,
    /// Supplementary — optional, provides additional signal.
    Supplementary,
}

/// A benchmark with its selection rationale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedBenchmark {
    /// The underlying benchmark candidate.
    pub candidate: BenchmarkCandidate,
    /// Assigned tier.
    pub tier: BenchmarkTier,
    /// Relevance score in `[0.0, 1.0]`.
    pub relevance_score: f32,
    /// Why this benchmark was selected.
    pub rationale: String,
}

/// A baseline method entry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BaselineEntry {
    /// Short name of the baseline (e.g. `"SchNet"`).
    pub name: String,
    /// Paper reference.
    pub paper: String,
    /// Source code URL.
    pub source: String,
    /// Notes about the baseline.
    pub notes: String,
}

// ---------------------------------------------------------------------------
// SelectorConfig
// ---------------------------------------------------------------------------

/// Configuration for [`SelectorAgent`].
#[derive(Debug, Clone)]
pub struct SelectorConfig {
    /// Maximum tier to include (1 = Primary only, 2 = Primary+Secondary, …).
    pub tier_limit: u32,
    /// Minimum benchmarks to select.
    pub min_benchmarks: usize,
    /// Minimum baselines to include.
    pub min_baselines: usize,
    /// Prefer benchmarks already cached locally.
    pub prefer_cached: bool,
    /// GPU memory available in MB (used for feasibility filtering).
    pub gpu_memory_mb: u64,
    /// Wall-clock time budget in seconds.
    pub time_budget_sec: u64,
    /// Network access policy (`"setup_only"` | `"full"` | `"none"`).
    pub network_policy: String,
}

impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            tier_limit: 2,
            min_benchmarks: 1,
            min_baselines: 2,
            prefer_cached: true,
            gpu_memory_mb: 49_000,
            time_budget_sec: 3600,
            network_policy: "setup_only".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// SelectorAgent
// ---------------------------------------------------------------------------

/// Ranks surveyed benchmark candidates and selects those most relevant to the topic.
pub struct SelectorAgent {
    config: SelectorConfig,
}

impl SelectorAgent {
    /// Create a selector agent with the given configuration.
    pub fn new(config: SelectorConfig) -> Self {
        Self { config }
    }

    /// Score a candidate based on simple heuristics (stars, HF availability, …).
    fn score(&self, candidate: &BenchmarkCandidate) -> f32 {
        let mut score: f32 = 0.5;
        if candidate.on_huggingface {
            score += 0.2;
        }
        if let Some(stars) = candidate.github_stars {
            score += (stars as f32 / 10_000.0).min(0.2);
        }
        if self.config.prefer_cached {
            // Boost cached entries (represented by hf_id being present)
            if candidate.hf_id.is_some() {
                score += 0.1;
            }
        }
        score.min(1.0)
    }
}

#[async_trait]
impl BaseAgent for SelectorAgent {
    fn name(&self) -> &str {
        "selector"
    }

    async fn plan(&self, context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Benchmark selection",
            vec![
                format!(
                    "Score candidates for topic '{}' by relevance and feasibility",
                    context.topic
                ),
                format!("Select top {} tier(s)", self.config.tier_limit),
                "Identify baseline methods".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(plan = %plan.title, "SelectorAgent running");

        // Retrieve candidates from plan metadata if present.
        let candidates: Vec<BenchmarkCandidate> = plan
            .metadata
            .get("candidates")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Score and sort.
        let mut scored: Vec<(f32, BenchmarkCandidate)> = candidates
            .into_iter()
            .map(|c| {
                let s = self.score(&c);
                (s, c)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Assign tiers.
        let mut selected: Vec<SelectedBenchmark> = scored
            .into_iter()
            .enumerate()
            .map(|(i, (score, candidate))| {
                let tier = if i == 0 {
                    BenchmarkTier::Primary
                } else if i < 3 {
                    BenchmarkTier::Secondary
                } else {
                    BenchmarkTier::Supplementary
                };
                SelectedBenchmark {
                    rationale: format!(
                        "Relevance score {:.2}; ranked #{} from survey",
                        score,
                        i + 1
                    ),
                    candidate,
                    tier,
                    relevance_score: score,
                }
            })
            .collect();

        // Apply tier_limit filter.
        let tier_limit = self.config.tier_limit;
        selected.retain(|s| match s.tier {
            BenchmarkTier::Primary => true,
            BenchmarkTier::Secondary => tier_limit >= 2,
            BenchmarkTier::Supplementary => tier_limit >= 3,
        });

        // Ensure minimum benchmarks.
        if selected.len() < self.config.min_benchmarks {
            return Ok(AgentStepResult::fail(format!(
                "Only {} benchmarks found; minimum is {}",
                selected.len(),
                self.config.min_benchmarks
            )));
        }

        let output = format!("Selected {} benchmarks.", selected.len());
        Ok(AgentStepResult::ok(output).with_artifact("selected_benchmarks", &selected))
    }
}
