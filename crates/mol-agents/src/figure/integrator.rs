//! Integrator agent — assembles all rendered figures into a manifest and
//! inserts figure references into the paper draft.

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::figure::critic::CriticVerdict;
use crate::figure::nano_banana::ImageGenerationResult;
use crate::figure::renderer::RenderResult;

// ---------------------------------------------------------------------------
// FigureEntry
// ---------------------------------------------------------------------------

/// A single entry in the final figure manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureEntry {
    /// Figure identifier.
    pub id: String,
    /// Path to the rendered image file.
    pub file_path: PathBuf,
    /// Caption text.
    pub caption: String,
    /// LaTeX label (e.g. `"fig:main_results"`).
    pub label: String,
    /// Whether the figure was produced by code (`"code"`) or Gemini (`"image"`).
    pub source: String,
    /// Whether the figure passed quality review.
    pub passed: bool,
}

// ---------------------------------------------------------------------------
// IntegratorAgent
// ---------------------------------------------------------------------------

/// Combines rendered figures and generated images into a complete manifest.
pub struct IntegratorAgent;

impl IntegratorAgent {
    /// Create a new integrator.
    pub fn new() -> Self {
        Self
    }

    /// Generate a markdown figure reference block.
    fn markdown_ref(entry: &FigureEntry) -> String {
        format!(
            "![{}]({})\n*Figure: {}*",
            entry.id,
            entry.file_path.display(),
            entry.caption
        )
    }

    /// Generate a LaTeX figure environment string.
    fn latex_figure(entry: &FigureEntry) -> String {
        format!(
            r#"\begin{{figure}}[htbp]
  \centering
  \includegraphics[width=\columnwidth]{{{path}}}
  \caption{{{caption}}}
  \label{{{label}}}
\end{{figure}}"#,
            path = entry.file_path.display(),
            caption = entry.caption,
            label = entry.label,
        )
    }
}

impl Default for IntegratorAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseAgent for IntegratorAgent {
    fn name(&self) -> &str {
        "integrator"
    }

    async fn plan(&self, _context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Figure integration",
            vec![
                "Collect all render and image results".to_owned(),
                "Filter to passed figures".to_owned(),
                "Build figure manifest".to_owned(),
                "Generate markdown and LaTeX references".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!("IntegratorAgent running");

        let render_results: Vec<RenderResult> = plan
            .metadata
            .get("render_results")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let image_results: Vec<ImageGenerationResult> = plan
            .metadata
            .get("image_generation_results")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let verdicts: Vec<CriticVerdict> = plan
            .metadata
            .get("critic_verdicts")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let passed_ids: std::collections::HashSet<String> = verdicts
            .iter()
            .filter(|v| v.passed)
            .map(|v| v.figure_id.clone())
            .collect();

        let mut entries: Vec<FigureEntry> = Vec::new();

        // Code-rendered figures.
        for result in &render_results {
            if let Some(ref path) = result.output_path {
                entries.push(FigureEntry {
                    id: result.figure_id.clone(),
                    file_path: path.clone(),
                    caption: format!("Figure: {}", result.figure_id),
                    label: format!("fig:{}", result.figure_id),
                    source: "code".to_owned(),
                    passed: passed_ids.contains(&result.figure_id),
                });
            }
        }

        // Gemini-generated images.
        for result in &image_results {
            if let Some(ref path) = result.output_path {
                entries.push(FigureEntry {
                    id: result.figure_id.clone(),
                    file_path: path.clone(),
                    caption: format!("Figure: {}", result.figure_id),
                    label: format!("fig:{}", result.figure_id),
                    source: "image".to_owned(),
                    passed: true, // Images not run through code critic
                });
            }
        }

        // Build markdown and LaTeX reference strings.
        let markdown_refs: String = entries
            .iter()
            .filter(|e| e.passed)
            .map(Self::markdown_ref)
            .collect::<Vec<_>>()
            .join("\n\n");

        let latex_refs: String = entries
            .iter()
            .filter(|e| e.passed)
            .map(Self::latex_figure)
            .collect::<Vec<_>>()
            .join("\n\n");

        let figure_descriptions: String = entries
            .iter()
            .enumerate()
            .map(|(i, e)| format!("Figure {}: {} ({})", i + 1, e.caption, e.id))
            .collect::<Vec<_>>()
            .join("\n");

        let output = format!(
            "Integrator assembled {} figures ({} passed).",
            entries.len(),
            entries.iter().filter(|e| e.passed).count()
        );

        Ok(AgentStepResult::ok(output)
            .with_artifact("figure_manifest", &entries)
            .with_artifact("markdown_refs", &markdown_refs)
            .with_artifact("latex_refs", &latex_refs)
            .with_artifact("figure_descriptions", &figure_descriptions))
    }
}
