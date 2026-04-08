//! PRM (Process Reward Model) quality gate for MetaMol.
//!
//! Uses an LLM-as-judge approach with majority voting to evaluate the quality
//! of pipeline stage outputs at key gate points.

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::task::JoinSet;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Stage-specific evaluation instructions
// ---------------------------------------------------------------------------

fn gate_instruction(stage: u32) -> &'static str {
    match stage {
        5 => {
            "Evaluate the quality of a literature screening result for academic research. \
             Check: (1) Are the selected papers relevant to the research topic? \
             (2) Is there sufficient coverage of key approaches? \
             (3) Are low-quality or irrelevant papers properly filtered out?"
        }
        9 => {
            "Evaluate the quality of an experiment design for academic research. \
             Check: (1) Are there proper baselines for comparison? \
             (2) Are ablation studies planned? \
             (3) Are statistical methods and metrics well-chosen? \
             (4) Is the experiment reproducible?"
        }
        15 => {
            "Evaluate whether a research PROCEED/PIVOT decision is well-justified. \
             Check: (1) Is there sufficient evidence to support the decision? \
             (2) Are alternative interpretations considered? \
             (3) Is the rationale logically sound?"
        }
        20 => {
            "Evaluate the overall quality of an academic paper. \
             Check: (1) Is the contribution novel and clearly stated? \
             (2) Is the methodology sound and well-described? \
             (3) Do the experiments adequately support the claims? \
             (4) Is the writing clear and well-structured?"
        }
        _ => "Evaluate the quality and correctness of this research pipeline output.",
    }
}

const JUDGE_SYSTEM: &str = "\
You are a quality reviewer for an automated academic research pipeline.
Based on the evaluation criteria and the provided output, decide:
  +1 = clearly meets quality standards and is ready to proceed
  -1 = fails core requirements or has critical issues
   0 = ambiguous or insufficient evidence to decide

Respond with ONLY \"Score: 1\", \"Score: -1\", or \"Score: 0\" on the first line,
followed by a brief justification.";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the PRM quality gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRMConfig {
    /// Base URL for the LLM judge API (OpenAI-compatible).
    pub api_base: String,

    /// API key for the judge endpoint.
    pub api_key: String,

    /// Model to use for judging (default: `"gpt-4o"`).
    pub model: String,

    /// Number of parallel judge votes (default: 3).
    pub votes: usize,

    /// Sampling temperature for judge calls (default: 0.6).
    pub temperature: f32,

    /// Pipeline stage numbers at which the gate is active (default: 5, 9, 15, 20).
    pub gate_stages: Vec<u32>,
}

impl Default for PRMConfig {
    fn default() -> Self {
        Self {
            api_base: String::new(),
            api_key: String::new(),
            model: "gpt-4o".to_owned(),
            votes: 3,
            temperature: 0.6,
            gate_stages: vec![5, 9, 15, 20],
        }
    }
}

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Result of a PRM evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRMResult {
    /// Whether the stage passed (majority score > 0).
    pub passed: bool,

    /// Majority-vote score: -1.0, 0.0, or 1.0.
    pub score: f32,

    /// Individual vote scores collected from the judges.
    pub votes: Vec<f32>,

    /// Feedback text extracted from the first judge that provided a
    /// justification.
    pub feedback: String,
}

// ---------------------------------------------------------------------------
// Gate implementation
// ---------------------------------------------------------------------------

/// PRM quality gate using majority-vote LLM-as-judge scoring.
#[derive(Debug, Clone)]
pub struct ResearchPRMGate {
    config: PRMConfig,
    http: Client,
}

impl ResearchPRMGate {
    /// Create a new gate from the given configuration.
    pub fn new(config: PRMConfig) -> Self {
        Self {
            config,
            http: Client::new(),
        }
    }

    /// Returns `true` when the gate is configured to run at the given stage.
    pub fn should_gate(&self, stage: u32) -> bool {
        self.config.gate_stages.contains(&stage)
    }

    /// Evaluate `content` for `stage` using parallel majority-vote judging.
    ///
    /// Returns a [`PRMResult`] regardless of individual judge failures;
    /// score defaults to `0.0` (ambiguous) when all judge calls fail.
    pub async fn evaluate(&self, stage: u32, content: &str) -> Result<PRMResult> {
        let instruction = gate_instruction(stage);
        let votes = self.config.votes.max(1);
        let content_truncated = &content[..content.len().min(6000)];

        let mut join_set: JoinSet<Option<(f32, String)>> = JoinSet::new();
        for _ in 0..votes {
            let http = self.http.clone();
            let api_base = self.config.api_base.clone();
            let api_key = self.config.api_key.clone();
            let model = self.config.model.clone();
            let temperature = self.config.temperature;
            let instruction = instruction.to_owned();
            let content_owned = content_truncated.to_owned();

            join_set.spawn(async move {
                single_judge_call(
                    &http,
                    &api_base,
                    &api_key,
                    &model,
                    &instruction,
                    &content_owned,
                    temperature,
                )
                .await
                .ok()
                .flatten()
            });
        }

        let mut scores: Vec<f32> = Vec::new();
        let mut feedback = String::new();

        while let Some(result) = join_set.join_next().await {
            if let Ok(Some((score, justification))) = result {
                scores.push(score);
                if feedback.is_empty() && !justification.is_empty() {
                    feedback = justification;
                }
            }
        }

        if scores.is_empty() {
            warn!(stage, "All PRM judge calls failed; returning ambiguous score");
            return Ok(PRMResult {
                passed: false,
                score: 0.0,
                votes: Vec::new(),
                feedback: "All judge calls failed.".to_owned(),
            });
        }

        let majority = majority_vote(&scores);
        let passed = majority > 0.0;

        info!(stage, score = majority, vote_count = scores.len(), "PRM evaluation complete");

        Ok(PRMResult {
            passed,
            score: majority,
            votes: scores,
            feedback,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Issue a single judge call and return `(score, justification)` on success.
async fn single_judge_call(
    http: &Client,
    api_base: &str,
    api_key: &str,
    model: &str,
    instruction: &str,
    output_text: &str,
    temperature: f32,
) -> Result<Option<(f32, String)>> {
    let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));

    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": JUDGE_SYSTEM},
            {"role": "user", "content": format!(
                "## Evaluation Criteria\n{instruction}\n\n## Output to Evaluate\n{output_text}"
            )},
        ],
        "temperature": temperature,
        "max_completion_tokens": 512,
    });

    let response = http
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Judge API error HTTP {status}: {text}"));
    }

    let data: Value = response.json().await?;
    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_owned();

    let score = parse_score(&content);
    let justification = parse_justification(&content);

    Ok(score.map(|s| (s, justification)))
}

/// Parse the `Score: X` token from judge output.
fn parse_score(text: &str) -> Option<f32> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Score:") {
            let val = rest.trim();
            return match val {
                "1" | "+1" => Some(1.0),
                "-1" => Some(-1.0),
                "0" => Some(0.0),
                _ => None,
            };
        }
    }
    None
}

/// Extract everything after the first line as justification text.
fn parse_justification(text: &str) -> String {
    text.lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Return the majority vote among `scores`.  Ties resolve to `0.0`.
fn majority_vote(scores: &[f32]) -> f32 {
    let pos = scores.iter().filter(|&&s| s > 0.0).count();
    let neg = scores.iter().filter(|&&s| s < 0.0).count();
    let zero = scores.len() - pos - neg;

    if pos > neg && pos > zero {
        1.0
    } else if neg > pos && neg > zero {
        -1.0
    } else if zero > pos && zero > neg {
        0.0
    } else {
        // Tie — ambiguous.
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majority_vote_positive() {
        assert_eq!(majority_vote(&[1.0, 1.0, -1.0]), 1.0);
    }

    #[test]
    fn majority_vote_negative() {
        assert_eq!(majority_vote(&[-1.0, -1.0, 1.0]), -1.0);
    }

    #[test]
    fn majority_vote_tie_is_ambiguous() {
        assert_eq!(majority_vote(&[1.0, -1.0]), 0.0);
    }

    #[test]
    fn parse_score_plus_one() {
        assert_eq!(parse_score("Score: 1\nGood output."), Some(1.0));
    }

    #[test]
    fn parse_score_minus_one() {
        assert_eq!(parse_score("Score: -1\nPoor quality."), Some(-1.0));
    }

    #[test]
    fn should_gate_known_stages() {
        let gate = ResearchPRMGate::new(PRMConfig::default());
        assert!(gate.should_gate(5));
        assert!(gate.should_gate(20));
        assert!(!gate.should_gate(1));
    }
}
