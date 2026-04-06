//! Multi-round multi-agent discussion engine.
//!
//! Ports the logic from `discussion_runner.py`.  Requires an LLM client.

use anyhow::Result;

// ---------------------------------------------------------------------------
// System prompts
// ---------------------------------------------------------------------------

pub const SYSTEM_PRESENT: &str = "\
You are a senior research coordinator facilitating a multi-agent research discussion.
Multiple research agents have independently studied the same topic and produced their
own literature syntheses. Your task is to analyze each agent's perspective and identify:
1. Key findings unique to each perspective
2. Common themes across perspectives
3. Knowledge gaps that no perspective addressed
4. Contradictions or disagreements between perspectives

Be specific and cite which perspective (Agent A, Agent B, etc.) each point comes from.
Respond in the same language as the syntheses (Chinese if they are in Chinese).";

pub const SYSTEM_CRITIQUE: &str = "\
You are a critical research reviewer. Given the initial analysis of multiple research
perspectives, your task is to:
1. Evaluate the strength of evidence for each key finding
2. Identify potential biases in individual perspectives
3. Find complementary findings that could be combined for stronger conclusions
4. Highlight the most promising research directions that emerge from combining perspectives
5. Note any methodological differences that explain contradictions

Be rigorous and constructive. Respond in the same language as the input.";

pub const SYSTEM_CONSENSUS: &str = "\
You are a research synthesis expert. Based on the multi-round discussion of independent
research perspectives, produce a unified consensus synthesis that:
1. Integrates the strongest findings from all perspectives
2. Resolves contradictions with reasoned explanations
3. Preserves novel insights that appeared in only one perspective
4. Identifies the most promising hypotheses suggested by the combined evidence
5. Notes remaining uncertainties and open questions

Format the output as a well-structured markdown document with clear sections.
This consensus will be used by all agents to generate research hypotheses.
Respond in the same language as the input.";

// ---------------------------------------------------------------------------
// Perspective collection
// ---------------------------------------------------------------------------

/// A named synthesis from one agent.
#[derive(Debug, Clone)]
pub struct Perspective {
    pub label: String,
    pub text: String,
}

/// Build perspectives block for LLM prompt.
fn perspectives_block(perspectives: &[Perspective]) -> String {
    perspectives
        .iter()
        .map(|p| format!("## {}\n\n{}", p.label, p.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn agent_label(index: usize) -> String {
    format!("Agent {}", (b'A' + index as u8) as char)
}

// ---------------------------------------------------------------------------
// Discussion result
// ---------------------------------------------------------------------------

/// Output of a completed discussion.
#[derive(Debug, Clone)]
pub struct DiscussionResult {
    pub transcript: String,
    pub consensus: String,
}

// ---------------------------------------------------------------------------
// LLM trait — callers supply an implementation
// ---------------------------------------------------------------------------

/// Minimal async LLM interface required by the discussion engine.
/// Implementors should call the actual LLM API.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat(&self, system: &str, user: &str, max_tokens: u32) -> Result<String>;
}

// ---------------------------------------------------------------------------
// Core discussion logic
// ---------------------------------------------------------------------------

/// Run a multi-round discussion among the provided perspectives.
///
/// # Rounds
/// - Round 1: Present and analyze all perspectives (`SYSTEM_PRESENT`)
/// - Round 2: Critical review of the initial analysis (`SYSTEM_CRITIQUE`)
/// - Round 3+: Consensus synthesis (`SYSTEM_CONSENSUS`)
pub async fn run_discussion(
    llm: &dyn LlmClient,
    topic: &str,
    perspectives: &[Perspective],
    num_rounds: u32,
) -> Result<DiscussionResult> {
    let block = perspectives_block(perspectives);
    let mut transcript_parts: Vec<String> = Vec::new();

    transcript_parts.push(format!("# Discussion Transcript\n\nTopic: {topic}\n"));
    transcript_parts.push(format!(
        "Participants: {}\n",
        perspectives.iter().map(|p| p.label.as_str()).collect::<Vec<_>>().join(", ")
    ));

    // Round 1: Perspective analysis
    let user_r1 = format!(
        "Research topic: {topic}\n\n\
         The following are independent literature syntheses from {} \
         research agents who studied this topic independently:\n\n\
         {block}\n\n\
         Please analyze these perspectives following your instructions.",
        perspectives.len()
    );
    let analysis = llm.chat(SYSTEM_PRESENT, &user_r1, 4096).await?;
    transcript_parts.push(format!("## Round 1: Perspective Analysis\n\n{analysis}\n"));

    // Round 2: Critical review
    let critique = if num_rounds >= 2 {
        let user_r2 = format!(
            "Original perspectives:\n\n{block}\n\n---\n\nInitial analysis:\n\n{analysis}\n\n\
             Please provide your critical review following your instructions."
        );
        let c = llm.chat(SYSTEM_CRITIQUE, &user_r2, 4096).await?;
        transcript_parts.push(format!("## Round 2: Critical Review\n\n{c}\n"));
        c
    } else {
        analysis.clone()
    };

    // Round 3+: Consensus
    let consensus = if num_rounds >= 3 {
        let user_r3 = format!(
            "Research topic: {topic}\n\n\
             Original perspectives:\n\n{block}\n\n---\n\n\
             Perspective analysis:\n\n{analysis}\n\n---\n\n\
             Critical review:\n\n{critique}\n\n\
             Please produce the consensus synthesis following your instructions."
        );
        let c = llm.chat(SYSTEM_CONSENSUS, &user_r3, 8192).await?;
        transcript_parts.push(format!("## Round 3: Consensus Synthesis\n\n{c}\n"));
        c
    } else {
        // Minimal consensus from analysis alone
        let user_c = format!(
            "Research topic: {topic}\n\n\
             Perspectives:\n\n{block}\n\n\
             Discussion so far:\n\n{critique}\n\n\
             Please produce a unified consensus synthesis."
        );
        let c = llm.chat(SYSTEM_CONSENSUS, &user_c, 8192).await?;
        transcript_parts.push(format!("## Consensus\n\n{c}\n"));
        c
    };

    let transcript = transcript_parts.join("\n---\n\n");
    Ok(DiscussionResult { transcript, consensus })
}

// ---------------------------------------------------------------------------
// Convenience: run discussion from synthesis directories
// ---------------------------------------------------------------------------

/// Read a synthesis file from a stage-07 directory.
pub fn read_synthesis(stage07_dir: &std::path::Path) -> Option<String> {
    for name in ["synthesis.md", "synthesis.txt"] {
        let path = stage07_dir.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Some(text);
        }
    }
    None
}

/// Build perspectives from a list of stage-07 directory paths.
pub fn collect_perspectives(stage_dirs: &[impl AsRef<std::path::Path>]) -> Vec<Perspective> {
    stage_dirs
        .iter()
        .enumerate()
        .filter_map(|(i, dir)| {
            let text = read_synthesis(dir.as_ref())?;
            Some(Perspective { label: agent_label(i), text })
        })
        .collect()
}

/// Write discussion outputs to `output_dir`.
pub fn write_outputs(output_dir: &std::path::Path, result: &DiscussionResult) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_dir)?;
    std::fs::write(output_dir.join("discussion_transcript.md"), &result.transcript)?;
    std::fs::write(output_dir.join("consensus_synthesis.md"), &result.consensus)?;
    Ok(())
}
