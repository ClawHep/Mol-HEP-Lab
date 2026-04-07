//! Multi-phase code generation agent.
//!
//! Ported from `backend/agent/researchclaw/pipeline/code_agent.py` (1 397 lines).
//!
//! # Phases
//! 1. **Blueprint Planning** — LLM produces a YAML blueprint with per-file
//!    pseudocode and dependency ordering.
//! 2. **Sequential File Generation** — files generated one-by-one following
//!    the blueprint dependency order, with CodeMem summaries injected into each
//!    subsequent prompt.  Falls back to single-shot generation when the blueprint
//!    is absent or invalid.
//! 2.5 **Hard Validation Gates** — regex-based heuristic checks; targeted LLM
//!    repair for critical issues (syntax markers, hardcoded metrics, cross-file
//!    import mismatches, etc.).
//! 3. **Execution-in-the-Loop** — run code in sandbox, feed stderr back for repair.
//! 4. **Solution Tree Search** — optional; explores multiple candidate
//!    implementations, evaluates via sandbox, picks the highest-scoring node.
//! 5. **Multi-Agent Review Dialog** — reviewer→coder loop with safety reversion.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// LLM / Sandbox traits
// ---------------------------------------------------------------------------

/// A minimal chat message struct (role + content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Lightweight response from an LLM call.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
}

/// Trait for LLM clients used by `CodeAgent`.
///
/// The agent only needs a single `chat` call.  Wire this to `mol-llm` or any
/// test double.
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        system: &str,
        max_tokens: u32,
    ) -> Result<LlmResponse>;
}

/// Result from a sandbox execution.
#[derive(Debug, Clone, Default)]
pub struct SandboxResult {
    pub returncode: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_sec: f64,
    pub metrics: HashMap<String, Value>,
    pub timed_out: bool,
}

/// Trait for sandbox backends used by `CodeAgent`.
///
/// Implementors write the provided files to a temp directory and execute
/// `entry_point`, capturing stdout/stderr and any parsed metrics.
#[async_trait]
pub trait SandboxLike: Send + Sync {
    async fn run_project(
        &self,
        project_dir: &Path,
        entry_point: &str,
        timeout_sec: u64,
    ) -> Result<SandboxResult>;
}

// ---------------------------------------------------------------------------
// CodeAgentConfig
// ---------------------------------------------------------------------------

/// Configuration for the multi-phase code generation agent.
///
/// Mirrors the Python `CodeAgentConfig` frozen dataclass.  All phase toggles
/// default to the same values as the Python original.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAgentConfig {
    pub enabled: bool,

    // Phase 1
    pub architecture_planning: bool,

    // Phase 2
    pub sequential_generation: bool,

    // Phase 2.5
    pub hard_validation: bool,
    pub hard_validation_max_repairs: u32,

    // Phase 3
    pub exec_fix_max_iterations: u32,
    pub exec_fix_timeout_sec: u64,

    // Phase 4 (tree search — off by default)
    pub tree_search_enabled: bool,
    pub tree_search_candidates: u32,
    pub tree_search_max_depth: u32,
    pub tree_search_eval_timeout_sec: u64,

    // Phase 5
    pub review_max_rounds: u32,
}

impl Default for CodeAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            architecture_planning: true,
            sequential_generation: true,
            hard_validation: true,
            hard_validation_max_repairs: 2,
            exec_fix_max_iterations: 3,
            exec_fix_timeout_sec: 60,
            tree_search_enabled: false,
            tree_search_candidates: 3,
            tree_search_max_depth: 2,
            tree_search_eval_timeout_sec: 120,
            review_max_rounds: 2,
        }
    }
}

// ---------------------------------------------------------------------------
// SolutionNode
// ---------------------------------------------------------------------------

/// One candidate solution in the tree search.
///
/// Mirrors Python `SolutionNode`.
#[derive(Debug, Clone, Default)]
pub struct SolutionNode {
    pub node_id: String,
    pub files: HashMap<String, String>,
    pub parent_id: Option<String>,
    pub depth: u32,
    // Evaluation
    pub runs_ok: bool,
    pub returncode: i32,
    pub stdout: String,
    pub stderr: String,
    pub metrics: HashMap<String, Value>,
    pub score: f64,
    pub generation_method: String,
}

impl SolutionNode {
    pub fn new(node_id: impl Into<String>, files: HashMap<String, String>) -> Self {
        Self {
            node_id: node_id.into(),
            files,
            returncode: -1,
            generation_method: "initial".to_owned(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// CodeAgentResult
// ---------------------------------------------------------------------------

/// Final output from the code generation agent.
///
/// Mirrors Python `CodeAgentResult`.
#[derive(Debug, Clone, Default)]
pub struct CodeAgentResult {
    pub files: HashMap<String, String>,
    pub architecture_spec: String,
    pub validation_log: Vec<String>,
    pub total_llm_calls: u32,
    pub total_sandbox_runs: u32,
    pub best_score: f64,
    pub tree_nodes_explored: u32,
    pub review_rounds: u32,
}

// ---------------------------------------------------------------------------
// Blueprint types (YAML deserialization)
// ---------------------------------------------------------------------------

/// A single file entry in the blueprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintFile {
    /// File name / relative path (e.g. `"main.py"`).
    #[serde(default)]
    pub path: String,
    /// Alias used in some blueprints (`name` vs `path`).
    #[serde(default)]
    pub name: String,
    /// Human-readable purpose string.
    #[serde(default)]
    pub purpose: String,
    /// Generation order index (lower = generated first).
    pub generation_order: Option<u32>,
    /// Files this file depends on.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Free-form pseudocode block.
    #[serde(default)]
    pub pseudocode: String,
    /// Any extra fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl BlueprintFile {
    /// Return the effective file name (prefers `name` over `path`).
    pub fn effective_name(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else {
            &self.path
        }
    }
}

/// Parsed blueprint returned by `CodeAgent::parse_blueprint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    /// Ordered file specifications.
    pub files: Vec<BlueprintFile>,
    /// Optional explicit generation order list (file names).
    #[serde(default)]
    pub generation_order: Vec<String>,
}

// ---------------------------------------------------------------------------
// CodeAgent
// ---------------------------------------------------------------------------

/// Multi-phase code generation agent.
///
/// Mirrors the Python `CodeAgent` class.  The LLM client and sandbox are
/// supplied as trait objects so the struct is easily testable.
pub struct CodeAgent {
    llm: Box<dyn LlmClient>,
    config: CodeAgentConfig,
    stage_dir: PathBuf,
    sandbox: Option<Box<dyn SandboxLike>>,
    /// Optional domain-profile context string (injected into Phase 1 prompt).
    domain_context: Option<String>,
    /// Optional code-search result context string (injected into Phase 1).
    code_search_context: Option<String>,

    // Mutable counters / log — wrapped in regular fields (agent is consumed
    // by `generate()` or used with `&mut self`).
    llm_calls: u32,
    sandbox_runs: u32,
    log: Vec<String>,
}

impl CodeAgent {
    // ── Constructor ────────────────────────────────────────────────────────

    pub fn new(
        llm: Box<dyn LlmClient>,
        config: CodeAgentConfig,
        stage_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            llm,
            config,
            stage_dir: stage_dir.into(),
            sandbox: None,
            domain_context: None,
            code_search_context: None,
            llm_calls: 0,
            sandbox_runs: 0,
            log: Vec::new(),
        }
    }

    /// Attach a sandbox backend (required for Phases 3 and 4).
    pub fn with_sandbox(mut self, sandbox: Box<dyn SandboxLike>) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    /// Inject pre-built domain profile context (Phase 1 blueprint prompt).
    pub fn with_domain_context(mut self, ctx: impl Into<String>) -> Self {
        self.domain_context = Some(ctx.into());
        self
    }

    /// Inject code-search result context (Phase 1 blueprint prompt).
    pub fn with_code_search_context(mut self, ctx: impl Into<String>) -> Self {
        self.code_search_context = Some(ctx.into());
        self
    }

    // ── Public API ─────────────────────────────────────────────────────────

    /// Execute all enabled phases and return generated files.
    pub async fn generate(
        &mut self,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
        max_tokens: u32,
    ) -> Result<CodeAgentResult> {
        let t0 = Instant::now();
        self.log_event("CodeAgent.generate() started");

        // Phase 1: Blueprint planning
        let mut arch_spec = String::new();
        let mut blueprint: Option<Blueprint> = None;
        if self.config.architecture_planning {
            let (spec, bp) =
                self.phase1_blueprint(topic, exp_plan, metric, pkg_hint).await?;
            arch_spec = spec;
            blueprint = bp;
        }

        // Phase 2 / 4: Code generation
        let mut nodes_explored = 0u32;
        let mut best = if self.config.tree_search_enabled && self.sandbox.is_some() {
            let (b, n) = self
                .phase3_tree_search(topic, exp_plan, metric, pkg_hint, &arch_spec, max_tokens)
                .await?;
            nodes_explored = n;
            b
        } else if self.config.sequential_generation
            && blueprint.as_ref().map(|b| Self::is_valid_blueprint(b)).unwrap_or(false)
        {
            let bp = blueprint.as_ref().unwrap();
            let mut files = self
                .phase2_sequential_generate(topic, exp_plan, metric, pkg_hint, &arch_spec, bp)
                .await?;
            if self.config.hard_validation {
                files = self
                    .hard_validate_and_repair(&files, topic, exp_plan, metric, pkg_hint, &arch_spec)
                    .await?;
            }
            files = self.exec_fix_loop(files).await?;
            SolutionNode {
                node_id: "sequential".to_owned(),
                files,
                runs_ok: true,
                score: 1.0,
                returncode: 0,
                ..Default::default()
            }
        } else {
            if self.config.sequential_generation && blueprint.is_none() {
                self.log_event(
                    "Sequential generation requested but blueprint invalid — \
                     falling back to single-shot",
                );
            }
            let mut files = self
                .phase2_generate_and_fix(topic, exp_plan, metric, pkg_hint, &arch_spec, max_tokens)
                .await?;
            if self.config.hard_validation && !files.is_empty() {
                files = self
                    .hard_validate_and_repair(&files, topic, exp_plan, metric, pkg_hint, &arch_spec)
                    .await?;
            }
            let ok = !files.is_empty();
            SolutionNode {
                node_id: "single".to_owned(),
                files,
                runs_ok: ok,
                score: if ok { 1.0 } else { 0.0 },
                returncode: if ok { 0 } else { -1 },
                ..Default::default()
            }
        };

        // Phase 5: Review dialog
        let mut review_rounds = 0u32;
        if self.config.review_max_rounds > 0 {
            let pre_review = best.files.clone();
            let (reviewed, rounds) = self
                .phase4_review(best.files, topic, exp_plan, metric)
                .await?;
            review_rounds = rounds;
            // Safety reversion: if review broke a previously-valid .py file,
            // revert to pre-review version (regex syntax check).
            let mut final_files = reviewed;
            for (fname, code) in &final_files.clone() {
                if fname.ends_with(".py") {
                    if !Self::passes_syntax_heuristic(code) {
                        if let Some(pre) = pre_review.get(fname) {
                            if Self::passes_syntax_heuristic(pre) {
                                self.log_event(&format!(
                                    "WARNING: Review broke {fname} — reverting to pre-review"
                                ));
                                final_files.insert(fname.clone(), pre.clone());
                            }
                        }
                    }
                }
            }
            best.files = final_files;
        }

        let elapsed = t0.elapsed().as_secs_f64();
        self.log_event(&format!(
            "CodeAgent.generate() done in {elapsed:.1}s — {} LLM calls, {} sandbox runs",
            self.llm_calls, self.sandbox_runs
        ));

        Ok(CodeAgentResult {
            files: best.files,
            architecture_spec: arch_spec,
            validation_log: self.log.clone(),
            total_llm_calls: self.llm_calls,
            total_sandbox_runs: self.sandbox_runs,
            best_score: best.score,
            tree_nodes_explored: nodes_explored,
            review_rounds,
        })
    }

    // ── Phase 1: Blueprint Planning ────────────────────────────────────────

    async fn phase1_blueprint(
        &mut self,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
    ) -> Result<(String, Option<Blueprint>)> {
        self.log_event("Phase 1: Blueprint planning");

        let system = "You are MolAgent, an expert scientific code generation architect. \
            Produce a detailed implementation blueprint in YAML format.";

        let mut user = format!(
            "Create a deep implementation blueprint for the following experiment.\n\n\
             ## Topic\n{topic}\n\n\
             ## Experiment Plan\n{exp_plan}\n\n\
             ## Primary Metric\n{metric}\n\n\
             ## Package Hint\n{pkg_hint}\n\n\
             Output a YAML blueprint with fields:\n\
             - files: list of {{name, purpose, generation_order, dependencies, pseudocode}}\n\
             - generation_order: list of file names in dependency order\n\
             Wrap the YAML in ```yaml ... ``` fences."
        );

        // Inject domain and code-search context
        let domain_ctx = self.build_domain_context();
        if !domain_ctx.is_empty() {
            user.push_str("\n\n");
            user.push_str(&domain_ctx);
            self.log_event("  Injected domain context into blueprint prompt");
        }

        let resp = self.chat(system, &user, 8192).await?;
        let mut arch_spec = resp.content.clone();

        // Extract YAML block
        let yaml_re = Regex::new(r"(?s)```ya?ml\s*\n(.*?)```").unwrap();
        if let Some(cap) = yaml_re.captures(&arch_spec) {
            arch_spec = cap[1].trim().to_owned();
        }

        self.log_event(&format!("  Blueprint spec: {} chars", arch_spec.len()));

        let blueprint = Self::parse_blueprint(&arch_spec);
        match &blueprint {
            Some(bp) => self.log_event(&format!("  Parsed blueprint: {} files", bp.files.len())),
            None => self.log_event("  WARNING: Could not parse blueprint YAML"),
        }

        Ok((arch_spec, blueprint))
    }

    fn build_domain_context(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(dc) = &self.domain_context {
            if !dc.is_empty() {
                parts.push(format!("# Domain-Specific Guidance\n{dc}"));
            }
        }
        if let Some(sc) = &self.code_search_context {
            if !sc.is_empty() {
                parts.push(format!(
                    "# Reference Code from GitHub\n\
                     The following patterns were found in relevant open-source projects. \
                     Use them as reference for API usage and project structure.\n\n{sc}"
                ));
            }
        }
        parts.join("\n\n")
    }

    /// Parse blueprint YAML text into a [`Blueprint`].
    ///
    /// Returns `None` when the YAML is malformed or lacks a `files` key.
    pub fn parse_blueprint(yaml_text: &str) -> Option<Blueprint> {
        match serde_yaml::from_str::<Blueprint>(yaml_text) {
            Ok(bp) => Some(bp),
            Err(e) => {
                debug!("[CodeAgent] Blueprint YAML parse error: {e}");
                None
            }
        }
    }

    /// Return `true` when the blueprint has the minimum required structure.
    ///
    /// Matches the Python original: at least 2 files must have a
    /// `generation_order` field set.
    pub fn is_valid_blueprint(bp: &Blueprint) -> bool {
        let has_order = bp.files.iter()
            .filter(|f| f.generation_order.is_some())
            .count();
        has_order >= 2
    }

    // ── Phase 2a: Sequential File Generation ──────────────────────────────

    async fn phase2_sequential_generate(
        &mut self,
        topic: &str,
        exp_plan: &str,
        _metric: &str,
        pkg_hint: &str,
        arch_spec: &str,
        blueprint: &Blueprint,
    ) -> Result<HashMap<String, String>> {
        self.log_event("Phase 2: Sequential generation (blueprint-guided)");

        let mut generated_files: HashMap<String, String> = HashMap::new();
        let mut code_memory: HashMap<String, CodeSummary> = HashMap::new();

        // Sort files by generation_order
        let mut file_specs: Vec<BlueprintFile> = blueprint.files.clone();
        for (i, fs) in file_specs.iter_mut().enumerate() {
            if fs.generation_order.is_none() {
                fs.generation_order = Some(i as u32 + 1);
            }
        }
        file_specs.sort_by_key(|f| f.generation_order.unwrap_or(99));

        for file_spec in &file_specs {
            let file_name = file_spec.effective_name();
            if file_name.is_empty() {
                continue;
            }

            self.log_event(&format!(
                "  Generating {file_name} (order={:?})",
                file_spec.generation_order
            ));

            // Build dependency context
            let (dep_summaries, dep_code) = {
                let mut summaries = String::new();
                let mut code_ctx = String::new();
                for dep in &file_spec.dependencies {
                    if let Some(summary) = code_memory.get(dep) {
                        summaries.push_str(&format!(
                            "\n### {dep} (summary)\n{}\n",
                            serde_json::to_string_pretty(summary).unwrap_or_default()
                        ));
                    }
                    if let Some(code) = generated_files.get(dep) {
                        code_ctx.push_str(&format!("\n### {dep}\n```python\n{code}\n```\n"));
                    }
                }
                if summaries.is_empty() {
                    summaries = "(no dependencies yet)".to_owned();
                }
                if code_ctx.is_empty() {
                    code_ctx = "(no dependencies yet)".to_owned();
                }
                (summaries, code_ctx)
            };

            let file_spec_json =
                serde_json::to_string_pretty(&file_spec).unwrap_or_default();
            let exp_plan_trunc = &exp_plan[..exp_plan.len().min(4000)];

            let system = "You are MolAgent, an expert scientific code generation assistant. \
                Generate one specific Python file following the blueprint exactly.";
            let user = format!(
                "Generate the Python file `{file_name}` according to this specification.\n\n\
                 ## File Specification\n```json\n{file_spec_json}\n```\n\n\
                 ## Full Blueprint\n{arch_spec}\n\n\
                 ## Already-Generated Dependencies\n{dep_code}\n\n\
                 ## Dependency Summaries\n{dep_summaries}\n\n\
                 ## Topic\n{topic}\n\n\
                 ## Experiment Plan\n{exp_plan_trunc}\n\n\
                 ## Package Hint\n{pkg_hint}\n\n\
                 Output the complete Python source for `{file_name}` inside a \
                 ```python ... ``` fenced block."
            );

            let resp = self.chat(system, &user, 8192).await?;
            let code = Self::extract_single_file_code(&resp.content, file_name);
            if code.is_empty() {
                self.log_event(&format!("  WARNING: Empty code for {file_name}"));
                continue;
            }

            let summary = Self::build_code_summary(file_name, &code);
            let n_classes = summary.classes.len();
            let n_lines = code.lines().count();
            self.log_event(&format!("  {file_name}: {n_lines} lines, {n_classes} classes"));

            code_memory.insert(file_name.to_owned(), summary);
            generated_files.insert(file_name.to_owned(), code);
        }

        // Ensure main.py exists
        if !generated_files.contains_key("main.py") {
            self.log_event("  WARNING: No main.py generated, promoting first file");
            if let Some(first_key) = generated_files.keys().next().cloned() {
                let code = generated_files.remove(&first_key).unwrap();
                generated_files.insert("main.py".to_owned(), code);
            }
        }

        self.log_event(&format!(
            "  Sequential generation complete: {} files",
            generated_files.len()
        ));
        Ok(generated_files)
    }

    /// Extract Python source from a single-file LLM response.
    ///
    /// Mirrors `_extract_single_file_code` in the Python original.
    pub fn extract_single_file_code(content: &str, expected_name: &str) -> String {
        // Try ```python ... ``` block
        let py_re = Regex::new(r"(?s)```python\s*\n(.*?)```").unwrap();
        if let Some(cap) = py_re.captures(content) {
            return cap[1].trim().to_owned();
        }
        // Try ```filename:expected_name ... ``` block
        let escaped = regex::escape(expected_name);
        let named_re =
            Regex::new(&format!(r"(?s)```(?:filename:)?{escaped}\s*\n(.*?)```")).unwrap();
        if let Some(cap) = named_re.captures(content) {
            return cap[1].trim().to_owned();
        }
        // If it looks like raw Python (starts with common Python tokens)
        let stripped = content.trim();
        if !stripped.is_empty()
            && (stripped.starts_with("import ")
                || stripped.starts_with("from ")
                || stripped.starts_with('#')
                || stripped.starts_with("def ")
                || stripped.starts_with("class ")
                || stripped.starts_with("\"\"\""))
        {
            return stripped.to_owned();
        }
        String::new()
    }

    /// Build a regex-based code summary (CodeMem substitute for Python AST).
    ///
    /// The Python version uses `ast.parse`; we approximate with regex patterns
    /// over the generated Python source.
    pub fn build_code_summary(filename: &str, code: &str) -> CodeSummary {
        let mut summary = CodeSummary {
            filename: filename.to_owned(),
            ..Default::default()
        };

        // Classes
        let class_re = Regex::new(r"(?m)^class\s+(\w+)(?:\(([^)]*)\))?:").unwrap();
        // Top-level functions (no leading spaces)
        let func_re = Regex::new(r"(?m)^def\s+(\w+)\s*\(([^)]*)\):").unwrap();
        // Method definitions (4-space or tab indent)
        let method_re = Regex::new(r"(?m)^    def\s+(\w+)\s*\(([^)]*)\):").unwrap();
        // Import lines
        let import_re =
            Regex::new(r"(?m)^(?:import\s+\S+|from\s+\S+\s+import\s+.+)").unwrap();

        // Collect class blocks — from `class X:` to the next `^class` or EOF,
        // so that method search is scoped to each class's own body.
        // Build a list of (byte_offset, capture) for each class header.
        let class_captures: Vec<(usize, regex::Captures)> =
            class_re.captures_iter(code)
                .map(|cap| {
                    let start = cap.get(0).unwrap().start();
                    (start, cap)
                })
                .collect();

        for (idx, (start, cap)) in class_captures.iter().enumerate() {
            let name = cap[1].to_owned();
            let bases = cap
                .get(2)
                .map(|m| {
                    m.as_str()
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            // Slice the class body: from this class header to the next `^class`
            // header (or EOF). This ensures methods are scoped to their class.
            let end = class_captures
                .get(idx + 1)
                .map(|(next_start, _)| *next_start)
                .unwrap_or(code.len());
            let class_block = &code[*start..end];

            // Collect methods inside this class block only
            let methods: Vec<MethodSummary> = method_re
                .captures_iter(class_block)
                .map(|mc| MethodSummary {
                    name: mc[1].to_owned(),
                    args: mc[2]
                        .split(',')
                        .map(|a| a.trim().trim_start_matches('*').to_owned())
                        .filter(|a| !a.is_empty() && a != "self")
                        .collect(),
                })
                .collect();

            summary.classes.push(ClassSummary { name, bases, methods });
        }

        // Top-level functions
        for cap in func_re.captures_iter(code) {
            summary.functions.push(FunctionSummary {
                name: cap[1].to_owned(),
                args: cap[2]
                    .split(',')
                    .map(|a| a.trim().to_owned())
                    .filter(|a| !a.is_empty())
                    .collect(),
            });
        }

        // Imports
        for cap in import_re.find_iter(code) {
            summary.imports.push(cap.as_str().trim().to_owned());
        }

        summary
    }

    // ── Phase 2.5: Hard Validation Gates ──────────────────────────────────

    async fn hard_validate_and_repair(
        &mut self,
        files: &HashMap<String, String>,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        _pkg_hint: &str,
        arch_spec: &str,
    ) -> Result<HashMap<String, String>> {
        self.log_event("Phase 2.5: Hard validation gates");
        let mut files = files.clone();

        for attempt in 0..=self.config.hard_validation_max_repairs {
            let (critical, warnings) = Self::hard_validate(&files);

            for w in &warnings {
                self.log_event(&format!("  WARNING: {w}"));
            }

            if critical.is_empty() {
                self.log_event(&format!(
                    "  Hard validation passed ({} warning(s), attempt {attempt})",
                    warnings.len()
                ));
                return Ok(files);
            }

            self.log_event(&format!(
                "  Hard validation found {} CRITICAL issue(s) (attempt {attempt}/{})",
                critical.len(),
                self.config.hard_validation_max_repairs
            ));
            for c in &critical {
                self.log_event(&format!("  CRITICAL: {c}"));
            }

            if attempt >= self.config.hard_validation_max_repairs {
                self.log_event("  Max repair attempts reached — proceeding with warnings");
                return Ok(files);
            }

            files = self
                .repair_critical_issues(&files, &critical, topic, exp_plan, metric, arch_spec)
                .await?;
        }

        Ok(files)
    }

    /// Regex-based heuristic hard validation.
    ///
    /// Returns `(critical_issues, warnings)`.  The Python original uses
    /// `ast.parse` and the `researchclaw.experiment.validator` module; here we
    /// approximate with pattern matching over generated Python text.
    pub fn hard_validate(
        files: &HashMap<String, String>,
    ) -> (Vec<String>, Vec<String>) {
        let mut critical: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // 1. Syntax heuristic — unbalanced triple-quote strings or obvious
        //    truncation markers are flagged as CRITICAL.
        for (fname, code) in files {
            if !fname.ends_with(".py") {
                continue;
            }
            if !Self::passes_syntax_heuristic(code) {
                critical.push(format!("[{fname}] Possible syntax error (unbalanced quotes or truncated file)"));
            }
        }

        // 2. Hardcoded metrics — CRITICAL
        let hardcoded_re =
            Regex::new(r#"(?m)(accuracy|loss|fid|score|metric)\s*=\s*0\.\d+"#).unwrap();
        for (fname, code) in files {
            if !fname.ends_with(".py") {
                continue;
            }
            for cap in hardcoded_re.find_iter(code) {
                critical.push(format!(
                    "[{fname}] Hardcoded metric value: `{}` — compute from actual data",
                    &code[cap.range()]
                ));
            }
        }

        // 3. nn.Module layers created in forward() — CRITICAL
        let forward_layer_re = Regex::new(
            r"(?s)def forward\s*\([^)]*\):[^\n]*\n(?:(?:[ \t]+[^\n]*\n)*?[ \t]+(?:nn\.|torch\.nn\.))",
        )
        .unwrap();
        for (fname, code) in files {
            if !fname.ends_with(".py") {
                continue;
            }
            if forward_layer_re.is_match(code) {
                critical.push(format!(
                    "[{fname}] nn.Module layer created inside forward() — move to __init__()"
                ));
            }
        }

        // 4. Cross-file import consistency
        let known_modules: HashSet<String> = files
            .keys()
            .filter(|f| f.ends_with(".py"))
            .map(|f| f.trim_end_matches(".py").to_owned())
            .collect();

        let from_import_re =
            Regex::new(r"(?m)^from\s+([\w.]+)\s+import\s+(.+)$").unwrap();
        for (fname, code) in files {
            if !fname.ends_with(".py") {
                continue;
            }
            for cap in from_import_re.captures_iter(code) {
                let module = &cap[1];
                let mod_top = module.split('.').next().unwrap_or("");
                if !known_modules.contains(mod_top) {
                    continue; // third-party import — skip
                }
                let target_file = format!("{mod_top}.py");
                if let Some(target_code) = files.get(&target_file) {
                    let names_str = cap[2].trim();
                    let exported = Self::extract_exported_names(target_code);
                    for name in names_str.split(',').map(|n| n.trim()) {
                        let clean = name.split_whitespace().next().unwrap_or(name);
                        if clean != "*" && !clean.is_empty() && !exported.contains(clean) {
                            critical.push(format!(
                                "[{fname}] ImportError: '{clean}' not defined in '{target_file}'"
                            ));
                        }
                    }
                }
            }
        }

        // 4. API correctness heuristic — NameError detection.
        //    Look for traceback-style "name 'X' is not defined" patterns embedded
        //    in code (e.g. inside string literals or comments left by the LLM).
        //    Also flag calls to bare names that look like undefined globals: any
        //    `name(` or `name.` usage where `name` is one of a set of commonly
        //    confused builtins/stdlib items that differ between Python versions.
        let name_error_literal_re =
            Regex::new(r"name\s+'(\w+)'\s+is\s+not\s+defined").unwrap();
        // Heuristic: referencing `undefined` (JS habit), `NULL` (SQL/C habit),
        // or `True`/`False`/`None` spelled incorrectly.
        let undefined_name_re =
            Regex::new(r"\b(undefined|NULL|TRUE|FALSE|NONE|Null|False\b|True\b|None\b)").unwrap();
        for (fname, code) in files.iter() {
            if !fname.ends_with(".py") {
                continue;
            }
            // Flag any embedded traceback NameError messages
            for cap in name_error_literal_re.captures_iter(code) {
                critical.push(format!(
                    "[{fname}] NameError heuristic: name '{}' appears to be undefined",
                    &cap[1]
                ));
            }
            // Flag non-Python identifiers that would cause NameError at runtime
            for cap in undefined_name_re.captures_iter(code) {
                let hit = &cap[1];
                // Filter out valid Python keywords embedded in strings/comments
                if matches!(hit, "True" | "False" | "None") {
                    continue; // these are valid Python
                }
                warnings.push(format!(
                    "[{fname}] Possible NameError: non-Python identifier `{hit}` — use Python equivalents"
                ));
            }
        }

        // 5. Variable scoping heuristic — UnboundLocalError detection.
        //    Look for the explicit augmented-assign-before-init pattern
        //    (`x += ...` without a prior `x = <value>` in the file).
        //    We cannot use negative lookahead (not supported by the `regex`
        //    crate), so we use a string scan on the slice of code before the
        //    augmented assignment.
        let augmented_assign_re =
            Regex::new(r"(?m)^([ \t]+)(\w+)\s*\+=").unwrap();
        for (fname, code) in files.iter() {
            if !fname.ends_with(".py") {
                continue;
            }
            for cap in augmented_assign_re.captures_iter(code) {
                let var_name = cap[2].to_owned();
                // Byte offset of this augmented assignment in the file
                let aug_offset = cap.get(0).unwrap().start();
                // Look in the text before this point for a plain assignment:
                // `<indent>var_name = ` (the char after `=` must not be `=`).
                let prior_code = &code[..aug_offset];
                let assign_pattern = format!("{var_name} = ");
                let found = prior_code
                    .lines()
                    .any(|line| {
                        let trimmed = line.trim_start();
                        trimmed.starts_with(&assign_pattern)
                            || trimmed.starts_with(&format!("{var_name}="))
                                && !trimmed.starts_with(&format!("{var_name}=="))
                    });
                if !found {
                    warnings.push(format!(
                        "[{fname}] Possible UnboundLocalError: `{var_name}` used with `+=` before assignment"
                    ));
                }
            }
        }

        // 5. Empty ablation / variant classes — WARNING (promote to CRITICAL
        //    when the body is just `pass`)
        let empty_class_re = Regex::new(r"(?m)^class\s+(\w+)\([^)]+\):\s*\n\s+pass\s*$").unwrap();
        for (fname, code) in files {
            if !fname.ends_with(".py") {
                continue;
            }
            for cap in empty_class_re.captures_iter(code) {
                warnings.push(format!(
                    "[{fname}] empty or trivial subclass `{}` — add real implementation",
                    &cap[1]
                ));
            }
        }

        (critical, warnings)
    }

    /// Returns `true` when the code passes a quick heuristic syntax check.
    pub fn passes_syntax_heuristic(code: &str) -> bool {
        // Count triple-quote occurrences — odd count means unbalanced
        let dq = code.matches("\"\"\"").count();
        let sq = code.matches("'''").count();
        if dq % 2 != 0 || sq % 2 != 0 {
            return false;
        }
        // Flag obvious LLM truncation
        if code.trim_end().ends_with("...") && code.lines().count() < 5 {
            return false;
        }
        true
    }

    /// Collect names exported by a Python source file (class defs, top-level
    /// function defs, and top-level assignments).
    fn extract_exported_names(code: &str) -> HashSet<String> {
        let mut names = HashSet::new();
        let class_re = Regex::new(r"(?m)^class\s+(\w+)").unwrap();
        let func_re = Regex::new(r"(?m)^def\s+(\w+)").unwrap();
        let assign_re = Regex::new(r"(?m)^(\w+)\s*=").unwrap();
        for cap in class_re.captures_iter(code) {
            names.insert(cap[1].to_owned());
        }
        for cap in func_re.captures_iter(code) {
            names.insert(cap[1].to_owned());
        }
        for cap in assign_re.captures_iter(code) {
            names.insert(cap[1].to_owned());
        }
        names
    }

    async fn repair_critical_issues(
        &mut self,
        files: &HashMap<String, String>,
        critical_issues: &[String],
        _topic: &str,
        _exp_plan: &str,
        _metric: &str,
        arch_spec: &str,
    ) -> Result<HashMap<String, String>> {
        self.log_event("  Targeted repair for critical issues");

        let files_ctx = Self::format_files(files);
        let issues_text = critical_issues
            .iter()
            .map(|i| format!("- {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let arch_trunc = &arch_spec[..arch_spec.len().min(4000)];

        let system = "You are MolAgent, an expert Python debugging assistant. Fix all listed issues.";
        let user = format!(
            "Your generated code has CRITICAL issues that will cause runtime failures. Fix ALL.\n\n\
             ## Critical Issues Found\n{issues_text}\n\n\
             ## Architecture Blueprint\n{arch_trunc}\n\n\
             ## Current Code\n{files_ctx}\n\n\
             ## Rules\n\
             1. Fix every critical issue listed above\n\
             2. Ablation/variant classes MUST have different implementations from parent\n\
             3. Never hardcode metric values — compute from actual data\n\
             4. nn.Module layers must be created in __init__(), not forward()\n\
             5. All cross-file imports must reference names that actually exist\n\
             6. Output ALL files in ```filename:xxx.py``` format"
        );

        let resp = self.chat(system, &user, 8192).await?;
        let fixed = Self::extract_files(&resp.content);
        if !fixed.is_empty() {
            let mut merged = files.clone();
            let fixed_names: Vec<_> = fixed.keys().cloned().collect();
            merged.extend(fixed);
            self.log_event(&format!(
                "  Repair updated {} file(s): {}",
                fixed_names.len(),
                fixed_names.join(", ")
            ));
            Ok(merged)
        } else {
            self.log_event("  WARNING: Repair produced no extractable files");
            Ok(files.clone())
        }
    }

    // ── Phase 2b: Single-Shot Generate + Exec-Fix ─────────────────────────

    async fn phase2_generate_and_fix(
        &mut self,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
        arch_spec: &str,
        max_tokens: u32,
    ) -> Result<HashMap<String, String>> {
        self.log_event("Phase 2: Single-shot generate + exec-fix");
        let files = self
            .generate_code(topic, exp_plan, metric, pkg_hint, arch_spec, max_tokens)
            .await?;
        if files.is_empty() {
            self.log_event("  WARNING: empty generation, returning fallback");
            return Ok(files);
        }
        self.exec_fix_loop(files).await
    }

    async fn exec_fix_loop(
        &mut self,
        mut files: HashMap<String, String>,
    ) -> Result<HashMap<String, String>> {
        if self.sandbox.is_none() || self.config.exec_fix_max_iterations == 0 {
            return Ok(files);
        }

        for i in 0..self.config.exec_fix_max_iterations {
            let result = self.run_in_sandbox(&files, None).await?;
            if result.returncode == 0 {
                self.log_event(&format!("  Exec-fix iter {i}: code runs OK"));
                break;
            }
            self.log_event(&format!(
                "  Exec-fix iter {i}: crashed (rc={}), stderr={} chars",
                result.returncode,
                result.stderr.len()
            ));
            files = self.fix_runtime_error(files, &result).await?;
        }

        Ok(files)
    }

    async fn generate_code(
        &mut self,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
        arch_spec: &str,
        max_tokens: u32,
    ) -> Result<HashMap<String, String>> {
        let mut hint = pkg_hint.to_owned();
        if !arch_spec.is_empty() {
            hint.push_str(
                "\n\n## ARCHITECTURE SPECIFICATION (follow this file and class structure)\n",
            );
            hint.push_str(arch_spec);
        }
        // Numerical stability requirements (BUG-004 parity)
        hint.push_str(
            "\n\n## NUMERICAL STABILITY (MANDATORY)\n\
             - Add gradient clipping: `torch.nn.utils.clip_grad_norm_(params, 1.0)`\n\
             - After each optimizer step, check for NaN loss:\n\
             `if torch.isnan(loss): print('FAIL: NaN detected'); break`\n\
             - When logging metrics, guard against NaN/Inf:\n\
             `v = float(val); v = 0.0 if (math.isnan(v) or math.isinf(v)) else v`\n\
             - For RL: clip rewards to [-10, 10], use reward normalization\n",
        );

        let system = "You are MolAgent, an expert scientific code generation assistant. \
            Write clean, correct, executable experiment code.";
        let user = format!(
            "Generate complete experiment code.\n\n\
             ## Topic\n{topic}\n\n\
             ## Metric\n{metric}\n\n\
             ## Package Hint\n{hint}\n\n\
             ## Experiment Plan\n{exp_plan}\n\n\
             Output ALL files in ```filename:xxx.py``` format."
        );

        let resp = self.chat(system, &user, max_tokens).await?;
        let mut files = Self::extract_files(&resp.content);

        if files.is_empty() && !resp.content.trim().is_empty() {
            self.log_event("  Empty extraction, retrying with higher token budget");
            let resp2 = self.chat(system, &user, 4096).await?;
            files = Self::extract_files(&resp2.content);
        }

        Ok(files)
    }

    // ── Exec-Fix helpers ───────────────────────────────────────────────────

    async fn fix_runtime_error(
        &mut self,
        files: HashMap<String, String>,
        result: &SandboxResult,
    ) -> Result<HashMap<String, String>> {
        let stderr_tail = result.stderr.chars().rev().take(3000).collect::<String>()
            .chars().rev().collect::<String>();
        let stdout_lines: Vec<&str> = result.stdout.lines().collect();
        let stdout_tail = stdout_lines
            .iter()
            .rev()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");

        // Try targeted repair first
        if let Some((fname, lineno, error_msg)) = Self::parse_error_location(&stderr_tail, &files) {
            self.log_event(&format!(
                "  Targeted repair: {fname}:{lineno} — {}",
                &error_msg[..error_msg.len().min(80)]
            ));
            if let Some(fixed) =
                self.targeted_file_repair(&files, &fname, lineno, &error_msg, &stderr_tail).await?
            {
                return Ok(fixed);
            }
        }

        // Fallback: full-file repair
        let files_ctx = Self::format_files(&files);
        let system = "You are MolAgent, a debugging expert. Fix the runtime error. Output ALL files.";
        let user = format!(
            "Fix the runtime error in this experiment code.\n\n\
             ## stderr (last 3000 chars)\n```\n{stderr_tail}\n```\n\n\
             ## stdout (last 50 lines)\n```\n{stdout_tail}\n```\n\n\
             ## Return Code\n{}\n\n\
             ## Current Code\n{files_ctx}\n\n\
             Output ALL fixed files in ```filename:xxx.py``` format.",
            result.returncode
        );

        let resp = self.chat(system, &user, 8192).await?;
        let fixed = Self::extract_files(&resp.content);
        if !fixed.is_empty() {
            let mut merged = files;
            merged.extend(fixed);
            return Ok(merged);
        }
        Ok(files)
    }

    /// Parse a Python traceback to find the failing file and line number.
    ///
    /// Returns `(filename, line_number, error_message)` or `None`.
    pub fn parse_error_location(
        stderr: &str,
        files: &HashMap<String, String>,
    ) -> Option<(String, usize, String)> {
        let known_files: HashSet<&str> = files.keys().map(String::as_str).collect();
        let tb_re = Regex::new(r#"File "(?:[^"]*[/\\])?([^"]+\.py)", line (\d+)"#).unwrap();
        let matches: Vec<_> = tb_re.captures_iter(stderr).collect();
        if matches.is_empty() {
            return None;
        }
        for cap in matches.iter().rev() {
            let fname = &cap[1];
            let lineno: usize = cap[2].parse().ok()?;
            if known_files.contains(fname) {
                let lines: Vec<&str> = stderr.trim().lines().collect();
                let error_msg = lines.last().unwrap_or(&"Unknown error").to_owned();
                return Some((fname.to_owned(), lineno, error_msg.to_owned()));
            }
        }
        None
    }

    async fn targeted_file_repair(
        &mut self,
        files: &HashMap<String, String>,
        target_file: &str,
        error_line: usize,
        error_msg: &str,
        full_stderr: &str,
    ) -> Result<Option<HashMap<String, String>>> {
        let code = match files.get(target_file) {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        let code_lines: Vec<&str> = code.lines().collect();
        let total_lines = code_lines.len();
        let window = 30usize;
        let start = error_line.saturating_sub(window + 1);
        let end = (error_line + window).min(total_lines);

        let numbered = code_lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4} | {line}", start + i + 1))
            .collect::<Vec<_>>()
            .join("\n");

        // Build compact dependency context
        let mut dep_summaries = String::new();
        for (fname, fcode) in files {
            if fname == target_file || !fname.ends_with(".py") {
                continue;
            }
            let s = Self::build_code_summary(fname, fcode);
            dep_summaries.push_str(&format!(
                "\n### {fname}: {} classes, {} functions\n",
                s.classes.len(),
                s.functions.len()
            ));
            for cls in &s.classes {
                let methods = cls.methods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ");
                dep_summaries.push_str(&format!(
                    "  class {}({}): [{methods}]\n",
                    cls.name,
                    cls.bases.join(", ")
                ));
            }
        }

        let stderr_trunc: String =
            full_stderr.chars().rev().take(1500).collect::<String>().chars().rev().collect();

        let system = "You are a debugging expert. Fix the specific runtime error. Output COMPLETE fixed file.";
        let user = format!(
            "Fix the runtime error in `{target_file}` at line {error_line}.\n\n\
             ## Error\n```\n{error_msg}\n```\n\n\
             ## Full Traceback (last 1500 chars)\n```\n{stderr_trunc}\n```\n\n\
             ## {target_file} (lines {}-{})\n```python\n{numbered}\n```\n\n\
             ## Other Files in Project\n{dep_summaries}\n\n\
             ## Full File ({target_file}, {total_lines} lines)\n```python\n{code}\n```\n\n\
             Output the COMPLETE fixed `{target_file}` in ```filename:{target_file}``` format.",
            start + 1,
            end
        );

        let resp = self.chat(system, &user, 8192).await?;
        let mut fixed = Self::extract_files(&resp.content);
        if fixed.is_empty() {
            // Try single-file extraction
            let single_re =
                Regex::new(r"(?s)```(?:python|filename:\S+)\s*\n(.*?)```").unwrap();
            if let Some(cap) = single_re.captures(&resp.content) {
                fixed.insert(target_file.to_owned(), cap[1].trim().to_owned());
            }
        }

        if fixed.contains_key(target_file) {
            let n_lines = fixed[target_file].lines().count();
            self.log_event(&format!(
                "  Targeted repair applied to {target_file} ({n_lines} lines)"
            ));
            let mut merged = files.clone();
            merged.extend(fixed);
            Ok(Some(merged))
        } else {
            Ok(None)
        }
    }

    // ── Phase 3: Solution Tree Search ──────────────────────────────────────

    async fn phase3_tree_search(
        &mut self,
        topic: &str,
        exp_plan: &str,
        metric: &str,
        pkg_hint: &str,
        arch_spec: &str,
        max_tokens: u32,
    ) -> Result<(SolutionNode, u32)> {
        self.log_event("Phase 3: Solution tree search");
        let mut all_nodes: Vec<SolutionNode> = Vec::new();

        let n_cand = self.config.tree_search_candidates.max(1);
        for k in 0..n_cand {
            self.log_event(&format!("  Generating candidate {}/{n_cand}", k + 1));
            let files = self
                .generate_code(topic, exp_plan, metric, pkg_hint, arch_spec, max_tokens)
                .await?;
            all_nodes.push(SolutionNode::new(format!("gen-{k}"), files));
        }

        for depth in 0..self.config.tree_search_max_depth {
            // Evaluate unevaluated nodes
            let node_ids: Vec<String> =
                all_nodes.iter().filter(|n| n.returncode == -1).map(|n| n.node_id.clone()).collect();
            for id in node_ids {
                let idx = all_nodes.iter().position(|n| n.node_id == id).unwrap();
                let files_clone = all_nodes[idx].files.clone();
                let timeout = self.config.tree_search_eval_timeout_sec;
                let result = self.run_in_sandbox(&files_clone, Some(timeout)).await?;
                let node = &mut all_nodes[idx];
                node.returncode = result.returncode;
                node.stdout = result.stdout.clone();
                node.stderr = result.stderr.clone();
                node.runs_ok = result.returncode == 0;
                node.metrics = result.metrics.clone();
                node.score = Self::score_node(node, metric);
            }

            all_nodes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            let best_node = &all_nodes[0];
            self.log_event(&format!(
                "  Depth {depth}: {} nodes, best={} score={:.2}",
                all_nodes.len(),
                best_node.node_id,
                best_node.score
            ));

            if all_nodes[0].runs_ok {
                break;
            }

            // Generate fix variants for top-2 crashing candidates
            let top2: Vec<_> = all_nodes.iter().take(2).cloned().collect();
            let mut new_nodes: Vec<SolutionNode> = Vec::new();
            for node in top2 {
                if !node.runs_ok {
                    let simple = SandboxResult {
                        returncode: node.returncode,
                        stdout: node.stdout.clone(),
                        stderr: node.stderr.clone(),
                        ..Default::default()
                    };
                    let fixed_files = self.fix_runtime_error(node.files.clone(), &simple).await?;
                    let new_node = SolutionNode {
                        node_id: format!("{}-fix{depth}", node.node_id),
                        files: fixed_files,
                        parent_id: Some(node.node_id.clone()),
                        depth: depth + 1,
                        generation_method: "fix".to_owned(),
                        returncode: -1,
                        ..Default::default()
                    };
                    new_nodes.push(new_node);
                }
            }
            all_nodes.extend(new_nodes);
        }

        // Final evaluation
        let remaining_ids: Vec<String> =
            all_nodes.iter().filter(|n| n.returncode == -1).map(|n| n.node_id.clone()).collect();
        for id in remaining_ids {
            let idx = all_nodes.iter().position(|n| n.node_id == id).unwrap();
            let files_clone = all_nodes[idx].files.clone();
            let timeout = self.config.tree_search_eval_timeout_sec;
            let result = self.run_in_sandbox(&files_clone, Some(timeout)).await?;
            let node = &mut all_nodes[idx];
            node.returncode = result.returncode;
            node.stdout = result.stdout.clone();
            node.stderr = result.stderr.clone();
            node.runs_ok = result.returncode == 0;
            node.metrics = result.metrics.clone();
            node.score = Self::score_node(node, metric);
        }

        all_nodes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let best = all_nodes.remove(0);
        let total = all_nodes.len() + 1;
        self.log_event(&format!(
            "  Tree search complete: best={} score={:.2}, explored {total} nodes",
            best.node_id, best.score
        ));

        Ok((best, total as u32))
    }

    /// Score a solution node based on execution results.
    ///
    /// Mirrors Python `_score_node`.
    pub fn score_node(node: &SolutionNode, metric_key: &str) -> f64 {
        let mut score = 0.0f64;
        if node.runs_ok {
            score += 1.0;
        }
        if node.stdout.len() > 100 {
            score += 0.3;
        }
        if !node.metrics.is_empty() {
            score += 0.5;
            if node.metrics.contains_key(metric_key) {
                score += 0.5;
            }
        }
        if node.stderr.contains("Error") {
            score -= 0.2;
        }
        score.max(0.0)
    }

    // ── Phase 5: Multi-Agent Review Dialog ────────────────────────────────

    async fn phase4_review(
        &mut self,
        files: HashMap<String, String>,
        topic: &str,
        exp_plan: &str,
        metric: &str,
    ) -> Result<(HashMap<String, String>, u32)> {
        self.log_event("Phase 4: Review dialog");
        let mut files = files;
        let mut rounds = 0u32;

        for r in 0..self.config.review_max_rounds {
            rounds += 1;
            let files_ctx = Self::format_files(&files);

            let system = "You are a rigorous scientific code reviewer. \
                Output a JSON review with fields: verdict (APPROVE|REJECT), score (1-10), \
                critical_issues (list of strings).";
            let user = format!(
                "Review this experiment code for correctness, scientific validity, and completeness.\n\n\
                 ## Topic\n{topic}\n\n\
                 ## Experiment Plan\n{exp_plan}\n\n\
                 ## Metric\n{metric}\n\n\
                 ## Code\n{files_ctx}\n\n\
                 Output ONLY valid JSON: {{\"verdict\": ..., \"score\": ..., \"critical_issues\": [...]}}"
            );

            let resp = self.chat(system, &user, 8192).await?;
            let review = Self::parse_json(&resp.content);

            let review = match review {
                Some(rv) => rv,
                None => {
                    self.log_event(&format!(
                        "  Review round {}: could not parse JSON, skipping",
                        r + 1
                    ));
                    break;
                }
            };

            let verdict = review
                .get("verdict")
                .and_then(Value::as_str)
                .unwrap_or("APPROVE")
                .to_owned();
            let score = review.get("score").and_then(Value::as_f64).unwrap_or(10.0);
            let critical: Vec<String> = review
                .get("critical_issues")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();

            self.log_event(&format!(
                "  Review round {}: verdict={verdict}, score={score:.1}, critical={}",
                r + 1,
                critical.len()
            ));

            if verdict == "APPROVE" || critical.is_empty() {
                break;
            }

            // Fix critical issues
            let fix_issues = critical.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n");
            let fix_system = "You are MolAgent. Fix the critical issues while preserving experiment design.";
            let fix_user = format!(
                "A code reviewer found critical issues. Fix ALL of them.\n\n\
                 ## Critical Issues\n{fix_issues}\n\n\
                 ## Current Code\n{files_ctx}\n\n\
                 Output ALL files in ```filename:xxx.py``` format, including unchanged files."
            );

            let fix_resp = self.chat(fix_system, &fix_user, 8192).await?;
            let fixed = Self::extract_files(&fix_resp.content);
            if !fixed.is_empty() {
                files.extend(fixed);
            }
        }

        Ok((files, rounds))
    }

    // ── LLM / Sandbox helpers ──────────────────────────────────────────────

    async fn chat(&mut self, system: &str, user: &str, max_tokens: u32) -> Result<LlmResponse> {
        self.llm_calls += 1;
        let messages = vec![ChatMessage {
            role: "user".to_owned(),
            content: user.to_owned(),
        }];
        self.llm.chat(&messages, system, max_tokens).await
    }

    async fn run_in_sandbox(
        &mut self,
        files: &HashMap<String, String>,
        timeout_sec: Option<u64>,
    ) -> Result<SandboxResult> {
        let sandbox = self
            .sandbox
            .as_ref()
            .context("No sandbox configured")?;

        self.sandbox_runs += 1;
        let timeout = timeout_sec.unwrap_or(self.config.exec_fix_timeout_sec);

        // Write files to a numbered attempt directory
        let run_dir = self
            .stage_dir
            .join("agent_runs")
            .join(format!("attempt_{:03}", self.sandbox_runs));
        std::fs::create_dir_all(&run_dir)?;

        for (fname, code) in files {
            let fpath = run_dir.join(fname);
            if let Some(parent) = fpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&fpath, code)?;
        }

        match sandbox.run_project(&run_dir, "main.py", timeout).await {
            Ok(result) => Ok(result),
            Err(e) => {
                self.log_event(&format!("  Sandbox run failed: {e}"));
                Ok(SandboxResult {
                    returncode: 1,
                    stderr: format!("Sandbox exception: {e}"),
                    ..Default::default()
                })
            }
        }
    }

    // ── Static helpers (public for testing) ───────────────────────────────

    /// Extract multi-file code blocks from LLM output.
    ///
    /// Looks for fenced blocks of the form:
    /// ` ```filename:foo.py ` ... ` ``` `
    /// as well as plain ` ```python ` blocks (assigned to `main.py`).
    pub fn extract_files(content: &str) -> HashMap<String, String> {
        let mut files = HashMap::new();

        // Named blocks: ```filename:xxx.py
        let named_re =
            Regex::new(r"(?s)```filename:(\S+)\s*\n(.*?)```").unwrap();
        for cap in named_re.captures_iter(content) {
            let name = cap[1].trim().to_owned();
            let code = cap[2].trim().to_owned();
            if !name.is_empty() && !code.is_empty() {
                files.insert(name, code);
            }
        }

        // Plain python block → main.py if no main.py yet
        if !files.contains_key("main.py") {
            let py_re = Regex::new(r"(?s)```python\s*\n(.*?)```").unwrap();
            if let Some(cap) = py_re.captures(content) {
                let code = cap[1].trim().to_owned();
                if !code.is_empty() {
                    files.insert("main.py".to_owned(), code);
                }
            }
        }

        files
    }

    /// Format a files map for inclusion in a prompt.
    pub fn format_files(files: &HashMap<String, String>) -> String {
        let mut keys: Vec<&str> = files.keys().map(String::as_str).collect();
        keys.sort();
        keys.iter()
            .map(|fname| format!("```filename:{fname}\n{}\n```", files[*fname]))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Best-effort JSON extraction from LLM text.
    ///
    /// Mirrors `_parse_json` in the Python original (BUG-17 fix: always returns
    /// `Option<Map>`, never a bare string or array).
    pub fn parse_json(text: &str) -> Option<serde_json::Map<String, Value>> {
        // Direct parse
        if let Ok(Value::Object(m)) = serde_json::from_str(text) {
            return Some(m);
        }
        // ```json ... ``` block
        let fence_re = Regex::new(r"(?s)```json\s*\n(.*?)```").unwrap();
        if let Some(cap) = fence_re.captures(text) {
            if let Ok(Value::Object(m)) = serde_json::from_str(&cap[1]) {
                return Some(m);
            }
        }
        // First `{...}` object (up to 2 levels of nesting)
        let brace_re = Regex::new(
            r"\{[^{}]*(?:\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}[^{}]*)*\}",
        )
        .unwrap();
        if let Some(cap) = brace_re.find(text) {
            if let Ok(Value::Object(m)) = serde_json::from_str(cap.as_str()) {
                return Some(m);
            }
        }
        None
    }

    fn log_event(&mut self, msg: &str) {
        info!("[CodeAgent] {}", msg);
        self.log.push(msg.to_owned());
    }
}

// ---------------------------------------------------------------------------
// CodeSummary types (CodeMem replacement)
// ---------------------------------------------------------------------------

/// Compact code summary produced by [`CodeAgent::build_code_summary`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeSummary {
    pub filename: String,
    pub classes: Vec<ClassSummary>,
    pub functions: Vec<FunctionSummary>,
    pub imports: Vec<String>,
    #[serde(default)]
    pub parse_error: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassSummary {
    pub name: String,
    pub bases: Vec<String>,
    pub methods: Vec<MethodSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionSummary {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MethodSummary {
    pub name: String,
    pub args: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Blueprint parsing ─────────────────────────────────────────────────

    #[test]
    fn parse_blueprint_valid_yaml() {
        let yaml = "files:\n  - path: main.py\n    purpose: entry point\ngeneration_order:\n  - main.py";
        let bp = CodeAgent::parse_blueprint(yaml).unwrap();
        assert_eq!(bp.files.len(), 1);
        assert_eq!(bp.files[0].path, "main.py");
    }

    #[test]
    fn parse_blueprint_with_name_field() {
        let yaml = r#"
files:
  - name: trainer.py
    purpose: training loop
    generation_order: 1
    dependencies: []
  - name: main.py
    purpose: entry point
    generation_order: 2
    dependencies:
      - trainer.py
generation_order:
  - trainer.py
  - main.py
"#;
        let bp = CodeAgent::parse_blueprint(yaml).unwrap();
        assert_eq!(bp.files.len(), 2);
        assert_eq!(bp.files[0].effective_name(), "trainer.py");
        assert_eq!(bp.files[1].effective_name(), "main.py");
    }

    #[test]
    fn parse_blueprint_invalid_yaml_returns_none() {
        assert!(CodeAgent::parse_blueprint("this: is: invalid: yaml: {{{{").is_none());
    }

    #[test]
    fn parse_blueprint_missing_files_key() {
        // YAML without `files` can still deserialize; files will be empty vec
        let yaml = "generation_order:\n  - main.py";
        // serde_yaml may succeed with empty files — just verify it doesn't panic
        let _ = CodeAgent::parse_blueprint(yaml);
    }

    #[test]
    fn is_valid_blueprint_true() {
        // Must have >= 2 files with generation_order set — matches Python spec.
        let yaml = r#"
files:
  - name: trainer.py
    purpose: training loop
    generation_order: 1
  - name: main.py
    purpose: entry point
    generation_order: 2
"#;
        let bp = CodeAgent::parse_blueprint(yaml).unwrap();
        assert!(CodeAgent::is_valid_blueprint(&bp));
    }

    #[test]
    fn is_valid_blueprint_only_one_ordered_file() {
        // Only 1 file has generation_order — should fail spec check.
        let yaml = "files:\n  - name: main.py\n    purpose: entry point\n    generation_order: 1";
        let bp = CodeAgent::parse_blueprint(yaml).unwrap();
        assert!(!CodeAgent::is_valid_blueprint(&bp));
    }

    #[test]
    fn is_valid_blueprint_no_generation_order() {
        // Files present but none have generation_order — should fail spec check.
        let yaml = "files:\n  - name: main.py\n    purpose: entry point";
        let bp = CodeAgent::parse_blueprint(yaml).unwrap();
        assert!(!CodeAgent::is_valid_blueprint(&bp));
    }

    #[test]
    fn is_valid_blueprint_empty_files() {
        let bp = Blueprint { files: vec![], generation_order: vec![] };
        assert!(!CodeAgent::is_valid_blueprint(&bp));
    }

    // ── File extraction ───────────────────────────────────────────────────

    #[test]
    fn extract_files_named_block() {
        let content = "Here is the code:\n\
            ```filename:main.py\n\
            print('hello')\n\
            ```\n\
            And also:\n\
            ```filename:utils.py\n\
            def helper(): pass\n\
            ```";
        let files = CodeAgent::extract_files(content);
        assert_eq!(files.len(), 2);
        assert!(files.contains_key("main.py"));
        assert!(files.contains_key("utils.py"));
    }

    #[test]
    fn extract_files_python_block_to_main() {
        let content = "```python\nprint('hi')\n```";
        let files = CodeAgent::extract_files(content);
        assert!(files.contains_key("main.py"));
        assert_eq!(files["main.py"], "print('hi')");
    }

    #[test]
    fn extract_files_empty_content() {
        assert!(CodeAgent::extract_files("no code blocks here").is_empty());
    }

    // ── Single-file code extraction ───────────────────────────────────────

    #[test]
    fn extract_single_file_python_fence() {
        let content = "Here:\n```python\nclass Foo: pass\n```";
        let code = CodeAgent::extract_single_file_code(content, "foo.py");
        assert_eq!(code, "class Foo: pass");
    }

    #[test]
    fn extract_single_file_named_fence() {
        let content = "```filename:model.py\nclass Model: pass\n```";
        let code = CodeAgent::extract_single_file_code(content, "model.py");
        assert_eq!(code, "class Model: pass");
    }

    #[test]
    fn extract_single_file_raw_python() {
        let content = "import torch\n\nclass Net: pass";
        let code = CodeAgent::extract_single_file_code(content, "net.py");
        assert_eq!(code, content.trim());
    }

    #[test]
    fn extract_single_file_empty_when_no_code() {
        let code = CodeAgent::extract_single_file_code("Just some prose.", "main.py");
        assert!(code.is_empty());
    }

    // ── Code summary ──────────────────────────────────────────────────────

    #[test]
    fn build_code_summary_basic() {
        let code = r#"
import torch
import torch.nn as nn

class MyModel(nn.Module):
    def __init__(self, hidden):
        super().__init__()
        self.fc = nn.Linear(hidden, 1)

    def forward(self, x):
        return self.fc(x)

def train(model, data):
    pass
"#;
        let summary = CodeAgent::build_code_summary("model.py", code);
        assert_eq!(summary.filename, "model.py");
        assert!(!summary.classes.is_empty(), "should find MyModel");
        assert!(!summary.functions.is_empty(), "should find train");
        assert!(!summary.imports.is_empty(), "should find imports");
    }

    #[test]
    fn build_code_summary_empty_file() {
        let summary = CodeAgent::build_code_summary("empty.py", "");
        assert!(summary.classes.is_empty());
        assert!(summary.functions.is_empty());
    }

    // ── Hard validation ───────────────────────────────────────────────────

    #[test]
    fn hard_validate_hardcoded_metric() {
        let mut files = HashMap::new();
        files.insert(
            "main.py".to_owned(),
            "accuracy = 0.95  # hardcoded\nprint(accuracy)".to_owned(),
        );
        let (critical, _) = CodeAgent::hard_validate(&files);
        assert!(
            critical.iter().any(|c| c.contains("Hardcoded")),
            "expected hardcoded metric critical, got: {critical:?}"
        );
    }

    #[test]
    fn hard_validate_cross_file_import_ok() {
        let mut files = HashMap::new();
        files.insert("utils.py".to_owned(), "def helper(): pass\n".to_owned());
        files.insert(
            "main.py".to_owned(),
            "from utils import helper\nhelper()\n".to_owned(),
        );
        let (critical, _) = CodeAgent::hard_validate(&files);
        assert!(
            !critical.iter().any(|c| c.contains("ImportError")),
            "should not flag valid cross-file import, got: {critical:?}"
        );
    }

    #[test]
    fn hard_validate_cross_file_import_missing() {
        let mut files = HashMap::new();
        files.insert("utils.py".to_owned(), "def helper(): pass\n".to_owned());
        files.insert(
            "main.py".to_owned(),
            "from utils import nonexistent\n".to_owned(),
        );
        let (critical, _) = CodeAgent::hard_validate(&files);
        assert!(
            critical.iter().any(|c| c.contains("ImportError") && c.contains("nonexistent")),
            "expected ImportError for nonexistent, got: {critical:?}"
        );
    }

    // ── Category 4: NameError heuristic ──────────────────────────────────

    #[test]
    fn hard_validate_name_error_literal_in_code() {
        let mut files = HashMap::new();
        files.insert(
            "main.py".to_owned(),
            "# name 'compute' is not defined\nresult = compute()".to_owned(),
        );
        let (critical, _) = CodeAgent::hard_validate(&files);
        assert!(
            critical.iter().any(|c| c.contains("NameError") && c.contains("compute")),
            "expected NameError heuristic for embedded traceback text, got: {critical:?}"
        );
    }

    #[test]
    fn hard_validate_non_python_identifier_null() {
        let mut files = HashMap::new();
        files.insert(
            "main.py".to_owned(),
            "value = NULL\nprint(value)".to_owned(),
        );
        let (_, warnings) = CodeAgent::hard_validate(&files);
        assert!(
            warnings.iter().any(|w| w.contains("NULL")),
            "expected NameError warning for NULL identifier, got: {warnings:?}"
        );
    }

    // ── Category 5: UnboundLocalError heuristic ───────────────────────────

    #[test]
    fn hard_validate_unbound_local_augmented_assign() {
        let mut files = HashMap::new();
        files.insert(
            "train.py".to_owned(),
            "def train():\n    total += 1\n    print(total)\n".to_owned(),
        );
        let (_, warnings) = CodeAgent::hard_validate(&files);
        assert!(
            warnings.iter().any(|w| w.contains("UnboundLocalError") && w.contains("total")),
            "expected UnboundLocalError warning for augmented assign before init, got: {warnings:?}"
        );
    }

    #[test]
    fn hard_validate_no_unbound_local_when_assigned_first() {
        let mut files = HashMap::new();
        files.insert(
            "train.py".to_owned(),
            "def train():\n    total = 0\n    total += 1\n    print(total)\n".to_owned(),
        );
        let (_, warnings) = CodeAgent::hard_validate(&files);
        assert!(
            !warnings.iter().any(|w| w.contains("UnboundLocalError") && w.contains("total")),
            "should not flag augmented assign when variable is assigned first, got: {warnings:?}"
        );
    }

    // ── Code summary scoping ──────────────────────────────────────────────

    #[test]
    fn build_code_summary_methods_scoped_to_class() {
        let code = r#"
class ModelA(nn.Module):
    def __init__(self):
        pass

    def forward(self, x):
        return x

class ModelB(nn.Module):
    def __init__(self):
        pass

    def predict(self, x):
        return x
"#;
        let summary = CodeAgent::build_code_summary("models.py", code);
        assert_eq!(summary.classes.len(), 2);
        let class_a = summary.classes.iter().find(|c| c.name == "ModelA").unwrap();
        let class_b = summary.classes.iter().find(|c| c.name == "ModelB").unwrap();
        // ModelA should have forward but NOT predict
        assert!(class_a.methods.iter().any(|m| m.name == "forward"),
            "ModelA should have forward method");
        assert!(!class_a.methods.iter().any(|m| m.name == "predict"),
            "ModelA should NOT have predict (belongs to ModelB)");
        // ModelB should have predict but NOT forward
        assert!(class_b.methods.iter().any(|m| m.name == "predict"),
            "ModelB should have predict method");
        assert!(!class_b.methods.iter().any(|m| m.name == "forward"),
            "ModelB should NOT have forward (belongs to ModelA)");
    }

    // ── Syntax heuristic ──────────────────────────────────────────────────

    #[test]
    fn passes_syntax_heuristic_ok() {
        let code = r#"
"""Module docstring."""
import sys

def main():
    print("hello")
"#;
        assert!(CodeAgent::passes_syntax_heuristic(code));
    }

    #[test]
    fn passes_syntax_heuristic_unbalanced_triple_quote() {
        let code = "x = \"\"\"open string without close";
        assert!(!CodeAgent::passes_syntax_heuristic(code));
    }

    // ── Error location parsing ────────────────────────────────────────────

    #[test]
    fn parse_error_location_found() {
        let mut files = HashMap::new();
        files.insert("main.py".to_owned(), String::new());

        let stderr = r#"Traceback (most recent call last):
  File "main.py", line 42, in <module>
    result = compute()
NameError: name 'compute' is not defined"#;

        let loc = CodeAgent::parse_error_location(stderr, &files);
        assert!(loc.is_some());
        let (fname, lineno, msg) = loc.unwrap();
        assert_eq!(fname, "main.py");
        assert_eq!(lineno, 42);
        assert!(msg.contains("NameError"));
    }

    #[test]
    fn parse_error_location_unknown_file() {
        let files: HashMap<String, String> = HashMap::new();
        let stderr = "File \"unknown.py\", line 1\nError: oops";
        assert!(CodeAgent::parse_error_location(stderr, &files).is_none());
    }

    // ── JSON parsing ──────────────────────────────────────────────────────

    #[test]
    fn parse_json_direct() {
        let text = r#"{"verdict": "APPROVE", "score": 9}"#;
        let m = CodeAgent::parse_json(text).unwrap();
        assert_eq!(m["verdict"].as_str().unwrap(), "APPROVE");
    }

    #[test]
    fn parse_json_fenced() {
        let text = "Some prose.\n```json\n{\"verdict\": \"REJECT\", \"score\": 3}\n```\nMore prose.";
        let m = CodeAgent::parse_json(text).unwrap();
        assert_eq!(m["verdict"].as_str().unwrap(), "REJECT");
    }

    #[test]
    fn parse_json_embedded() {
        let text = "The review says: {\"verdict\": \"APPROVE\", \"score\": 10} — done.";
        let m = CodeAgent::parse_json(text).unwrap();
        assert_eq!(m["score"].as_f64().unwrap(), 10.0);
    }

    #[test]
    fn parse_json_none_when_no_object() {
        assert!(CodeAgent::parse_json("no json here").is_none());
    }

    // ── Score node ────────────────────────────────────────────────────────

    #[test]
    fn score_node_runs_ok_with_metric() {
        let mut metrics = HashMap::new();
        metrics.insert("accuracy".to_owned(), Value::from(0.95));
        let node = SolutionNode {
            runs_ok: true,
            stdout: "x".repeat(200),
            metrics,
            ..Default::default()
        };
        let score = CodeAgent::score_node(&node, "accuracy");
        // 1.0 (runs_ok) + 0.3 (stdout > 100) + 0.5 (metrics non-empty) + 0.5 (metric key found)
        assert!((score - 2.3).abs() < 1e-9, "expected 2.3, got {score}");
    }

    #[test]
    fn score_node_crashed() {
        let node = SolutionNode {
            runs_ok: false,
            stderr: "RuntimeError: crashed".to_owned(),
            ..Default::default()
        };
        let score = CodeAgent::score_node(&node, "loss");
        assert_eq!(score, 0.0, "score should be 0 after clamping");
    }

    // ── Format files ─────────────────────────────────────────────────────

    #[test]
    fn format_files_sorted_output() {
        let mut files = HashMap::new();
        files.insert("utils.py".to_owned(), "def foo(): pass".to_owned());
        files.insert("main.py".to_owned(), "import utils".to_owned());
        let out = CodeAgent::format_files(&files);
        // main.py should come before utils.py (alphabetical order)
        let main_pos = out.find("main.py").unwrap();
        let utils_pos = out.find("utils.py").unwrap();
        assert!(main_pos < utils_pos);
    }
}
