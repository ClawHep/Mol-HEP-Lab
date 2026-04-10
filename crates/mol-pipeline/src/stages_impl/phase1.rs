//! Phase 1: Strategy — TopicInit and ProblemDecompose stage executors.

use crate::executor::{StageContext, StageResult};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// TopicInit
// ---------------------------------------------------------------------------

/// Execute the TopicInit stage via agentic executor.
///
/// Produces `goal.md` (SMART research goal) via agentic extraction, plus
/// code-driven `hardware_profile.json`.
pub async fn execute_topic_init(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "goal.md".into(),
            description: "SMART research goal statement — specific, measurable, achievable, \
                relevant, and time-bound research objective with success criteria".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Code-driven: hardware_profile.json
    let stage_dir = ctx.stage_dir(stage);
    let hardware_json = serde_json::json!({
        "detected_at": crate::executor::utcnow_iso(),
        "cpu": {
            "cores": num_cpus(),
            "architecture": std::env::consts::ARCH,
        },
        "os": std::env::consts::OS,
        "gpu": detect_gpu(),
        "memory_gb": detect_memory_gb(),
        "notes": "Hardware profile generated at TopicInit. Actual GPU availability checked at ExperimentCycle."
    });
    if let Err(e) = fs::write(
        stage_dir.join("hardware_profile.json"),
        serde_json::to_string_pretty(&hardware_json).unwrap_or_default(),
    ) {
        return StageResult::failure(stage, format!("write hardware_profile.json: {e}"));
    }
    result.artifacts.push("hardware_profile.json".to_owned());

    result
}

// ---------------------------------------------------------------------------
// ProblemDecompose
// ---------------------------------------------------------------------------

/// Execute the ProblemDecompose stage via agentic executor.
///
/// Produces `problem_tree.md` via agentic extraction, plus code-driven
/// `topic_evaluation.json` with LLM-generated evaluation scores.
pub async fn execute_problem_decompose(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "problem_tree.md".into(),
            description: "hierarchical problem decomposition tree — break the research goal \
                into sub-problems, each with scope, approach, dependencies, and success criteria"
                .into(),
        },
        ArtifactSpec {
            filename: "topic_evaluation.md".into(),
            description: "topic feasibility evaluation — scores for feasibility, novelty, \
                impact, resource requirements, and estimated timeline".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
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
                domain: "hep".to_owned(),
                analysis_type: None,
                knowledge_chain: mol_common::KnowledgeChain::new(vec![std::path::PathBuf::from("hep"), std::path::PathBuf::from("generic")]),
                datasets_dir: String::new(),
            },
            prior_artifacts: HashMap::new(),
            auto_approve_gates: false,
            llm: None,
            prompt_engine: None,
        }
    }

    #[tokio::test]
    async fn topic_init_no_engine_returns_failure() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "deep learning for drug discovery");
        let result = execute_topic_init(Stage::TopicInit, &ctx).await;

        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }

    #[tokio::test]
    async fn problem_decompose_no_engine_returns_failure() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "neural network optimization");

        let result = execute_problem_decompose(Stage::ProblemDecompose, &ctx).await;
        assert_eq!(result.status, StageStatus::Failed);
        assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
    }
}
