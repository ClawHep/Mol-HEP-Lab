# Design: Unified Template Architecture + Runner Heartbeat Monitoring

**Date**: 2026-04-08
**Status**: In Review
**Scope**: 3-phase refactoring across `crates/mol-pipeline`, `crates/mol-common`, `hep/templates/`

## Problem

The Rust general pipeline and HEP vertical pipeline evolved independently. End-to-end testing revealed:

1. **25 hardcoded system prompts** in stage executors — generic ML-oriented, no HEP domain knowledge
2. **26 hardcoded fallback templates** (incl. `discussion.rs`) — produce fake data when LLM fails, silently masking failures
3. **No runtime bridge** between Rust pipeline and HEP domain knowledge — `MolConfig` has no `domain`, `templates_dir`, or `analysis_type` field
4. **No liveness monitoring** during stage execution — heartbeat only written after completion
5. **260+ lines of hardcoded domain detection** in `executor.rs` — not configurable
6. **NeurIPS-specific content** hardcoded where venue should be configurable
7. **18 HEP agent definitions, 3 conventions files, blinding protocol** — none wired into the Rust pipeline

The HEP pipeline solves these correctly: Tera templates with `{{ variable }}` substitution, analysis-type-aware conventions injection, rich agent definitions as system prompts. The Rust pipeline should adopt this architecture.

## Design Overview — 3 Phases

```
Phase 1: Foundation (this spec)
  ├── MolConfig gains domain/templates_dir/analysis_type
  ├── StagePromptEngine loads templates from disk
  ├── 26 stage template files created in hep/templates/stages/
  ├── All 25 system prompts + 26 fallbacks extracted to templates
  ├── llm_generate → Result<String>, failure = honest failure
  ├── Runner periodic heartbeat during stage execution
  └── graceful_degradation semantic update

Phase 2: HEP Domain Knowledge Injection (future spec)
  ├── 18 HEP agent definitions as system prompt sources
  ├── conventions/ auto-injected by analysis_type
  ├── Blinding protocol integrated into runner gates
  └── Domain-specific artifact naming in contracts

Phase 3: Multi-Domain Generalization (future spec)
  ├── Domain plugin mechanism (HEP / generic ML / other)
  ├── detect_domain from hardcoded → configurable
  ├── Venue-configurable paper format (NeurIPS → analysis note)
  └── Contract registry loaded from domain config
```

## Phase 1 Detailed Design

### 1. MolConfig Extension

Add domain-awareness to the executor config:

```rust
// executor.rs
pub struct MolConfig {
    pub topic: String,
    pub settings: HashMap<String, String>,
    // NEW fields:
    pub domain: String,                    // "hep", "ml", "physics", ...
    pub analysis_type: Option<String>,     // "extraction", "search", "measurement"
    pub templates_dir: Option<PathBuf>,    // path to stage templates
}
```

The runner populates these from the project config at startup. If `templates_dir` is None, the engine looks for `hep/templates/stages/` relative to the working directory.

### 2. StagePromptEngine

New struct in `executor.rs` that loads and renders stage templates:

```rust
pub struct StagePromptEngine {
    engine: PromptEngine,
}

impl StagePromptEngine {
    /// Load all stage templates from a directory.
    pub fn load(templates_dir: &Path) -> Result<Self>;

    /// Render system + user prompts for a stage.
    /// Splits on `---user---` delimiter.
    pub fn render_prompt(
        &self,
        stage: Stage,
        ctx: &StageContext,
    ) -> Result<(String, String)>;
}
```

**Template format parsing**: Each `.md` file is rendered as a single Tera template, then split on the literal line `---user---`. Everything before it becomes the system prompt; everything after becomes the user prompt. This is a simple `str::split_once("---user---")` — no custom parser needed.

**Access pattern**: `StagePromptEngine` is constructed once at pipeline startup and stored on `StageContext`:

```rust
pub struct StageContext {
    // ... existing fields ...
    pub prompt_engine: Arc<StagePromptEngine>,  // NEW
}
```

All stage executor functions already receive `&StageContext`, so no signature changes needed.

### 3. Template Files

Create `hep/templates/stages/` with 27 files (26 stages + 1 discussion):

```
hep/templates/stages/
  topic_init.md
  problem_decompose.md
  search_strategy.md
  literature_collect.md
  literature_screen.md
  knowledge_extract.md
  synthesis.md
  hypothesis_gen.md
  experiment_design.md
  codebase_search.md
  code_generation.md
  sanity_check.md
  resource_planning.md
  experiment_run.md
  iterative_refine.md
  result_analysis.md
  research_decision.md
  knowledge_summary.md
  paper_outline.md
  paper_draft.md
  peer_review.md
  paper_revision.md
  quality_gate.md
  knowledge_archive.md
  export_publish.md
  citation_verify.md
  discussion.md
```

Template content is migrated from the existing Rust source, enriched with HEP domain knowledge. Example:

```markdown
You are a high-energy physics software engineer writing clean, reproducible
Python experiment code using the HEP analysis stack (pyhf, uproot, mplhep,
hist, vector, awkward-array).

{{ conventions }}

---user---

Generate a complete Python experiment for the following analysis:

Topic: {{ topic }}
Analysis type: {{ analysis_type }}

Experiment plan:
{{ experiment_plan }}

Requirements:
- Use pyhf for statistical model building and hypothesis testing
- Use uproot for ROOT file I/O
- Use mplhep for ATLAS/CMS-style plots
- Include proper error handling and logging
- Output results to JSON format with CLs values and signal strengths

{{ output_spec }}
```

### 4. Template Variables

| Variable | Source | Fallback |
|----------|--------|----------|
| `{{ topic }}` | `config.topic` | (required) |
| `{{ analysis_type }}` | `config.analysis_type` | `"general"` |
| `{{ domain }}` | `config.domain` | `"hep"` |
| `{{ conventions }}` | Loaded from `hep/conventions/{type}.md` if exists | `""` (empty) |
| `{{ prior_context }}` | Assembled from `read_prior_artifact_pub()` for relevant prior artifacts | `""` |
| `{{ timestamp }}` | `utcnow_iso()` | (always available) |
| `{{ hardware_profile }}` | From `hardware_profile.json` if available | `""` |

All variables use Tera's `{{ var | default(value="") }}` in templates to avoid hard errors on missing context. Templates that need a specific prior artifact (e.g., `{{ experiment_plan }}`) use `default` filter — if the artifact doesn't exist, the LLM gets an empty string and must work with what it has.

### 5. llm_generate Signature Change

```rust
// OLD: returns empty string on any failure
pub async fn llm_generate(ctx, system, user, json) -> String

// NEW: returns Result, callers handle error explicitly
pub async fn llm_generate(ctx, system, user, json) -> Result<String>
```

When `ctx.llm` is `None` (no LLM provider configured), returns `Err(anyhow!("No LLM provider configured"))` instead of empty string.

### 6. Stage Executor Refactoring

All 26 stage executors (+ discussion) follow the same new pattern:

```rust
pub async fn execute_topic_init(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    fs::create_dir_all(&stage_dir)?;

    // Render prompt from template
    let (system, user) = ctx.prompt_engine.render_prompt(stage, ctx)?;

    // Call LLM — failure is honest failure
    let result = llm_generate(ctx, &system, &user, false).await
        .map_err(|e| anyhow!("LLM generation failed: {e}"))?;

    if result.is_empty() {
        return StageResult::failure(stage, "LLM returned empty response".into());
    }

    // Write output
    fs::write(stage_dir.join("goal.md"), &result)?;

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["goal.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}
```

**Per-phase call site count** (all to be refactored):

| File | System prompts | Fallback blocks | Total call sites |
|------|---------------|-----------------|-----------------|
| `phase1.rs` | 2 | 2 | 2 |
| `phase2.rs` | 5 | 5 | 5 |
| `phase3.rs` | 7 | 7 | 7 |
| `phase4.rs` | 3 | 3 | 3 |
| `phase5.rs` | 7 | 7 | 7 |
| `discussion.rs` | 1 | 1 | 1 |
| **Total** | **25** | **25** | **25** |

### 7. graceful_degradation Semantic Update

With fallbacks removed, `graceful_degradation` mode changes meaning:

- **Old**: "Use degraded fallback content and continue" (fake data)
- **New**: "Log the failure, skip the failed stage, continue pipeline with available artifacts"

This becomes functionally equivalent to `skip_noncritical` but applies to ALL stages, not just noncritical ones. Update the doc comment and CLI help text to reflect this. The `--no-graceful-degradation` flag remains: when set, any stage failure aborts the pipeline.

### 8. Runner Heartbeat Monitoring

#### 8.1 Periodic Heartbeat

Before each stage dispatch, spawn a background task writing heartbeat every 30s:

```rust
let stage_start = Instant::now();
let heartbeat_handle = tokio::spawn({
    let run_dir = run_dir.clone();
    let stage = stage;
    let run_id = run_id.clone();
    async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = write_heartbeat(
                &run_dir, stage, &run_id,
                "running", stage_start.elapsed().as_secs_f64(),
            );
        }
    }
});

let result = execute_stage(stage, &context).await;
heartbeat_handle.abort();

// Write final heartbeat with completion status
let status = if result.status == StageStatus::Done { "completed" } else { "failed" };
let _ = write_heartbeat(&run_dir, stage, &run_id, status, stage_start.elapsed().as_secs_f64());
```

#### 8.2 Enhanced HeartbeatRecord

```rust
#[derive(Serialize)]
struct HeartbeatRecord {
    pid: u32,
    last_stage: i32,
    last_stage_name: String,
    phase_label: String,      // "3.3 Code Generation"
    run_id: String,
    timestamp: String,        // ISO-8601 UTC
    status: String,           // "running" | "completed" | "failed"
    elapsed_secs: f64,        // seconds since stage started
}
```

Updated `write_heartbeat` signature:

```rust
pub fn write_heartbeat(
    run_dir: &Path,
    stage: Stage,
    run_id: &str,
    status: &str,
    elapsed_secs: f64,
) -> Result<()>
```

#### 8.3 Advisory Only — No Timeout Enforcement

The heartbeat is for external monitoring. The runner does NOT enforce any timeout. External systems (main agent, frontend) can read `heartbeat.json` timestamp and decide independently whether to intervene.

## Files Changed

| File | Change |
|------|--------|
| `hep/templates/stages/*.md` | **NEW**: 27 Tera template files (26 stages + discussion) |
| `crates/mol-pipeline/src/executor.rs` | Add `StagePromptEngine`, extend `MolConfig`, change `llm_generate` → `Result<String>` |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | Remove 2 hardcoded prompts + 2 fallbacks, use template engine |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | Remove 5 hardcoded prompts + 5 fallbacks |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | Remove 7 hardcoded prompts + 7 fallbacks |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | Remove 3 hardcoded prompts + 3 fallbacks |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | Remove 7 hardcoded prompts + 7 fallbacks |
| `crates/mol-pipeline/src/stages_impl/discussion.rs` | Remove 1 hardcoded prompt + 1 fallback |
| `crates/mol-pipeline/src/runner.rs` | Periodic heartbeat spawn/cancel, construct StagePromptEngine |
| `crates/mol-pipeline/src/checkpoint.rs` | Extend `HeartbeatRecord`, update `write_heartbeat` signature |
| `crates/mol-common/src/prompts.rs` | No changes needed (existing PromptEngine sufficient) |
| `crates/mol-cli/src/commands/run.rs` | Pass `domain`/`templates_dir`/`analysis_type` to MolConfig |
| `config.mol.yaml` | Add `research.analysis_type` field |

## Testing Strategy

1. **Template rendering tests**: New test module verifying all 27 templates render without Tera errors given mock variables
2. **StagePromptEngine unit tests**: `render_prompt()` returns valid (system, user) tuple for all 26 stages; missing template returns error
3. **llm_generate error propagation**: Test that `Err` propagates correctly to stage executor, producing `StageResult::failure`
4. **Heartbeat tests**: Verify `write_heartbeat` writes correct JSON with all new fields
5. **Existing tests update**: Stage executor tests that relied on fallback behavior → mock LLM provider returning domain-relevant content. Estimate ~20-30 tests affected (those calling stage executors without LLM configured)
6. **End-to-end**: Full 26-stage pipeline with LLM provider, verify no fallback warnings in output

## Risk Mitigation

1. **Transient LLM failures now hard-fail stages**: Without fallbacks, rate limits or brief API outages cause stage failures. Mitigation: runner's `skip_noncritical` + `graceful_degradation` modes handle this at the orchestration level. Retry mechanism deferred to Phase 2.
2. **Template files missing at runtime**: If binary runs outside repo root, templates won't be found. Mitigation: `templates_dir` is configurable in `config.mol.yaml`; `StagePromptEngine::load()` returns clear error with path.
3. **Large refactoring across 6 files simultaneously**: Mitigation: refactor one phase file at a time, run tests between each.

## Phase 2 Scope (Future)

- Wire 18 HEP agent definitions (`hep/agents/`) as system prompt sources, replacing the generic role descriptions in stage templates
- Auto-inject `hep/conventions/{analysis_type}.md` into `{{ conventions }}` variable
- Integrate blinding protocol (`hep/methodology/04-blinding.md`) into runner gate mechanism at Phase 4 transitions
- Domain-specific artifact naming: contracts load expected artifact names from domain config

## Phase 3 Scope (Future)

- Domain plugin system: `templates/{domain}/stages/` directory structure
- `detect_domain()` reads from config instead of 260-line keyword table
- Venue-configurable paper format (NeurIPS checklist → HEP analysis note template)
- Contract registry loaded from `templates/{domain}/contracts.yaml`
