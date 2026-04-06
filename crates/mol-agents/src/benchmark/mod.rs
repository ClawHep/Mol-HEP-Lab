//! Benchmark agent sub-system.
//!
//! Coordinates a four-stage pipeline:
//! **Surveyor** → **Selector** → **Acquirer** → **Validator**
//!
//! Produces a [`BenchmarkPlan`] consumed by experiment design and code
//! generation stages.  Mirrors
//! `backend/agent/mol/agents/benchmark_agent/orchestrator.py`.

pub mod acquirer;
pub mod selector;
pub mod surveyor;
pub mod validator;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::base::{AgentContext, AgentOrchestrator, AgentStepResult, BaseAgent};
use acquirer::AcquirerAgent;
use selector::{SelectorAgent, SelectorConfig};
use surveyor::{SurveyorAgent, SurveyorConfig};
use validator::ValidatorAgent;

// ---------------------------------------------------------------------------
// BenchmarkAgentConfig
// ---------------------------------------------------------------------------

/// Configuration for the full benchmark agent system.
#[derive(Debug, Clone)]
pub struct BenchmarkAgentConfig {
    /// Enable or disable the whole subsystem.
    pub enabled: bool,
    // Surveyor settings
    pub enable_hf_search: bool,
    pub max_hf_results: usize,
    pub enable_web_search: bool,
    pub max_web_results: usize,
    pub web_search_min_local: usize,
    // Selector settings
    pub tier_limit: u32,
    pub min_benchmarks: usize,
    pub min_baselines: usize,
    pub prefer_cached: bool,
    // Orchestrator settings
    pub max_iterations: u32,
    // Hardware constraints
    pub gpu_memory_mb: u64,
    pub time_budget_sec: u64,
    pub network_policy: String,
}

impl Default for BenchmarkAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_hf_search: true,
            max_hf_results: 10,
            enable_web_search: false,
            max_web_results: 5,
            web_search_min_local: 3,
            tier_limit: 2,
            min_benchmarks: 1,
            min_baselines: 2,
            prefer_cached: true,
            max_iterations: 2,
            gpu_memory_mb: 49_000,
            time_budget_sec: 3600,
            network_policy: "setup_only".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// BenchmarkPlan — output of the orchestrator
// ---------------------------------------------------------------------------

/// Final output from the benchmark agent system.
///
/// Consumed by experiment design, code generation, and Docker sandbox stages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkPlan {
    // Selected items
    pub selected_benchmarks: Vec<Value>,
    pub selected_baselines: Vec<Value>,
    pub matched_domains: Vec<String>,

    // Generated code
    pub data_loader_code: String,
    pub baseline_code: String,
    pub setup_code: String,
    pub requirements: String,

    // Metadata
    pub rationale: String,
    pub experiment_notes: String,
    pub validation_passed: bool,
    pub validation_warnings: Vec<String>,

    // Stats
    pub total_llm_calls: u32,
    pub total_tokens: u32,
    pub elapsed_sec: f64,
}

impl BenchmarkPlan {
    /// Format the plan as a prompt block for injection into code generation.
    pub fn to_prompt_block(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if !self.selected_benchmarks.is_empty() {
            parts.push("## Selected Benchmarks".to_owned());
            for b in &self.selected_benchmarks {
                let name = b
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                parts.push(format!("- **{name}**"));
            }
        }

        if !self.data_loader_code.is_empty() {
            parts.push("\n## Data Loading Code (READY TO USE)".to_owned());
            parts.push("```python".to_owned());
            parts.push(self.data_loader_code.clone());
            parts.push("```".to_owned());
        }

        if !self.experiment_notes.is_empty() {
            parts.push(format!("\n## Experiment Notes\n{}", self.experiment_notes));
        }

        parts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// BenchmarkOrchestrator
// ---------------------------------------------------------------------------

/// Coordinates Surveyor → Selector → Acquirer → Validator pipeline.
pub struct BenchmarkOrchestrator {
    config: BenchmarkAgentConfig,
    orchestrator: AgentOrchestrator,
    stage_dir: Option<PathBuf>,
}

impl BenchmarkOrchestrator {
    /// Create a new orchestrator with the given configuration.
    pub fn new(config: BenchmarkAgentConfig, stage_dir: Option<PathBuf>) -> Self {
        let max_iter = config.max_iterations;
        Self {
            config,
            orchestrator: AgentOrchestrator::new(max_iter),
            stage_dir,
        }
    }

    /// Build a fresh [`AgentContext`] for the given topic.
    fn make_context(&self, topic: &str) -> AgentContext {
        let mut ctx = AgentContext::new(topic);
        ctx.stage = Some("benchmark_agent".to_owned());
        ctx
    }

    /// Run the full pipeline and produce a [`BenchmarkPlan`].
    pub async fn orchestrate(&mut self, topic: &str) -> Result<BenchmarkPlan> {
        info!(topic, "BenchmarkOrchestrator starting");
        let start = Instant::now();
        let context = self.make_context(topic);

        // --- Surveyor ---
        let surveyor = SurveyorAgent::new(SurveyorConfig {
            enable_hf_search: self.config.enable_hf_search,
            max_hf_results: self.config.max_hf_results,
            enable_web_search: self.config.enable_web_search,
            max_web_results: self.config.max_web_results,
            web_search_min_local: self.config.web_search_min_local,
        });
        let surveyor_plan = surveyor.plan(&context).await?;
        let surveyor_result = surveyor.run(&surveyor_plan).await?;
        self.orchestrator.accumulate(&surveyor_result);

        // --- Selector ---
        let selector = SelectorAgent::new(SelectorConfig {
            tier_limit: self.config.tier_limit,
            min_benchmarks: self.config.min_benchmarks,
            min_baselines: self.config.min_baselines,
            prefer_cached: self.config.prefer_cached,
            gpu_memory_mb: self.config.gpu_memory_mb,
            time_budget_sec: self.config.time_budget_sec,
            network_policy: self.config.network_policy.clone(),
        });
        let mut selector_plan = selector.plan(&context).await?;
        // Forward surveyor artifacts into selector plan metadata.
        if let Some(candidates) = surveyor_result.artifacts.get("candidates") {
            selector_plan
                .metadata
                .insert("candidates".to_owned(), candidates.clone());
        }
        let selector_result = selector.run(&selector_plan).await?;
        self.orchestrator.accumulate(&selector_result);

        // --- Acquirer + Validator (with retry) ---
        let acquirer = AcquirerAgent::new(
            self.stage_dir
                .as_deref()
                .unwrap_or(std::path::Path::new("/tmp/mol-benchmarks"))
                .to_path_buf(),
        );
        let validator = ValidatorAgent::new(false);

        let mut last_validator_result: AgentStepResult = AgentStepResult::default();
        let mut validation_passed = false;

        for iteration in 0..self.config.max_iterations {
            info!(iteration, "Acquirer→Validator iteration");

            let mut acquirer_plan = acquirer.plan(&context).await?;
            // Forward selected benchmarks.
            if let Some(sel) = selector_result.artifacts.get("selected_benchmarks") {
                acquirer_plan
                    .metadata
                    .insert("selected_benchmarks".to_owned(), sel.clone());
            }
            let acquirer_result = acquirer.run(&acquirer_plan).await?;
            self.orchestrator.accumulate(&acquirer_result);

            let mut validator_plan = validator.plan(&context).await?;
            if let Some(recs) = acquirer_result.artifacts.get("acquisition_records") {
                validator_plan
                    .metadata
                    .insert("acquisition_records".to_owned(), recs.clone());
            }
            let validator_result = validator.run(&validator_plan).await?;
            self.orchestrator.accumulate(&validator_result);

            validation_passed = validator_result.success;
            last_validator_result = validator_result;

            if validation_passed {
                break;
            }
            warn!(iteration, "Validation failed, retrying");
        }

        // Build the plan from accumulated results.
        let data_loader_code = selector_result
            .artifacts
            .get("data_loader_code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let requirements = selector_result
            .artifacts
            .get("requirements")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let selected_benchmarks = selector_result
            .artifacts
            .get("selected_benchmarks")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<Value>>(v).ok())
            .unwrap_or_default();

        let validation_warnings: Vec<String> = last_validator_result
            .artifacts
            .get("validation_warnings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let elapsed = start.elapsed().as_secs_f64();
        info!(
            elapsed,
            validation_passed,
            total_llm_calls = self.orchestrator.total_llm_calls,
            "BenchmarkOrchestrator complete"
        );

        Ok(BenchmarkPlan {
            selected_benchmarks,
            data_loader_code,
            requirements,
            validation_passed,
            validation_warnings,
            total_llm_calls: self.orchestrator.total_llm_calls,
            total_tokens: self.orchestrator.total_tokens,
            elapsed_sec: elapsed,
            ..Default::default()
        })
    }
}
