//! Stage dispatch and execution.
//!
//! `execute_stage` is the single dispatch point that the runner calls for
//! every pipeline stage.  Individual stage implementations live (or will
//! live) in domain-specific crates; for now every stage returns a stub
//! `StageResult` so the pipeline wiring can be exercised end-to-end.

use crate::stages::{Stage, StageStatus};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// TODO: replace with real mol-config / mol-llm types once those crates
// expose a stable public API.
// ---------------------------------------------------------------------------

/// Minimal configuration surface used by stage executors.
///
/// TODO: replace with `mol_config::MolConfig` once that crate is stable.
#[derive(Debug, Clone)]
pub struct MolConfig {
    /// Research topic / title.
    pub topic: String,
    /// Free-form key-value settings forwarded to individual stage executors.
    pub settings: HashMap<String, String>,
}

impl Default for MolConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            settings: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// StageResult
// ---------------------------------------------------------------------------

/// Outcome of executing a single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// The stage that was executed.
    pub stage: Stage,
    /// Final execution status.
    pub status: StageStatus,
    /// Names of artifacts produced (relative to the stage output directory).
    pub artifacts: Vec<String>,
    /// Error message when `status` is `Failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Decision token emitted by the stage: `"proceed"`, `"pivot"`,
    /// `"refine"`, `"degraded"`, etc.
    pub decision: String,
    /// Wall-clock execution time in seconds.
    pub elapsed_secs: f64,
}

impl StageResult {
    /// Create a successful stub result with no artifacts.
    pub fn stub_success(stage: Stage) -> Self {
        Self {
            stage,
            status: StageStatus::Done,
            artifacts: Vec::new(),
            error: None,
            decision: "proceed".to_owned(),
            elapsed_secs: 0.0,
        }
    }

    /// Create a failed result.
    pub fn failure(stage: Stage, error: impl Into<String>) -> Self {
        Self {
            stage,
            status: StageStatus::Failed,
            artifacts: Vec::new(),
            error: Some(error.into()),
            decision: "retry".to_owned(),
            elapsed_secs: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// StageContext
// ---------------------------------------------------------------------------

/// Runtime context passed to every stage executor.
///
/// Carries the run directory, configuration, and a snapshot of all artifacts
/// produced by earlier stages.
#[derive(Debug, Clone)]
pub struct StageContext {
    /// Root directory for this pipeline run (e.g. `runs/run-abc123/`).
    pub run_dir: PathBuf,
    /// Run identifier string.
    pub run_id: String,
    /// Pipeline configuration.
    pub config: MolConfig,
    /// Artifacts produced by stages that completed before this one.
    /// Keys are artifact names; values are paths relative to `run_dir`.
    pub prior_artifacts: HashMap<String, PathBuf>,
    /// Whether gate stages should be auto-approved without HITL interaction.
    pub auto_approve_gates: bool,
}

impl StageContext {
    /// Return the output directory for `stage` within this run.
    pub fn stage_dir(&self, stage: Stage) -> PathBuf {
        self.run_dir.join(format!("stage-{:02}", stage.as_i32()))
    }
}

// ---------------------------------------------------------------------------
// Context options for build_context_preamble
// ---------------------------------------------------------------------------

/// Controls which optional sections are included in the context preamble.
#[derive(Debug, Clone)]
pub struct ContextOpts {
    pub include_goal: bool,
    pub include_hypotheses: bool,
    pub include_synthesis: bool,
    pub include_exp_plan: bool,
    pub include_analysis: bool,
    pub include_decision: bool,
}

impl Default for ContextOpts {
    fn default() -> Self {
        Self {
            include_goal: false,
            include_hypotheses: false,
            include_synthesis: false,
            include_exp_plan: false,
            include_analysis: false,
            include_decision: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Infrastructure helpers
// ---------------------------------------------------------------------------

/// Scan stage directories in `run_dir` backward (highest stage number first)
/// and return the string content of the first directory that contains
/// `filename`.  Versioned dirs like `stage-13_v1` are tried after their
/// non-versioned counterpart at the same stage number.
fn read_prior_artifact(run_dir: &Path, filename: &str) -> Option<String> {
    let mut stage_dirs: Vec<_> = std::fs::read_dir(run_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("stage-"))
        .collect();

    // Sort: highest stage number first, non-versioned before versioned within
    // the same base name.
    stage_dirs.sort_by(|a, b| {
        let an = a.file_name().to_string_lossy().to_string();
        let bn = b.file_name().to_string_lossy().to_string();

        fn sort_key(name: &str) -> (String, i32) {
            if let Some((base, ver)) = name.rsplit_once("_v") {
                if let Ok(v) = ver.parse::<i32>() {
                    return (base.to_string(), -v);
                }
            }
            (name.to_string(), 0)
        }

        let (ab, av) = sort_key(&an);
        let (bb, bv) = sort_key(&bn);
        bb.cmp(&ab).then(bv.cmp(&av))
    });

    for entry in stage_dirs {
        let candidate = entry.path().join(filename);
        if candidate.is_file() {
            return std::fs::read_to_string(&candidate).ok();
        }
    }
    None
}

/// Same as [`read_prior_artifact`] but returns the `PathBuf` instead of the
/// file content.
fn find_prior_file(run_dir: &Path, filename: &str) -> Option<PathBuf> {
    let mut stage_dirs: Vec<_> = std::fs::read_dir(run_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("stage-"))
        .collect();

    stage_dirs.sort_by(|a, b| {
        let an = a.file_name().to_string_lossy().to_string();
        let bn = b.file_name().to_string_lossy().to_string();

        fn sort_key(name: &str) -> (String, i32) {
            if let Some((base, ver)) = name.rsplit_once("_v") {
                if let Ok(v) = ver.parse::<i32>() {
                    return (base.to_string(), -v);
                }
            }
            (name.to_string(), 0)
        }

        let (ab, av) = sort_key(&an);
        let (bb, bv) = sort_key(&bn);
        bb.cmp(&ab).then(bv.cmp(&av))
    });

    for entry in stage_dirs {
        let candidate = entry.path().join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Extract a YAML block from `text` which may contain LLM/ACP noise.
///
/// Tries in order: ` ```yaml ` fence → ` ```yml ` fence → bare ` ``` ` fence
/// → raw YAML detection (lines starting with `word:`).
fn extract_yaml_block(text: &str) -> String {
    // Try ```yaml fence
    if let Some(start) = text.find("```yaml") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Try ```yml fence
    if let Some(start) = text.find("```yml") {
        let after = &text[start + 6..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Try bare ``` fence
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            let block = after[..end].trim();
            // Only treat as YAML if it looks like YAML (contains key: value)
            if block.contains(": ") || block.starts_with('-') {
                return block.to_string();
            }
        }
    }
    // Fall back: collect lines that look like raw YAML
    let yaml_lines: Vec<&str> = text
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && (t.starts_with('-')
                    || t.contains(": ")
                    || t.ends_with(':'))
        })
        .collect();
    if !yaml_lines.is_empty() {
        return yaml_lines.join("\n");
    }
    text.to_string()
}

/// Multi-strategy JSON parsing.
///
/// 1. Direct parse.
/// 2. Extract from ` ```json ` fences.
/// 3. Balanced-brace matching (largest `{}` block first).
/// 4. Balanced-bracket matching for arrays.
fn safe_json_loads(text: &str) -> Option<serde_json::Value> {
    // 1. Direct parse
    if let Ok(v) = serde_json::from_str(text.trim()) {
        return Some(v);
    }

    // 2. Extract from ```json fence
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            let block = after[..end].trim();
            if let Ok(v) = serde_json::from_str(block) {
                return Some(v);
            }
        }
    }

    // 3. Balanced brace matching – collect all {…} spans, try largest first
    let mut brace_spans: Vec<(usize, usize)> = Vec::new();
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    brace_spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    }
    // Sort by span length descending
    brace_spans.sort_by_key(|(s, e)| std::cmp::Reverse(e - s));
    for (s, e) in brace_spans {
        if let Ok(v) = serde_json::from_str(&text[s..e]) {
            return Some(v);
        }
    }

    // 4. Balanced bracket matching for arrays
    let mut bracket_spans: Vec<(usize, usize)> = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            b']' => {
                depth -= 1;
                if depth == 0 {
                    bracket_spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    }
    bracket_spans.sort_by_key(|(s, e)| std::cmp::Reverse(e - s));
    for (s, e) in bracket_spans {
        if let Ok(v) = serde_json::from_str(&text[s..e]) {
            return Some(v);
        }
    }

    None
}

/// Detect the research domain for a given topic string.
///
/// Returns `(domain_id, display_name, top_venues)`.  Keyword matching is used
/// against 7 domains.  Theoretical-intent words boost non-empirical domains.
fn detect_domain(topic: &str) -> (&'static str, &'static str, &'static str) {
    let lower = topic.to_lowercase();

    // Theoretical intent words
    let theoretical_words = [
        "derive",
        "prove",
        "mathematical formulation",
        "theorem",
        "proof",
        "axiom",
        "formal",
        "theoretical",
        "conjecture",
        "lemma",
    ];
    let has_theoretical_intent = theoretical_words.iter().any(|w| lower.contains(w));

    // Domain keyword tables: (domain_id, display_name, top_venues, keywords)
    let domains: &[(&str, &str, &str, &[&str])] = &[
        (
            "ml",
            "Machine Learning / AI",
            "NeurIPS, ICML, ICLR, CVPR, ACL",
            &[
                "machine learning",
                "deep learning",
                "neural network",
                "transformer",
                "diffusion",
                "llm",
                "language model",
                "reinforcement learning",
                "computer vision",
                "nlp",
                "classification",
                "embedding",
                "attention",
                "generative",
                "gpt",
                "bert",
            ],
        ),
        (
            "physics",
            "Physics",
            "Physical Review Letters, Nature Physics, JHEP",
            &[
                "quantum",
                "thermodynamic",
                "particle",
                "relativity",
                "hamiltonian",
                "lagrangian",
                "field theory",
                "simulation",
                "monte carlo",
                "schrödinger",
                "entropy",
                "condensed matter",
                "optics",
                "photon",
                "boson",
                "fermion",
            ],
        ),
        (
            "chemistry",
            "Chemistry",
            "JACS, Angewandte Chemie, Nature Chemistry",
            &[
                "molecule",
                "reaction",
                "synthesis",
                "catalyst",
                "protein",
                "drug",
                "binding",
                "docking",
                "force field",
                "md simulation",
                "chemical",
                "compound",
                "ligand",
                "polymer",
                "spectroscopy",
            ],
        ),
        (
            "economics",
            "Economics",
            "AER, QJE, Econometrica, JPE",
            &[
                "econom",
                "market",
                "equilibrium",
                "welfare",
                "game theory",
                "auction",
                "mechanism design",
                "fiscal",
                "monetary",
                "utility",
                "demand",
                "supply",
                "price",
                "gdp",
                "trade",
            ],
        ),
        (
            "mathematics",
            "Mathematics",
            "Annals of Mathematics, JAMS, Inventiones",
            &[
                "topology",
                "algebra",
                "geometry",
                "combinatorics",
                "number theory",
                "calculus",
                "manifold",
                "group theory",
                "category theory",
                "stochastic",
                "differential equation",
                "probability",
                "statistics",
                "analysis",
                "metric space",
                "mathematical formulation",
                "mathematical",
                "formulation",
            ],
        ),
        (
            "engineering",
            "Engineering",
            "IEEE Transactions, ASME, ACM TOCS",
            &[
                "control",
                "robot",
                "embedded",
                "hardware",
                "circuit",
                "signal processing",
                "sensor",
                "actuator",
                "autonomous",
                "mechanical",
                "thermal",
                "structural",
                "antenna",
                "network",
                "fault",
            ],
        ),
        (
            "biology",
            "Biology",
            "Cell, Nature, Science, PNAS",
            &[
                "gene",
                "genome",
                "evolution",
                "cell",
                "neuron",
                "neuroscience",
                "ecology",
                "metabol",
                "crispr",
                "sequencing",
                "mrna",
                "pathogen",
                "bacteria",
                "virus",
                "organism",
            ],
        ),
    ];

    // Score each domain
    let mut best_id = "ml";
    let mut best_name = "Machine Learning / AI";
    let mut best_venues = "NeurIPS, ICML, ICLR, CVPR, ACL";
    let mut best_score = 0usize;

    for &(id, name, venues, keywords) in domains {
        let mut score = keywords.iter().filter(|kw| lower.contains(*kw)).count();
        // Boost non-empirical domains for theoretical intent
        if has_theoretical_intent && matches!(id, "mathematics" | "physics" | "economics") {
            score += 3;
        }
        if score > best_score {
            best_score = score;
            best_id = id;
            best_name = name;
            best_venues = venues;
        }
    }

    (best_id, best_name, best_venues)
}

/// Build fallback search queries for a topic, handling mixed Chinese-English.
fn build_fallback_queries(topic: &str) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();

    // Chinese-to-English domain mapping
    let zh_map: &[(&str, &[&str])] = &[
        ("具身智能", &["embodied intelligence", "embodied AI"]),
        ("视觉语言动作", &["vision language action", "VLA"]),
        ("世界模型", &["world model"]),
        ("机器人", &["robot", "robotics"]),
        ("强化学习", &["reinforcement learning"]),
        ("扩散策略", &["diffusion policy"]),
        ("自主代理", &["autonomous agent"]),
        ("大语言模型", &["large language model", "LLM"]),
        ("神经网络", &["neural network"]),
        ("深度学习", &["deep learning"]),
        ("迁移学习", &["transfer learning"]),
        ("自监督", &["self-supervised"]),
        ("多模态", &["multimodal"]),
        ("语言模型", &["language model"]),
        ("视觉", &["vision"]),
        ("最新研究", &["recent advances", "survey"]),
    ];

    // Check if the topic contains Chinese characters
    let has_chinese = topic.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));

    if has_chinese {
        // Collect English tokens already in the topic
        let eng_tokens: Vec<&str> = topic
            .split(|c: char| !c.is_ascii_alphabetic() && c != ' ')
            .flat_map(|s| s.split_whitespace())
            .filter(|s| !s.is_empty())
            .collect();

        if !eng_tokens.is_empty() {
            queries.push(eng_tokens.join(" "));
        }

        // Translate Chinese terms
        let mut translated: Vec<&str> = Vec::new();
        for (zh, en_list) in zh_map {
            if topic.contains(zh) {
                translated.extend_from_slice(en_list);
            }
        }
        if !translated.is_empty() {
            queries.push(translated.join(" "));
            // Also add individual translated terms as separate queries
            for term in &translated {
                if !queries.contains(&term.to_string()) {
                    queries.push(term.to_string());
                }
            }
        }
    } else {
        // English topic: split on punctuation and extract key terms
        let cleaned: String = topic
            .chars()
            .map(|c| if c.is_ascii_punctuation() && c != '-' { ' ' } else { c })
            .collect();
        let words: Vec<&str> = cleaned.split_whitespace().collect();

        // Add full cleaned topic
        queries.push(cleaned.trim().to_string());

        // Add bigrams and trigrams of content words
        let stop: &[&str] = &[
            "a", "an", "the", "of", "for", "on", "in", "to", "and", "or", "with",
            "is", "are", "was", "be", "by", "at", "from",
        ];
        let content: Vec<&str> = words
            .iter()
            .filter(|w| !stop.contains(&w.to_lowercase().as_str()))
            .copied()
            .collect();

        if content.len() >= 3 {
            // Add triplets
            for chunk in content.windows(3) {
                let q = chunk.join(" ");
                if !queries.contains(&q) {
                    queries.push(q);
                }
            }
        }
        if content.len() >= 2 {
            // Add pairs
            for chunk in content.windows(2) {
                let q = chunk.join(" ");
                if !queries.contains(&q) {
                    queries.push(q);
                }
            }
        }
    }

    queries.dedup();
    queries.truncate(12);
    queries
}

/// Read `human_feedback.jsonl` from `run_dir`, returning formatted feedback
/// lines as a single string.  Lines already consumed (tracked via a stamp
/// file) are excluded.
fn load_human_feedback(run_dir: &Path, _stage: Stage) -> String {
    let jsonl_path = run_dir.join("human_feedback.jsonl");
    let stamp_path = run_dir.join(".feedback_consumed_up_to");

    let consumed_up_to: Option<String> = std::fs::read_to_string(&stamp_path)
        .ok()
        .map(|s| s.trim().to_string());

    let content = match std::fs::read_to_string(&jsonl_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut lines_out: Vec<String> = Vec::new();
    let mut last_ts: Option<String> = None;
    let mut past_consumed = consumed_up_to.is_none();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            let ts = val.get("timestamp").and_then(|v| v.as_str()).map(String::from);
            let layer = val
                .get("layer")
                .and_then(|v| v.as_str())
                .unwrap_or("user");
            let text = val
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !past_consumed {
                if let (Some(t), Some(cu)) = (&ts, &consumed_up_to) {
                    if t == cu {
                        past_consumed = true;
                    }
                }
                continue;
            }

            lines_out.push(format!("- [{}] {}", layer, text));
            if let Some(t) = ts {
                last_ts = Some(t);
            }
        }
    }

    // Update stamp
    if let Some(ref ts) = last_ts {
        let _ = std::fs::write(&stamp_path, ts);
    }

    lines_out.join("\n")
}

/// Try to load evolution lessons from `run_dir/evolution/` and return them
/// as a formatted string.  Returns empty string on any error.
fn get_evolution_overlay(run_dir: &Path, stage_name: &str) -> String {
    let evo_dir = run_dir.join("evolution");
    if !evo_dir.is_dir() {
        return String::new();
    }
    // Try stage-specific file first, then generic lessons file
    let candidates = [
        evo_dir.join(format!("{}_lessons.md", stage_name)),
        evo_dir.join("lessons.md"),
        evo_dir.join("evolution.md"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.trim().is_empty() {
                return format!("\n## Evolution Lessons\n{}\n", content.trim());
            }
        }
    }
    String::new()
}

/// Build a research context preamble string for LLM prompts.
pub fn build_context_preamble(config: &MolConfig, run_dir: &Path, opts: &ContextOpts) -> String {
    let mut parts: Vec<String> = Vec::new();
    let (domain_id, domain_name, top_venues) = detect_domain(&config.topic);

    parts.push(format!("## Research Topic\n{}", config.topic));
    parts.push(format!(
        "## Domain\n{} ({})\nTop venues: {}",
        domain_name, domain_id, top_venues
    ));

    if opts.include_goal {
        if let Some(goal) = read_prior_artifact(run_dir, "goal.md") {
            parts.push(format!("## Research Goal\n{}", goal.trim()));
        }
    }
    if opts.include_hypotheses {
        if let Some(hyp) = read_prior_artifact(run_dir, "hypotheses.md") {
            parts.push(format!("## Hypotheses\n{}", hyp.trim()));
        }
    }
    if opts.include_synthesis {
        if let Some(syn) = read_prior_artifact(run_dir, "synthesis_report.md") {
            parts.push(format!("## Synthesis\n{}", syn.trim()));
        }
    }
    if opts.include_exp_plan {
        if let Some(ep) = read_prior_artifact(run_dir, "experiment_plan.md") {
            parts.push(format!("## Experiment Plan\n{}", ep.trim()));
        }
    }
    if opts.include_analysis {
        if let Some(an) = read_prior_artifact(run_dir, "analysis_report.md") {
            parts.push(format!("## Analysis\n{}", an.trim()));
        }
    }
    if opts.include_decision {
        if let Some(dec) = read_prior_artifact(run_dir, "decision_record.md") {
            parts.push(format!("## Decision\n{}", dec.trim()));
        }
    }

    parts.join("\n\n")
}

/// Return the current UTC time as an ISO 8601 string (e.g. `2026-04-06T12:34:56Z`).
pub fn utcnow_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Aggregate experiment metrics from `run_dir/runs/` subdirectories.
///
/// Looks for a `metrics.json` file in each run subdirectory, extracts
/// `metric_key`, and builds a summary with min/max/mean/count plus a LaTeX
/// table.
pub fn collect_experiment_results(
    run_dir: &Path,
    metric_key: &str,
    metric_direction: &str,
) -> serde_json::Value {
    let runs_dir = run_dir.join("runs");
    let mut values: Vec<(String, f64)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let metrics_path = entry.path().join("metrics.json");
            if let Ok(content) = std::fs::read_to_string(&metrics_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(v) = val.get(metric_key).and_then(|v| v.as_f64()) {
                        let run_name = entry.file_name().to_string_lossy().to_string();
                        values.push((run_name, v));
                    }
                }
            }
        }
    }

    if values.is_empty() {
        return serde_json::json!({
            "metric_key": metric_key,
            "count": 0,
            "runs": []
        });
    }

    let count = values.len();
    let sum: f64 = values.iter().map(|(_, v)| v).sum();
    let mean = sum / count as f64;
    let min = values.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let max = values
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);

    let best = if metric_direction == "max" {
        values.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    } else {
        values.iter().min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    };

    let best_run = best.map(|(n, v)| serde_json::json!({"run": n, "value": v}));

    // Generate a simple LaTeX table
    let latex_rows: Vec<String> = values
        .iter()
        .map(|(name, val)| format!("  {} & {:.4} \\\\", name, val))
        .collect();
    let latex_table = format!(
        "\\begin{{tabular}}{{ll}}\n\\hline\nRun & {} \\\\\n\\hline\n{}\n\\hline\n\\end{{tabular}}",
        metric_key,
        latex_rows.join("\n")
    );

    serde_json::json!({
        "metric_key": metric_key,
        "metric_direction": metric_direction,
        "count": count,
        "min": min,
        "max": max,
        "mean": mean,
        "best_run": best_run,
        "latex_table": latex_table,
        "runs": values.iter().map(|(n, v)| serde_json::json!({"run": n, "value": v})).collect::<Vec<_>>()
    })
}

/// Generate a NeurIPS-style paper checklist appendix in Markdown.
pub fn generate_neurips_checklist(
    has_experiments: bool,
    has_theory: bool,
    has_code: bool,
) -> String {
    let mut lines: Vec<&str> = vec![
        "## NeurIPS Paper Checklist",
        "",
        "### Claims",
        "- [ ] Do the main claims in the abstract and introduction accurately reflect the paper's contributions and scope?",
        "",
        "### Limitations",
        "- [ ] Does the paper discuss the limitations of the work?",
        "",
        "### Theory Assumptions and Proofs",
    ];

    if has_theory {
        lines.push("- [ ] For each theoretical result, are all assumptions clearly stated?");
        lines.push("- [ ] Are all proofs complete and fully presented in the paper?");
    } else {
        lines.push("- [N/A] No theoretical results in this paper.");
    }

    lines.push("");
    lines.push("### Experiments");

    if has_experiments {
        lines.push("- [ ] Does the paper include sufficient experimental results?");
        lines.push("- [ ] Are the experimental setup and baselines clearly described?");
        lines.push(
            "- [ ] Are mean ± std reported and statistical significance tests conducted where appropriate?",
        );
        lines.push("- [ ] Are all ablations and sensitivity analyses included?");
    } else {
        lines.push("- [N/A] No experiments in this paper.");
    }

    lines.push("");
    lines.push("### Code");

    if has_code {
        lines.push("- [ ] Is the code publicly available or will it be released?");
        lines.push(
            "- [ ] Does the released code provide everything needed to reproduce the results?",
        );
    } else {
        lines.push("- [N/A] No code is submitted with this paper.");
    }

    lines.push("");
    lines.push("### Broader impacts");
    lines.push("- [ ] Does the paper discuss societal impacts, including potential negative impacts?");
    lines.push("");
    lines.push("### Safeguards");
    lines.push("- [ ] Does the paper describe safeguards for assets or models that could be misused?");
    lines.push("");
    lines.push("### Licenses for Existing Assets");
    lines.push("- [ ] Are the licenses of all used assets (datasets, code, models) reported?");
    lines.push("");
    lines.push("### Human Subjects");
    lines.push("- [ ] Does the paper involve human subjects? If so, is IRB approval documented?");

    lines.join("\n")
}

/// Extract the paper title from Markdown text.
///
/// Looks for H1 (`#`) or H2 (`##`) headings that appear before the Abstract
/// section and contain at least 4 words.  Prefers longer / more capitalised
/// headings.
pub fn extract_paper_title(md_text: &str) -> String {
    let mut candidates: Vec<String> = Vec::new();
    let mut past_abstract = false;

    for line in md_text.lines() {
        let trimmed = line.trim();

        // Stop collecting once we hit the abstract
        if trimmed.to_lowercase().starts_with("## abstract")
            || trimmed.to_lowercase().starts_with("# abstract")
        {
            past_abstract = true;
        }
        if past_abstract {
            break;
        }

        // Match H1 or H2 headings
        let heading = if trimmed.starts_with("## ") {
            Some(trimmed[3..].trim())
        } else if trimmed.starts_with("# ") {
            Some(trimmed[2..].trim())
        } else {
            None
        };

        if let Some(h) = heading {
            let words: Vec<&str> = h.split_whitespace().collect();
            if words.len() >= 4 {
                candidates.push(h.to_string());
            }
        }
    }

    // Prefer the candidate with the most capitalised words (title-case heuristic)
    candidates
        .into_iter()
        .max_by_key(|title| {
            title
                .split_whitespace()
                .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
                .count()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Stage dispatch
// ---------------------------------------------------------------------------

/// Execute `stage` using the supplied context, returning a `StageResult`.
///
/// Currently every stage is a stub that logs and returns a synthetic
/// `Done` result.  Each arm should be replaced with a call to the
/// corresponding domain-crate implementation as those crates are built.
pub async fn execute_stage(stage: Stage, context: &StageContext) -> Result<StageResult> {
    let stage_dir = context.stage_dir(stage);
    tokio::fs::create_dir_all(&stage_dir).await?;

    debug!(
        stage = stage.name(),
        run_id = %context.run_id,
        dir = %stage_dir.display(),
        "dispatching stage"
    );

    let t0 = std::time::Instant::now();

    let mut result = match stage {
        // Phase A: Research Scoping ----------------------------------------
        Stage::TopicInit => stub_stage(stage, &["topic_brief", "research_questions"]).await,
        Stage::ProblemDecompose => stub_stage(stage, &["problem_tree", "sub_problems"]).await,

        // Phase B: Literature Discovery ------------------------------------
        Stage::SearchStrategy => stub_stage(stage, &["search_queries", "source_list"]).await,
        Stage::LiteratureCollect => stub_stage(stage, &["raw_papers", "paper_metadata"]).await,
        Stage::LiteratureScreen => {
            stub_stage(stage, &["screened_papers", "exclusion_reasons"]).await
        }
        Stage::KnowledgeExtract => stub_stage(stage, &["knowledge_cards", "citation_map"]).await,

        // Phase C: Knowledge Synthesis -------------------------------------
        Stage::Synthesis => stub_stage(stage, &["synthesis_report", "gap_analysis"]).await,
        Stage::HypothesisGen => stub_stage(stage, &["hypotheses", "rationale"]).await,

        // Phase D: Experiment Design ----------------------------------------
        Stage::ExperimentDesign => {
            stub_stage(stage, &["experiment_plan", "success_criteria"]).await
        }
        Stage::CodebaseSearch => {
            stub_stage(stage, &["codebase_context", "relevant_files"]).await
        }
        Stage::CodeGeneration => stub_stage(stage, &["experiment_code", "code_readme"]).await,
        Stage::SanityCheck => stub_stage(stage, &["sanity_report"]).await,
        Stage::ResourcePlanning => {
            stub_stage(stage, &["resource_plan", "compute_estimate"]).await
        }

        // Phase E: Experiment Execution ------------------------------------
        Stage::ExperimentRun => stub_stage(stage, &["raw_results", "run_logs"]).await,
        Stage::IterativeRefine => {
            stub_stage(stage, &["refined_results", "refinement_log"]).await
        }

        // Phase F: Analysis & Decision -------------------------------------
        Stage::ResultAnalysis => stub_stage(stage, &["analysis_report", "figures"]).await,
        Stage::ResearchDecision => stub_stage(stage, &["decision_record"]).await,
        Stage::KnowledgeSummary => stub_stage(stage, &["knowledge_summary"]).await,

        // Phase G: Paper Writing -------------------------------------------
        Stage::PaperOutline => stub_stage(stage, &["paper_outline"]).await,
        Stage::PaperDraft => stub_stage(stage, &["paper_draft"]).await,
        Stage::PeerReview => stub_stage(stage, &["review_comments"]).await,
        Stage::PaperRevision => stub_stage(stage, &["paper_revised", "revision_notes"]).await,

        // Phase H: Finalization --------------------------------------------
        Stage::QualityGate => stub_stage(stage, &["quality_report"]).await,
        Stage::KnowledgeArchive => stub_stage(stage, &["archive_manifest"]).await,
        Stage::ExportPublish => stub_stage(stage, &["paper_final", "paper_tex"]).await,
        Stage::CitationVerify => {
            stub_stage(stage, &["verification_report", "paper_final_verified"]).await
        }

        // Special ----------------------------------------------------------
        Stage::Discussion => stub_stage(stage, &["discussion_notes"]).await,
    };

    result.elapsed_secs = t0.elapsed().as_secs_f64();

    info!(
        stage = stage.name(),
        status = %result.status,
        elapsed = result.elapsed_secs,
        artifacts = ?result.artifacts,
        "stage complete"
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return a stub `Done` result listing `artifact_names` as produced outputs.
async fn stub_stage(stage: Stage, artifact_names: &[&str]) -> StageResult {
    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: artifact_names.iter().map(|s| s.to_string()).collect(),
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_context(run_dir: &Path) -> StageContext {
        StageContext {
            run_dir: run_dir.to_owned(),
            run_id: "test-run".to_owned(),
            config: MolConfig::default(),
            prior_artifacts: HashMap::new(),
            auto_approve_gates: false,
        }
    }

    #[tokio::test]
    async fn execute_topic_init_returns_done() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let result = execute_stage(Stage::TopicInit, &ctx).await.unwrap();
        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"topic_brief".to_owned()));
    }

    #[tokio::test]
    async fn execute_creates_stage_dir() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        execute_stage(Stage::TopicInit, &ctx).await.unwrap();
        assert!(dir.path().join("stage-01").exists());
    }

    #[tokio::test]
    async fn stub_result_decision_is_proceed() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let result = execute_stage(Stage::SanityCheck, &ctx).await.unwrap();
        assert_eq!(result.decision, "proceed");
    }

    // -----------------------------------------------------------------
    // Helper function tests
    // -----------------------------------------------------------------

    #[test]
    fn safe_json_loads_direct() {
        let val = safe_json_loads(r#"{"key": "value"}"#);
        assert!(val.is_some());
        assert_eq!(val.unwrap()["key"], "value");
    }

    #[test]
    fn safe_json_loads_from_fence() {
        let input = "Here:\n```json\n{\"key\": \"value\"}\n```\nDone.";
        let val = safe_json_loads(input);
        assert!(val.is_some());
    }

    #[test]
    fn safe_json_loads_brace_matching() {
        let input = "noise {\"a\":1} more noise {\"b\":2, \"c\":3} end";
        let val = safe_json_loads(input);
        assert!(val.is_some());
        // Should find the largest dict
        assert!(val.unwrap().get("b").is_some());
    }

    #[test]
    fn extract_yaml_block_from_fence() {
        let input = "text\n```yaml\nkey: value\nlist:\n  - a\n  - b\n```\nmore";
        let yaml = extract_yaml_block(input);
        assert!(yaml.contains("key: value"));
    }

    #[test]
    fn detect_domain_ml() {
        let (id, _, _) = detect_domain("deep learning transformer architecture");
        assert_eq!(id, "ml");
    }

    #[test]
    fn detect_domain_physics() {
        let (id, _, _) = detect_domain("quantum thermodynamic simulation");
        assert_eq!(id, "physics");
    }

    #[test]
    fn detect_domain_math_theoretical() {
        let (id, _, _) = detect_domain("derive the mathematical formulation of diffusion model");
        assert_eq!(id, "mathematics");
    }

    #[test]
    fn build_fallback_queries_english() {
        let queries = build_fallback_queries("deep learning for protein folding prediction");
        assert!(!queries.is_empty());
        assert!(queries.len() <= 12);
    }

    #[test]
    fn build_fallback_queries_chinese() {
        let queries = build_fallback_queries("具身智能 VLA world model 最新研究");
        assert!(!queries.is_empty());
        // Should contain English translations
        assert!(queries.iter().any(|q| q.contains("embodied")));
    }

    #[test]
    fn read_prior_artifact_finds_latest() {
        let tmp = TempDir::new().unwrap();
        let s01 = tmp.path().join("stage-01");
        std::fs::create_dir_all(&s01).unwrap();
        std::fs::write(s01.join("goal.md"), "# Goal v1").unwrap();
        let s02 = tmp.path().join("stage-02");
        std::fs::create_dir_all(&s02).unwrap();
        std::fs::write(s02.join("goal.md"), "# Goal v2").unwrap();
        let content = read_prior_artifact(tmp.path(), "goal.md").unwrap();
        assert!(content.contains("v2")); // Should find stage-02's version
    }

    #[test]
    fn extract_paper_title_from_h1() {
        let md = "# My Amazing Research Paper on Deep Learning\n\n## Abstract\nThis paper...";
        let title = extract_paper_title(md);
        assert_eq!(title, "My Amazing Research Paper on Deep Learning");
    }

    #[test]
    fn generate_neurips_checklist_has_sections() {
        let cl = generate_neurips_checklist(true, false, true);
        assert!(cl.contains("NeurIPS"));
        assert!(cl.contains("Claims"));
        assert!(cl.contains("Broader impacts"));
    }

    #[test]
    fn utcnow_iso_format() {
        let ts = utcnow_iso();
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z') || ts.contains('+'));
    }
}
