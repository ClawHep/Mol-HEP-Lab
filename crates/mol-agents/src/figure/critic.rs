//! Critic agent — evaluates rendered figure quality.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent, ReviewOutcome};
use crate::figure::renderer::RenderResult;

// ---------------------------------------------------------------------------
// CriticVerdict
// ---------------------------------------------------------------------------

/// Quality verdict for a single figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticVerdict {
    /// Figure identifier.
    pub figure_id: String,
    /// Whether the figure passes quality standards.
    pub passed: bool,
    /// Quality score in `[0.0, 1.0]`.
    pub score: f32,
    /// Issues found by the critic.
    pub issues: Vec<String>,
    /// Improvement suggestions.
    pub suggestions: Vec<String>,
}

// ---------------------------------------------------------------------------
// CriticAgent
// ---------------------------------------------------------------------------

/// Evaluates rendered figures for publication quality.
pub struct CriticAgent {
    /// When `true`, any detected issue causes a failure verdict.
    pub strict_mode: bool,
}

impl CriticAgent {
    /// Create a critic agent.
    pub fn new(strict_mode: bool) -> Self {
        Self { strict_mode }
    }

    /// Evaluate a single render result.
    fn evaluate(&self, result: &RenderResult) -> CriticVerdict {
        if !result.success {
            return CriticVerdict {
                figure_id: result.figure_id.clone(),
                passed: false,
                score: 0.0,
                issues: vec![format!("Rendering failed (exit {}): {}", result.exit_code, result.stderr.lines().next().unwrap_or("unknown error"))],
                suggestions: vec!["Fix the rendering script and retry.".to_owned()],
            };
        }

        let mut issues: Vec<String> = Vec::new();
        let mut suggestions: Vec<String> = Vec::new();
        let mut score: f32 = 1.0;

        // Check output file exists.
        if result.output_path.is_none() {
            issues.push("Output file was not produced despite zero exit code.".to_owned());
            suggestions.push("Ensure the script calls plt.savefig().".to_owned());
            score -= 0.5;
        }

        // Check for common matplotlib warnings.
        if result.stderr.contains("UserWarning") {
            suggestions.push("Address matplotlib UserWarnings.".to_owned());
            if self.strict_mode {
                score -= 0.1;
            }
        }

        if result.stderr.contains("DeprecationWarning") {
            suggestions.push("Update deprecated API usage.".to_owned());
        }

        let passed = issues.is_empty() || (!self.strict_mode && score > 0.5);

        CriticVerdict {
            figure_id: result.figure_id.clone(),
            passed,
            score: score.max(0.0),
            issues,
            suggestions,
        }
    }
}

#[async_trait]
impl BaseAgent for CriticAgent {
    fn name(&self) -> &str {
        "critic"
    }

    async fn plan(&self, _context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Figure quality review",
            vec![
                "Evaluate each render result".to_owned(),
                "Check for rendering errors and warnings".to_owned(),
                "Score figures and generate improvement suggestions".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(strict = self.strict_mode, "CriticAgent running");

        let render_results: Vec<RenderResult> = plan
            .metadata
            .get("render_results")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let verdicts: Vec<CriticVerdict> = render_results
            .iter()
            .map(|r| self.evaluate(r))
            .collect();

        let passed_count = verdicts.iter().filter(|v| v.passed).count();
        let avg_score: f32 = if verdicts.is_empty() {
            0.0
        } else {
            verdicts.iter().map(|v| v.score).sum::<f32>() / verdicts.len() as f32
        };

        let output = format!(
            "Critic: {}/{} figures passed (avg score {:.2}).",
            passed_count,
            verdicts.len(),
            avg_score
        );

        let all_passed = verdicts.iter().all(|v| v.passed);
        let mut step = AgentStepResult::ok(output).with_artifact("critic_verdicts", &verdicts);
        step.success = all_passed;
        if !all_passed {
            step.next_action = "retry".to_owned();
            step.error = "Some figures failed quality review.".to_owned();
        }

        Ok(step)
    }

    async fn review(&self, result: &AgentStepResult) -> Result<ReviewOutcome> {
        let verdicts: Vec<CriticVerdict> = result
            .artifacts
            .get("critic_verdicts")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let all_passed = verdicts.iter().all(|v| v.passed);
        let avg_score: f32 = if verdicts.is_empty() {
            0.0
        } else {
            verdicts.iter().map(|v| v.score).sum::<f32>() / verdicts.len() as f32
        };

        let issues: Vec<String> = verdicts
            .iter()
            .flat_map(|v| v.issues.iter().cloned())
            .collect();

        if all_passed {
            Ok(ReviewOutcome::accept(avg_score, "All figures passed quality review."))
        } else {
            Ok(ReviewOutcome::reject(avg_score, "Some figures require revision.", issues))
        }
    }
}
