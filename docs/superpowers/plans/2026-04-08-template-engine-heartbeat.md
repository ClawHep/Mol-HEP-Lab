# Template Engine + Heartbeat Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 24 hardcoded system prompts and 24 fallback templates in the Rust pipeline with Tera-powered stage templates loaded from disk, add periodic heartbeat monitoring during stage execution, and remove dead graceful_degradation code.

**Architecture:** `StagePromptEngine` wraps the existing `PromptEngine` (in `mol-common`) to load 26 `.md` templates from `hep/templates/stages/`, render them with stage-specific variables, and split on `---user---` into (system, user) prompt pairs. `llm_generate` changes from `-> String` to `-> Result<String>` so LLM failures propagate honestly. The runner spawns a background heartbeat task per stage.

**Tech Stack:** Rust, Tera (via existing `mol-common::PromptEngine`), tokio (async runtime), serde_json

**Spec:** `docs/superpowers/specs/2026-04-08-template-engine-heartbeat-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/mol-pipeline/src/executor.rs` | Modify | Add 3 fields to `MolConfig`, add `prompt_engine` to `StageContext`, add `StagePromptEngine` struct, change `llm_generate` → `Result<String>` |
| `crates/mol-pipeline/src/checkpoint.rs` | Modify | Extend `HeartbeatRecord` with `phase_label`, `status`, `elapsed_secs`; update `write_heartbeat` signature |
| `crates/mol-pipeline/src/runner.rs` | Modify | Construct `StagePromptEngine` at startup, pass to `StageContext`; spawn periodic heartbeat task; remove `"degraded"` dead code |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | Modify | Replace 2 hardcoded prompts + 2 fallbacks with template engine calls |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | Modify | Replace 5 hardcoded prompts + 5 fallbacks |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | Modify | Replace 7 hardcoded prompts + 7 fallbacks |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | Modify | Replace 3 hardcoded prompts + 3 fallbacks |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | Modify | Replace 7 hardcoded prompts + 7 fallbacks |
| `crates/mol-pipeline/src/stages_impl/discussion.rs` | Modify | Update `StageContext` construction in tests only (no prompt changes) |
| `hep/templates/stages/*.md` | Create | 26 new Tera template files (one per pipeline stage, excluding Discussion) |
| `crates/mol-cli/src/commands/run.rs` | Modify | Pass `domain`/`analysis_type`/`templates_dir` to `MolConfig` |

---

### Task 1: Extend MolConfig with domain fields

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:25-39` (MolConfig struct + Default impl)

- [ ] **Step 1: Add new fields to MolConfig**

In `executor.rs`, add three fields to the `MolConfig` struct (after line 29):

```rust
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
    /// Path to stage template directory. Defaults to `hep/templates/stages/`.
    pub templates_dir: Option<PathBuf>,
}
```

Add `use std::path::PathBuf;` to the imports if not already present.

Update the `Default` impl (line 32-39):

```rust
impl Default for MolConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            settings: HashMap::new(),
            domain: "hep".to_owned(),
            analysis_type: None,
            templates_dir: None,
        }
    }
}
```

- [ ] **Step 2: Run `cargo check -p mol-pipeline`**

Expected: Compilation errors in `runner.rs` and test helpers where `MolConfig` is constructed without the new fields. This is expected — we'll fix these in Task 3 and Task 8.

- [ ] **Step 3: Fix all MolConfig construction sites**

Search for `MolConfig {` across the codebase. For each site, add the three new fields with default values:

```rust
domain: "hep".to_owned(),
analysis_type: None,
templates_dir: None,
```

Construction sites to fix:
- `crates/mol-pipeline/src/stages_impl/phase1.rs` test helper `make_ctx` (~line 402)
- `crates/mol-pipeline/src/stages_impl/phase2.rs` test helper `make_ctx` (~line 676)
- `crates/mol-pipeline/src/stages_impl/phase3.rs` test helper `make_ctx`
- `crates/mol-pipeline/src/stages_impl/phase4.rs` test helper `make_ctx` (~line 463)
- `crates/mol-pipeline/src/stages_impl/phase5.rs` test helper `make_ctx` (~line 1300)
- `crates/mol-pipeline/src/stages_impl/discussion.rs` if it has MolConfig construction
- `crates/mol-pipeline/src/runner.rs` test helpers
- `crates/mol-pipeline/src/executor.rs` test helpers
- Any other files found by grep

- [ ] **Step 4: Run `cargo check -p mol-pipeline`**

Expected: PASS (no errors). The new fields exist but are not yet used.

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/
git commit -m "feat(pipeline): extend MolConfig with domain, analysis_type, templates_dir"
```

---

### Task 2: Create StagePromptEngine

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs` (add new struct after MolConfig)

- [ ] **Step 1: Write test for StagePromptEngine::load**

Add to the `#[cfg(test)] mod tests` block in `executor.rs`:

```rust
#[test]
fn stage_prompt_engine_loads_templates() {
    use std::fs;
    let tmp = tempfile::tempdir().unwrap();
    let stages_dir = tmp.path().join("stages");
    fs::create_dir_all(&stages_dir).unwrap();

    // Write a minimal template
    fs::write(
        stages_dir.join("topic_init.md"),
        "You are a researcher.\n\n---user---\n\nAnalyze: {{ topic }}",
    ).unwrap();

    let engine = StagePromptEngine::load(&stages_dir).unwrap();
    assert!(engine.has_template(Stage::TopicInit));
    assert!(!engine.has_template(Stage::Discussion));
}
```

- [ ] **Step 2: Write test for StagePromptEngine::render_prompt**

```rust
#[test]
fn stage_prompt_engine_renders_split_prompt() {
    use std::fs;
    let tmp = tempfile::tempdir().unwrap();
    let stages_dir = tmp.path().join("stages");
    fs::create_dir_all(&stages_dir).unwrap();

    fs::write(
        stages_dir.join("topic_init.md"),
        "You are a {{ domain }} researcher.\n\n---user---\n\nAnalyze: {{ topic }}",
    ).unwrap();

    let engine = StagePromptEngine::load(&stages_dir).unwrap();

    let mut vars = HashMap::new();
    vars.insert("topic".to_owned(), "Higgs boson".to_owned());
    vars.insert("domain".to_owned(), "hep".to_owned());

    let (system, user) = engine.render_prompt(Stage::TopicInit, &vars).unwrap();
    assert!(system.contains("hep researcher"));
    assert!(user.contains("Higgs boson"));
}

#[test]
fn stage_prompt_engine_missing_template_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = StagePromptEngine::load(tmp.path()).unwrap();
    let vars: HashMap<String, String> = HashMap::new();
    assert!(engine.render_prompt(Stage::TopicInit, &vars).is_err());
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p mol-pipeline stage_prompt_engine`
Expected: FAIL — `StagePromptEngine` not defined yet.

- [ ] **Step 4: Implement StagePromptEngine**

Add after the `MolConfig` Default impl in `executor.rs`:

```rust
use mol_common::PromptEngine;
use std::sync::Arc;

/// Loads and renders stage-specific prompt templates from disk.
///
/// Each stage maps to a `.md` file named `{stage_name_lowercase}.md`.
/// Templates are rendered with Tera and split on `---user---` into
/// (system_prompt, user_prompt) pairs.
pub struct StagePromptEngine {
    engine: PromptEngine,
}

impl StagePromptEngine {
    /// Load all stage templates from `templates_dir`.
    pub fn load(templates_dir: &Path) -> Result<Self> {
        let engine = PromptEngine::from_directory(templates_dir)
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
        let rendered = self.engine.render_str_vars(&name, vars.iter().map(|(k, v)| (k.as_str(), v.as_str())))
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mol-pipeline stage_prompt_engine`
Expected: PASS — all 3 tests green.

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/src/executor.rs
git commit -m "feat(pipeline): add StagePromptEngine for disk-based template loading"
```

---

### Task 3: Add prompt_engine to StageContext

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:99-113` (StageContext struct)
- Modify: `crates/mol-pipeline/src/runner.rs:248-255,480-487` (StageContext construction)
- Modify: All `make_ctx` test helpers in phase1-5.rs, discussion.rs, runner.rs, executor.rs

- [ ] **Step 1: Add `prompt_engine` field to StageContext**

In `executor.rs`, add to the `StageContext` struct:

```rust
/// Stage prompt template engine — loaded once at pipeline startup.
pub prompt_engine: Option<Arc<StagePromptEngine>>,
```

Using `Option` allows tests to set it to `None` without loading templates from disk.

- [ ] **Step 2: Run `cargo check -p mol-pipeline` and fix all construction sites**

Every place that constructs `StageContext { ... }` needs `prompt_engine: None,` added. Search with:
```bash
grep -rn "StageContext {" crates/mol-pipeline/src/
```

For each site, add: `prompt_engine: None,`

For the two runner.rs construction sites (~lines 248, 480), we'll wire in the real engine in Task 7. For now, add `prompt_engine: None,`.

- [ ] **Step 3: Run `cargo test -p mol-pipeline`**

Expected: PASS — all existing tests still work with `prompt_engine: None`.

- [ ] **Step 4: Add a convenience method to StageContext for building template vars**

In `executor.rs`, add to the `impl StageContext` block:

```rust
/// Build the template variable map for a stage render.
///
/// Populates common variables (topic, domain, analysis_type, timestamp)
/// and reads relevant prior artifacts into the map.
pub fn template_vars(&self) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("topic".to_owned(), self.config.topic.clone());
    vars.insert("domain".to_owned(), self.config.domain.clone());
    vars.insert(
        "analysis_type".to_owned(),
        self.config.analysis_type.clone().unwrap_or_else(|| "general".to_owned()),
    );
    vars.insert("timestamp".to_owned(), utcnow_iso());

    // Prior artifacts — read common ones if they exist
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

    vars
}
```

- [ ] **Step 5: Run `cargo test -p mol-pipeline`**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/
git commit -m "feat(pipeline): add prompt_engine to StageContext with template_vars helper"
```

---

### Task 4: Change llm_generate to return Result<String>

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:139-163` (llm_generate function)
- Modify: All 24 call sites in `phase1.rs`, `phase2.rs`, `phase3.rs`, `phase4.rs`, `phase5.rs`

- [ ] **Step 1: Change llm_generate signature and body**

In `executor.rs`, change the function (lines 139-163):

```rust
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
```

- [ ] **Step 2: Run `cargo check -p mol-pipeline`**

Expected: MANY compilation errors in phase1-5.rs — every `llm_generate` call site now returns `Result<String>` instead of `String`. This is expected. List the error count.

- [ ] **Step 3: Update all 24 call sites mechanically**

For each call site, the pattern changes from:

```rust
// OLD
let llm_result = llm_generate(ctx, &system, &user, false).await;
if !llm_result.is_empty() {
    // write artifact with llm_result
    return Ok(StageResult { ... });
}
// fallback template code
```

To:

```rust
// NEW
let result = llm_generate(ctx, &system, &user, false).await
    .map_err(|e| anyhow::anyhow!("LLM generation failed for {}: {e}", stage.name()))?;

if result.is_empty() {
    return Ok(StageResult::failure(stage, "LLM returned empty response".into()));
}
// write artifact with result (keep existing write logic)
// REMOVE entire fallback template block
```

**IMPORTANT**: For this step, only change the `llm_generate` call and its immediate error handling + empty check. Do NOT remove the fallback template blocks yet — that happens in Task 6. For now, just make it compile by changing the call sites to handle `Result`:

```rust
// TEMPORARY (to compile): unwrap the Result, keep old fallback logic
let llm_result = llm_generate(ctx, &system, &user, false).await.unwrap_or_default();
```

This intermediate step ensures compilation works before the larger refactor.

- [ ] **Step 4: Run `cargo check -p mol-pipeline`**

Expected: PASS — all call sites compile with `.unwrap_or_default()`.

- [ ] **Step 5: Run `cargo test -p mol-pipeline`**

Expected: PASS — tests with `llm: None` now get empty string from `.unwrap_or_default()`, hitting the same fallback paths as before.

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/
git commit -m "feat(pipeline): change llm_generate to return Result<String>"
```

---

### Task 5: Create 26 stage template files

**Files:**
- Create: `hep/templates/stages/` directory
- Create: 26 `.md` files (one per pipeline stage)

Each template follows this structure:
1. System prompt section (HEP-domain-aware role + conventions reference)
2. `---user---` delimiter
3. User prompt section (task instructions with `{{ variable }}` placeholders)

- [ ] **Step 1: Create the stages directory**

```bash
mkdir -p hep/templates/stages
```

- [ ] **Step 2: Create Phase 1 templates (2 files)**

Create `hep/templates/stages/topic_init.md`:
```markdown
You are a high-energy physics research planner. Your role is to formulate
a clear research goal with specific physics objectives, identify the relevant
experimental context (detector, dataset, luminosity), and produce a structured
research brief.

{{ conventions | default(value="") }}

---user---

Define the research goal and initial analysis plan for the following topic:

Topic: {{ topic }}
Analysis type: {{ analysis_type | default(value="general") }}

Produce a structured research brief containing:
1. Physics motivation and theoretical context
2. Key observables and measurements
3. Expected datasets and detector requirements
4. Initial analysis strategy
5. Success criteria and milestones

Also produce a hardware_profile.json with fields: gpu_available (bool),
gpu_count (int), gpu_memory_gb (float), cpu_cores (int), ram_gb (float).

{{ output_spec | default(value="") }}
```

Create `hep/templates/stages/problem_decompose.md`:
```markdown
You are a research strategist specializing in decomposing complex high-energy
physics analysis problems into tractable sub-problems with clear dependencies.

{{ conventions | default(value="") }}

---user---

Decompose the following research goal into a structured problem tree:

Research goal:
{{ goal | default(value="") }}

Topic: {{ topic }}
Analysis type: {{ analysis_type | default(value="general") }}

Produce:
1. A problem_tree.md with hierarchical decomposition of sub-problems
2. A topic_evaluation.json with fields: feasibility (0-1), novelty (0-1),
   impact (0-1), sub_problems (list of strings), key_challenges (list),
   estimated_phases (int)
```

- [ ] **Step 3: Create Phase 2 templates (6 files)**

Create the following files in `hep/templates/stages/`:

**search_strategy.md** — systematic literature search planner for HEP
**literature_collect.md** — research librarian collecting HEP papers
**literature_screen.md** — paper screening specialist (this is a GATE stage)
**knowledge_extract.md** — scientific knowledge extraction from HEP literature
**synthesis.md** — senior physicist synthesizing literature findings
**hypothesis_gen.md** — physicist formulating falsifiable hypotheses

Each template must:
- Reference HEP-specific tools (INSPIRE-HEP, arXiv, CDS)
- Use `{{ topic }}`, `{{ analysis_type }}`, and relevant prior artifact variables
- Include the `---user---` delimiter
- Specify the exact output artifact names matching the contract (see `contracts.rs`)

- [ ] **Step 4: Create Phase 3 templates (7 files)**

**experiment_design.md** — HEP experiment designer (GATE stage)
**codebase_search.md** — software engineer surveying HEP analysis code
**code_generation.md** — HEP software engineer using pyhf, uproot, mplhep, etc.
**sanity_check.md** — code reviewer for HEP experiment code
**resource_planning.md** — compute resource planner for HEP analysis
**experiment_run.md** — experiment runner summarizing multi-seed results
**iterative_refine.md** — researcher refining experiment configurations

The `code_generation.md` template is particularly important — must reference the HEP analysis stack: pyhf, uproot, mplhep, hist, vector, awkward-array.

- [ ] **Step 5: Create Phase 4 templates (3 files)**

**result_analysis.md** — physicist analyzing experiment results
**research_decision.md** — research director making go/no-go decisions
**knowledge_summary.md** — knowledge manager distilling findings

- [ ] **Step 6: Create Phase 5 templates (8 files)**

**paper_outline.md** — academic writing coach for HEP analysis notes
**paper_draft.md** — academic writer drafting HEP analysis notes
**peer_review.md** — HEP peer reviewer (NOT NeurIPS — use HEP review criteria)
**paper_revision.md** — writer revising based on peer review
**quality_gate.md** — quality assurance reviewer (GATE stage)
**knowledge_archive.md** — knowledge archival specialist
**export_publish.md** — copy editor finalizing for publication
**citation_verify.md** — citation verification specialist

**IMPORTANT for peer_review.md**: The old template referenced "NeurIPS/ICML/ICLR" — this must be replaced with HEP-appropriate review criteria (physics validity, statistical treatment, systematic uncertainties, reproducibility).

- [ ] **Step 7: Verify all 26 templates exist and have correct names**

Run: `ls hep/templates/stages/*.md | wc -l`
Expected: 26

Verify names match Stage::name().to_ascii_lowercase():
```bash
for f in hep/templates/stages/*.md; do echo "$(basename $f)"; done | sort
```

Expected output (alphabetical):
```
codebase_search.md
code_generation.md
citation_verify.md
experiment_design.md
experiment_run.md
export_publish.md
hypothesis_gen.md
iterative_refine.md
knowledge_archive.md
knowledge_extract.md
knowledge_summary.md
literature_collect.md
literature_screen.md
paper_draft.md
paper_outline.md
paper_revision.md
peer_review.md
problem_decompose.md
quality_gate.md
resource_planning.md
result_analysis.md
sanity_check.md
search_strategy.md
synthesis.md
topic_init.md
```

Wait — that's 25. Missing: `research_decision.md`. Verify it exists.

- [ ] **Step 8: Commit**

```bash
git add hep/templates/stages/
git commit -m "feat(templates): create 26 HEP stage templates with domain knowledge"
```

---

### Task 6: Refactor phase executors to use template engine

This is the largest task — refactoring all 24 call sites across 5 phase files. Do one file at a time, testing between each.

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase1.rs`
- Modify: `crates/mol-pipeline/src/stages_impl/phase2.rs`
- Modify: `crates/mol-pipeline/src/stages_impl/phase3.rs`
- Modify: `crates/mol-pipeline/src/stages_impl/phase4.rs`
- Modify: `crates/mol-pipeline/src/stages_impl/phase5.rs`

For each call site, the refactoring pattern is:

```rust
// OLD pattern (per call site):
let system = "You are a ... (hardcoded system prompt)";
let user = format!("... {} ...", ctx.config.topic);  // hardcoded user prompt
let llm_result = llm_generate(ctx, &system, &user, false).await.unwrap_or_default();
if !llm_result.is_empty() {
    // write artifact(s)
    return Ok(StageResult { status: StageStatus::Done, ... });
}
// 50-200 lines of fallback template code

// NEW pattern:
let vars = ctx.template_vars();
let engine = ctx.prompt_engine.as_ref()
    .ok_or_else(|| anyhow::anyhow!("No prompt engine configured"))?;
let (system, user) = engine.render_prompt(stage, &vars)?;

let result = llm_generate(ctx, &system, &user, false).await
    .map_err(|e| anyhow::anyhow!("{}: {e}", stage.name()))?;
if result.is_empty() {
    return Ok(StageResult::failure(stage, "LLM returned empty response".into()));
}
// write artifact(s) — keep existing write logic, use `result` instead of `llm_result`
// DELETE entire fallback template block
```

- [ ] **Step 1: Refactor phase1.rs (2 call sites)**

Refactor `execute_topic_init` (line ~30) and `execute_problem_decompose` (line ~178).

For each function:
1. Remove the hardcoded `let system = "..."` string
2. Remove the hardcoded user prompt construction
3. Add template engine rendering at the top
4. Change `llm_generate` call from `.unwrap_or_default()` to `.map_err(...)?`
5. Add empty-result check returning `StageResult::failure`
6. Keep the artifact write logic (goal.md, hardware_profile.json, etc.)
7. Delete the entire fallback template block (the large `format!()` producing fake data)

- [ ] **Step 2: Run `cargo test -p mol-pipeline -- phase1`**

Expected: Some test failures — tests with `llm: None` and `prompt_engine: None` now get errors instead of fallback data. This is expected and will be fixed in Task 8.

- [ ] **Step 3: Refactor phase2.rs (5 call sites)**

Functions: `execute_search_strategy`, `execute_literature_collect`, `execute_knowledge_extract`, `execute_synthesis`, `execute_hypothesis_gen`.

Same pattern as Step 1. Delete all fallback template blocks.

Note: `execute_literature_screen` is a GATE stage that does NOT call `llm_generate` — it has its own logic. Leave it unchanged.

- [ ] **Step 4: Run `cargo check -p mol-pipeline`**

Expected: PASS compilation.

- [ ] **Step 5: Refactor phase3.rs (7 call sites)**

Functions: `execute_experiment_design`, `execute_codebase_search`, `execute_code_generation`, `execute_sanity_check`, `execute_resource_planning`, `execute_experiment_run`, `execute_iterative_refine`.

**Special case — `execute_experiment_design`**: This is a GATE stage. After the template refactor, keep the existing gate logic (BlockedApproval if not auto_approve).

**Special case — `execute_code_generation`**: This is the largest fallback (~290 lines of hardcoded numpy/sklearn code). Delete all of it.

- [ ] **Step 6: Run `cargo check -p mol-pipeline`**

Expected: PASS.

- [ ] **Step 7: Refactor phase4.rs (3 call sites)**

Functions: `execute_result_analysis`, `execute_research_decision`, `execute_knowledge_summary`.

**Special case — `execute_research_decision`**: Keep the decision field parsing logic (extracting "proceed"/"pivot"/"stop" from JSON response).

- [ ] **Step 8: Run `cargo check -p mol-pipeline`**

Expected: PASS.

- [ ] **Step 9: Refactor phase5.rs (7 call sites)**

Functions: `execute_paper_outline`, `execute_paper_draft`, `execute_peer_review`, `execute_paper_revision`, `execute_quality_gate`, `execute_export_publish`, `execute_citation_verify`.

**Special case — `execute_quality_gate`**: GATE stage. Keep gate logic.

**Special case — `execute_export_publish`**: Has conditional `if !prior_paper.is_empty()` before the `llm_generate` call. The template engine handles missing prior artifacts via `{{ paper_revised | default(value="") }}`, so this conditional can be simplified.

- [ ] **Step 10: Run `cargo check -p mol-pipeline`**

Expected: PASS — all 24 call sites refactored, all fallback blocks removed.

- [ ] **Step 11: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/
git commit -m "feat(pipeline): replace 24 hardcoded prompts + fallbacks with template engine"
```

---

### Task 7: Enhance HeartbeatRecord and add periodic heartbeat

**Files:**
- Modify: `crates/mol-pipeline/src/checkpoint.rs:34-40,141-155`
- Modify: `crates/mol-pipeline/src/runner.rs` (heartbeat spawn/cancel, StagePromptEngine construction)

- [ ] **Step 1: Extend HeartbeatRecord**

In `checkpoint.rs`, update the struct (lines 34-40):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeartbeatRecord {
    pid: u32,
    last_stage: i32,
    last_stage_name: String,
    phase_label: String,
    run_id: String,
    timestamp: String,
    status: String,
    elapsed_secs: f64,
}
```

- [ ] **Step 2: Update write_heartbeat signature and body**

Change the function signature (lines 141-155):

```rust
pub async fn write_heartbeat(
    run_dir: &Path,
    stage: Stage,
    run_id: &str,
    status: &str,
    elapsed_secs: f64,
) -> Result<()> {
    let record = HeartbeatRecord {
        pid: std::process::id(),
        last_stage: stage.as_i32(),
        last_stage_name: stage.name().to_owned(),
        phase_label: stage.phase_label().to_owned(),
        run_id: run_id.to_owned(),
        timestamp: Utc::now().to_rfc3339(),
        status: status.to_owned(),
        elapsed_secs,
    };
    let json = serde_json::to_string_pretty(&record).context("serialise heartbeat")?;
    let path = run_dir.join("heartbeat.json");
    fs::write(&path, json.as_bytes())
        .await
        .with_context(|| format!("write heartbeat to {}", path.display()))?;
    Ok(())
}
```

- [ ] **Step 3: Fix the write_heartbeat call site in runner.rs**

The existing call (runner.rs ~line 317):
```rust
if let Err(e) = write_heartbeat(run_dir, stage, run_id).await {
```

Change to:
```rust
if let Err(e) = write_heartbeat(run_dir, stage, run_id, "completed", elapsed).await {
```

Where `elapsed` is `stage_start.elapsed().as_secs_f64()` — you may need to add `let stage_start = std::time::Instant::now();` before the `execute_stage` call.

- [ ] **Step 4: Add periodic heartbeat spawn in runner.rs**

Before the `execute_stage` call in the main loop, add:

```rust
let stage_start = std::time::Instant::now();
let heartbeat_handle = tokio::spawn({
    let run_dir = run_dir.to_owned();
    let run_id = run_id.to_owned();
    async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = write_heartbeat(
                &run_dir, stage, &run_id,
                "running", stage_start.elapsed().as_secs_f64(),
            ).await;
        }
    }
});
```

After the `execute_stage` call, add:
```rust
heartbeat_handle.abort();
```

- [ ] **Step 5: Construct StagePromptEngine at runner startup**

Near the top of the `run_pipeline` function in `runner.rs`, after the run directory is created:

```rust
// Load stage templates
let templates_dir = config.templates_dir.clone()
    .unwrap_or_else(|| PathBuf::from("hep/templates/stages"));
let prompt_engine = Arc::new(
    StagePromptEngine::load(&templates_dir)
        .context("Failed to load stage templates")?
);
```

Then update the two `StageContext` construction sites to use:
```rust
prompt_engine: Some(prompt_engine.clone()),
```

Add the necessary imports at the top of `runner.rs`:
```rust
use crate::executor::StagePromptEngine;
```

- [ ] **Step 6: Run `cargo check -p mol-pipeline`**

Expected: PASS.

- [ ] **Step 7: Write heartbeat test**

Add to the test module in `checkpoint.rs`:

```rust
#[tokio::test]
async fn write_heartbeat_includes_all_fields() {
    let tmp = tempfile::tempdir().unwrap();
    write_heartbeat(tmp.path(), Stage::CodeGeneration, "run-123", "running", 42.5)
        .await.unwrap();

    let content = tokio::fs::read_to_string(tmp.path().join("heartbeat.json"))
        .await.unwrap();
    let record: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(record["last_stage"], 11);
    assert_eq!(record["last_stage_name"], "CODE_GENERATION");
    assert_eq!(record["phase_label"], "3.3 Code Generation");
    assert_eq!(record["status"], "running");
    assert_eq!(record["elapsed_secs"], 42.5);
    assert_eq!(record["run_id"], "run-123");
}
```

- [ ] **Step 8: Run `cargo test -p mol-pipeline -- heartbeat`**

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/mol-pipeline/src/checkpoint.rs crates/mol-pipeline/src/runner.rs
git commit -m "feat(pipeline): periodic heartbeat with phase_label, status, elapsed_secs"
```

---

### Task 8: Remove graceful_degradation dead code and fix tests

**Files:**
- Modify: `crates/mol-pipeline/src/runner.rs` (remove "degraded" logic)
- Modify: All phase test files (update make_ctx, fix test expectations)

- [ ] **Step 1: Remove "degraded" decision dead code in runner.rs**

Remove or update these locations:

1. Line ~106: `let degraded = results.iter().any(|r| r.decision == "degraded");` — remove this line
2. Lines ~273-277: The `if result.decision == "degraded"` log branch — remove entire block
3. Any `PipelineSummary` field or reference that uses `degraded` — remove

Also update the doc comment on `graceful_degradation` in `PipelineConfig` (line ~47):

```rust
/// When `true` the runner skips failed stages and continues the pipeline
/// with available artifacts. No fallback content is generated.
pub graceful_degradation: bool,
```

- [ ] **Step 2: Update test helpers in all phase files**

Every `make_ctx` function needs `prompt_engine: None` (already added in Task 3). Tests that previously relied on fallback output (running stage executors with `llm: None`) now need to either:

a) Provide a mock LLM that returns valid content, OR
b) Expect `StageResult::failure` or an `Err`

For simplicity, update tests to expect the error case:

```rust
#[tokio::test]
async fn topic_init_fails_without_llm() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = make_ctx(tmp.path(), "test topic");
    // No LLM and no prompt_engine → should fail
    let result = execute_topic_init(Stage::TopicInit, &ctx).await;
    // Depending on implementation, either Err or StageResult with failure status
    assert!(result.is_err() || result.unwrap().status != StageStatus::Done);
}
```

- [ ] **Step 3: Run `cargo test -p mol-pipeline`**

Expected: All tests PASS. Count the total passing tests and report.

- [ ] **Step 4: Commit**

```bash
git add crates/mol-pipeline/
git commit -m "fix(pipeline): remove degraded dead code, update tests for honest failure"
```

---

### Task 9: Add config fields and wire CLI

**Files:**
- Modify: `crates/mol-config/src/types.rs:97-110` (ResearchConfig struct)
- Modify: `crates/mol-cli/src/commands/run.rs`
- Modify: `config.mol.yaml`

- [ ] **Step 1: Add analysis_type and templates_dir to ResearchConfig**

In `crates/mol-config/src/types.rs`, add two fields to `ResearchConfig` (after line 109):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResearchConfig {
    pub topic: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub daily_paper_count: u32,
    #[serde(default)]
    pub quality_threshold: f64,
    #[serde(default = "defaults::bool_true")]
    pub graceful_degradation: bool,
    #[serde(default)]
    pub reference_papers: Vec<String>,
    /// Analysis type within the domain (e.g. "extraction", "search", "measurement").
    #[serde(default)]
    pub analysis_type: Option<String>,
    /// Path to stage template directory. Overrides default `hep/templates/stages/`.
    #[serde(default)]
    pub templates_dir: Option<PathBuf>,
}
```

Add `use std::path::PathBuf;` to the imports if not already present.

- [ ] **Step 2: Update config.mol.yaml**

Add `analysis_type` under the `research` section:

```yaml
research:
  topic: "Search for rare Z boson decays"
  domains: ["high-energy-physics"]
  analysis_type: "search"  # extraction, search, measurement, or unfolding
```

- [ ] **Step 3: Read run.rs to find MolConfig construction**

```bash
grep -n "MolConfig" crates/mol-cli/src/commands/run.rs
```

- [ ] **Step 4: Add domain/analysis_type/templates_dir population**

Where `MolConfig` is constructed from the project config, add:

```rust
domain: project_config.research.domains.first()
    .map(|d| d.to_owned())
    .unwrap_or_else(|| "hep".to_owned()),
analysis_type: project_config.research.analysis_type.clone(),
templates_dir: project_config.research.templates_dir.clone(),
```

- [ ] **Step 5: Run `cargo check`**

Expected: PASS across all crates.

- [ ] **Step 6: Commit**

```bash
git add crates/mol-cli/ crates/mol-config/ config.mol.yaml
git commit -m "feat(config): add analysis_type and templates_dir to ResearchConfig"
```

---

### Task 10: Full compilation and test verification

- [ ] **Step 1: Run full build**

```bash
cargo build --release 2>&1 | tail -5
```

Expected: Build succeeds with exit 0.

- [ ] **Step 2: Run full test suite**

```bash
cargo test 2>&1 | tail -20
```

Expected: All tests pass. Report exact count (should be ~559+ tests).

- [ ] **Step 3: Verify template loading works**

```bash
cargo test -p mol-pipeline -- stage_prompt_engine --nocapture
```

Expected: All StagePromptEngine tests PASS.

- [ ] **Step 4: Verify heartbeat tests**

```bash
cargo test -p mol-pipeline -- heartbeat --nocapture
```

Expected: All heartbeat tests PASS.

- [ ] **Step 5: Spot-check template rendering for each phase**

Write a quick integration test or run an ad-hoc check:
```bash
cargo test -p mol-pipeline -- template_render --nocapture
```

Verify all 26 templates render without Tera errors.

- [ ] **Step 6: Commit any final fixes**

```bash
git add -A
git commit -m "fix: resolve remaining compilation and test issues"
```

- [ ] **Step 7: Final commit — mark Phase 1 complete**

```bash
git add -A
git commit -m "feat: Phase 1 complete — template engine + heartbeat monitoring

- 26 Tera stage templates in hep/templates/stages/
- StagePromptEngine loads and renders templates with ---user--- split
- llm_generate returns Result<String> — honest failure, no fallbacks
- Periodic heartbeat with phase_label, status, elapsed_secs
- MolConfig extended with domain, analysis_type, templates_dir
- graceful_degradation dead code removed
- All tests passing"
```
