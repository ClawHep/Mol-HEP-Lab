//! Base agent trait and shared orchestration types.
//!
//! Mirrors `backend/agent/researchclaw/agents/base.py`:
//! - [`BaseAgent`] — async trait every sub-agent implements.
//! - [`AgentOrchestrator`] — coordinator base for multi-agent workflows.
//! - Supporting structs: [`AgentContext`], [`AgentPlan`], [`AgentStepResult`],
//!   [`ReviewOutcome`].

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// AgentContext
// ---------------------------------------------------------------------------

/// Shared context passed into every agent call.
///
/// Carries the research topic, accumulated data from upstream stages, and
/// free-form key/value metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentContext {
    /// Primary research topic string.
    pub topic: String,

    /// Free-form context data (JSON values keyed by name).
    pub data: HashMap<String, Value>,

    /// Unique identifier for this research run.
    pub run_id: Option<String>,

    /// Which pipeline stage is calling this agent (e.g. `"code_search"`).
    pub stage: Option<String>,
}

impl AgentContext {
    /// Create a minimal context from a topic string.
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            ..Default::default()
        }
    }

    /// Insert a JSON-serialisable value into the context data map.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Serialize) {
        self.data
            .insert(key.into(), serde_json::to_value(value).unwrap_or(Value::Null));
    }

    /// Retrieve a value from the context data map.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }
}

// ---------------------------------------------------------------------------
// AgentPlan
// ---------------------------------------------------------------------------

/// High-level plan produced by [`BaseAgent::plan`].
///
/// Represents the agent's intended steps before execution begins.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentPlan {
    /// Short title for the plan.
    pub title: String,

    /// Ordered list of step descriptions the agent will carry out.
    pub steps: Vec<String>,

    /// Estimated LLM calls needed.
    pub estimated_llm_calls: u32,

    /// Free-form notes / rationale.
    pub notes: String,

    /// Arbitrary structured data attached to the plan.
    pub metadata: HashMap<String, Value>,
}

impl AgentPlan {
    /// Create a minimal plan from a title and step list.
    pub fn new(title: impl Into<String>, steps: Vec<String>) -> Self {
        Self {
            title: title.into(),
            steps,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// AgentStepResult
// ---------------------------------------------------------------------------

/// Output from a single agent step (mirrors Python `AgentStepResult`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentStepResult {
    /// Whether the step completed successfully.
    pub success: bool,

    /// Primary textual output (summary, generated code, etc.).
    pub output: String,

    /// Structured artifacts produced by the step (keyed by name).
    pub artifacts: HashMap<String, Value>,

    /// Hint to the orchestrator about what to do next.
    /// For example `"retry"`, `"continue"`, `"done"`, `"escalate"`.
    pub next_action: String,

    /// Human-readable error message when `success == false`.
    pub error: String,

    /// Number of LLM API calls made during this step.
    pub llm_calls: u32,

    /// Total tokens consumed during this step.
    pub token_usage: u32,
}

impl AgentStepResult {
    /// Construct a successful result.
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            next_action: "continue".to_owned(),
            ..Default::default()
        }
    }

    /// Construct a failure result.
    pub fn fail(error: impl Into<String>) -> Self {
        Self {
            success: false,
            error: error.into(),
            next_action: "retry".to_owned(),
            ..Default::default()
        }
    }

    /// Attach a serialisable artifact.
    pub fn with_artifact(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.artifacts
            .insert(key.into(), serde_json::to_value(value).unwrap_or(Value::Null));
        self
    }
}

// ---------------------------------------------------------------------------
// ReviewOutcome
// ---------------------------------------------------------------------------

/// Verdict returned by [`BaseAgent::review`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewOutcome {
    /// Overall pass/fail decision.
    pub passed: bool,

    /// Score in `[0.0, 1.0]`.
    pub score: f32,

    /// Human-readable feedback from the critic.
    pub feedback: String,

    /// Specific issues identified (empty when `passed == true`).
    pub issues: Vec<String>,

    /// Recommended next action: `"accept"`, `"revise"`, `"reject"`.
    pub recommendation: String,
}

impl ReviewOutcome {
    /// Construct a passing review.
    pub fn accept(score: f32, feedback: impl Into<String>) -> Self {
        Self {
            passed: true,
            score,
            feedback: feedback.into(),
            issues: Vec::new(),
            recommendation: "accept".to_owned(),
        }
    }

    /// Construct a failing review.
    pub fn reject(score: f32, feedback: impl Into<String>, issues: Vec<String>) -> Self {
        Self {
            passed: false,
            score,
            feedback: feedback.into(),
            issues,
            recommendation: "revise".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// BaseAgent trait
// ---------------------------------------------------------------------------

/// Async trait every sub-agent must implement.
///
/// The three-phase lifecycle mirrors the Python base class:
/// 1. [`plan`](BaseAgent::plan) — produce a high-level action plan.
/// 2. [`run`](BaseAgent::run) — execute the plan and return a result.
/// 3. [`review`](BaseAgent::review) — self-evaluate the result.
#[async_trait]
pub trait BaseAgent: Send + Sync {
    /// Agent name used in logging and metrics.
    fn name(&self) -> &str;

    /// Produce an execution plan given the current context.
    async fn plan(&self, context: &AgentContext) -> Result<AgentPlan>;

    /// Execute the provided plan and return a step result.
    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult>;

    /// Evaluate the quality of a completed step result.
    ///
    /// Default implementation always accepts the result with a perfect score.
    async fn review(&self, result: &AgentStepResult) -> Result<ReviewOutcome> {
        if result.success {
            Ok(ReviewOutcome::accept(1.0, "Step completed successfully."))
        } else {
            Ok(ReviewOutcome::reject(
                0.0,
                "Step failed.",
                vec![result.error.clone()],
            ))
        }
    }

    /// Convenience: plan + run + review in a single call.
    ///
    /// Returns the step result; callers can check `success` or the review.
    async fn execute(&self, context: &AgentContext) -> Result<(AgentStepResult, ReviewOutcome)> {
        let plan = self.plan(context).await?;
        let result = self.run(&plan).await?;
        let review = self.review(&result).await?;
        Ok((result, review))
    }
}

// ---------------------------------------------------------------------------
// AgentOrchestrator
// ---------------------------------------------------------------------------

/// Base coordinator for multi-agent workflows.
///
/// Concrete orchestrators (e.g. `BenchmarkOrchestrator`) extend this struct
/// and implement their own async orchestrate method.
#[derive(Debug)]
pub struct AgentOrchestrator {
    /// Maximum number of retry / refinement iterations.
    pub max_iterations: u32,

    /// Cumulative LLM call count across all sub-agents.
    pub total_llm_calls: u32,

    /// Cumulative token usage across all sub-agents.
    pub total_tokens: u32,
}

impl AgentOrchestrator {
    /// Create a new orchestrator with the given iteration limit.
    pub fn new(max_iterations: u32) -> Self {
        Self {
            max_iterations,
            total_llm_calls: 0,
            total_tokens: 0,
        }
    }

    /// Accumulate usage stats from a completed step.
    pub fn accumulate(&mut self, result: &AgentStepResult) {
        self.total_llm_calls += result.llm_calls;
        self.total_tokens += result.token_usage;
    }
}

impl Default for AgentOrchestrator {
    fn default() -> Self {
        Self::new(3)
    }
}
