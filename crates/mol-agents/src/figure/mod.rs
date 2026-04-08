//! Figure agent sub-system.
//!
//! Coordinates a multi-stage pipeline:
//! **Decision** → (Code: **Planner → CodeGen → Renderer → Critic** loop) +
//! (Image: **NanoBanana**) → **Integrator**
//!
//! Produces a [`FigurePlan`] consumed by paper drafting and export stages.

pub mod codegen;
pub mod critic;
pub mod decision;
pub mod integrator;
pub mod nano_banana;
pub mod planner;
pub mod renderer;
pub mod style;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::base::{AgentContext, AgentOrchestrator, AgentStepResult, BaseAgent};
use codegen::CodeGenAgent;
use critic::CriticAgent;
use decision::{DecisionAgent, FigureKind, FigureRequest};
use integrator::IntegratorAgent;
use nano_banana::NanoBananaAgent;
use planner::PlannerAgent;
use renderer::RendererAgent;
use style::StyleConfig;

// ---------------------------------------------------------------------------
// FigureAgentConfig
// ---------------------------------------------------------------------------

/// Configuration for the full figure agent system.
#[derive(Debug, Clone)]
pub struct FigureAgentConfig {
    /// Enable or disable the whole subsystem.
    pub enabled: bool,
    // Planner
    pub min_figures: usize,
    pub max_figures: usize,
    // Orchestrator
    pub max_iterations: u32,
    // Renderer
    pub render_timeout_sec: u64,
    pub use_docker: bool,
    pub docker_image: String,
    // Code generation
    pub output_format: String,
    // Nano Banana (Gemini)
    pub gemini_api_key: String,
    pub gemini_model: String,
    pub nano_banana_enabled: bool,
    // Critic
    pub strict_mode: bool,
    // Output
    pub dpi: u32,
    // Style preset
    pub style: StyleConfig,
}

impl Default for FigureAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_figures: 3,
            max_figures: 8,
            max_iterations: 3,
            render_timeout_sec: 30,
            use_docker: false,
            docker_image: "researchmol/experiment:latest".to_owned(),
            output_format: "python".to_owned(),
            gemini_api_key: String::new(),
            gemini_model: "gemini-2.0-flash-preview-image-generation".to_owned(),
            nano_banana_enabled: true,
            strict_mode: false,
            dpi: 300,
            style: StyleConfig::arxiv(),
        }
    }
}

// ---------------------------------------------------------------------------
// FigurePlan — output of the orchestrator
// ---------------------------------------------------------------------------

/// Final output from the figure agent system.
///
/// Consumed by paper draft (figure_descriptions) and export (manifest) stages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FigurePlan {
    /// List of figure metadata dicts (see [`integrator::FigureEntry`]).
    pub manifest: Vec<Value>,

    /// Markdown figure reference block for embedding in paper draft.
    pub markdown_refs: String,

    /// Prose descriptions of all figures for the writing prompt.
    pub figure_descriptions: String,

    /// LaTeX figure environments.
    pub latex_refs: String,

    /// Output directory path.
    pub output_dir: String,

    /// Path to the saved manifest JSON file.
    pub manifest_path: String,

    // Stats
    pub figure_count: usize,
    pub passed_count: usize,
    pub total_llm_calls: u32,
    pub total_tokens: u32,
    pub elapsed_sec: f64,
}

impl FigurePlan {
    /// Return filenames of rendered chart files from the manifest.
    pub fn chart_files(&self) -> Vec<String> {
        self.manifest
            .iter()
            .filter_map(|entry| {
                entry
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(|p| {
                        PathBuf::from(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(p)
                            .to_owned()
                    })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// FigureOrchestrator
// ---------------------------------------------------------------------------

/// Coordinates Decision → (Code-to-Viz | Nano Banana) → Integrator.
pub struct FigureOrchestrator {
    config: FigureAgentConfig,
    orchestrator: AgentOrchestrator,
    stage_dir: PathBuf,
}

impl FigureOrchestrator {
    /// Create a new orchestrator.
    pub fn new(config: FigureAgentConfig, stage_dir: Option<PathBuf>) -> Self {
        let max_iter = config.max_iterations;
        let stage_dir = stage_dir.unwrap_or_else(|| {
            std::env::temp_dir().join("mol-figures")
        });
        Self {
            config,
            orchestrator: AgentOrchestrator::new(max_iter),
            stage_dir,
        }
    }

    /// Run the full figure generation pipeline.
    pub async fn orchestrate(
        &mut self,
        topic: &str,
        experiment_results: Option<Value>,
    ) -> Result<FigurePlan> {
        info!(topic, "FigureOrchestrator starting");
        let start = Instant::now();

        let has_results = experiment_results.is_some();
        let mut context = AgentContext::new(topic);
        context.stage = Some("figure_agent".to_owned());

        // --- Decision ---
        let decision_agent = DecisionAgent::new(
            self.config.min_figures,
            self.config.max_figures,
        );
        let mut decision_plan = decision_agent.plan(&context).await?;
        decision_plan.metadata.insert("topic".to_owned(), serde_json::json!(topic));
        decision_plan
            .metadata
            .insert("has_experiment_results".to_owned(), serde_json::json!(has_results));
        let decision_result = decision_agent.run(&decision_plan).await?;
        self.orchestrator.accumulate(&decision_result);

        let all_requests: Vec<FigureRequest> = decision_result
            .artifacts
            .get("figure_requests")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Split into code and image requests.
        let code_requests: Vec<FigureRequest> = all_requests
            .iter()
            .filter(|r| r.kind == FigureKind::Code)
            .cloned()
            .collect();

        let image_requests: Vec<FigureRequest> = all_requests
            .iter()
            .filter(|r| r.kind == FigureKind::Image)
            .cloned()
            .collect();

        // --- Code-to-Viz pipeline ---
        let planner = PlannerAgent::new(
            self.config.style.clone(),
            self.config.min_figures,
            self.config.max_figures,
        );
        let mut planner_plan = planner.plan(&context).await?;
        planner_plan
            .metadata
            .insert("figure_requests".to_owned(), serde_json::json!(&code_requests));
        let planner_result = planner.run(&planner_plan).await?;
        self.orchestrator.accumulate(&planner_result);

        let codegen = CodeGenAgent::new(&self.config.output_format);
        let mut codegen_plan = codegen.plan(&context).await?;
        if let Some(specs) = planner_result.artifacts.get("figure_specs") {
            codegen_plan
                .metadata
                .insert("figure_specs".to_owned(), specs.clone());
        }
        let codegen_result = codegen.run(&codegen_plan).await?;
        self.orchestrator.accumulate(&codegen_result);

        let renderer = RendererAgent::new(
            &self.stage_dir,
            "python3",
            self.config.render_timeout_sec,
            self.config.use_docker,
            &self.config.docker_image,
        );
        let critic = CriticAgent::new(self.config.strict_mode);

        let mut last_critic_result = AgentStepResult::default();

        for iteration in 0..self.config.max_iterations {
            let mut renderer_plan = renderer.plan(&context).await?;
            if let Some(code) = codegen_result.artifacts.get("generated_code") {
                renderer_plan
                    .metadata
                    .insert("generated_code".to_owned(), code.clone());
            }
            let renderer_result = renderer.run(&renderer_plan).await?;
            self.orchestrator.accumulate(&renderer_result);

            let mut critic_plan = critic.plan(&context).await?;
            if let Some(rr) = renderer_result.artifacts.get("render_results") {
                critic_plan
                    .metadata
                    .insert("render_results".to_owned(), rr.clone());
            }
            let critic_result = critic.run(&critic_plan).await?;
            self.orchestrator.accumulate(&critic_result);

            last_critic_result = critic_result;
            if last_critic_result.success {
                break;
            }
            warn!(iteration, "Critic failed, retrying");
        }

        // --- Nano Banana (image figures) ---
        let mut nano_result = AgentStepResult::default();
        if self.config.nano_banana_enabled && !image_requests.is_empty() {
            let nano = NanoBananaAgent::new(
                self.config.gemini_api_key.clone(),
                self.config.gemini_model.clone(),
                &self.stage_dir,
            );
            let mut nano_plan = nano.plan(&context).await?;
            nano_plan
                .metadata
                .insert("image_requests".to_owned(), serde_json::json!(&image_requests));
            nano_result = nano.run(&nano_plan).await?;
            self.orchestrator.accumulate(&nano_result);
        }

        // --- Integrator ---
        let integrator = IntegratorAgent::new();
        let mut integrator_plan = integrator.plan(&context).await?;

        if let Some(rr) = last_critic_result.artifacts.get("render_results").or(
            last_critic_result.artifacts.get("render_results"),
        ) {
            integrator_plan
                .metadata
                .insert("render_results".to_owned(), rr.clone());
        }
        if let Some(ir) = nano_result.artifacts.get("image_generation_results") {
            integrator_plan
                .metadata
                .insert("image_generation_results".to_owned(), ir.clone());
        }
        if let Some(cv) = last_critic_result.artifacts.get("critic_verdicts") {
            integrator_plan
                .metadata
                .insert("critic_verdicts".to_owned(), cv.clone());
        }
        let integrator_result = integrator.run(&integrator_plan).await?;
        self.orchestrator.accumulate(&integrator_result);

        // Build plan.
        let manifest: Vec<Value> = integrator_result
            .artifacts
            .get("figure_manifest")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let markdown_refs = integrator_result
            .artifacts
            .get("markdown_refs")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let latex_refs = integrator_result
            .artifacts
            .get("latex_refs")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let figure_descriptions = integrator_result
            .artifacts
            .get("figure_descriptions")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let passed_count = manifest
            .iter()
            .filter(|e| e.get("passed").and_then(|v| v.as_bool()).unwrap_or(false))
            .count();

        let elapsed = start.elapsed().as_secs_f64();
        info!(
            elapsed,
            figures = manifest.len(),
            passed = passed_count,
            total_llm_calls = self.orchestrator.total_llm_calls,
            "FigureOrchestrator complete"
        );

        Ok(FigurePlan {
            manifest,
            markdown_refs,
            figure_descriptions,
            latex_refs,
            output_dir: self.stage_dir.display().to_string(),
            manifest_path: self.stage_dir.join("manifest.json").display().to_string(),
            figure_count: all_requests.len(),
            passed_count,
            total_llm_calls: self.orchestrator.total_llm_calls,
            total_tokens: self.orchestrator.total_tokens,
            elapsed_sec: elapsed,
        })
    }
}
