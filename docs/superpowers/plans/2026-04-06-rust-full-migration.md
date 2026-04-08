# Mol-HEP-Lab: 100% Rust Migration & Brand Unification Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Achieve 100% functional parity between the Rust codebase and the Python `researchclaw` package, apply Mol-HEP-Lab branding throughout, then remove all Python code.

**Architecture:** The Rust workspace (`crates/mol-*`) already mirrors Python's module structure. Most infrastructure is implemented (turn loop, LLM clients, sandboxes, agents framework, state machine). The remaining work is: (1) filling pipeline executor stage implementations, (2) porting 4 agent runtimes + code agent + external bridges, (3) porting utility modules, (4) branding cleanup, (5) Python deletion.

**Tech Stack:** Rust 2024 edition, tokio async runtime, axum (web), reqwest (HTTP), serde (serialization), tera (templates), plotters (visualization replacement for matplotlib), git2 (git operations), tree-sitter (AST parsing replacement for Python ast module)

---

## Revised Audit Summary

After deep reading of all Rust source files, the actual completion status is higher than initially estimated:

| Crate | Actual Status | Notes |
|-------|--------------|-------|
| mol-engine (turn_loop, tools, codegen) | **95% complete** | All 6 tools implemented, turn loop works, codegen strategies done |
| mol-agents (benchmark, code_searcher, figure) | **85% complete** | Orchestrators implemented; code_searcher submodules need filling |
| mol-experiment (docker, ssh, colab, validation, metrics) | **90% complete** | All sandboxes implemented with GPU/NPU support |
| mol-pipeline (stages, runner, checkpoint, contracts) | **Framework 100%, stages 0%** | State machine + runner complete, all 26 stage executions are stubs |
| mol-services (agent_bridge) | **80% complete** | Large implementation exists, needs message handler completion |
| mol-config, mol-llm, mol-literature, mol-templates, mol-domains, mol-metamol, mol-evolution, mol-health, mol-common | **90-100%** | Production-ready |

**True remaining work: ~12,000 lines of Rust** (not 35-40K as initially estimated)

---

## Phase 1: Branding Unification (687 references)

### Task 1.1: Rust Comment & Doc Branding

**Files:**
- Modify: All `crates/mol-*/src/*.rs` files containing "ResearchClaw", "Claw AI", "MetaClaw", "OpenClaw" in comments
- Modify: `Cargo.toml` descriptions if any reference old names
- Modify: `README.md`

- [ ] **Step 1: Find and list all Rust branding references**

```bash
rg -n "(?i)(researchclaw|claw.ai|metaclaw|openclaw)" crates/ --type rust
```

- [ ] **Step 2: Apply replacements across Rust crates**

Mapping:
- "ResearchClaw" / "researchclaw" -> "Mol-HEP-Lab" / "mol"
- "Claw AI Lab" -> "Mol-HEP-Lab"
- "MetaClaw" / "metaclaw" -> "MetaMol" / "metamol" (already done in type names, fix comments)
- "OpenClaw" / "openclaw" -> "OpenMol" / "openmol"
- "ClawEngine" -> "MolEngine"

- [ ] **Step 3: Update README.md branding**

Replace all "Claw AI Lab" references with "Mol-HEP-Lab" in README.md. Update project description, titles, and showcase references.

- [ ] **Step 4: Update .gitignore cache paths**

`.researchclaw_cache` -> `.molheplab_cache`

- [ ] **Step 5: Update frontend/package.json if needed**

- [ ] **Step 6: Run `cargo test` to verify no breakage**

```bash
cargo test --workspace
```
Expected: 182 tests pass

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "brand: unify all references to Mol-HEP-Lab"
```

### Task 1.2: Data & Config Branding (Python code will be deleted, skip renaming Python package)

**Files:**
- Modify: `backend/agent/researchclaw/data/docker_profiles.yaml`
- Modify: showcase markdown files in `assets/showcase/`

- [ ] **Step 1: Update Docker image references**

`researchclaw/sandbox-*` -> `molheplab/sandbox-*` in `data/docker_profiles.yaml`

- [ ] **Step 2: Update showcase files** — Replace "Claw AI Lab" with "Mol-HEP-Lab" in all `assets/showcase/*.md`

- [ ] **Step 3: Commit**

```bash
git commit -m "brand: update Docker images and showcase references to Mol-HEP-Lab"
```

> **Note:** Python package directory rename is skipped — the entire `backend/` is deleted in Phase 7.

---

## Phase 2: Missing Utility Modules (~1,500 lines Rust)

### Task 2.1: Adapter Protocols (mol-common)

**Files:**
- Create: `crates/mol-common/src/adapters.rs`
- Modify: `crates/mol-common/src/lib.rs`
- Create: `crates/mol-common/tests/test_adapters.rs`

Port from: `backend/agent/researchclaw/adapters.py` (107 lines)

- [ ] **Step 1: Write failing test**

```rust
// tests/test_adapters.rs
#[test]
fn recording_adapters_capture_calls() {
    let bundle = AdapterBundle::default();
    // All adapters should be recording stubs
    let result = bundle.message.notify("ch", "subj", "body");
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p mol-common test_adapters
```

- [ ] **Step 3: Implement adapter traits and recording stubs**

```rust
// crates/mol-common/src/adapters.rs
use async_trait::async_trait;

#[async_trait]
pub trait CronAdapter: Send + Sync {
    async fn schedule_resume(&self, run_id: &str, stage_id: i32, reason: &str) -> anyhow::Result<String>;
}

#[async_trait]
pub trait MessageAdapter: Send + Sync {
    async fn notify(&self, channel: &str, subject: &str, body: &str) -> anyhow::Result<String>;
}

#[async_trait]
pub trait MemoryAdapter: Send + Sync {
    async fn append(&self, namespace: &str, content: &str) -> anyhow::Result<String>;
}

#[async_trait]
pub trait SessionsAdapter: Send + Sync {
    async fn spawn(&self, name: &str, command: &str) -> anyhow::Result<String>;
}

#[async_trait]
pub trait WebFetchAdapter: Send + Sync {
    async fn fetch(&self, url: &str) -> anyhow::Result<FetchResponse>;
}

#[async_trait]
pub trait BrowserAdapter: Send + Sync {
    async fn open(&self, url: &str) -> anyhow::Result<BrowserPage>;
}

#[derive(Debug, Clone)]
pub struct FetchResponse { pub url: String, pub status_code: u16, pub text: String }

#[derive(Debug, Clone)]
pub struct BrowserPage { pub url: String, pub title: String }

// Recording stubs (capture calls for testing)
#[derive(Default)]
pub struct RecordingMessageAdapter { pub calls: std::sync::Mutex<Vec<(String, String, String)>> }
// ... implement all Recording* variants ...

pub struct AdapterBundle {
    pub cron: Box<dyn CronAdapter>,
    pub message: Box<dyn MessageAdapter>,
    pub memory: Box<dyn MemoryAdapter>,
    pub sessions: Box<dyn SessionsAdapter>,
    pub web_fetch: Box<dyn WebFetchAdapter>,
    pub browser: Box<dyn BrowserAdapter>,
}
impl Default for AdapterBundle { /* recording stubs */ }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p mol-common test_adapters
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(mol-common): add adapter protocol traits and recording stubs"
```

### Task 2.2: Writing Guide (mol-common)

**Files:**
- Create: `crates/mol-common/src/writing_guide.rs`
- Modify: `crates/mol-common/src/lib.rs`

Port from: `backend/agent/researchclaw/writing_guide.py` (78 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn format_writing_tips_returns_all_categories() {
    let tips = format_writing_tips(None);
    assert!(tips.contains("title"));
    assert!(tips.contains("abstract"));
    assert!(tips.contains("experiments"));
}

#[test]
fn format_writing_tips_filters_categories() {
    let tips = format_writing_tips(Some(&["title", "abstract"]));
    assert!(tips.contains("title"));
    assert!(!tips.contains("common_rejections"));
}
```

- [ ] **Step 2: Implement writing guide as static data**

Transcribe the CONFERENCE_WRITING_TIPS dictionary and `format_writing_tips()` function to Rust. Use `phf` or a simple match for category lookup.

- [ ] **Step 3: Run tests, commit**

### Task 2.3: Experiment Harness Template (mol-experiment)

**Files:**
- Create: `crates/mol-experiment/src/harness.rs`
- Modify: `crates/mol-experiment/src/lib.rs`

Port from: `backend/agent/researchclaw/experiment/harness_template.py` (119 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn harness_time_budget_progress() {
    let h = ExperimentHarness::new(120);
    assert!(h.progress() < 0.01);
    assert!(!h.should_stop());
}

#[test]
fn harness_rejects_nan() {
    let mut h = ExperimentHarness::new(120);
    assert!(!h.check_value(f64::NAN, "x"));
}
```

- [ ] **Step 2: Implement ExperimentHarness struct**

- Time tracking (Instant-based), progress(), should_stop() at 80%
- check_value() rejecting NaN/Inf, counting bad values, abort at 5
- report_metric(), log_result(), finalize() writing results.json
- step() incrementing counter

- [ ] **Step 3: Run tests, commit**

### Task 2.4: Git Experiment Manager (mol-experiment)

**Files:**
- Create: `crates/mol-experiment/src/git_manager.rs`
- Modify: `crates/mol-experiment/src/lib.rs`
- Modify: `crates/mol-experiment/Cargo.toml` (add `git2` dependency)

Port from: `backend/agent/researchclaw/experiment/git_manager.py` (166 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn git_manager_creates_experiment_branch() {
    let tmp = tempdir().unwrap();
    // init git repo in tmp
    let mgr = ExperimentGitManager::new(tmp.path());
    assert!(mgr.is_git_repo());
    let branch = mgr.create_experiment_branch("test-run").unwrap();
    assert!(branch.starts_with("experiment/"));
}
```

- [ ] **Step 2: Implement using git2 crate**

- create_experiment_branch(), commit_experiment(), discard_experiment()
- get_experiment_history(), get_experiment_diff()
- return_to_original_branch(), clean_untracked()

- [ ] **Step 3: Run tests, commit**

### Task 2.5: Convergence Evaluator (mol-experiment)

**Files:**
- Create: `crates/mol-experiment/src/evaluators/mod.rs`
- Create: `crates/mol-experiment/src/evaluators/convergence.rs`
- Modify: `crates/mol-experiment/src/lib.rs`

Port from: `backend/agent/researchclaw/experiment/evaluators/convergence.py` (~200 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn convergence_order_from_known_data() {
    // h = [1.0, 0.5, 0.25], errors = [1.0, 0.25, 0.0625] => order ~2.0
    let (order, r2) = compute_convergence_order(&[1.0, 0.5, 0.25], &[1.0, 0.25, 0.0625]);
    assert!((order - 2.0).abs() < 0.1);
    assert!(r2 > 0.99);
}
```

- [ ] **Step 2: Implement log-log linear regression**

- compute_convergence_order() using manual log-log regression (no numpy needed)
- analyze_convergence() producing ConvergenceReport
- ConvergenceResult, ConvergenceReport structs

- [ ] **Step 3: Run tests, commit**

### Task 2.6: Visualization Engine (mol-experiment)

**Files:**
- Create: `crates/mol-experiment/src/visualize.rs`
- Modify: `crates/mol-experiment/Cargo.toml` (add `plotters` dependency)

Port from: `backend/agent/researchclaw/experiment/visualize.py` (~711 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn plot_condition_comparison_creates_png() {
    let tmp = tempdir().unwrap();
    let out = tmp.path().join("chart.png");
    let data = vec![ConditionSummary { name: "baseline".into(), metrics: /* ... */ }];
    let result = plot_condition_comparison(&data, &out, "accuracy", "");
    assert!(result.is_some());
    assert!(out.exists());
}
```

- [ ] **Step 2: Implement using plotters crate**

- Paul Tol colorblind-safe palette (7 bright + 15 extended colors)
- _is_excluded_metric(), _shorten_label(), _format_cond_name()
- plot_condition_comparison() with grouped bars, error bars, value labels
- Academic styling (serif font, 300 DPI, no top/right spines)

- [ ] **Step 3: Run tests, commit**

### Task 2.7: Codebase Manifest (mol-common)

**Files:**
- Create: `crates/mol-common/src/codebase_manifest.rs`
- Modify: `crates/mol-common/Cargo.toml` (add `tree-sitter`, `tree-sitter-python` dependencies)

Port from: `backend/agent/researchclaw/utils/codebase_manifest.py` (312 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn manifest_to_prompt_includes_repo_name() {
    let manifest = generate_manifest(Path::new("fixtures/sample_repo"));
    let prompt = manifest_to_prompt(&manifest);
    assert!(prompt.contains("sample_repo"));
}
```

- [ ] **Step 2: Implement using tree-sitter for Python AST parsing**

- generate_manifest() scanning .py files, extracting classes/functions via tree-sitter
- manifest_to_prompt() formatting for LLM injection
- Caching via _manifest.json with mtime hash
- _trim_readme() stripping HTML/badges

- [ ] **Step 3: Run tests, commit**

### Task 2.8: Experiment Schema (mol-domains)

**Files:**
- Create: `crates/mol-domains/src/experiment_schema.rs`
- Modify: `crates/mol-domains/src/lib.rs`

Port from: `backend/agent/researchclaw/domains/experiment_schema.py` (252 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn experiment_type_from_str() {
    let et: ExperimentType = "ablation".parse().unwrap();
    assert_eq!(et, ExperimentType::Ablation);
}

#[test]
fn condition_role_default_is_experimental() {
    let c = Condition::new("test");
    assert_eq!(c.role, ConditionRole::Experimental);
}
```

- [ ] **Step 2: Implement types**

- `ConditionRole` enum (Baseline, Experimental, Ablation, Oracle)
- `ExperimentType` enum (Comparison, Ablation, Scaling, Reproduction, Exploration)
- `Condition` struct (name, role, parameters, description)
- `ExperimentSchema` struct (experiment_type, conditions, primary_metric, secondary_metrics, success_criteria)
- Schema validation and serialization

- [ ] **Step 3: Run tests, commit**

### Task 2.9: Domain Prompt Adapter (mol-domains)

**Files:**
- Create: `crates/mol-domains/src/prompt_adapter.rs`
- Modify: `crates/mol-domains/src/lib.rs`

Port from: `backend/agent/researchclaw/domains/prompt_adapter.py` (325 lines)

- [ ] **Step 1: Write test**

```rust
#[test]
fn ml_domain_includes_pytorch_hints() {
    let blocks = get_prompt_blocks(ResearchDomain::MachineLearning, PromptContext::CodeGen);
    assert!(blocks.domain_hints.contains("torch"));
}
```

- [ ] **Step 2: Implement PromptBlocks and per-domain adapters**

- `PromptBlocks` struct (domain_hints, code_gen_tips, evaluation_guidance, common_pitfalls)
- `PromptContext` enum (CodeGen, Evaluation, Writing, Experiment)
- `get_prompt_blocks(domain, context)` returning domain-specific prompt blocks
- Per-domain hint content (ML: PyTorch/JAX, Physics: numerical methods, Chemistry: molecular sims, etc.)

- [ ] **Step 3: Run tests, commit**

### Task 2.10: Web Search Agent (mol-web)

**Files:**
- Create: `crates/mol-web/src/agent.rs`
- Modify: `crates/mol-web/src/lib.rs`

Port from: `backend/agent/researchclaw/web/agent.py` (348 lines)

- [ ] **Step 1: Write test**

```rust
#[tokio::test]
async fn web_agent_search_returns_results() {
    let agent = WebSearchAgent::new(WebSearchConfig::default());
    // Unit test with mock HTTP client
    let results = agent.search("test query").await;
    assert!(results.is_ok());
}
```

- [ ] **Step 2: Implement WebSearchAgent**

- `WebSearchAgent` struct orchestrating Tavily + DuckDuckGo + Scholar + Crawler + PDF extraction
- `search()` — unified search across all sources with deduplication
- `crawl_and_extract()` — fetch URL, extract text, handle PDF vs HTML
- `search_scholar()` — Google Scholar integration
- Error handling with graceful degradation per source

- [ ] **Step 3: Run tests, commit**

---

## Phase 3: Pipeline Agent Runtimes (~2,000 lines Rust)

These 4 modules share a common pattern: prepare workspace -> build prompts -> run AgentTurnLoop -> collect results. The Rust mol-engine already provides AgentTurnLoop, StageSession, and all tools.

### Task 3.1: Sanity Check Runtime (mol-pipeline)

**Files:**
- Create: `crates/mol-pipeline/src/runtimes/mod.rs`
- Create: `crates/mol-pipeline/src/runtimes/sanity_check.rs`
- Modify: `crates/mol-pipeline/src/lib.rs`

Port from: `backend/agent/researchclaw/pipeline/sanity_check/runtime.py` + system_prompt.py

- [ ] **Step 1: Write test**

```rust
#[tokio::test]
async fn sanity_check_prepares_workspace() {
    let tmp = tempdir().unwrap();
    let stage_dir = tmp.path().join("stage-12");
    fs::create_dir_all(&stage_dir).unwrap();
    // Create mock experiment dir
    let exp_dir = tmp.path().join("stage-11/experiment");
    fs::create_dir_all(&exp_dir).unwrap();
    fs::write(exp_dir.join("main.py"), "print('hello')").unwrap();

    let workspace = SanityCheckRuntime::prepare_workspace(&stage_dir, &exp_dir, tmp.path(), &config).unwrap();
    assert!(workspace.join("main.py").exists());
}
```

- [ ] **Step 2: Implement SanityCheckRuntime**

```rust
pub struct SanityCheckRuntime;

impl SanityCheckRuntime {
    pub async fn execute(
        stage_dir: &Path, run_dir: &Path, config: &MolConfig,
        adapters: &AdapterBundle, llm_config: Option<&LlmConfig>,
    ) -> anyhow::Result<StageResult> { ... }

    fn prepare_workspace(stage_dir: &Path, experiment_dir: &Path, run_dir: &Path, config: &MolConfig) -> anyhow::Result<PathBuf> { ... }
    fn find_experiment_dir(run_dir: &Path) -> Option<PathBuf> { ... }
    fn build_system_prompt(plan_summary: &str, file_list: &[String]) -> String { ... }
    fn build_user_message() -> String { ... }
    fn check_success(turn_result: &TurnResult, workspace: &Path, max_iterations: u32) -> bool { ... }
    fn copy_fixes_back(workspace: &Path, experiment_dir: &Path) -> anyhow::Result<()> { ... }
}
```

Key logic:
- Copy experiment/ to isolated workspace with symlinks to datasets/checkpoints
- Run AgentTurnLoop with max 30 iterations, 180s bash timeout
- Success detection via pass phrases / error detection
- Copy fixed .py files back

- [ ] **Step 3: Run tests, commit**

### Task 3.2: Experiment Run Runtime (mol-pipeline)

**Files:**
- Create: `crates/mol-pipeline/src/runtimes/experiment_run.rs`

Port from: `backend/agent/researchclaw/pipeline/experiment_run/runtime.py`

- [ ] **Step 1: Write test for GPU detection**

```rust
#[test]
fn find_free_gpu_returns_valid_id() {
    // Mock nvidia-smi output or skip if no GPU
    let gpu = ExperimentRunRuntime::find_free_gpu();
    // Should return "0" or similar, or None if no GPU
}
```

- [ ] **Step 2: Implement ExperimentRunRuntime**

- find_free_gpu() via nvidia-smi subprocess
- ensure_deps() auto-installing safe packages
- prepare_workspace() with symlinks
- execute() running AgentTurnLoop with time_budget + 600s timeout, max 20 iterations
- copy_results_back()
- Output: runs/run_report.json

- [ ] **Step 3: Run tests, commit**

### Task 3.3: Iterative Refine Runtime (mol-pipeline)

**Files:**
- Create: `crates/mol-pipeline/src/runtimes/iterative_refine.rs`

Port from: `backend/agent/researchclaw/pipeline/iterative_refine/runtime.py`

- [ ] **Step 1: Write test**

```rust
#[test]
fn check_improvement_maximize() {
    assert!(IterativeRefineRuntime::check_improvement("0.5", "0.8", "accuracy", "maximize"));
    assert!(!IterativeRefineRuntime::check_improvement("0.5", "0.3", "accuracy", "maximize"));
}
```

- [ ] **Step 2: Implement IterativeRefineRuntime**

- Load baseline results from S14
- Run AgentTurnLoop with max_iterations * 8 iterations
- Metric comparison (minimize vs maximize)
- Output: refinement_log.json, experiment_final/

- [ ] **Step 3: Run tests, commit**

### Task 3.4: Result Analysis Runtime (mol-pipeline)

**Files:**
- Create: `crates/mol-pipeline/src/runtimes/result_analysis.rs`

Port from: `backend/agent/researchclaw/pipeline/result_analysis/runtime.py`

- [ ] **Step 1: Write test**

```rust
#[test]
fn list_data_files_excludes_pycache() {
    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("__pycache__")).unwrap();
    fs::write(tmp.path().join("__pycache__/foo.pyc"), "").unwrap();
    fs::write(tmp.path().join("results.json"), "{}").unwrap();
    let files = ResultAnalysisRuntime::list_data_files(tmp.path());
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("results.json"));
}
```

- [ ] **Step 2: Implement ResultAnalysisRuntime**

- Collect raw experiment output files into workspace
- Run AgentTurnLoop with 25 iterations, 300s bash timeout
- Agent writes analysis scripts, produces experiment_summary.json and analysis.md
- Copy charts to stage_dir

- [ ] **Step 3: Run tests, commit**

---

## Phase 4: Pipeline Executor Stage Implementations (~4,000 lines Rust)

This is the core: replacing all 26 `stub_stage()` calls in `executor.rs` with real implementations.

### Task 4.1: Executor Infrastructure

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs`

- [ ] **Step 1: Add helper functions**

Port from Python executor.py helpers:
- `read_prior_artifact(run_dir, filename)` — search backward through stage dirs
- `find_prior_file(run_dir, filename)` -> PathBuf
- `extract_yaml_block(text)` — strip ACP noise, extract YAML
- `safe_json_loads(text)` — multi-strategy JSON parsing (fence extraction, brace matching)
- `chat_with_prompt(llm, system, user, json_mode, max_tokens, retries)` — LLM call with retry
- `detect_domain(topic)` -> domain string
- `build_fallback_queries(topic)` -> Vec<String> (handle Chinese-English mixed)
- `load_human_feedback(run_dir, stage)` -> Option<String>
- `get_evolution_overlay(run_dir, stage_name)` -> String

- [ ] **Step 2: Write tests for helpers**

```rust
#[test]
fn safe_json_loads_extracts_from_fence() {
    let input = "Here is the result:\n```json\n{\"key\": \"value\"}\n```\nDone.";
    let val: serde_json::Value = safe_json_loads(input).unwrap();
    assert_eq!(val["key"], "value");
}

#[test]
fn read_prior_artifact_finds_latest() {
    let tmp = tempdir().unwrap();
    let s01 = tmp.path().join("stage-01");
    fs::create_dir_all(&s01).unwrap();
    fs::write(s01.join("goal.md"), "# Goal").unwrap();
    let content = read_prior_artifact(tmp.path(), "goal.md").unwrap();
    assert!(content.contains("Goal"));
}
```

- [ ] **Step 3: Run tests, commit**

### Task 4.2: Phase A Stages (TopicInit + ProblemDecompose)

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs`

- [ ] **Step 1: Implement TopicInit**

Replace `stub_stage(stage, &["topic_brief", "research_questions"])` with:
- Call LLM with SMART goal prompt (use mol-common/prompts)
- Fallback to template if no LLM
- Call `mol_common::hardware::detect_hardware()`
- Write `goal.md` + `hardware_profile.json`

- [ ] **Step 2: Implement ProblemDecompose**

Replace stub with:
- Read `goal.md` from prior stage
- Call LLM for problem decomposition
- Optional topic quality evaluation (novelty/specificity/feasibility)
- Write `problem_tree.md`, optionally `topic_evaluation.json`

- [ ] **Step 3: Write tests for both stages**
- [ ] **Step 4: Run tests, commit**

### Task 4.3: Phase B Stages (SearchStrategy + LiteratureCollect + LiteratureScreen + KnowledgeExtract)

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs`

- [ ] **Step 1: Implement SearchStrategy**

- Read problem_tree.md
- LLM generates YAML search plan + source list
- Fallback: build_fallback_queries() for Chinese-English topics
- Write: search_plan.yaml, sources.json, queries.json

- [ ] **Step 2: Implement LiteratureCollect**

- Read queries.json
- Pre-flight ping literature APIs (use mol-literature clients)
- Spawn tokio tasks for parallel search via mol-literature::search
- Fallback to LLM-generated synthetic papers if APIs unreachable
- Write: candidates.jsonl

- [ ] **Step 3: Implement LiteratureScreen (GATE)**

- Read candidates.jsonl + paper_metadata
- LLM dual scoring (relevance + quality)
- Write: screened_papers.jsonl, exclusion_reasons.json
- Return BlockedApproval if gate required

- [ ] **Step 4: Implement KnowledgeExtract**

- Read screened papers
- LLM extracts knowledge cards
- Write: knowledge_cards/ directory, citation_map.json

- [ ] **Step 5: Write tests, run, commit**

### Task 4.4: Phase C Stages (Synthesis + HypothesisGen)

- [ ] **Step 1: Implement Synthesis** — Read knowledge cards, LLM identifies clusters + gaps, write synthesis_report.md + gap_analysis.json
- [ ] **Step 2: Implement HypothesisGen** — Read synthesis, LLM generates falsifiable hypotheses, write hypotheses.json + rationale.md
- [ ] **Step 3: Tests, commit**

### Task 4.5: Phase D Stages (ExperimentDesign + CodebaseSearch + CodeGeneration + SanityCheck + ResourcePlanning)

- [ ] **Step 1: Implement ExperimentDesign (GATE)** — Design experiment plan with baselines/ablations/metrics, write exp_plan.yaml
- [ ] **Step 2: Implement CodebaseSearch** — Use mol-agents::CodeSearchAgent, write codebase_context.json + relevant_files.json
- [ ] **Step 3: Implement CodeGeneration** — Delegate to mol-engine::CodegenRuntime, write experiment/ + experiment_spec.md
- [ ] **Step 4: Implement SanityCheck** — Delegate to runtimes::SanityCheckRuntime, write sanity_report.json
- [ ] **Step 5: Implement ResourcePlanning** — Estimate GPU/time needs, write resource_plan.json + schedule.json
- [ ] **Step 6: Tests, commit**

### Task 4.6: Phase E Stages (ExperimentRun + IterativeRefine)

- [ ] **Step 1: Implement ExperimentRun** — Delegate to runtimes::ExperimentRunRuntime, write raw_results/ + run_logs/
- [ ] **Step 2: Implement IterativeRefine** — Delegate to runtimes::IterativeRefineRuntime, write refined_results/ + refinement_log.json
- [ ] **Step 3: Tests, commit**

### Task 4.7: Phase F Stages (ResultAnalysis + ResearchDecision + KnowledgeSummary)

- [ ] **Step 1: Implement ResultAnalysis** — Delegate to runtimes::ResultAnalysisRuntime, write analysis_report.md + figures/
- [ ] **Step 2: Implement ResearchDecision** — LLM decides PROCEED/PIVOT/REFINE, write decision_record.json
- [ ] **Step 3: Implement KnowledgeSummary** — Write findings to knowledge base entry, write knowledge_summary.json
- [ ] **Step 4: Tests, commit**

### Task 4.8: Phase G Stages (PaperOutline + PaperDraft + PeerReview + PaperRevision)

- [ ] **Step 1: Implement PaperOutline** — LLM generates outline from knowledge + hypotheses, write paper_outline.md
- [ ] **Step 2: Implement PaperDraft** — Multi-section generation with short paper fast-path, framework diagram prompt, NeurIPS checklist, write paper_draft.md
- [ ] **Step 3: Implement PeerReview** — Simulated multi-reviewer perspectives, write review_comments.json
- [ ] **Step 4: Implement PaperRevision** — Revise based on feedback, write paper_revised.md + revision_notes.md
- [ ] **Step 5: Tests, commit**

### Task 4.9: Phase H Stages (QualityGate + KnowledgeArchive + ExportPublish + CitationVerify)

- [ ] **Step 1: Implement QualityGate (GATE)** — Quality score evaluation, write quality_report.json
- [ ] **Step 2: Implement KnowledgeArchive** — Archive findings to KB, write archive_manifest.json
- [ ] **Step 3: Implement ExportPublish** — Use mol-templates for LaTeX compilation, write paper_final.md + paper.tex
- [ ] **Step 4: Implement CitationVerify** — Use mol-literature::citation::verify_citations, write verification_report.json + references_verified.bib
- [ ] **Step 5: Tests, commit**

### Task 4.10: Discussion Stage

- [ ] **Step 1: Implement Discussion stage** — Multi-agent debate using mol-services::discussion engine, write discussion_notes.md
- [ ] **Step 2: Write test with mock discussion**
- [ ] **Step 3: Commit**

### Task 4.11: Enable Contract Validation in Runner

**Files:**
- Modify: `crates/mol-pipeline/src/runner.rs`

- [ ] **Step 1: Add pre-flight input validation**

Before calling `execute_stage()`, call `contracts::validate_inputs(stage, &available_artifacts)`.

- [ ] **Step 2: Add post-execution output validation**

After `execute_stage()`, call `contracts::validate_outputs(stage, &result.artifacts)`.

- [ ] **Step 3: Add MetaClaw PRM quality gate evaluation**

If MetaMol is configured, evaluate stage output quality via `mol_metamol::prm_gate`.

- [ ] **Step 4: Tests, commit**

---

## Phase 5: Code Agent & External Bridges (~3,000 lines Rust)

### Task 5.1: Code Agent Multi-Phase Architecture (mol-engine)

**Files:**
- Create: `crates/mol-engine/src/codegen/code_agent.rs`
- Modify: `crates/mol-engine/src/codegen/mod.rs`

Port from: `backend/agent/researchclaw/pipeline/code_agent.py` (1,397 lines)

- [ ] **Step 1: Define types**

```rust
pub struct CodeAgentConfig {
    pub enabled: bool,
    pub architecture_planning: bool,
    pub sequential_generation: bool,
    pub hard_validation: bool,
    pub hard_validation_max_repairs: u32,
    pub exec_fix_max_iterations: u32,
    pub exec_fix_timeout_sec: u64,
    pub tree_search_enabled: bool,
    pub tree_search_candidates: u32,
    pub tree_search_max_depth: u32,
    pub tree_search_eval_timeout_sec: u64,
    pub review_max_rounds: u32,
}

pub struct SolutionNode { /* ... */ }
pub struct CodeAgentResult { /* ... */ }
pub struct CodeAgent { /* ... */ }
```

- [ ] **Step 2: Implement 5-phase pipeline**

Phase 1: Blueprint planning (LLM generates YAML with per-file pseudocode)
Phase 2: Sequential generation following blueprint order + hard validation
Phase 3: Execution-in-the-loop (sandbox runs, error feedback)
Phase 4: Solution tree search (optional)
Phase 5: Multi-agent review with safety reversion

- [ ] **Step 3: Write tests for blueprint parsing**

```rust
#[test]
fn parse_blueprint_valid_yaml() {
    let yaml = "files:\n  - path: main.py\n    purpose: entry point\ngeneration_order:\n  - main.py";
    let bp = CodeAgent::parse_blueprint(yaml).unwrap();
    assert_eq!(bp.files.len(), 1);
}
```

- [ ] **Step 4: Run tests, commit**

### Task 5.2: OpenCode Bridge (mol-engine)

**Files:**
- Create: `crates/mol-engine/src/bridges/mod.rs`
- Create: `crates/mol-engine/src/bridges/opencode.rs`

Port from: `backend/agent/researchclaw/pipeline/opencode_bridge.py` (852 lines)

- [ ] **Step 1: Implement complexity scoring**

```rust
pub struct ComplexityScore {
    pub score: f64,
    pub signals: HashMap<String, f64>,
    pub recommendation: String, // "beast_mode" | "code_agent" | "legacy"
    pub reason: String,
}

pub fn score_complexity(exp_plan: &str, topic: &str, historical_failures: u32, threshold: f64) -> ComplexityScore { ... }
```

6 signals: component_count (0.25), file_count_hint (0.20), domain_complexity (0.20), condition_count (0.15), historical_failure (0.10), dependency_depth (0.10)

- [ ] **Step 2: Implement OpenCodeBridge**

- check_available() via `which opencode`
- run() spawning opencode subprocess with mega-prompt
- Workspace preparation

- [ ] **Step 3: Tests, commit**

### Task 5.3: OpenHands/Aider Bridge (mol-engine)

**Files:**
- Create: `crates/mol-engine/src/bridges/openhands.rs`

Port from: `backend/agent/researchclaw/pipeline/openhands_bridge.py` (1,238 lines)

- [ ] **Step 1: Implement Aider bridge**

- find_binary() locating aider on PATH
- check_available()
- TODO-driven loop: skeleton generation -> iterative TODO filling
- Prompt templates (_RULES, _SKELETON_PROMPT, _FILL_TODO_PROMPT, _FIX_PROMPT)
- Max 10 TODO iterations, max 2 fix attempts

- [ ] **Step 2: Tests, commit**

---

## Phase 6: Service Handler Completion (~500 lines Rust)

### Task 6.1: Complete mol-services WebSocket Handlers

**Files:**
- Modify: `crates/mol-services/src/agent_bridge.rs`
- Modify: `crates/mol-services/src/resource_monitor.rs`
- Modify: `crates/mol-services/src/discussion.rs`

- [ ] **Step 1: Audit current agent_bridge.rs implementation** (28K lines, likely mostly complete)
- [ ] **Step 2: Fill any remaining message handler gaps**
- [ ] **Step 3: Implement GPU metrics collection in resource_monitor**
- [ ] **Step 4: Complete discussion message routing**
- [ ] **Step 5: Tests, commit**

---

## Phase 7: Verification & Python Deletion

### Task 7.1: Full Integration Test

**Files:**
- Create: `tests/integration/full_pipeline.rs`

- [ ] **Step 1: Write integration test running a minimal pipeline**

```rust
#[tokio::test]
async fn pipeline_topic_init_to_problem_decompose() {
    let tmp = tempdir().unwrap();
    let config = MolConfig::from_template(tmp.path());
    let summary = execute_pipeline(/* from TopicInit to ProblemDecompose */).await.unwrap();
    assert_eq!(summary.stages_completed, 2);
    assert!(tmp.path().join("stage-01/goal.md").exists());
    assert!(tmp.path().join("stage-02/problem_tree.md").exists());
}
```

- [ ] **Step 2: Run full test suite**

```bash
cargo test --workspace
```

- [ ] **Step 3: Compare output artifacts against Python reference run**

Run both Python and Rust pipelines on same input, diff outputs for functional parity.

- [ ] **Step 4: Commit**

### Task 7.2: YAML & Data Files Migration (MUST run before Python deletion)

- [ ] **Step 1: Copy all data files to Rust-accessible location**

```bash
mkdir -p crates/mol-common/data/
cp -r backend/agent/researchclaw/data/ crates/mol-common/data/
```

This includes `*.yaml` files AND `framework_docs/` subdirectory (axolotl.md, llamafactory.md, peft.md, transformers_training.md, trl.md).

- [ ] **Step 2: Use include_str!() or runtime loading for YAML/MD files**
- [ ] **Step 3: Run cargo test to verify data loads correctly**
- [ ] **Step 4: Commit**

### Task 7.3: Parity Verification Checklist

- [ ] **Step 1: Generate Python→Rust mapping**

Create a checklist mapping every Python file under `backend/agent/researchclaw/` to its Rust equivalent. Mark each as: "ported", "merged into X", or "not needed (reason)".

- [ ] **Step 2: Run both pipelines on same input, diff artifacts**
- [ ] **Step 3: Commit checklist to docs/**

### Task 7.4: Delete Python Code

- [ ] **Step 1: Remove backend/ directory**

```bash
rm -rf backend/
```

- [ ] **Step 2: Update .gitignore** — remove Python-specific entries
- [ ] **Step 3: Update README.md** — remove Python installation instructions
- [ ] **Step 4: Update start.sh** — point to Rust binary only
- [ ] **Step 5: Run cargo test one final time**
- [ ] **Step 6: Commit**

```bash
git commit -m "chore: remove legacy Python codebase — Rust migration complete"
```

---

## Execution Order & Dependencies

```
Phase 1 (Branding) ─────────────────────────────────── can start immediately
Phase 2 (Utilities) ─────────────────────────────────── can start immediately
Phase 3 (Agent Runtimes) ────────────────────────────── depends on Phase 2.1 (adapters)
Phase 4 (Pipeline Stages) ──────────────────────────── depends on Phase 2.8-2.10 + Phase 3 + Phase 4.1
Phase 5 (Code Agent + Bridges) ─────────────────────── depends on Phase 2 + mol-engine + mol-experiment
Phase 6 (Services) ──────────────────────────────────── can start immediately
Phase 7 (Verification + Deletion) ──────────────────── depends on ALL above
```

Phases 1, 2, 6 can run in parallel. Phase 3 follows 2.1. Phase 4 follows 2.8-2.10 + 3. Phase 5 follows 2. Phase 7 is last.

**Branch strategy:** Each phase should be developed on a separate branch and merged via PR, allowing easy rollback if a phase introduces regressions.

## Estimated Scope

| Phase | New Rust Lines | Tasks |
|-------|---------------|-------|
| Phase 1: Branding | ~0 (edits only) | 2 |
| Phase 2: Utilities | ~2,400 | 10 |
| Phase 3: Agent Runtimes | ~2,000 | 4 |
| Phase 4: Pipeline Stages | ~5,500 | 11 |
| Phase 5: Code Agent + Bridges | ~3,000 | 3 |
| Phase 6: Services | ~500 | 1 |
| Phase 7: Verification + Deletion | ~200 | 4 |
| **Total** | **~13,600** | **35** |
