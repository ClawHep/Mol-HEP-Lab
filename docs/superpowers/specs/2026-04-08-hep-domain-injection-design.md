# Phase 2: HEP Domain Knowledge Injection — Design Spec

**Status:** Approved  
**Date:** 2026-04-08  
**Depends on:** Phase 1 (template engine + heartbeat) — completed

## Problem

Phase 1 replaced 24 hardcoded prompts with Tera templates loaded from disk.
The templates contain inline HEP knowledge, but the pipeline still ignores
three rich knowledge sources that already exist on disk:

- **18 agent definitions** in `hep/agents/` (3130 lines) — specialized roles
  like lead-analyst, signal-lead, systematics-fitter
- **3 convention files** in `hep/conventions/` (711 lines) — extraction, search,
  unfolding analysis protocols
- **Blinding protocol** in `hep/methodology/04-blinding.md` — staged unblinding
  gates that the pipeline does not enforce

Additionally:
- `discussion.rs` still uses a 130-line `format!()` producing fake multi-agent
  dialogue — the only stage not converted to LLM-driven generation
- Stage contracts are hardcoded with generic artifact names — no domain override

## Solution Overview

Five subsystems, unified by a single `knowledge_root` path:

```
knowledge_root (default: "hep/")
├── agents/           → {{ agent_role }} variable
├── conventions/      → {{ conventions }} variable
├── methodology/      → {{ blinding_protocol }} variable
├── templates/stages/ → StagePromptEngine (Phase 1)
└── contracts.yaml    → domain-specific contract overrides
```

All injection flows through `template_vars(stage)` — the single entry point
for populating Tera template variables. No new abstraction layers.

## Detailed Design

### 1. Knowledge Root Abstraction

**MolConfig changes:**

```rust
pub struct MolConfig {
    pub topic: String,
    pub settings: HashMap<String, String>,
    pub domain: String,
    pub analysis_type: Option<String>,
    pub knowledge_root: PathBuf,    // NEW — replaces templates_dir
    // templates_dir removed; derived as knowledge_root.join("templates/stages")
}
```

Default: `knowledge_root: PathBuf::from("hep")`.

`StagePromptEngine::load()` in `runner.rs` changes from:
```rust
config.templates_dir.unwrap_or("hep/templates/stages")
```
to:
```rust
config.knowledge_root.join("templates/stages")
```

**config.mol.yaml:**
```yaml
research:
  knowledge_root: "hep"       # replaces templates_dir
  analysis_type: "search"
```

**StageContext helper methods:**

```rust
impl StageContext {
    /// Resolve a path within the domain knowledge tree.
    fn knowledge_path(&self, relative: &str) -> PathBuf {
        self.config.knowledge_root.join(relative)
    }

    /// Read a knowledge file, return None if missing.
    fn read_knowledge(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.knowledge_path(relative)).ok()
    }
}
```

### 2. Stage-Agent Mapping

A static `match` in `executor.rs` maps each stage to its primary agent:

| Phase | Stage | Agent |
|-------|-------|-------|
| 1 | TopicInit, ProblemDecompose | lead-analyst |
| 2 | SearchStrategy, LiteratureCollect, LiteratureScreen, KnowledgeExtract | investigator |
| 2 | Synthesis | theory-scout |
| 2 | HypothesisGen | lead-analyst |
| 3 | ExperimentDesign, ResourcePlanning | lead-analyst |
| 3 | CodebaseSearch, CodeGeneration, ExperimentRun | signal-lead |
| 3 | SanityCheck | cross-checker |
| 3 | IterativeRefine | systematics-fitter |
| 4 | ResultAnalysis, KnowledgeSummary | lead-analyst |
| 4 | ResearchDecision | arbiter |
| 5 | PaperOutline, PaperDraft, PaperRevision | note-writer |
| 5 | PeerReview | physics-reviewer |
| 5 | QualityGate | arbiter |
| 5 | KnowledgeArchive, ExportPublish, CitationVerify | note-writer |
| — | Discussion | None (multi-agent, uses own template) |

Function signature: `fn agent_for_stage(stage: Stage) -> Option<&'static str>`

Agent file content is read via `read_knowledge("agents/{name}.md")`, YAML
frontmatter is stripped (everything before first `---\n` after the opening
`---\n`), and the markdown body is injected as `{{ agent_role }}`.

### 3. Variable Injection in template_vars(stage)

`template_vars()` signature changes to `template_vars(&self, stage: Stage)`.

After existing artifact reads, three new injections:

```rust
// 1. Agent role
if let Some(name) = agent_for_stage(stage) {
    if let Some(raw) = self.read_knowledge(&format!("agents/{name}.md")) {
        vars.insert("agent_role".into(), strip_frontmatter(&raw));
    }
}

// 2. Conventions (by analysis_type)
let at = self.config.analysis_type.as_deref().unwrap_or("general");
if let Some(content) = self.read_knowledge(&format!("conventions/{at}.md")) {
    vars.insert("conventions".into(), content);
}

// 3. Blinding protocol
if let Some(content) = self.read_knowledge("methodology/04-blinding.md") {
    vars.insert("blinding_protocol".into(), content);
}
```

All callers of `template_vars()` (24 stage executors in phase1–5.rs + discussion.rs)
must pass `stage` — currently they all have `stage` in scope so this is a
mechanical change: `ctx.template_vars()` → `ctx.template_vars(stage)`.

### 4. Template Updates

Each of the 26 existing templates in `hep/templates/stages/` gets a
`{{ agent_role | default(value="") }}` block prepended before the existing
system prompt text. The agent_role provides the specialized persona and
methodology; the existing template text provides the stage-specific task.

Templates that already have `{{ conventions | default(value="") }}` keep it.
Templates for Phase 3+ stages gain `{{ blinding_protocol | default(value="") }}`
where relevant (experiment_design, result_analysis, quality_gate).

### 5. Discussion LLM Conversion

`discussion.rs` is converted from hardcoded `format!()` to the standard
template + LLM pattern:

1. New template `hep/templates/stages/discussion.md`:
   - System: multi-agent discussion coordinator role, instructs LLM to
     generate a structured dialogue between HEP roles (Lead Analyst,
     Experimentalist, Theorist, Critic)
   - User: injects synthesis_report, hypotheses, analysis_report via
     template variables

2. `execute_discussion()` becomes identical in structure to other stages:
   ```rust
   let vars = ctx.template_vars(stage);
   let engine = ctx.prompt_engine.as_ref()
       .ok_or_else(|| ...)?;
   let (system, user) = engine.render_prompt(stage, &vars)?;
   let result = llm_generate(ctx, &system, &user, false).await?;
   // write discussion_notes.md
   ```

3. The 130-line `format!()` block is deleted entirely.

### 6. Blinding Gate

A pre-execution check in `runner.rs` enforces the blinding protocol at the
Phase 3→4 transition:

```rust
// In the stage loop, before executing ResultAnalysis:
if stage == Stage::ResultAnalysis {
    match check_blinding_gate(run_dir, pipeline_config).await? {
        BlindingStatus::Approved => { /* continue */ }
        BlindingStatus::Blocked => {
            // Record as BlockedApproval, pipeline stops here
            return blocked_result;
        }
    }
}
```

**`check_blinding_gate()` logic:**
1. Read `{run_dir}/blinding_status.json`
2. If missing → create `{"status": "blinded", "phase": "4a_asimov"}`
3. If `status == "blinded"` and `!pipeline_config.auto_approve`:
   - Write heartbeat with status "blocked_blinding"
   - Return `BlindingStatus::Blocked`
4. If `auto_approve`:
   - Write `{"status": "approved_auto", "approved_at": timestamp}`
   - Log warning: "Blinding auto-approved — not recommended for production"
   - Return `BlindingStatus::Approved`

The blinding gate is separate from stage-internal gates (LiteratureScreen,
QualityGate) — it operates at the phase-transition level in the runner.

**Gate placement rationale:** The blinding gate fires before `ResultAnalysis`
(first Phase 4 stage) because Phases 1–3 are designed to operate on
Asimov/blinded datasets only. `IterativeRefine` (Stage 15, last Phase 3 stage)
refines experiment parameters and code — it does not access unblinded data.
The actual unblinding occurs when `ResultAnalysis` interprets real data, hence
the gate is placed at the Phase 3→4 boundary. See `methodology/04-blinding.md`
§4.1 for the blinding stage definitions.

### 7. Domain-Specific Contract Overrides

**New file: `hep/contracts.yaml`**

```yaml
# Optional overrides for stage contract artifact names.
# Only listed stages are overridden; unlisted stages keep defaults.
overrides:
  ExperimentDesign:
    expected_outputs: ["exp_plan.yaml", "success_criteria", "blinding_config.json"]
  ResultAnalysis:
    expected_outputs: ["analysis_report", "figures", "systematics_table.json"]
  IterativeRefine:
    expected_outputs: ["refined_results", "refinement_log", "experiment_final"]
```

**Loading mechanism:**

**Type change:** `StageContract` fields change from `Vec<&'static str>` to
`Vec<String>` to support both hardcoded defaults and YAML-deserialized overrides.
The existing `get_contract()` hardcoded literals use `.to_owned()`. This is a
breaking API change — `validate_inputs()` and `validate_outputs()` also update
to accept `String` slices.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageContract {
    #[serde(default)]
    pub required_inputs: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
}

pub struct ContractOverrides {
    overrides: HashMap<String, StageContract>,
}

impl ContractOverrides {
    pub fn load(knowledge_root: &Path) -> Self { ... }
    pub fn get(&self, stage: Stage) -> Option<&StageContract> { ... }
}
```

`get_contract()` changes to `get_contract(stage, overrides: Option<&ContractOverrides>)`.
`validate_inputs()` and `validate_outputs()` gain an `overrides` parameter
that flows through to `get_contract()`. Runner loads overrides once at startup
(alongside StagePromptEngine), passes to all contract validation calls.

**Backward compatible:** if `contracts.yaml` is missing, all contracts use
existing hardcoded defaults.

### 8. Artifact Alias Updates

`contracts.rs` alias table gains new HEP-specific aliases to match domain
artifact names:

```rust
("blinding_config", &["blinding_config.json", "blinding_status.json"]),
("systematics_table", &["systematics_table.json", "systematics.json"]),
("experiment_final", &["experiment_final/", "experiment/"]),
```

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `crates/mol-pipeline/src/executor.rs` | Modify | Add `knowledge_root` to MolConfig (replace `templates_dir`), add `agent_for_stage()`, add `knowledge_path()`/`read_knowledge()`, change `template_vars()` → `template_vars(stage)`, add `strip_frontmatter()` |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | Modify | `ctx.template_vars()` → `ctx.template_vars(stage)` (2 sites) |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | Modify | Same change (5 sites) |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | Modify | Same change (7 sites) |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | Modify | Same change (3 sites) |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | Modify | Same change (7 sites) |
| `crates/mol-pipeline/src/stages_impl/discussion.rs` | Modify | Replace `format!()` with template + LLM pattern |
| `crates/mol-pipeline/src/runner.rs` | Modify | Load from `knowledge_root`, add blinding gate check, load contract overrides, define `BlindingStatus` enum |
| `crates/mol-pipeline/src/contracts.rs` | Modify | Change `StageContract` to `Vec<String>`, add `ContractOverrides`, change `get_contract()`/`validate_inputs()`/`validate_outputs()` signatures, add aliases |
| `crates/mol-pipeline/src/checkpoint.rs` | Modify | Add "blocked_blinding" heartbeat status |
| `crates/mol-pipeline/src/lib.rs` | Modify | Update public re-exports if contract API changes |
| `crates/mol-config/src/types.rs` | Modify | Replace `templates_dir` with `knowledge_root` in ResearchConfig |
| `crates/mol-cli/src/commands/run.rs` | Modify | Wire `knowledge_root` |
| `hep/templates/stages/*.md` | Modify | Add `{{ agent_role }}` and `{{ blinding_protocol }}` placeholders |
| `hep/templates/stages/discussion.md` | Create | New discussion template |
| `hep/contracts.yaml` | Create | Domain contract overrides |
| `config.mol.yaml` | Modify | Replace `templates_dir` with `knowledge_root` |

## Call-Site Counts

- `template_vars()` → `template_vars(stage)`: **24 mechanical changes** in phase1–5.rs. `discussion.rs` is a full rewrite (Section 5) and gains 1 new `template_vars(stage)` call.
- `agent_for_stage()` mapping: **27 match arms** (26 returning `Some(agent_name)` + 1 returning `None` for Discussion)
- Template `{{ agent_role }}` addition: **26 templates** (including new discussion.md)
- Template `{{ blinding_protocol }}` addition: **3 templates** (experiment_design, result_analysis, quality_gate)

## Testing Strategy

1. **Unit tests for `agent_for_stage()`**: every Stage variant has a mapping (Some or None for Discussion)
2. **Unit tests for `strip_frontmatter()`**: with/without frontmatter, empty input
3. **Unit tests for `template_vars(stage)`**: verify agent_role, conventions, blinding_protocol populated
4. **Unit test for blinding gate**: blinded → blocked, auto_approve → approved
5. **Unit test for `ContractOverrides::load()`**: with/without YAML, override merging
6. **Integration: discussion.rs** fails without engine (same pattern as other stages)
7. **All existing 130 tests must continue to pass**

## Risk Mitigation

1. **Large agent files inflate prompt tokens**: Agent files average 170 lines.
   Mitigation: only one agent injected per stage (not all 18). Future: add
   `max_agent_tokens` config to truncate if needed.
2. **Missing knowledge files at runtime**: All injections use
   `read_knowledge()` which returns `Option` — missing files produce empty
   template variables, not errors.
3. **`templates_dir` → `knowledge_root` migration**: The old `templates_dir`
   field is removed. `config.mol.yaml` must update. Compile error if old
   field used anywhere.
4. **Blinding gate blocks CI**: `auto_approve: true` bypasses the gate with
   a logged warning. Test pipelines always use `auto_approve`.
5. **Convention file mismatch**: If `analysis_type` is set but the
   corresponding `conventions/{type}.md` does not exist, `read_knowledge()`
   returns `None` and the template variable is empty. A `warn!()` is logged
   so users can discover typos. Available types: extraction, search, unfolding.
