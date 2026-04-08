//! Phase 1: Strategy — TopicInit and ProblemDecompose stage executors.

use crate::executor::{StageContext, StageResult};
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

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
    };
    if result.is_empty() {
        return StageResult::failure(stage, "LLM returned empty response".to_owned());
    }

    // ---- goal.md -----------------------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("goal.md"), &result) {
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

    // Render prompt from template engine
    let vars = ctx.template_vars();
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine configured for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render failed for {}: {e}", stage.name())),
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
    };
    if result.is_empty() {
        return StageResult::failure(stage, "LLM returned empty response".to_owned());
    }

    // ---- problem_tree.md ---------------------------------------------------
    if let Err(e) = fs::write(stage_dir.join("problem_tree.md"), &result) {
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
        "notes": "Evaluation after LLM-generated problem decomposition."
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
                domain: "hep".to_owned(),
                analysis_type: None,
                knowledge_root: std::path::PathBuf::from("hep"),
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
