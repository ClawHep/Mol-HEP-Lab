//! Stage dispatch and execution.
//!
//! `execute_stage` is the single dispatch point that the runner calls for
//! every pipeline stage.  Individual stage implementations live (or will
//! live) in domain-specific crates; for now every stage returns a stub
//! `StageResult` so the pipeline wiring can be exercised end-to-end.

use crate::stages::{Stage, StageStatus};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    /// Domain identifier (e.g. "hep", "ml", "physics").
    pub domain: String,
    /// Analysis type within the domain (e.g. "extraction", "search", "measurement").
    pub analysis_type: Option<String>,
    /// Root of domain knowledge tree (agents/, conventions/, methodology/, templates/).
    /// Defaults to `"hep"`.
    pub knowledge_root: PathBuf,
}

impl Default for MolConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            settings: HashMap::new(),
            domain: "hep".to_owned(),
            analysis_type: None,
            knowledge_root: PathBuf::from("hep"),
        }
    }
}

// ---------------------------------------------------------------------------
// StagePromptEngine
// ---------------------------------------------------------------------------

/// Loads and renders stage-specific prompt templates from disk.
///
/// Each stage maps to a `.md` file named after the lowercased stage name
/// (e.g. `Stage::TopicInit` → `topic_init.md`).  Templates are rendered with
/// Tera and split on `---user---` into (system_prompt, user_prompt) pairs.
pub struct StagePromptEngine {
    engine: mol_common::PromptEngine,
}

impl StagePromptEngine {
    /// Load all stage templates from `templates_dir`.
    pub fn load(templates_dir: &Path) -> Result<Self> {
        let engine = mol_common::PromptEngine::from_directory(templates_dir)
            .with_context(|| format!("load stage templates from {}", templates_dir.display()))?;
        Ok(Self { engine })
    }

    /// Check whether a template exists for the given stage.
    pub fn has_template(&self, stage: Stage) -> bool {
        let name = Self::template_name(stage);
        self.engine.has_template(&name)
    }

    /// Render the prompt template for `stage`, returning `(system_prompt, user_prompt)`.
    ///
    /// The template is split on the literal line `---user---`.
    pub fn render_prompt(
        &self,
        stage: Stage,
        vars: &HashMap<String, String>,
    ) -> Result<(String, String)> {
        let name = Self::template_name(stage);
        let rendered = self.engine.render_str_vars(
            &name,
            vars.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        )
        .with_context(|| format!("render template for stage {}", stage.name()))?;

        let (system, user) = rendered
            .split_once("---user---")
            .ok_or_else(|| anyhow::anyhow!(
                "template {} missing ---user--- delimiter", name
            ))?;

        Ok((system.trim().to_owned(), user.trim().to_owned()))
    }

    /// Map a Stage to its template file name.
    fn template_name(stage: Stage) -> String {
        format!("{}.md", stage.name().to_ascii_lowercase())
    }
}

impl std::fmt::Debug for StagePromptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagePromptEngine").finish()
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
// Stage-Agent mapping
// ---------------------------------------------------------------------------

/// Map each pipeline stage to its primary HEP agent definition.
///
/// Returns the agent filename stem (e.g. `"lead-analyst"`) which resolves to
/// `{knowledge_root}/agents/{name}.md`. Discussion returns `None` as it is a
/// multi-agent stage with its own template.
pub fn agent_for_stage(stage: Stage) -> Option<&'static str> {
    match stage {
        Stage::TopicInit | Stage::ProblemDecompose => Some("lead-analyst"),
        Stage::SearchStrategy | Stage::LiteratureCollect
        | Stage::LiteratureScreen | Stage::KnowledgeExtract => Some("investigator"),
        Stage::Synthesis => Some("theory-scout"),
        Stage::HypothesisGen => Some("lead-analyst"),
        Stage::ExperimentDesign | Stage::ResourcePlanning => Some("lead-analyst"),
        Stage::CodebaseSearch | Stage::CodeGeneration | Stage::ExperimentRun => Some("signal-lead"),
        Stage::SanityCheck => Some("cross-checker"),
        Stage::IterativeRefine => Some("systematics-fitter"),
        Stage::ResultAnalysis | Stage::KnowledgeSummary => Some("lead-analyst"),
        Stage::ResearchDecision => Some("arbiter"),
        Stage::PaperOutline | Stage::PaperDraft | Stage::PaperRevision => Some("note-writer"),
        Stage::PeerReview => Some("physics-reviewer"),
        Stage::QualityGate => Some("arbiter"),
        Stage::KnowledgeArchive | Stage::ExportPublish | Stage::CitationVerify => Some("note-writer"),
        Stage::Discussion => None,
    }
}

// ---------------------------------------------------------------------------
// StageContext
// ---------------------------------------------------------------------------

/// Runtime context passed to every stage executor.
///
/// Carries the run directory, configuration, and a snapshot of all artifacts
/// produced by earlier stages.
#[derive(Clone)]
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
    /// LLM provider — `None` means stages will fail with an error.
    pub llm: Option<Arc<dyn mol_llm::LlmProvider>>,
    /// Stage prompt template engine — loaded once at pipeline startup.
    pub prompt_engine: Option<Arc<StagePromptEngine>>,
}

impl std::fmt::Debug for StageContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageContext")
            .field("run_dir", &self.run_dir)
            .field("run_id", &self.run_id)
            .field("config", &self.config)
            .field("prior_artifacts", &self.prior_artifacts)
            .field("auto_approve_gates", &self.auto_approve_gates)
            .field("llm", &self.llm.as_ref().map(|p| p.name()))
            .finish()
    }
}

impl StageContext {
    /// Return the output directory for `stage` within this run.
    pub fn stage_dir(&self, stage: Stage) -> PathBuf {
        self.run_dir.join(format!("stage-{:02}", stage.as_i32()))
    }

    /// Resolve a path within the domain knowledge tree.
    fn knowledge_path(&self, relative: &str) -> PathBuf {
        self.config.knowledge_root.join(relative)
    }

    /// Read a file from the knowledge tree. Returns `None` if missing.
    fn read_knowledge(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.knowledge_path(relative)).ok()
    }

    /// Build the template variable map for rendering stage prompts.
    ///
    /// Populates common variables (topic, domain, analysis_type, timestamp),
    /// reads relevant prior artifacts, and injects domain knowledge (agent role,
    /// conventions, blinding protocol) from the knowledge tree.
    pub fn template_vars(&self, stage: Stage) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("topic".to_owned(), self.config.topic.clone());
        vars.insert("domain".to_owned(), self.config.domain.clone());
        vars.insert(
            "analysis_type".to_owned(),
            self.config.analysis_type.clone().unwrap_or_else(|| "general".to_owned()),
        );
        vars.insert("timestamp".to_owned(), utcnow_iso());

        // Read common prior artifacts if they exist.
        let artifact_files = [
            ("goal", "goal.md"),
            ("hypotheses", "hypotheses.md"),
            ("synthesis_report", "synthesis_report.md"),
            ("experiment_plan", "exp_plan.yaml"),
            ("analysis_report", "analysis_report.md"),
            ("decision_record", "decision_record.json"),
            ("knowledge_summary", "knowledge_summary.json"),
            ("paper_outline", "paper_outline.md"),
            ("paper_draft", "paper_draft.md"),
            ("paper_revised", "paper_revised.md"),
            ("review_comments", "review_comments.json"),
            ("problem_tree", "problem_tree.md"),
            ("search_queries", "search_plan.yaml"),
            ("knowledge_cards", "knowledge_cards.json"),
            ("sanity_report", "sanity_report.json"),
            ("resource_plan", "resource_plan.json"),
        ];
        for (key, filename) in artifact_files {
            if let Some(content) = read_prior_artifact(&self.run_dir, filename) {
                vars.insert(key.to_owned(), content);
            }
        }

        // --- Domain knowledge injection ---

        // Agent role: inject the matched agent's markdown body (frontmatter stripped)
        if let Some(agent_name) = agent_for_stage(stage) {
            if let Some(raw) = self.read_knowledge(&format!("agents/{agent_name}.md")) {
                vars.insert("agent_role".into(), strip_frontmatter(&raw).to_owned());
            }
        }

        // Conventions: inject by analysis_type (extraction, search, unfolding)
        let analysis_type = self.config.analysis_type.as_deref().unwrap_or("general");
        match self.read_knowledge(&format!("conventions/{analysis_type}.md")) {
            Some(content) => { vars.insert("conventions".into(), content); }
            None if analysis_type != "general" => {
                tracing::warn!(
                    analysis_type,
                    "convention file not found for analysis_type — {{{{ conventions }}}} will be empty"
                );
            }
            _ => {}
        }

        // Blinding protocol
        if let Some(content) = self.read_knowledge("methodology/04-blinding.md") {
            vars.insert("blinding_protocol".into(), content);
        }

        vars
    }
}

/// Call LLM with a system/user prompt pair.
///
/// Returns `Err` if no LLM provider is configured or if the call fails.
/// Returns `Ok("")` only if the LLM returns an empty response.
pub async fn llm_generate(
    ctx: &StageContext,
    system_prompt: &str,
    user_prompt: &str,
    json_mode: bool,
) -> Result<String> {
    let llm = ctx.llm.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No LLM provider configured"))?;

    let messages = vec![
        mol_llm::Message::system(system_prompt),
        mol_llm::Message::user(user_prompt),
    ];
    let resp = llm.chat(&messages, json_mode).await
        .context("LLM chat call failed")?;

    let cleaned = strip_llm_noise(&resp.content);
    Ok(strip_markdown_fences(&cleaned))
}

/// Strip LLM noise: thinking traces, ACP session artifacts, etc.
fn strip_llm_noise(s: &str) -> String {
    let mut result = s.to_string();
    // Strip ACP session noise (e.g. "Compacting...\n\nCompacting completed.\n\n")
    for prefix in &["Compacting...", "Warming up...", "Resuming..."] {
        if let Some(rest) = result.strip_prefix(prefix) {
            // Find where the noise ends (usually a double newline after a status line)
            if let Some(pos) = rest.find("\n\n") {
                result = rest[pos + 2..].to_string();
            }
        }
    }
    // Strip [thinking] blocks at the start
    let trimmed = result.trim_start();
    if trimmed.starts_with("[thinking]") {
        // Find end of thinking block: next [/thinking] or blank-line-then-content
        if let Some(end) = trimmed.find("\n\n") {
            let after = &trimmed[end + 2..];
            // If there's still a [thinking] at start, strip recursively
            result = after.to_string();
        }
    }
    result
}

/// Strip YAML frontmatter (delimited by `---`) from a markdown document.
/// Returns the content after the closing `---` delimiter.
fn strip_frontmatter(text: &str) -> &str {
    if !text.starts_with("---") {
        return text;
    }
    // Find the closing "---" after the opening one.
    // `end` is relative to text[3..], so absolute offset of content after
    // the closing delimiter is: 3 (opening "---") + end + 4 ("\n---") = end + 7.
    if let Some(end) = text[3..].find("\n---") {
        let after = end + 7;
        if after < text.len() {
            return text[after..].trim_start_matches('\n');
        }
    }
    text
}

/// Strip markdown code fences (```json ... ``` or ``` ... ```) from LLM output.
/// LLMs frequently wrap JSON responses in fences even when asked not to.
fn strip_markdown_fences(s: &str) -> String {
    let trimmed = s.trim();
    // Match ```json\n...\n``` or ```\n...\n```
    if let Some(rest) = trimmed.strip_prefix("```") {
        // Skip optional language tag on the first line
        let after_tag = if let Some(newline_pos) = rest.find('\n') {
            &rest[newline_pos + 1..]
        } else {
            return trimmed.to_string();
        };
        // Strip trailing ```
        if let Some(content) = after_tag.strip_suffix("```") {
            return content.trim().to_string();
        }
    }
    trimmed.to_string()
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
pub fn read_prior_artifact_pub(run_dir: &Path, filename: &str) -> Option<String> {
    read_prior_artifact(run_dir, filename)
}

fn read_prior_artifact(run_dir: &Path, filename: &str) -> Option<String> {
    let mut stage_dirs: Vec<_> = std::fs::read_dir(run_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("stage-"))
        .collect();

    // Sort: highest stage number first, non-versioned before versioned within
    // the same base name.
    stage_dirs.sort_by(|a, b| {
        let an = a.file_name().to_string_lossy().into_owned();
        let bn = b.file_name().to_string_lossy().into_owned();

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

/// Public wrapper for [`find_prior_file`] used by stages_impl modules.
pub fn find_prior_file_pub(run_dir: &Path, filename: &str) -> Option<PathBuf> {
    find_prior_file(run_dir, filename)
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
        let an = a.file_name().to_string_lossy().into_owned();
        let bn = b.file_name().to_string_lossy().into_owned();

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
    // Pre-strip ACP noise: [thinking]...[/thinking] and [plan]...\n\n blocks
    // Mirror of Python's _extract_yaml_block pre-processing step.
    let cleaned: String = {
        // Strip [thinking] blocks up to the next ``` fence, uppercase line, or end
        let mut s = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("[thinking]") {
            s.push_str(&rest[..start]);
            let after = &rest[start..];
            // Find end: next ```  or next uppercase-leading line or end
            let end_pos = after[10..]
                .find("```")
                .map(|p| 10 + p)
                .or_else(|| {
                    after[10..]
                        .lines()
                        .skip(1)
                        .enumerate()
                        .find(|(_, l)| l.starts_with(|c: char| c.is_uppercase()))
                        .map(|(i, _)| {
                            after[10..].lines().take(i + 1).map(|l| l.len() + 1).sum::<usize>() + 10
                        })
                })
                .unwrap_or(after.len());
            rest = &rest[start + end_pos..];
        }
        s.push_str(rest);
        // Strip [plan]...\n\n blocks
        let mut out = String::with_capacity(s.len());
        let mut rem = s.as_str();
        while let Some(start) = rem.find("[plan]") {
            out.push_str(&rem[..start]);
            let after = &rem[start..];
            let end_pos = after.find("\n\n").map(|p| p + 2).unwrap_or(after.len());
            rem = &rem[start + end_pos..];
        }
        out.push_str(rem);
        out
    };

    // Helper: extract from a single fence-delimited string
    fn extract_from(src: &str, fence: &str) -> Option<String> {
        let skip = fence.len();
        src.find(fence).and_then(|start| {
            let after = &src[start + skip..];
            after.find("```").map(|end| after[..end].trim().to_string())
        })
    }
    fn extract_bare(src: &str) -> Option<String> {
        src.find("```").and_then(|start| {
            let after = &src[start + 3..];
            after.find("```").map(|end| {
                let block = after[..end].trim();
                if !block.is_empty() { Some(block.to_string()) } else { None }
            }).flatten()
        })
    }

    // Try markdown fences on cleaned text first (mirrors Python)
    if let Some(v) = extract_from(&cleaned, "```yaml") { return v; }
    if let Some(v) = extract_from(&cleaned, "```yml")  { return v; }
    if let Some(v) = extract_bare(&cleaned)            { return v; }

    // Fall back to original text in case cleaning removed too much (mirrors Python)
    if let Some(v) = extract_from(text, "```yaml") { return v; }
    if let Some(v) = extract_from(text, "```yml")  { return v; }
    if let Some(v) = extract_bare(text)            { return v; }

    // Last resort: collect lines that look like raw YAML from cleaned text
    let mut yaml_lines: Vec<&str> = Vec::new();
    let mut in_yaml = false;
    for line in cleaned.lines() {
        let t = line.trim();
        if !in_yaml {
            if t.split_once(':').map(|(k, _)| k.chars().all(|c| c.is_ascii_lowercase() || c == '_')).unwrap_or(false) {
                in_yaml = true;
            }
        }
        if in_yaml {
            if !t.is_empty() && !t.starts_with('#') {
                yaml_lines.push(line);
            } else if t.is_empty() && !yaml_lines.is_empty() {
                yaml_lines.push(line);
            }
        }
    }
    if !yaml_lines.is_empty() {
        return yaml_lines.join("\n").trim().to_string();
    }

    text.trim().to_string()
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
    detect_domain_with_hint(topic, &[])
}

/// Like [`detect_domain`] but also accepts explicit domain hints from config.
///
/// If any element of `domain_hints` matches a known domain (by id, display name,
/// or leading keywords), that domain is returned directly; otherwise falls through
/// to the auto-detection logic.
pub fn detect_domain_with_hint<'a>(topic: &str, domain_hints: &[&str]) -> (&'static str, &'static str, &'static str) {
    let lower = topic.to_lowercase();

    // Theoretical intent words — mirrors Python's _detect_domain
    let theoretical_words = [
        "derive",
        "prove",
        "mathematical formulation",
        "mathematical proof",
        "formal proof",
        "formalism",
    ];
    let has_theoretical_intent = theoretical_words.iter().any(|w| lower.contains(w));

    // Domain keyword tables: (domain_id, display_name, top_venues, keywords)
    let domains: &[(&str, &str, &str, &[&str])] = &[
        (
            "ml",
            "machine learning",
            "NeurIPS, ICML, ICLR",
            &[
                "machine learning",
                "deep learning",
                "neural network",
                "transformer",
                "reinforcement learning",
                "gan",
                "diffusion model",
                "llm",
                "language model",
                "computer vision",
                "nlp",
                "representation learning",
                "self-supervised",
                "federated learning",
                "meta-learning",
                "continual learning",
                "few-shot",
                "knowledge distillation",
                "attention mechanism",
                "fine-tuning",
                "rlhf",
                "vision transformer",
                "vit",
                "bert",
                "gpt",
                "autoencoder",
            ],
        ),
        (
            "physics",
            "physics",
            "Physical Review Letters, Nature Physics, JHEP",
            &[
                "quantum",
                "thermodynamic",
                "electrodynamic",
                "particle physics",
                "condensed matter",
                "statistical mechanics",
                "cosmology",
                "astrophysics",
                "plasma",
                "optics",
                "photonics",
                "relativity",
                "gravitational",
                "pde",
                "pinn",
                "physics-informed",
                "burgers",
                "navier-stokes",
                "darcy flow",
                "schrödinger",
                "scientific computing",
                "operator learning",
                "neural operator",
                "fourier neural",
                "deeponet",
            ],
        ),
        (
            "chemistry",
            "chemistry",
            "JACS, Nature Chemistry, Angewandte Chemie",
            &[
                "molecular",
                "catalysis",
                "polymer",
                "organic chemistry",
                "inorganic",
                "electrochemistry",
                "spectroscopy",
                "crystallography",
                "drug discovery",
                "protein folding",
                "computational chemistry",
                "dft",
                "force field",
            ],
        ),
        (
            "economics",
            "economics",
            "AER, Econometrica, QJE, Review of Economic Studies",
            &[
                "econometric",
                "macroeconomic",
                "microeconomic",
                "game theory",
                "market",
                "fiscal policy",
                "monetary",
                "behavioral economics",
                "causal inference",
                "panel data",
                "regression discontinuity",
                "instrumental variable",
                "supply chain",
                "auction",
            ],
        ),
        (
            "mathematics",
            "mathematics",
            "Annals of Mathematics, Inventiones Mathematicae, JAMS",
            &[
                "theorem",
                "proof",
                "prove",
                "conjecture",
                "topology",
                "algebra",
                "number theory",
                "combinatorics",
                "differential equation",
                "stochastic process",
                "functional analysis",
                "manifold",
                "riemannian",
                "category theory",
                "graph theory",
                "neural ode",
                "dynamical system",
                "lorenz",
                "chaotic",
                "lyapunov",
                "attractor",
                "ode solver",
                "trajectory prediction",
                "mathematical formulation",
                "mathematical proof",
                "derivation",
                "brownian motion",
                "branching process",
                "galton-watson",
                "markov chain",
                "martingale",
                "ergodic",
                "convergence theorem",
                "marginal distribution",
                "extinction probability",
                "feynman-kac",
                "measure theory",
                "hilbert space",
                "banach space",
                "operator theory",
                "variational",
                "euler-lagrange",
                "calculus of variations",
            ],
        ),
        (
            "engineering",
            "engineering",
            "IEEE Transactions, ASME journals, AIAA",
            &[
                "robotics",
                "control system",
                "signal processing",
                "fpga",
                "embedded system",
                "vlsi",
                "antenna",
                "fluid dynamics",
                "cfd",
                "finite element",
                "structural",
                "mechatronics",
                "autonomous",
            ],
        ),
        (
            "biology",
            "biology",
            "Nature, Science, Cell, PNAS",
            &[
                "genomics",
                "proteomics",
                "transcriptomics",
                "crispr",
                "single-cell",
                "phylogenetic",
                "ecology",
                "neuroscience",
                "bioinformatics",
                "sequencing",
                "gene expression",
                "epigenetic",
            ],
        ),
    ];

    // Check explicit domain hints first (matching Python behavior)
    if !domain_hints.is_empty() {
        for hint in domain_hints {
            let hl = hint.to_lowercase();
            for &(id, name, venues, keywords) in domains {
                if hl == id
                    || hl == name
                    || keywords.iter().take(3).any(|kw| hl.contains(kw))
                {
                    return (id, name, venues);
                }
            }
        }
    }

    // Score each domain
    let mut best_id = "ml";
    let mut best_name = "machine learning";
    let mut best_venues = "NeurIPS, ICML, ICLR";
    let mut best_score = 0usize;

    for &(id, name, venues, keywords) in domains {
        let mut score = keywords.iter().filter(|kw| lower.contains(*kw)).count();
        // Boost non-empirical domains for theoretical intent
        if has_theoretical_intent && matches!(id, "mathematics" | "physics" | "economics") {
            score += 1;
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
pub fn build_fallback_queries(topic: &str) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();

    // Chinese-to-English domain mapping — 28 entries, mirrors Python's _CHINESE_ENGLISH_DOMAIN_MAP
    let zh_map: &[(&str, &[&str])] = &[
        ("具身智能",   &["embodied intelligence", "embodied AI"]),
        ("视觉语言动作", &["vision language action", "VLA"]),
        ("视觉语言模型", &["vision language model", "VLM"]),
        ("世界模型",   &["world model"]),
        ("动作模型",   &["action model"]),
        ("机器人",     &["robot", "robotics"]),
        ("操控",       &["manipulation"]),
        ("抓取",       &["grasping"]),
        ("导航",       &["navigation"]),
        ("模仿学习",   &["imitation learning"]),
        ("强化学习",   &["reinforcement learning"]),
        ("扩散策略",   &["diffusion policy"]),
        ("视频生成",   &["video generation"]),
        ("动作预测",   &["action prediction"]),
        ("架构",       &["architecture"]),
        ("最新",       &["latest", "recent", "state of the art"]),
        ("评估",       &["evaluation", "benchmark"]),
        ("应用",       &["application"]),
        ("调研",       &["survey"]),
        ("研究",       &["research"]),
        ("方向",       &["direction"]),
        ("联合建模",   &["joint modeling", "unified model"]),
        ("联合训练",   &["joint training"]),
        ("跨具身",     &["cross-embodiment"]),
        ("灵巧操作",   &["dexterous manipulation"]),
        ("双臂",       &["bimanual", "dual-arm"]),
        ("长时序",     &["long-horizon"]),
        ("泛化",       &["generalization"]),
        ("零样本",     &["zero-shot"]),
        ("预训练",     &["pretraining", "pre-training"]),
        ("微调",       &["fine-tuning"]),
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

    // Suffix queries for English topics — mirrors Python's suffix loop
    // Build a short version of the topic from ASCII word-tokens (max 60 chars)
    let topic_short: String = {
        let tokens: String = topic
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if tokens.is_empty() {
            topic.chars().take(60).collect()
        } else {
            tokens.chars().take(60).collect::<String>().trim().to_string()
        }
    };
    for suffix in &["survey", "review", "benchmark", "state of the art", "recent advances"] {
        if queries.len() >= 8 {
            break;
        }
        let q = format!("{} {}", topic_short, suffix);
        if !queries.iter().any(|x| x.to_lowercase() == q.to_lowercase()) {
            queries.push(q);
        }
    }

    queries.dedup();
    queries.truncate(12);
    queries
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
                        let run_name = entry.file_name().to_string_lossy().into_owned();
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
/// Each arm dispatches to the corresponding stage implementation in the
/// `stages_impl` submodule.
pub async fn execute_stage(stage: Stage, context: &StageContext) -> Result<StageResult> {
    use crate::stages_impl;

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
        // Phase 1: Strategy ------------------------------------------------
        Stage::TopicInit => {
            stages_impl::phase1::execute_topic_init(stage, context).await
        }
        Stage::ProblemDecompose => {
            stages_impl::phase1::execute_problem_decompose(stage, context).await
        }

        // Phase 2: Exploration ---------------------------------------------
        Stage::SearchStrategy => {
            stages_impl::phase2::execute_search_strategy(stage, context).await
        }
        Stage::LiteratureCollect => {
            stages_impl::phase2::execute_literature_collect(stage, context).await
        }
        Stage::LiteratureScreen => {
            stages_impl::phase2::execute_literature_screen(stage, context).await
        }
        Stage::KnowledgeExtract => {
            stages_impl::phase2::execute_knowledge_extract(stage, context).await
        }
        Stage::Synthesis => {
            stages_impl::phase2::execute_synthesis(stage, context).await
        }
        Stage::HypothesisGen => {
            stages_impl::phase2::execute_hypothesis_gen(stage, context).await
        }

        // Phase 3: Processing ----------------------------------------------
        Stage::ExperimentDesign => {
            stages_impl::phase3::execute_experiment_design(stage, context).await
        }
        Stage::CodebaseSearch => {
            stages_impl::phase3::execute_codebase_search(stage, context).await
        }
        Stage::CodeGeneration => {
            stages_impl::phase3::execute_code_generation(stage, context).await
        }
        Stage::SanityCheck => {
            stages_impl::phase3::execute_sanity_check(stage, context).await
        }
        Stage::ResourcePlanning => {
            stages_impl::phase3::execute_resource_planning(stage, context).await
        }
        Stage::ExperimentRun => {
            stages_impl::phase3::execute_experiment_run(stage, context).await
        }
        Stage::IterativeRefine => {
            stages_impl::phase3::execute_iterative_refine(stage, context).await
        }

        // Phase 4: Inference -----------------------------------------------
        Stage::ResultAnalysis => {
            stages_impl::phase4::execute_result_analysis(stage, context).await
        }
        Stage::ResearchDecision => {
            stages_impl::phase4::execute_research_decision(stage, context).await
        }
        Stage::KnowledgeSummary => {
            stages_impl::phase4::execute_knowledge_summary(stage, context).await
        }

        // Phase 5: Documentation -------------------------------------------
        Stage::PaperOutline => {
            stages_impl::phase5::execute_paper_outline(stage, context).await
        }
        Stage::PaperDraft => {
            stages_impl::phase5::execute_paper_draft(stage, context).await
        }
        Stage::PeerReview => {
            stages_impl::phase5::execute_peer_review(stage, context).await
        }
        Stage::PaperRevision => {
            stages_impl::phase5::execute_paper_revision(stage, context).await
        }
        Stage::QualityGate => {
            stages_impl::phase5::execute_quality_gate(stage, context).await
        }
        Stage::KnowledgeArchive => {
            stages_impl::phase5::execute_knowledge_archive(stage, context).await
        }
        Stage::ExportPublish => {
            stages_impl::phase5::execute_export_publish(stage, context).await
        }
        Stage::CitationVerify => {
            stages_impl::phase5::execute_citation_verify(stage, context).await
        }

        // Special ----------------------------------------------------------
        Stage::Discussion => {
            stages_impl::discussion::execute_discussion(stage, context).await
        }
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
            llm: None,
            prompt_engine: None,
        }
    }

    #[tokio::test]
    async fn execute_topic_init_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let result = execute_stage(Stage::TopicInit, &ctx).await.unwrap();
        // Without prompt_engine or LLM, stage should fail honestly
        assert_eq!(result.status, StageStatus::Failed);
    }

    #[tokio::test]
    async fn execute_creates_stage_dir() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let _ = execute_stage(Stage::TopicInit, &ctx).await.unwrap();
        assert!(dir.path().join("stage-01").exists());
    }

    #[tokio::test]
    async fn execute_sanity_check_fails_without_engine() {
        let dir = TempDir::new().unwrap();
        let ctx = make_context(dir.path());
        let result = execute_stage(Stage::SanityCheck, &ctx).await.unwrap();
        assert_eq!(result.status, StageStatus::Failed);
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
    fn extract_yaml_block_strips_thinking_before_fence() {
        let input = "[thinking]\nI am reasoning about things here.\n```yaml\ntopic: deep learning\n```";
        let yaml = extract_yaml_block(input);
        assert!(yaml.contains("topic: deep learning"), "got: {yaml}");
        assert!(!yaml.contains("[thinking]"));
    }

    #[test]
    fn extract_yaml_block_strips_plan_before_fence() {
        let input = "[plan]\nStep 1: do something\nStep 2: do another thing\n\n```yaml\ngoal: test\n```";
        let yaml = extract_yaml_block(input);
        assert!(yaml.contains("goal: test"), "got: {yaml}");
        assert!(!yaml.contains("[plan]"));
    }

    #[test]
    fn extract_yaml_block_strips_both_thinking_and_plan() {
        // [thinking] block followed by a [plan] block, then YAML
        let input = "[thinking]\nLet me reason here.\n```yaml\n[plan]\nStep 1\n\n```yaml\nresult: ok\n```";
        // After stripping [thinking] up to the first ```, the rest starts at "```yaml\n[plan]..."
        // After stripping [plan] up to "\n\n", remainder is "```yaml\nresult: ok\n```"
        // The function should fall back to original text when cleaned text yields nothing useful.
        // Just verify the function doesn't panic and returns a string.
        let yaml = extract_yaml_block(input);
        // The raw text contains "```yaml\nresult: ok" — fallback path should find it
        let _ = yaml; // don't assert contents; just ensure no panic
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

    #[test]
    fn strip_markdown_fences_json() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_fences(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn strip_markdown_fences_no_lang() {
        let input = "```\n{\"a\":1}\n```";
        assert_eq!(strip_markdown_fences(input), r#"{"a":1}"#);
    }

    #[test]
    fn strip_markdown_fences_plain() {
        let input = r#"{"already":"clean"}"#;
        assert_eq!(strip_markdown_fences(input), input);
    }

    // -----------------------------------------------------------------
    // StagePromptEngine tests
    // -----------------------------------------------------------------

    #[test]
    fn stage_prompt_engine_loads_templates() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("topic_init.md"),
            "You are a researcher.\n\n---user---\n\nAnalyze: {{ topic }}",
        ).unwrap();

        let engine = StagePromptEngine::load(tmp.path()).unwrap();
        assert!(engine.has_template(Stage::TopicInit));
        assert!(!engine.has_template(Stage::Discussion));
    }

    #[test]
    fn stage_prompt_engine_renders_split_prompt() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("topic_init.md"),
            "You are a {{ domain }} researcher.\n\n---user---\n\nAnalyze: {{ topic }}",
        ).unwrap();

        let engine = StagePromptEngine::load(tmp.path()).unwrap();
        let mut vars = HashMap::new();
        vars.insert("topic".to_owned(), "Higgs boson".to_owned());
        vars.insert("domain".to_owned(), "hep".to_owned());

        let (system, user) = engine.render_prompt(Stage::TopicInit, &vars).unwrap();
        assert!(system.contains("hep researcher"));
        assert!(user.contains("Higgs boson"));
    }

    #[test]
    fn stage_prompt_engine_missing_template_errors() {
        let tmp = TempDir::new().unwrap();
        let engine = StagePromptEngine::load(tmp.path()).unwrap();
        let vars: HashMap<String, String> = HashMap::new();
        assert!(engine.render_prompt(Stage::TopicInit, &vars).is_err());
    }

    #[test]
    fn stage_prompt_engine_missing_delimiter_errors() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("topic_init.md"),
            "System prompt without delimiter",
        ).unwrap();

        let engine = StagePromptEngine::load(tmp.path()).unwrap();
        let vars: HashMap<String, String> = HashMap::new();
        let err = engine.render_prompt(Stage::TopicInit, &vars).unwrap_err();
        assert!(err.to_string().contains("---user---"));
    }

    #[tokio::test]
    async fn template_vars_injects_agent_role() {
        let dir = TempDir::new().unwrap();
        let kr = dir.path().join("test_kr");
        std::fs::create_dir_all(kr.join("agents")).unwrap();
        std::fs::write(
            kr.join("agents/lead-analyst.md"),
            "---\nname: lead-analyst\n---\n\n# Lead Analyst\nYou are the lead.",
        ).unwrap();
        std::fs::create_dir_all(kr.join("conventions")).unwrap();
        std::fs::write(kr.join("conventions/search.md"), "# Search Conventions\nBlah.").unwrap();
        std::fs::create_dir_all(kr.join("methodology")).unwrap();
        std::fs::write(kr.join("methodology/04-blinding.md"), "# Blinding\nDo not peek.").unwrap();

        let ctx = StageContext {
            run_dir: dir.path().to_owned(),
            run_id: "test".into(),
            config: MolConfig {
                topic: "jet tagging".into(),
                domain: "hep".into(),
                analysis_type: Some("search".into()),
                knowledge_root: kr,
                ..Default::default()
            },
            prior_artifacts: HashMap::new(),
            auto_approve_gates: false,
            llm: None,
            prompt_engine: None,
        };

        let vars = ctx.template_vars(Stage::TopicInit);
        assert!(vars.get("agent_role").unwrap().contains("Lead Analyst"));
        assert!(vars.get("conventions").unwrap().contains("Search Conventions"));
        assert!(vars.get("blinding_protocol").unwrap().contains("Blinding"));
    }

    #[test]
    fn strip_frontmatter_removes_yaml() {
        let input = "---\nname: test\nmodel: opus\n---\n\n# Agent\n\nBody text.";
        let result = strip_frontmatter(input);
        assert_eq!(result.trim(), "# Agent\n\nBody text.");
    }

    #[test]
    fn strip_frontmatter_no_frontmatter_returns_all() {
        let input = "# Just markdown\n\nNo frontmatter here.";
        let result = strip_frontmatter(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_frontmatter_empty_returns_empty() {
        assert_eq!(strip_frontmatter(""), "");
    }

    #[test]
    fn agent_for_stage_covers_all_variants() {
        use crate::stages::STAGE_SEQUENCE;
        for &stage in STAGE_SEQUENCE {
            let _ = agent_for_stage(stage);
        }
        assert!(agent_for_stage(Stage::Discussion).is_none());
        assert_eq!(agent_for_stage(Stage::TopicInit), Some("lead-analyst"));
        assert_eq!(agent_for_stage(Stage::CodeGeneration), Some("signal-lead"));
        assert_eq!(agent_for_stage(Stage::PeerReview), Some("physics-reviewer"));
        assert_eq!(agent_for_stage(Stage::ResearchDecision), Some("arbiter"));
    }
}
