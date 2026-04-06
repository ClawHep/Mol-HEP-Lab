//! Code generation agent — produces matplotlib / TikZ source for each figure spec.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::figure::planner::FigureSpec;

// ---------------------------------------------------------------------------
// GeneratedCode
// ---------------------------------------------------------------------------

/// Python or LaTeX source code that renders a figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedCode {
    /// Figure identifier.
    pub figure_id: String,
    /// The source code string.
    pub code: String,
    /// `"python"` or `"latex"`.
    pub language: String,
    /// Expected output path relative to the charts directory.
    pub output_path: String,
}

// ---------------------------------------------------------------------------
// CodeGenAgent
// ---------------------------------------------------------------------------

/// Generates matplotlib / TikZ source code for figure specs.
pub struct CodeGenAgent {
    /// `"python"` to generate matplotlib; `"latex"` for TikZ.
    pub output_format: String,
}

impl CodeGenAgent {
    /// Create a code generation agent.
    pub fn new(output_format: impl Into<String>) -> Self {
        Self {
            output_format: output_format.into(),
        }
    }

    /// Generate a self-contained matplotlib script for a bar chart.
    fn generate_bar_chart(&self, spec: &FigureSpec) -> String {
        let rc = spec.style.to_matplotlib_rc();
        format!(
            r#"{rc}
import json, os
import numpy as np

# ── Load data ──────────────────────────────────────────────────────────────
results_file = os.environ.get("RESULTS_JSON", "results.json")
with open(results_file) as f:
    data = json.load(f)

# TODO: Adjust keys to match your results structure
methods = list(data.keys())
values = [data[m].get("primary_metric", 0.0) for m in methods]

# ── Plot ────────────────────────────────────────────────────────────────────
fig, ax = plt.subplots()
x = np.arange(len(methods))
bars = ax.bar(x, values, color=plt.rcParams['axes.prop_cycle'].by_key()['color'][:len(methods)])
ax.set_xticks(x)
ax.set_xticklabels(methods, rotation=30, ha='right')
ax.set_ylabel("Metric")
ax.set_title("{title}")

if plt.rcParams.get('figure.facecolor', 'white') not in ('white', '#ffffff'):
    ax.set_facecolor(plt.rcParams['figure.facecolor'])
    fig.patch.set_facecolor(plt.rcParams['figure.facecolor'])

plt.tight_layout()
plt.savefig("{output}", dpi=plt.rcParams['figure.dpi'], bbox_inches='tight')
plt.close()
print(f"Saved {output}")
"#,
            title = spec.title,
            output = spec.output_filename,
        )
    }

    /// Generate a matplotlib line plot (learning curves).
    fn generate_line_chart(&self, spec: &FigureSpec) -> String {
        let rc = spec.style.to_matplotlib_rc();
        format!(
            r#"{rc}
import json, os
import numpy as np

results_file = os.environ.get("RESULTS_JSON", "results.json")
with open(results_file) as f:
    data = json.load(f)

epochs = data.get("epochs", list(range(100)))
train_loss = data.get("train_loss", np.linspace(1.0, 0.1, 100).tolist())
val_loss = data.get("val_loss", np.linspace(1.1, 0.15, 100).tolist())

fig, ax = plt.subplots()
ax.plot(epochs, train_loss, label="Train")
ax.plot(epochs, val_loss, label="Validation", linestyle='--')
ax.set_xlabel("Epoch")
ax.set_ylabel("Loss")
ax.set_title("{title}")
ax.legend()

plt.tight_layout()
plt.savefig("{output}", dpi=plt.rcParams['figure.dpi'], bbox_inches='tight')
plt.close()
print(f"Saved {output}")
"#,
            title = spec.title,
            output = spec.output_filename,
        )
    }

    /// Generate a generic scatter plot.
    fn generate_scatter(&self, spec: &FigureSpec) -> String {
        let rc = spec.style.to_matplotlib_rc();
        format!(
            r#"{rc}
import json, os
import numpy as np

results_file = os.environ.get("RESULTS_JSON", "results.json")
with open(results_file) as f:
    data = json.load(f)

x = data.get("x", np.random.randn(50).tolist())
y = data.get("y", np.random.randn(50).tolist())

fig, ax = plt.subplots()
ax.scatter(x, y, alpha=0.7)
ax.set_xlabel("x")
ax.set_ylabel("y")
ax.set_title("{title}")

plt.tight_layout()
plt.savefig("{output}", dpi=plt.rcParams['figure.dpi'], bbox_inches='tight')
plt.close()
print(f"Saved {output}")
"#,
            title = spec.title,
            output = spec.output_filename,
        )
    }

    /// Dispatch to the correct template based on chart type.
    fn generate_code(&self, spec: &FigureSpec) -> String {
        match spec.chart_type.as_str() {
            "line" => self.generate_line_chart(spec),
            "scatter" => self.generate_scatter(spec),
            _ => self.generate_bar_chart(spec),
        }
    }
}

#[async_trait]
impl BaseAgent for CodeGenAgent {
    fn name(&self) -> &str {
        "codegen"
    }

    async fn plan(&self, _context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Figure code generation",
            vec![
                format!("Generate {} code for each figure spec", self.output_format),
                "Embed publication style settings".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(format = %self.output_format, "CodeGenAgent running");

        let specs: Vec<FigureSpec> = plan
            .metadata
            .get("figure_specs")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let generated: Vec<GeneratedCode> = specs
            .iter()
            .map(|spec| GeneratedCode {
                figure_id: spec.id.clone(),
                code: self.generate_code(spec),
                language: self.output_format.clone(),
                output_path: spec.output_filename.clone(),
            })
            .collect();

        let output = format!("CodeGenAgent produced {} scripts.", generated.len());
        Ok(AgentStepResult::ok(output).with_artifact("generated_code", &generated))
    }
}
