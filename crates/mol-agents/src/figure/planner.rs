//! Planner agent — refines figure requests into detailed generation specs.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::figure::decision::FigureRequest;
use crate::figure::style::StyleConfig;

// ---------------------------------------------------------------------------
// FigureSpec
// ---------------------------------------------------------------------------

/// Detailed specification for generating a single figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureSpec {
    /// Identifier (matches [`FigureRequest::id`]).
    pub id: String,
    /// Title of the figure.
    pub title: String,
    /// What the figure must convey.
    pub description: String,
    /// Expected output file name (e.g. `"fig_main_results.pdf"`).
    pub output_filename: String,
    /// Chart type hint (e.g. `"bar"`, `"line"`, `"scatter"`, `"heatmap"`).
    pub chart_type: String,
    /// Data sources or result keys the code should reference.
    pub data_sources: Vec<String>,
    /// Caption for the paper.
    pub caption: String,
    /// Publication style preset to apply.
    pub style: StyleConfig,
}

// ---------------------------------------------------------------------------
// PlannerAgent
// ---------------------------------------------------------------------------

/// Refines [`FigureRequest`] items into detailed [`FigureSpec`] objects.
pub struct PlannerAgent {
    /// Style preset to apply to generated figures.
    pub style: StyleConfig,
    pub min_figures: usize,
    pub max_figures: usize,
}

impl PlannerAgent {
    /// Create a planner agent.
    pub fn new(style: StyleConfig, min_figures: usize, max_figures: usize) -> Self {
        Self {
            style,
            min_figures,
            max_figures,
        }
    }

    /// Infer an appropriate chart type from the request description.
    fn infer_chart_type(&self, description: &str) -> &'static str {
        let lower = description.to_lowercase();
        if lower.contains("curve") || lower.contains("loss") || lower.contains("convergence") {
            "line"
        } else if lower.contains("heatmap") || lower.contains("correlation") {
            "heatmap"
        } else if lower.contains("scatter") || lower.contains("embedding") {
            "scatter"
        } else if lower.contains("distribution") || lower.contains("histogram") {
            "histogram"
        } else {
            "bar"
        }
    }

    /// Convert a [`FigureRequest`] to a [`FigureSpec`].
    fn spec_from_request(&self, req: &FigureRequest) -> FigureSpec {
        let chart_type = self.infer_chart_type(&req.description).to_owned();
        let ext = "pdf";
        let output_filename = format!("{}.{ext}", req.id);

        FigureSpec {
            id: req.id.clone(),
            title: req.title.clone(),
            description: req.description.clone(),
            output_filename,
            chart_type,
            data_sources: vec!["results.json".to_owned()],
            caption: req.caption.clone(),
            style: self.style.clone(),
        }
    }
}

#[async_trait]
impl BaseAgent for PlannerAgent {
    fn name(&self) -> &str {
        "planner"
    }

    async fn plan(&self, context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Figure planning",
            vec![
                format!("Refine figure requests for '{}'", context.topic),
                "Assign chart types and data sources".to_owned(),
                "Apply publication style preset".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(plan = %plan.title, "PlannerAgent running");

        let requests: Vec<FigureRequest> = plan
            .metadata
            .get("figure_requests")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let specs: Vec<FigureSpec> = requests
            .iter()
            .map(|r| self.spec_from_request(r))
            .collect();

        let output = format!("Planner produced {} figure specs.", specs.len());
        Ok(AgentStepResult::ok(output).with_artifact("figure_specs", &specs))
    }
}
