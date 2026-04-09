# Agent Direct-Write Pipeline Simplification

## Summary

Replace the current two-phase extraction pipeline (LLM text → parse → validate → retry → write) with a direct-write model where the ACP agent writes artifact files to disk using its Write/Bash tools. The executor checks for files on disk after the LLM call returns, with a simple fallback for API-only providers.

This aligns with the proven MoltHep architecture and the slop-X methodology spec, both of which use "agent writes files, orchestrator reads files" as the fundamental interface.

## Motivation

The current `execute_agentic()` function in `executor.rs` implements a complex extraction pipeline:

1. Phase 1: LLM free-form analysis
2. Phase 2: Per-artifact focused extraction prompt with format validation
3. Retry with increasingly explicit prompts (up to 3 attempts)
4. Format-specific validation (`is_valid_json`, `is_structured_yaml`, `is_meaningful_json`)
5. Noise stripping (`strip_llm_noise`, `strip_llm_preamble`, `is_acp_chatter`)
6. Final cleaning (`clean_artifact_output`)

This ~400 lines of extraction logic produced 19 documented friction points during E2E testing. The root cause: we capture the agent's text output and try to parse structured data from it, instead of letting the agent write files directly — which it already has the capability to do via ACP/CLI.

**Pre-merge evidence:**
- **MoltHep (Python):** Agent writes `STRATEGY.md` etc. via Write tool. Executor reads from disk. Zero extraction logic. 5 phases run cleanly.
- **slop-X methodology:** "Each session reads files, writes files, and exits. The files are the interface." All artifacts are markdown. Decision routing uses grep, not JSON parsing.

## Advantages

- **Code reduction ~60%** — executor.rs drops from ~700 to ~200 lines
- **Zero extraction friction** — no format validation failures, no Phase 2 retries, no noise filtering
- **Agent capability maximized** — agent can freely create subdirectories, multiple files, figures, etc.
- **MoltHep-proven** — identical architecture already validated across 5 phases
- **Natural code execution and figure generation** — agent runs Python via Bash tool, matplotlib/mplhep produce real plots directly in the stage directory. No separate sandbox wiring needed.
- **slop-X aligned** — matches the reference methodology's "files are the interface" principle

## Architecture

### Core Flow

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│  Prompt Engine    │────►│  ACP Agent       │────►│  Stage Directory │
│  (template +     │     │  (Claude Code)   │     │  (files on disk) │
│   output_spec)   │     │  Write/Bash tools │     │                  │
└──────────────────┘     └──────────────────┘     └──────────────────┘
                                                          │
                                                          ▼
                                                   ┌──────────────────┐
                                                   │  Executor checks │
                                                   │  files exist     │
                                                   │  on disk         │
                                                   └──────────────────┘
```

### output_spec Injection

All 52 stage templates (26 HEP + 26 generic) already contain `{{ output_spec | default(value="") }}`. The executor generates the `output_spec` variable from `ArtifactSpec` declarations:

```markdown
## MANDATORY OUTPUT

You MUST write the following files to the current working directory using the Write tool:

- `goal.md`: structured research brief — motivation, key questions, data requirements, strategy, success criteria, milestones
- `hardware_profile.json`: hardware detection results — GPU availability, CPU cores, RAM

Write substantive content to each file. Do NOT just print the content to stdout — you MUST use the Write tool to create each file on disk.
```

**Template changes required:** Generic templates (27/27) already have the `{{ output_spec }}` placeholder. HEP stage templates (26/27, all except `topic_init.md`) need a single line appended: `{{ output_spec | default(value="") }}`. This is a mechanical one-line addition per file — template content (domain knowledge, analysis instructions) is not modified.

### Simplified execute_agentic()

```rust
pub async fn execute_agentic(
    stage: Stage,
    ctx: &StageContext,
    artifact_specs: &[ArtifactSpec],
) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    std::fs::create_dir_all(&stage_dir)?;

    // Reset LLM session between stages
    if let Some(provider) = ctx.llm.as_ref() {
        let _ = provider.reset_session().await;
    }

    // Build template vars — includes output_spec with file-writing instructions
    let mut vars = ctx.template_vars(stage);
    vars.insert("output_spec".to_owned(), build_output_spec(artifact_specs));

    // Render and send prompt
    let engine = ctx.prompt_engine.as_ref()
        .ok_or("No prompt engine")?;
    let (system, user) = engine.render_prompt(stage, &vars)?;
    let response = llm_generate(ctx, &system, &user, false).await?;

    // Check which files the agent wrote to disk
    let mut produced = Vec::new();
    let mut fallback_used = false;
    for spec in artifact_specs {
        let path = stage_dir.join(&spec.filename);
        if path.exists() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            produced.push(spec.filename.clone());
        } else if !response.is_empty() && !fallback_used {
            // Fallback: agent didn't write the file, use response text
            // Only use fallback for the FIRST missing artifact to avoid
            // writing identical content to multiple files.
            let content = simple_clean(&response);
            std::fs::write(&path, &content)?;
            produced.push(spec.filename.clone());
            fallback_used = true;
        }
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: produced,
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}
```

### Simplified ArtifactSpec

```rust
pub struct ArtifactSpec {
    /// File name to write (e.g. `"goal.md"`).
    pub filename: String,
    /// Description used in the output_spec prompt section.
    pub description: String,
}
```

`ArtifactFormat` enum is deleted. The agent decides format based on the filename extension and prompt instructions.

## Artifact Format Changes

Most artifacts change from JSON/YAML to markdown. The agent writes whatever format is natural for the content.

### Artifacts that stay as-is

| Stage | Artifact | Reason |
|-------|----------|--------|
| TopicInit | `hardware_profile.json` | Programmatically consumed by downstream code |
| CodebaseSearch (fast-path) | `codebase_context.json`, `relevant_files.json` | Generated by Rust code, not LLM |

### Artifacts that change to markdown

| Stage | Before | After |
|-------|--------|-------|
| ProblemDecompose | `topic_evaluation.json` | `topic_evaluation.md` |
| SearchStrategy | `search_plan.yaml` + `queries.json` + `sources.json` | `search_plan.md` |
| LitCollect | `candidates.jsonl` | `candidates.md` |
| LitScreen | `screened_papers.jsonl` + `exclusion_reasons.json` | `screened_papers.md` |
| KnowledgeExtract | `knowledge_cards.json` + `citation_map.json` | `knowledge_cards.md` |
| Synthesis | `gap_analysis.json` | Merged into `synthesis_report.md` |
| ExperimentDesign | `exp_plan.yaml` | `exp_plan.md` |
| SanityCheck | `sanity_report.json` | `sanity_report.md` |
| ResourcePlanning | `resource_plan.json` + `schedule.json` | `resource_plan.md` |
| ExperimentRun | `run_report.json` | `run_report.md` + `figures/` (from real execution) |
| IterativeRefine | `refinement_log.json` | `refinement_log.md` |
| ResultAnalysis | `experiment_summary.json` | Merged into `analysis_report.md` |
| ResearchDecision | `decision_record.json` | `decision_record.md` |
| KnowledgeSummary | `knowledge_summary.json` | `knowledge_summary.md` |
| PeerReview | `review_comments.json` | `review_comments.md` |
| QualityGate | `quality_report.json` | `quality_report.md` |
| KnowledgeArchive | `archive_manifest.json` | `archive_manifest.md` |

### Artifacts where agent writes code directly

| Stage | Before | After |
|-------|--------|-------|
| CodeGeneration | `experiment_code.md` → strip_markdown_fences → `experiment/main.py` | Agent writes `experiment/main.py` directly |
| IterativeRefine | `refined_code.md` → strip_markdown_fences → `experiment_final/main.py` | Agent writes `experiment_final/main.py` directly |

## Decision Routing (ResearchDecision Stage)

Current approach parses `decision_record.json` for a `decision` field. New approach uses pattern matching on markdown (same as slop-X's `extract_decision`):

```rust
fn extract_decision_from_md(content: &str) -> &str {
    let lower = content.to_lowercase();
    if lower.contains("decision: proceed") || lower.contains("**proceed**") {
        "proceed"
    } else if lower.contains("decision: pivot") || lower.contains("**pivot**") {
        "pivot"
    } else if lower.contains("decision: refine") || lower.contains("**refine**") {
        "refine"
    } else {
        "proceed" // default: continue
    }
}
```

The prompt template instructs the agent to include a clear `**Decision: proceed/pivot/refine**` line.

## Experiment Execution (Stage 14: ExperimentRun)

The agent handles experiment execution directly via its Bash tool:

```
Prompt: "Run experiment/main.py. Save figures to figures/. Write run_report.md
         with execution results, metrics, and figure references."

Agent:
  1. Bash → pip install dependencies (if needed)
  2. Bash → python experiment/main.py
  3. Python code → matplotlib → figures/invariant_mass.png, figures/signal_bg.png
  4. Write → run_report.md (references figures, includes metrics)
```

No Rust-side sandbox wiring needed. The agent IS the sandbox — it runs in the stage directory with full Bash access. This is simpler and more flexible than calling `mol_experiment::LocalSandbox` from Rust, because the agent can debug failures, modify code, and retry.

**Fallback:** If no `experiment/main.py` exists from the CodeGeneration stage, the agent generates and runs code in a single session.

## Prior Artifact Reading

`template_vars()` updates the artifact filename mapping:

```rust
let artifact_files = [
    ("goal", "goal.md"),
    ("hypotheses", "hypotheses.md"),
    ("synthesis_report", "synthesis_report.md"),
    ("experiment_plan", "exp_plan.md"),          // was .yaml
    ("analysis_report", "analysis_report.md"),
    ("decision_record", "decision_record.md"),   // was .json
    ("knowledge_summary", "knowledge_summary.md"), // was .json
    ("paper_outline", "paper_outline.md"),
    ("paper_draft", "paper_draft.md"),
    ("paper_revised", "paper_revised.md"),
    ("problem_tree", "problem_tree.md"),
    ("search_plan", "search_plan.md"),           // was .yaml
    ("knowledge_cards", "knowledge_cards.md"),   // was .json
    ("sanity_report", "sanity_report.md"),       // was .json
    ("resource_plan", "resource_plan.md"),       // was .json
    ("review_comments", "review_comments.md"),   // was .json
];
```

For backward compatibility with existing runs, `read_prior_artifact()` tries `.md` first, then falls back to `.json`/`.yaml`:

```rust
fn read_prior_artifact(run_dir: &Path, filename: &str) -> Option<String> {
    // Search all stage directories for the file
    for entry in std::fs::read_dir(run_dir).ok()? {
        let path = entry.ok()?.path().join(filename);
        if path.exists() {
            return std::fs::read_to_string(&path).ok();
        }
    }
    // Fallback: try old format names (.json, .yaml)
    if filename.ends_with(".md") {
        let stem = filename.trim_end_matches(".md");
        for ext in &[".json", ".yaml", ".yml"] {
            let old_name = format!("{}{}", stem, ext);
            if let Some(content) = read_prior_artifact_exact(run_dir, &old_name) {
                return Some(content);
            }
        }
    }
    None
}
```

## Fallback for API-only Providers

When the LLM provider is API-only (no tool access), the agent cannot write files. The fallback:

```rust
if !path.exists() && !response.is_empty() {
    let content = simple_clean(&response);
    std::fs::write(&path, &content)?;
}
```

`simple_clean()` is a minimal function (~20 lines) that strips obvious LLM preamble ("Here is...", "I'll create...") from the response text. It does NOT do complex extraction, format validation, or retries. API-only mode accepts lower artifact quality as a known trade-off.

## ACP cwd Alignment

The ACP agent's working directory must be the stage directory so that `Write("goal.md", ...)` creates files in the right place. This requires passing `stage_dir` as cwd to the LLM provider.

Current `llm_generate()` does not pass cwd — it uses the global cwd from `ACPConfig`. This needs a small change:

```rust
async fn llm_generate(ctx: &StageContext, stage: Stage, system: &str, user: &str) -> Result<String> {
    let provider = ctx.llm.as_ref().ok_or("no LLM")?;
    // Set working directory to stage directory for this call
    provider.set_cwd(ctx.stage_dir(stage)).await;
    provider.generate(system, user).await
}
```

The `LlmProvider` trait gets a `set_cwd()` method (default no-op for API providers).

## Code Deleted

| Function/Type | Lines | Reason |
|---------------|-------|--------|
| `ArtifactFormat` enum | ~10 | No format distinction needed |
| `extract_artifact()` | ~110 | No Phase 2 extraction |
| `try_parse_from_analysis()` | ~40 | No Phase 1 parsing |
| `validate_artifact_format()` | ~30 | No format validation |
| `clean_artifact_output()` | ~40 | No final cleaning pass |
| `strip_llm_preamble()` | ~50 | No preamble stripping |
| `strip_llm_noise()` | ~30 | No noise stripping |
| `is_acp_chatter()` | ~30 | No chatter detection |
| `is_meaningful_json()` | ~15 | No JSON validation |
| `is_structured_yaml()` | ~15 | No YAML validation |
| `extract_json_block()` | ~25 | No JSON extraction |
| `extract_yaml_block()` | ~20 | No YAML extraction |
| `collect_json_lines()` | ~15 | No JSONL collection |
| `is_valid_json()` | ~10 | No JSON validation |
| `format_name()` | ~8 | No format names |
| **Total** | **~450** | |

## Code Added

| Function | Lines | Purpose |
|----------|-------|---------|
| `build_output_spec()` | ~15 | Generate output_spec template variable from ArtifactSpec list |
| `simple_clean()` | ~20 | Minimal LLM preamble stripping for API fallback |
| `extract_decision_from_md()` | ~15 | Grep decision from markdown for ResearchDecision routing |
| `set_cwd()` on LlmProvider | ~10 | Set working directory per stage |
| **Total** | **~60** | |

**Net: ~390 lines deleted.**

## Files Modified

| File | Change |
|------|--------|
| `crates/mol-pipeline/src/executor.rs` | Major simplification — delete extraction logic, add output_spec generation |
| `crates/mol-pipeline/src/contracts.rs` | Update artifact filename aliases (.json/.yaml → .md) with backward-compat fallback |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | Simplify ArtifactSpec declarations |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | Simplify ArtifactSpec declarations |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | Simplify ArtifactSpec declarations, ExperimentRun changes |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | Simplify ArtifactSpec declarations, decision routing |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | Simplify ArtifactSpec declarations |
| `crates/mol-llm/src/provider.rs` | Add `set_cwd()` to LlmProvider trait |
| `crates/mol-llm/src/acp.rs` | Implement `set_cwd()` for ACPClient |
| `hep/templates/stages/*.md` | Append `{{ output_spec \| default(value="") }}` to 26 files (one-line mechanical addition) |

## Files NOT Modified

- `hep/methodology/**`, `hep/conventions/**`, `hep/agents/**`, `hep/orchestration/**` — all domain knowledge files (zero changes)
- `generic/**/*` — all 34 knowledge chain files (zero changes, already have output_spec)
- `generic/templates/stages/*.md` — all 27 stage templates (zero changes, already have output_spec)
- `crates/mol-common/` — zero changes
- `crates/mol-experiment/` — zero changes (not wired from Rust; agent uses Bash directly)
- `crates/mol-cli/` — zero changes
- Pipeline runner — zero changes
- Frontend — zero changes

## Contract Alias Updates (contracts.rs)

The `artifact_satisfied()` function in `contracts.rs:228-285` maps logical artifact names to file paths. All `.json`/`.yaml` aliases must add `.md` alternatives for backward + forward compatibility:

```rust
// Before:
("knowledge_cards", &["knowledge_cards.json"]),
("decision_record", &["decision_record.json"]),
("experiment_plan", &["exp_plan.yaml"]),

// After (add .md, keep old for backward compat with existing runs):
("knowledge_cards", &["knowledge_cards.md", "knowledge_cards.json"]),
("decision_record", &["decision_record.md", "decision_record.json"]),
("experiment_plan", &["exp_plan.md", "exp_plan.yaml"]),
```

The `.md` variant is listed first so new runs match it preferentially. Old `.json`/`.yaml` files from previous runs still satisfy the contract.

## Compatibility and Friction Points

| Risk | Impact | Mitigation |
|------|--------|------------|
| Agent doesn't use Write tool | Files missing on disk | Fallback writes LLM response text; stdout fallback (MoltHep pattern) |
| ACP cwd misalignment | Files written to wrong directory | `set_cwd()` sets stage_dir per call; verify in integration test |
| Multi-artifact stage: agent writes only some files | Partial completion | Per-file fallback check; partial success is still success |
| API-only provider (no tools) | All files via fallback | `simple_clean` + write; documented quality trade-off |
| Old runs with JSON artifacts | Resume can't find .md files | `read_prior_artifact` falls back to .json/.yaml |
| Decision routing grep misfire | Wrong pipeline branch | Default to "proceed"; prompt enforces `**Decision: X**` format |
| Agent creates unexpected files | Directory clutter | Non-issue — extra files don't break anything |
| ExperimentRun Python execution fails | No figures produced | Agent captures error, writes diagnostic run_report.md |
| HEP templates missing output_spec placeholder | Agent doesn't get file-write instructions | Add `{{ output_spec \| default(value="") }}` to 26 HEP stage templates (mechanical one-liner) |

## Testing Strategy

1. **Unit tests:** `build_output_spec()`, `simple_clean()`, `extract_decision_from_md()`, `read_prior_artifact()` with fallback
2. **Integration test:** Run 2-3 stages with mock LLM provider that writes files to disk
3. **E2E test:** Full 26-stage pipeline run, verify:
   - All artifacts exist on disk
   - Markdown artifacts contain substantive content (no ACP noise)
   - ExperimentRun produces real figures
   - Decision routing works
   - Prior artifact reading works across stages
