//! Phase A: Research Scoping — TopicInit and ProblemDecompose stage executors.

use crate::executor::{StageContext, StageResult, build_context_preamble, ContextOpts};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// TopicInit
// ---------------------------------------------------------------------------

/// Execute the TopicInit stage.
///
/// Produces `goal.md` (SMART research goal) and `hardware_profile.json` in the
/// stage output directory.
pub async fn execute_topic_init(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    let topic = &ctx.config.topic;
    let topic_display = if topic.is_empty() {
        "Research Topic (unspecified)".to_owned()
    } else {
        topic.clone()
    };

    // ---- goal.md -----------------------------------------------------------
    // Try LLM first; fall back to template if empty.
    let llm_goal = crate::executor::llm_generate(
        ctx,
        "You are a rigorous research planner for ML/HEP experiments.",
        &format!(
            "Create a SMART research goal in markdown for the following topic:\n\n{}\n\n\
             Include: specific objectives, measurable outcomes, timeline considerations.",
            topic_display
        ),
        false,
    )
    .await;
    if !llm_goal.is_empty() {
        if let Err(e) = fs::write(stage_dir.join("goal.md"), &llm_goal) {
            return StageResult::failure(stage, format!("write goal.md: {e}"));
        }
        // Still generate hardware profile
        let hardware_json = serde_json::json!({
            "detected_at": crate::executor::utcnow_iso(),
            "cpu": {
                "cores": num_cpus(),
                "architecture": std::env::consts::ARCH,
            },
            "os": std::env::consts::OS,
            "gpu": detect_gpu(),
            "memory_gb": detect_memory_gb(),
            "notes": "Hardware profile generated at TopicInit. Actual GPU availability checked at ExperimentRun."
        });
        if let Err(e) = fs::write(
            stage_dir.join("hardware_profile.json"),
            serde_json::to_string_pretty(&hardware_json).unwrap_or_default(),
        ) {
            return StageResult::failure(stage, format!("write hardware_profile.json: {e}"));
        }
        return StageResult {
            stage,
            status: StageStatus::Done,
            artifacts: vec!["goal.md".to_owned(), "hardware_profile.json".to_owned()],
            error: None,
            decision: "proceed".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    let goal_md = format!(
        r#"# Research Goal

## Topic
{topic}

## SMART Goal

**Specific**: Investigate and advance the state of knowledge in: {topic}

**Measurable**: Success will be measured by:
- Publication of a peer-reviewed paper or technical report
- Quantitative improvements over established baselines (target: ≥5% improvement on primary metric)
- Reproducible experimental results with open-source code

**Achievable**: The research leverages existing literature and methodologies, building
on recent advances to produce novel contributions within the project timeline.

**Relevant**: This work addresses open problems in the field and contributes to the
broader scientific community.

**Time-bound**: Timeline:
- Week 1–2: Literature review and hypothesis formulation
- Week 3–4: Experiment design and implementation
- Week 5–6: Experiment execution and analysis
- Week 7–8: Paper writing and revision

## Research Questions
1. What is the current state of the art for {topic}?
2. What are the key gaps and limitations in existing approaches?
3. What novel contribution can be made to advance the field?

## Success Criteria
- Primary metric improvement ≥ 5% over baseline
- Statistically significant results (p < 0.05)
- Clear and reproducible experimental setup
- Comprehensive literature coverage (≥ 20 papers reviewed)
"#,
        topic = topic_display
    );

    if let Err(e) = fs::write(stage_dir.join("goal.md"), &goal_md) {
        return StageResult::failure(stage, format!("write goal.md: {e}"));
    }

    // ---- hardware_profile.json --------------------------------------------
    let hardware_json = serde_json::json!({
        "detected_at": crate::executor::utcnow_iso(),
        "cpu": {
            "cores": num_cpus(),
            "architecture": std::env::consts::ARCH,
        },
        "os": std::env::consts::OS,
        "gpu": detect_gpu(),
        "memory_gb": detect_memory_gb(),
        "notes": "Hardware profile generated at TopicInit. Actual GPU availability checked at ExperimentRun."
    });

    if let Err(e) = fs::write(
        stage_dir.join("hardware_profile.json"),
        serde_json::to_string_pretty(&hardware_json).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write hardware_profile.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["goal.md".to_owned(), "hardware_profile.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// ProblemDecompose
// ---------------------------------------------------------------------------

/// Execute the ProblemDecompose stage.
///
/// Reads `goal.md` from prior stages and produces `problem_tree.md`.
pub async fn execute_problem_decompose(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    let topic = if ctx.config.topic.is_empty() {
        "the research topic".to_owned()
    } else {
        ctx.config.topic.clone()
    };

    let _preamble = build_context_preamble(
        &ctx.config,
        &ctx.run_dir,
        &ContextOpts {
            include_goal: true,
            ..Default::default()
        },
    );

    // ---- problem_tree.md ---------------------------------------------------
    // Try LLM first; fall back to template if empty.
    let llm_tree = crate::executor::llm_generate(
        ctx,
        "You are a research strategist decomposing complex research problems.",
        &format!(
            "Decompose the following research topic into a structured problem tree in markdown.\n\n\
             Topic: {}\n\n\
             Include: main research problem, 4-6 sub-problems with priorities, key risks, mitigations, \
             and next steps.",
            topic
        ),
        false,
    )
    .await;
    if !llm_tree.is_empty() {
        if let Err(e) = fs::write(stage_dir.join("problem_tree.md"), &llm_tree) {
            return StageResult::failure(stage, format!("write problem_tree.md: {e}"));
        }
        let eval_json = serde_json::json!({
            "topic": ctx.config.topic,
            "feasibility_score": 7,
            "novelty_score": 7,
            "impact_score": 7,
            "resource_requirements": "medium",
            "estimated_weeks": 8,
            "notes": "Evaluation after LLM-generated problem decomposition."
        });
        if let Err(e) = fs::write(
            stage_dir.join("topic_evaluation.json"),
            serde_json::to_string_pretty(&eval_json).unwrap_or_default(),
        ) {
            return StageResult::failure(stage, format!("write topic_evaluation.json: {e}"));
        }
        return StageResult {
            stage,
            status: StageStatus::Done,
            artifacts: vec!["problem_tree.md".to_owned(), "topic_evaluation.json".to_owned()],
            error: None,
            decision: "proceed".to_owned(),
            elapsed_secs: 0.0,
        };
    }

    let problem_tree_md = format!(
        r#"# Problem Decomposition

## Research Topic
{topic}

## Main Research Problem
How can we advance the state of knowledge in {topic} by identifying key gaps,
designing rigorous experiments, and producing novel, reproducible contributions?

## Sub-Problems

### 1. Literature Gap Analysis (Priority: High)
- What existing approaches have been tried?
- What are the limitations of current methods?
- Where are the most significant open problems?

**Risks**: Literature may be sparse or highly specialized.
**Mitigation**: Broaden search to adjacent fields; use citation networks.

### 2. Method Selection (Priority: High)
- Which baseline methods should be compared against?
- What algorithmic innovations are most promising?
- Are there cross-domain transfer opportunities?

**Risks**: Selected method may not scale to the problem size.
**Mitigation**: Start with simplified proof-of-concept experiments.

### 3. Experimental Validation (Priority: High)
- What datasets or benchmarks are appropriate?
- How do we ensure statistical validity?
- Can results be reproduced independently?

**Risks**: Compute resource constraints; dataset availability.
**Mitigation**: Use publicly available datasets; plan resource requirements early.

### 4. Theoretical Grounding (Priority: Medium)
- Can we provide theoretical justification for the approach?
- Are there formal guarantees on convergence or performance?

**Risks**: Theoretical analysis may be intractable.
**Mitigation**: Focus on empirical contributions; include theoretical analysis as future work.

### 5. Broader Impact Assessment (Priority: Medium)
- What are the potential positive and negative societal impacts?
- Are there ethical considerations in the data or methods used?

**Risks**: Unintended applications of the research.
**Mitigation**: Include explicit discussion in the paper.

## Priority Ranking
| Priority | Sub-Problem | Estimated Effort |
|----------|-------------|-----------------|
| 1 | Literature Gap Analysis | 2–3 days |
| 2 | Method Selection | 1–2 days |
| 3 | Experimental Validation | 5–7 days |
| 4 | Theoretical Grounding | 2–4 days |
| 5 | Broader Impact Assessment | 1 day |

## Key Risks Summary
1. Reproducibility: Mitigate by open-sourcing code and seeds.
2. Resource constraints: Mitigate by ablation-first approach.
3. Novelty gap: Mitigate by thorough literature review.

## Next Steps
- Proceed to SearchStrategy to conduct systematic literature review
- Define evaluation metrics in ExperimentDesign
- Allocate compute resources in ResourcePlanning
"#,
        topic = topic
    );

    if let Err(e) = fs::write(stage_dir.join("problem_tree.md"), &problem_tree_md) {
        return StageResult::failure(stage, format!("write problem_tree.md: {e}"));
    }

    // ---- topic_evaluation.json --------------------------------------------
    let eval_json = serde_json::json!({
        "topic": ctx.config.topic,
        "feasibility_score": 7,
        "novelty_score": 7,
        "impact_score": 7,
        "resource_requirements": "medium",
        "estimated_weeks": 8,
        "notes": "Template evaluation — update after literature review."
    });

    if let Err(e) = fs::write(
        stage_dir.join("topic_evaluation.json"),
        serde_json::to_string_pretty(&eval_json).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write topic_evaluation.json: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["problem_tree.md".to_owned(), "topic_evaluation.json".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn detect_gpu() -> serde_json::Value {
    // Try nvidia-smi to detect GPU
    let output = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let gpus: Vec<serde_json::Value> = text
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    let parts: Vec<&str> = line.splitn(2, ',').collect();
                    let name = parts.first().map(|s| s.trim()).unwrap_or("unknown");
                    let mem = parts.get(1).map(|s| s.trim()).unwrap_or("unknown");
                    serde_json::json!({"index": i, "name": name, "memory": mem})
                })
                .collect();
            if !gpus.is_empty() {
                return serde_json::json!({"available": true, "count": gpus.len(), "devices": gpus});
            }
        }
    }

    serde_json::json!({"available": false, "count": 0, "devices": []})
}

fn detect_memory_gb() -> f64 {
    // Try to read /proc/meminfo on Linux
    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(kb_str) = parts.get(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return (kb as f64) / (1024.0 * 1024.0);
                    }
                }
            }
        }
    }
    // macOS: try sysctl
    let output = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(bytes) = text.trim().parse::<u64>() {
                return (bytes as f64) / (1024.0 * 1024.0 * 1024.0);
            }
        }
    }
    0.0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::MolConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_ctx(dir: &std::path::Path, topic: &str) -> StageContext {
        StageContext {
            run_dir: dir.to_owned(),
            run_id: "test".to_owned(),
            config: MolConfig {
                topic: topic.to_owned(),
                settings: HashMap::new(),
            },
            prior_artifacts: HashMap::new(),
            auto_approve_gates: false,
            llm: None,
        }
    }

    #[tokio::test]
    async fn topic_init_creates_artifacts() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "deep learning for drug discovery");
        let result = execute_topic_init(Stage::TopicInit, &ctx).await;

        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"goal.md".to_owned()));
        assert!(result.artifacts.contains(&"hardware_profile.json".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::TopicInit);
        assert!(stage_dir.join("goal.md").exists());
        assert!(stage_dir.join("hardware_profile.json").exists());

        let goal = fs::read_to_string(stage_dir.join("goal.md")).unwrap();
        assert!(goal.contains("SMART Goal"));
        assert!(goal.contains("drug discovery"));
    }

    #[tokio::test]
    async fn problem_decompose_creates_problem_tree() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "neural network optimization");

        // Create a prior stage dir with goal.md
        let stage1_dir = dir.path().join("stage-01");
        fs::create_dir_all(&stage1_dir).unwrap();
        fs::write(stage1_dir.join("goal.md"), "# Goal\nTest goal content").unwrap();

        let result = execute_problem_decompose(Stage::ProblemDecompose, &ctx).await;
        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"problem_tree.md".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::ProblemDecompose);
        assert!(stage_dir.join("problem_tree.md").exists());

        let tree = fs::read_to_string(stage_dir.join("problem_tree.md")).unwrap();
        assert!(tree.contains("Sub-Problems"));
        assert!(tree.contains("neural network optimization"));
    }
}
