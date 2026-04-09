# Agent Direct-Write Pipeline Simplification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the two-phase extraction pipeline with a direct-write model where agents write files to disk, reducing executor.rs by ~60% and eliminating all 19 documented friction points.

**Architecture:** Agent receives `output_spec` in prompt telling it which files to write. After LLM call, executor checks disk for files. Fallback writes response text for API-only providers. Decision routing uses markdown grep instead of JSON parsing.

**Tech Stack:** Rust (mol-pipeline, mol-llm crates), Tera templates, ACP/acpx CLI

---

## File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `crates/mol-pipeline/src/executor.rs` | Core executor — ArtifactSpec, execute_agentic, helpers | Major rewrite (delete ~450 lines, add ~60) |
| `crates/mol-pipeline/src/contracts.rs:228-285` | Artifact filename aliases for contract validation | Update aliases (.md added) |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | TopicInit, ProblemDecompose | Simplify ArtifactSpec |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | SearchStrategy through HypothesisGen | Simplify ArtifactSpec |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | ExperimentDesign through IterativeRefine | Simplify ArtifactSpec + ExperimentRun |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | ResultAnalysis, ResearchDecision, KnowledgeSummary | Simplify + decision routing |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | PaperOutline through CitationVerify + KnowledgeArchive | Simplify ArtifactSpec |
| `crates/mol-pipeline/src/stages_impl/discussion.rs` | Discussion stage | Simplify ArtifactSpec |
| `crates/mol-llm/src/provider.rs` | LlmProvider trait | Add `set_cwd()` method |
| `crates/mol-llm/src/acp.rs` | ACPClient | Implement `set_cwd()` |
| `hep/templates/stages/*.md` (26 files) | HEP stage prompt templates | Append output_spec placeholder |

---

### Task 1: Add output_spec placeholder to HEP templates

**Files:**
- Modify: `hep/templates/stages/problem_decompose.md` (and 25 others)

- [ ] **Step 1: Verify which HEP templates lack the placeholder**

Run: `grep -rL 'output_spec' hep/templates/stages/`
Expected: 26 files listed (all except topic_init.md)

- [ ] **Step 2: Append placeholder to all 26 files**

For each file missing `{{ output_spec }}`, append one blank line + the placeholder at the end:

```bash
for f in $(grep -rL 'output_spec' hep/templates/stages/); do
  echo '' >> "$f"
  echo '{{ output_spec | default(value="") }}' >> "$f"
done
```

- [ ] **Step 3: Verify all templates now have the placeholder**

Run: `grep -rL 'output_spec' hep/templates/stages/`
Expected: No output (all files have it)

- [ ] **Step 4: Verify template rendering still works (Rust tests)**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: All existing tests pass (placeholder renders as empty string when output_spec not provided)

- [ ] **Step 5: Commit**

```bash
git add hep/templates/stages/
git commit -m "feat: add output_spec placeholder to all HEP stage templates"
```

---

### Task 2: Add set_cwd() to LlmProvider trait and implement for ACP

**Files:**
- Modify: `crates/mol-llm/src/provider.rs:27-42`
- Modify: `crates/mol-llm/src/acp.rs:20-37` (ACPConfig)

- [ ] **Step 1: Add set_cwd to LlmProvider trait**

In `crates/mol-llm/src/provider.rs`, add to the `LlmProvider` trait (after `reset_session`):

```rust
    /// Set working directory for the next LLM call.
    /// CLI/ACP: changes subprocess cwd. API: no-op.
    async fn set_cwd(&self, _path: std::path::PathBuf) {
        // Default no-op for API providers
    }
```

- [ ] **Step 2: Implement set_cwd for CliProvider**

In `crates/mol-llm/src/provider.rs`, add to `impl LlmProvider for CliProvider`:

```rust
    async fn set_cwd(&self, path: std::path::PathBuf) {
        let mut client = self.inner.lock().await;
        client.config.cwd = path;
    }
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p mol-llm 2>&1 | tail -5`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/mol-llm/src/provider.rs
git commit -m "feat: add set_cwd() to LlmProvider trait for per-stage working directory"
```

---

### Task 3: Simplify ArtifactSpec and add new helper functions in executor.rs

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:317-347` (ArtifactSpec + ArtifactFormat)
- Modify: `crates/mol-pipeline/src/executor.rs:770-796` (llm_generate)

- [ ] **Step 1: Write tests for the new helper functions**

Add to the `#[cfg(test)] mod tests` section at the bottom of `executor.rs`:

```rust
    #[test]
    fn build_output_spec_single_artifact() {
        let specs = vec![ArtifactSpec {
            filename: "goal.md".into(),
            description: "research goal statement".into(),
        }];
        let spec = build_output_spec(&specs);
        assert!(spec.contains("MANDATORY OUTPUT"));
        assert!(spec.contains("`goal.md`"));
        assert!(spec.contains("research goal statement"));
        assert!(spec.contains("Write tool"));
    }

    #[test]
    fn build_output_spec_multiple_artifacts() {
        let specs = vec![
            ArtifactSpec { filename: "a.md".into(), description: "first".into() },
            ArtifactSpec { filename: "b.json".into(), description: "second".into() },
        ];
        let spec = build_output_spec(&specs);
        assert!(spec.contains("`a.md`"));
        assert!(spec.contains("`b.json`"));
    }

    #[test]
    fn simple_clean_strips_preamble() {
        assert_eq!(simple_clean("Here is the content:\n\nActual content"), "Actual content");
        assert_eq!(simple_clean("I'll create the file.\n\n# Title\nBody"), "# Title\nBody");
        assert_eq!(simple_clean("# Already clean\nBody"), "# Already clean\nBody");
    }

    #[test]
    fn simple_clean_preserves_clean_text() {
        let clean = "# Research Goal\n\nThis is the goal.";
        assert_eq!(simple_clean(clean), clean);
    }

    #[test]
    fn extract_decision_from_md_finds_proceed() {
        assert_eq!(extract_decision_from_md("**Decision: proceed**\nWe should continue."), "proceed");
        assert_eq!(extract_decision_from_md("The decision is to **proceed** with writing."), "proceed");
    }

    #[test]
    fn extract_decision_from_md_finds_pivot() {
        assert_eq!(extract_decision_from_md("**Decision: pivot**\nNew direction needed."), "pivot");
    }

    #[test]
    fn extract_decision_from_md_defaults_to_proceed() {
        assert_eq!(extract_decision_from_md("No clear decision mentioned here."), "proceed");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mol-pipeline build_output_spec simple_clean extract_decision 2>&1 | tail -10`
Expected: FAIL (functions don't exist yet)

- [ ] **Step 3: Replace ArtifactSpec and ArtifactFormat**

Replace the `ArtifactSpec` struct and `ArtifactFormat` enum at `executor.rs:317-347` with:

```rust
/// Describes one artifact that a stage must produce.
///
/// The executor injects these into the prompt's `{{ output_spec }}` variable,
/// instructing the agent to write each file to disk using its Write tool.
#[derive(Debug, Clone)]
pub struct ArtifactSpec {
    /// File name to write (e.g. `"goal.md"`).
    pub filename: String,
    /// Description shown in the output_spec prompt section.
    pub description: String,
}
```

- [ ] **Step 4: Add build_output_spec function**

Add after the `ArtifactSpec` definition:

```rust
/// Generate the `output_spec` template variable content from artifact specs.
///
/// Produces a markdown section telling the agent which files to write.
fn build_output_spec(specs: &[ArtifactSpec]) -> String {
    if specs.is_empty() {
        return String::new();
    }
    let files: Vec<String> = specs
        .iter()
        .map(|s| format!("- `{}`: {}", s.filename, s.description))
        .collect();
    format!(
        "## MANDATORY OUTPUT\n\n\
         You MUST write the following files to the current working directory \
         using the Write tool:\n\n{}\n\n\
         Write substantive content to each file. Do NOT just print the content \
         to stdout — you MUST use the Write tool to create each file on disk.\n\n\
         If a file requires code (e.g. `.py`), write executable Python code directly. \
         If a file requires figures, run the code via Bash and save figures to `figures/`.",
        files.join("\n")
    )
}
```

- [ ] **Step 5: Add simple_clean function**

```rust
/// Minimal cleaning for API-only fallback — strips obvious LLM preamble.
///
/// Unlike the deleted `strip_llm_preamble`, this is intentionally simple:
/// just drop leading lines that are clearly meta-commentary.
fn simple_clean(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    // Drop leading preamble lines
    while let Some(first) = lines.first() {
        let t = first.trim();
        if t.is_empty()
            || t.starts_with("Here is")
            || t.starts_with("Here's")
            || t.starts_with("I'll ")
            || t.starts_with("I will")
            || t.starts_with("I've ")
            || t.starts_with("Let me")
            || t.starts_with("Sure,")
            || t.starts_with("OK,")
            || t.starts_with("Okay,")
        {
            lines.remove(0);
        } else {
            break;
        }
    }
    // Drop trailing empty lines
    while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}
```

- [ ] **Step 6: Add extract_decision_from_md function**

```rust
/// Extract decision keyword from markdown content (slop-X style grep).
///
/// Looks for "Decision: proceed/pivot/refine" or bold markers.
/// Defaults to "proceed" if no match found.
fn extract_decision_from_md(content: &str) -> &'static str {
    let lower = content.to_lowercase();
    if lower.contains("decision: pivot") || lower.contains("**pivot**") {
        "pivot"
    } else if lower.contains("decision: refine") || lower.contains("**refine**") {
        "refine"
    } else if lower.contains("decision: stop") || lower.contains("**stop**") {
        "stop"
    } else {
        "proceed"
    }
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p mol-pipeline build_output_spec simple_clean extract_decision 2>&1 | tail -10`
Expected: All 7 new tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/mol-pipeline/src/executor.rs
git commit -m "feat: add build_output_spec, simple_clean, extract_decision_from_md helpers"
```

---

### Task 4: Rewrite execute_agentic() — the core change

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:349-428` (execute_agentic)
- Modify: `crates/mol-pipeline/src/executor.rs:770-796` (llm_generate — add stage param for cwd)

- [ ] **Step 1: Update llm_generate to accept stage parameter for cwd**

Replace the current `llm_generate` function at line ~778 with:

```rust
/// Call LLM with a system/user prompt pair.
///
/// Sets the provider's working directory to the stage directory so the agent's
/// Write tool creates files in the correct location.
pub async fn llm_generate(
    ctx: &StageContext,
    stage: Stage,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let llm = ctx.llm.as_ref()
        .ok_or_else(|| anyhow::anyhow!("No LLM provider configured"))?;

    // Set cwd to stage directory so agent writes files there
    llm.set_cwd(ctx.stage_dir(stage)).await;

    let messages = vec![
        mol_llm::Message::system(system_prompt),
        mol_llm::Message::user(user_prompt),
    ];
    let resp = llm.chat(&messages, false).await
        .context("LLM chat call failed")?;

    Ok(resp.content)
}
```

Note: We no longer strip noise or markdown fences from the response — the agent writes files directly. The response text is only used as fallback.

- [ ] **Step 2: Replace execute_agentic with the simplified version**

Replace the current `execute_agentic` function (lines ~358-428) with:

```rust
/// Execute a stage by sending a prompt and letting the agent write files to disk.
///
/// The agent receives `output_spec` in the prompt telling it which files to create.
/// After the LLM call, the executor checks disk for each expected file.
/// Falls back to writing the response text for API-only providers.
pub async fn execute_agentic(
    stage: Stage,
    ctx: &StageContext,
    artifact_specs: &[ArtifactSpec],
) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = std::fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Reset LLM session to prevent context leakage between stages
    if let Some(provider) = ctx.llm.as_ref() {
        if let Err(e) = provider.reset_session().await {
            tracing::warn!(stage = %stage.name(), "Failed to reset LLM session: {e}");
        }
    }

    // Build template vars — includes output_spec with file-writing instructions
    let mut vars = ctx.template_vars(stage);
    vars.insert("output_spec".to_owned(), build_output_spec(artifact_specs));

    // Render the prompt
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => return StageResult::failure(stage, format!("No prompt engine for {}", stage.name())),
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => return StageResult::failure(stage, format!("Template render: {e}")),
    };

    // Send prompt — agent writes files to stage_dir via its Write/Bash tools
    let response = match llm_generate(ctx, stage, &system, &user).await {
        Ok(r) => r,
        Err(e) => return StageResult::failure(stage, format!("{}: {e}", stage.name())),
    };

    // Check which files the agent wrote to disk
    let mut produced = Vec::new();
    let mut fallback_used = false;
    for spec in artifact_specs {
        let path = stage_dir.join(&spec.filename);
        if path.exists() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            produced.push(spec.filename.clone());
            info!(stage = %stage.name(), artifact = %spec.filename, "Artifact written by agent");
        } else if !response.is_empty() && !fallback_used {
            // Fallback: agent didn't write the file — use response text
            let content = simple_clean(&response);
            if let Err(e) = std::fs::write(&path, &content) {
                return StageResult::failure(stage, format!("write {}: {e}", spec.filename));
            }
            produced.push(spec.filename.clone());
            fallback_used = true;
            tracing::warn!(
                stage = %stage.name(),
                artifact = %spec.filename,
                "Agent did not write file; used response text fallback"
            );
        } else {
            tracing::warn!(
                stage = %stage.name(),
                artifact = %spec.filename,
                "Artifact not produced and no fallback available"
            );
        }
    }

    StageResult {
        stage,
        status: if produced.is_empty() { StageStatus::Failed } else { StageStatus::Done },
        artifacts: produced,
        error: if produced.is_empty() { Some("No artifacts produced".into()) } else { None },
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}
```

- [ ] **Step 3: Delete all extraction/validation/noise functions**

Delete these functions from executor.rs (they are no longer called):
- `extract_artifact()` (~line 435-544)
- `try_parse_from_analysis()` (~line 547-583)
- `validate_artifact_format()` (~line 586-612)
- `strip_llm_preamble()` (~line 614-674)
- `is_acp_chatter()` (~line 677-715)
- `format_name()` (~line 717-724)
- `clean_artifact_output()` (~line 726-768)
- `llm_generate_json()` (~line 809-845)
- `llm_generate_jsonl()` (~line 850-879)
- `ArtifactFormat` enum (~line 340-347)
- `is_valid_json()`, `is_meaningful_json()`, `extract_json_block()`, `collect_json_lines()`, `is_structured_yaml()`, `extract_yaml_block()`
- `strip_llm_noise()` (keep `strip_markdown_fences` — still used for code extraction)

Also remove the old `llm_generate` (the 4-param version with `json_mode`).

- [ ] **Step 4: Update template_vars artifact filename mapping**

In `template_vars()` (~line 237-258), update the artifact_files array:

```rust
        let artifact_files = [
            ("goal", "goal.md"),
            ("hypotheses", "hypotheses.md"),
            ("synthesis_report", "synthesis_report.md"),
            ("experiment_plan", "exp_plan.md"),
            ("analysis_report", "analysis_report.md"),
            ("decision_record", "decision_record.md"),
            ("knowledge_summary", "knowledge_summary.md"),
            ("paper_outline", "paper_outline.md"),
            ("paper_draft", "paper_draft.md"),
            ("paper_revised", "paper_revised.md"),
            ("problem_tree", "problem_tree.md"),
            ("search_plan", "search_plan.md"),
            ("knowledge_cards", "knowledge_cards.md"),
            ("sanity_report", "sanity_report.md"),
            ("resource_plan", "resource_plan.md"),
            ("review_comments", "review_comments.md"),
            ("topic_evaluation", "topic_evaluation.md"),
            ("run_report", "run_report.md"),
        ];
```

- [ ] **Step 5: Update read_prior_artifact for backward compatibility**

Update `read_prior_artifact` (or `read_prior_artifact_pub`) to try `.md` first, then fall back to old formats:

```rust
/// Read a prior artifact, trying new .md format first, then old .json/.yaml.
pub fn read_prior_artifact(run_dir: &Path, filename: &str) -> Option<String> {
    // First try the exact filename
    if let Some(content) = read_prior_artifact_exact(run_dir, filename) {
        return Some(content);
    }
    // Backward compat: if looking for .md, try .json and .yaml
    if filename.ends_with(".md") {
        let stem = filename.trim_end_matches(".md");
        for ext in &[".json", ".yaml", ".yml", ".jsonl"] {
            let old_name = format!("{stem}{ext}");
            if let Some(content) = read_prior_artifact_exact(run_dir, &old_name) {
                return Some(content);
            }
        }
    }
    None
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p mol-pipeline 2>&1 | tail -20`
Expected: Compilation errors from stage files still referencing old ArtifactFormat — this is expected, fixed in Tasks 5-8.

- [ ] **Step 7: Commit (work in progress — won't compile until stages updated)**

```bash
git add crates/mol-pipeline/src/executor.rs
git commit -m "feat: rewrite execute_agentic to agent-direct-write model

Delete ~450 lines of extraction/validation/noise logic.
Agent now writes files to disk via Write tool.
Executor checks disk after LLM call with simple fallback."
```

---

### Task 5: Simplify Phase 1 stages (TopicInit, ProblemDecompose)

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase1.rs:16-87`

- [ ] **Step 1: Update execute_topic_init**

Replace the ArtifactSpec to use new simplified struct (remove `format`, `schema_hint`):

```rust
pub async fn execute_topic_init(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "goal.md".into(),
            description: "structured research brief — motivation, key questions, data \
                requirements, initial strategy, success criteria, and milestones".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    // ... rest unchanged (hardware_profile.json code-driven section stays)
```

- [ ] **Step 2: Update execute_problem_decompose**

```rust
pub async fn execute_problem_decompose(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "problem_tree.md".into(),
            description: "hierarchical problem decomposition — sub-problems with scope, \
                approach, dependencies, complexity estimates, and required tools".into(),
        },
        ArtifactSpec {
            filename: "topic_evaluation.md".into(),
            description: "topic feasibility evaluation — scores for feasibility, novelty, \
                impact, resource requirements, estimated timeline, key challenges".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 3: Remove `use ArtifactFormat` import**

Remove `ArtifactFormat` from the import line.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p mol-pipeline 2>&1 | grep "phase1" | head -5`
Expected: No errors for phase1.rs

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/phase1.rs
git commit -m "feat: simplify Phase 1 ArtifactSpecs for direct-write model"
```

---

### Task 6: Simplify Phase 2 stages (Search through HypothesisGen)

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase2.rs`

- [ ] **Step 1: Update execute_search_strategy**

```rust
pub async fn execute_search_strategy(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "search_plan.md".into(),
            description: "literature search strategy — databases, search terms, \
                inclusion/exclusion criteria, sources to query, and structured queries".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 2: Update execute_literature_collect**

```rust
pub async fn execute_literature_collect(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "candidates.md".into(),
            description: "literature candidates — for each paper: title, authors, year, \
                venue, abstract summary, and relevance score (0-1). Use markdown tables \
                or structured sections.".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 3: Update execute_literature_screen**

The LiteratureScreen stage currently has code-driven JSONL parsing logic. Since the upstream artifact is now markdown, simplify to agentic:

```rust
pub async fn execute_literature_screen(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "screened_papers.md".into(),
            description: "screened literature — papers that passed relevance screening with \
                inclusion rationale. Papers below threshold listed with exclusion reasons.".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // GATE: block for approval if not auto-approved
    if !ctx.auto_approve_gates {
        result.status = StageStatus::BlockedApproval;
        result.decision = "awaiting_approval".to_owned();
    }

    result
}
```

- [ ] **Step 4: Update execute_knowledge_extract**

```rust
pub async fn execute_knowledge_extract(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "knowledge_cards.md".into(),
            description: "knowledge cards extracted from literature — each card has: source, \
                category, key finding, numerical values with uncertainties, and applicability".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 5: Update execute_synthesis**

```rust
pub async fn execute_synthesis(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "synthesis_report.md".into(),
            description: "literature synthesis — key themes, methodological trends, consensus, \
                contradictions, identified research gaps with severity and priority".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 6: Update execute_hypothesis_gen**

```rust
pub async fn execute_hypothesis_gen(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "hypotheses.md".into(),
            description: "testable research hypotheses — each with statement, rationale, \
                proposed test, expected outcome, and falsification criteria".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 7: Remove old imports and dead code**

Remove `use crate::executor::{ArtifactFormat, ...}` and `use crate::executor::utcnow_iso` if no longer needed. Remove the old `execute_literature_screen` code-driven JSONL parsing.

- [ ] **Step 8: Update tests**

Update `literature_screen_blocks_without_auto_approve` test — it should still work since the gate logic is preserved. The `literature_screen_passes_with_auto_approve` test needs updating since it previously relied on code-driven JSONL creation (now requires prompt engine).

- [ ] **Step 9: Verify compilation**

Run: `cargo check -p mol-pipeline 2>&1 | grep "phase2" | head -5`
Expected: No errors

- [ ] **Step 10: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/phase2.rs
git commit -m "feat: simplify Phase 2 stages — markdown artifacts, agentic screening"
```

---

### Task 7: Simplify Phase 3 stages (ExperimentDesign through IterativeRefine)

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase3.rs`

- [ ] **Step 1: Update execute_experiment_design**

```rust
pub async fn execute_experiment_design(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![
        ArtifactSpec {
            filename: "exp_plan.md".into(),
            description: "experiment design plan — methodology, variables, controls, \
                datasets, evaluation metrics, and success criteria".into(),
        },
    ];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // GATE: block for approval if not auto-approved
    if !ctx.auto_approve_gates {
        result.status = StageStatus::BlockedApproval;
        result.decision = "awaiting_approval".to_owned();
    }

    result
}
```

- [ ] **Step 2: Keep execute_codebase_search fast-path unchanged**

The CodebaseSearch fast-path (no codebases → write default JSON) stays as-is since it's code-driven, not LLM-driven. Only update the LLM path's ArtifactSpec:

```rust
    let specs = vec![
        ArtifactSpec {
            filename: "codebase_context.json".into(),
            description: "codebase analysis — relevant frameworks, patterns, dependencies".into(),
        },
        ArtifactSpec {
            filename: "relevant_files.json".into(),
            description: "relevant files list — paths with relevance and descriptions".into(),
        },
    ];
```

- [ ] **Step 3: Update execute_code_generation**

Agent now writes `experiment/main.py` directly — no more `experiment_code.md` → strip_markdown_fences:

```rust
pub async fn execute_code_generation(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let stage_dir = ctx.stage_dir(stage);
    let experiment_dir = stage_dir.join("experiment");
    let _ = fs::create_dir_all(&experiment_dir);

    let specs = vec![
        ArtifactSpec {
            filename: "experiment_spec.md".into(),
            description: "experiment specification — entry point, dependencies, how to run, \
                and experiment code overview".into(),
        },
        ArtifactSpec {
            filename: "experiment/main.py".into(),
            description: "complete experiment code — executable Python script implementing \
                data loading, analysis/training, evaluation, and result/figure output. \
                Save figures to figures/ directory.".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 4: Update execute_sanity_check and execute_resource_planning**

```rust
pub async fn execute_sanity_check(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};
    let specs = vec![ArtifactSpec {
        filename: "sanity_report.md".into(),
        description: "sanity check — code review, dependency check, potential issues, \
            and overall pass/fail assessment".into(),
    }];
    execute_agentic(stage, ctx, &specs).await
}

pub async fn execute_resource_planning(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};
    let specs = vec![ArtifactSpec {
        filename: "resource_plan.md".into(),
        description: "resource plan — compute requirements, storage, GPU hours, cost \
            estimates, milestones, and schedule".into(),
    }];
    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 5: Update execute_experiment_run — agent executes code directly**

```rust
pub async fn execute_experiment_run(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let specs = vec![ArtifactSpec {
        filename: "run_report.md".into(),
        description: "experiment run report — execute experiment/main.py using Bash, \
            capture metrics, save figures to figures/ directory, and document results \
            including any errors. If experiment/main.py exists from a prior stage, \
            run it. Otherwise generate and run the experiment code.".into(),
    }];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 6: Update execute_iterative_refine**

```rust
pub async fn execute_iterative_refine(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};

    let stage_dir = ctx.stage_dir(stage);
    let _ = fs::create_dir_all(stage_dir.join("experiment_final"));

    let specs = vec![
        ArtifactSpec {
            filename: "refinement_log.md".into(),
            description: "iterative refinement log — changes made, metrics before/after, \
                convergence status, and remaining issues".into(),
        },
        ArtifactSpec {
            filename: "experiment_final/main.py".into(),
            description: "refined experiment code — improved Python code with all \
                refinements applied. Must be directly executable.".into(),
        },
    ];

    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 7: Remove old imports and dead code (ArtifactFormat, strip_markdown_fences calls)**

- [ ] **Step 8: Verify compilation**

Run: `cargo check -p mol-pipeline 2>&1 | grep "phase3" | head -5`
Expected: No errors

- [ ] **Step 9: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/phase3.rs
git commit -m "feat: simplify Phase 3 stages — agent writes code/figures directly"
```

---

### Task 8: Simplify Phase 4 stages + decision routing

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase4.rs`

- [ ] **Step 1: Update execute_result_analysis**

```rust
pub async fn execute_result_analysis(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};
    let specs = vec![ArtifactSpec {
        filename: "analysis_report.md".into(),
        description: "experiment result analysis — statistical analysis, comparison with \
            baselines, significance tests, key findings, and primary metrics summary".into(),
    }];
    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 2: Update execute_research_decision with markdown grep routing**

```rust
pub async fn execute_research_decision(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic, extract_decision_from_md};

    let specs = vec![ArtifactSpec {
        filename: "decision_record.md".into(),
        description: "research decision record — decision (proceed/pivot/refine/stop), rationale, \
            confidence, alternatives considered, and next steps. \
            IMPORTANT: Include a line: **Decision: proceed** (or pivot/refine/stop)".into(),
    }];

    let mut result = execute_agentic(stage, ctx, &specs).await;
    if result.status != StageStatus::Done {
        return result;
    }

    // Extract decision from markdown (slop-X style grep)
    let stage_dir = ctx.stage_dir(stage);
    if let Ok(content) = fs::read_to_string(stage_dir.join("decision_record.md")) {
        result.decision = extract_decision_from_md(&content).to_owned();
    }

    result
}
```

- [ ] **Step 3: Update execute_knowledge_summary**

```rust
pub async fn execute_knowledge_summary(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};
    let specs = vec![ArtifactSpec {
        filename: "knowledge_summary.md".into(),
        description: "knowledge summary — key findings, validated hypotheses, methodology \
            insights, limitations, and recommendations for future work".into(),
    }];
    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 4: Remove old imports**

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p mol-pipeline 2>&1 | grep "phase4" | head -5`

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/phase4.rs
git commit -m "feat: simplify Phase 4 — markdown decision routing, no JSON parsing"
```

---

### Task 9: Simplify Phase 5 stages + Discussion + KnowledgeArchive

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase5.rs`
- Modify: `crates/mol-pipeline/src/stages_impl/discussion.rs`

- [ ] **Step 1: Update Phase 5 agentic stages**

Update `execute_paper_outline`, `execute_paper_draft`, `execute_paper_revision`, `execute_export_publish` — remove `ArtifactFormat` and `schema_hint` from all specs. These are all already markdown, so just simplify the struct.

Update `execute_peer_review`:
```rust
    let specs = vec![ArtifactSpec {
        filename: "review_comments.md".into(),
        description: "peer review comments — simulated multi-reviewer feedback with \
            per-section comments, severity ratings, and overall assessment".into(),
    }];
```

Update `execute_quality_gate`:
```rust
    let specs = vec![ArtifactSpec {
        filename: "quality_report.md".into(),
        description: "quality gate report — overall quality score (1-10), per-section \
            scores, identified issues, and pass/fail determination".into(),
    }];
```

Update `execute_citation_verify`:
```rust
    let specs = vec![ArtifactSpec {
        filename: "verification_report.md".into(),
        description: "citation verification — each citation checked for existence and \
            correctness, with overall verification status".into(),
    }];
```

- [ ] **Step 2: Update execute_knowledge_archive**

Update the `artifact_names` lookup array to use new filenames:

```rust
    let artifact_names = [
        ("goal.md", "Research goal definition"),
        ("hypotheses.md", "Research hypotheses"),
        ("synthesis_report.md", "Literature synthesis report"),
        ("knowledge_cards.md", "Extracted knowledge cards"),
        ("exp_plan.md", "Experiment plan"),
        ("analysis_report.md", "Result analysis report"),
        ("paper_final.md", "Final paper"),
        ("knowledge_summary.md", "Knowledge summary"),
    ];
```

Also update the `read_prior_artifact_pub` call to use `"knowledge_summary.md"` instead of `"knowledge_summary.json"`.

- [ ] **Step 3: Update discussion.rs**

```rust
pub async fn execute_discussion(stage: Stage, ctx: &StageContext) -> StageResult {
    use crate::executor::{ArtifactSpec, execute_agentic};
    let specs = vec![ArtifactSpec {
        filename: "discussion_notes.md".into(),
        description: "multi-perspective discussion transcript — structured debate covering \
            methodology, result interpretation, implications, and future directions".into(),
    }];
    execute_agentic(stage, ctx, &specs).await
}
```

- [ ] **Step 4: Remove old imports from both files**

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p mol-pipeline 2>&1 | tail -10`
Expected: Clean compilation — all stages updated

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/phase5.rs crates/mol-pipeline/src/stages_impl/discussion.rs
git commit -m "feat: simplify Phase 5 + Discussion stages for direct-write model"
```

---

### Task 10: Update contract aliases in contracts.rs

**Files:**
- Modify: `crates/mol-pipeline/src/contracts.rs:228-285`

- [ ] **Step 1: Write test for new aliases**

Add to the tests module in contracts.rs:

```rust
    #[test]
    fn new_md_aliases_satisfy_contracts() {
        // New .md artifacts should satisfy logical contract names
        let md_artifacts = vec!["knowledge_cards.md".into(), "synthesis_report.md".into()];
        assert!(artifact_satisfied("knowledge_cards", &md_artifacts));
        assert!(artifact_satisfied("synthesis_report", &md_artifacts));
    }

    #[test]
    fn old_json_aliases_still_satisfy_contracts() {
        // Backward compat: old .json artifacts from previous runs
        let old_artifacts = vec!["knowledge_cards.json".into(), "decision_record.json".into()];
        assert!(artifact_satisfied("knowledge_cards", &old_artifacts));
        assert!(artifact_satisfied("decision_record", &old_artifacts));
    }
```

- [ ] **Step 2: Run tests to verify they fail for .md**

Run: `cargo test -p mol-pipeline new_md_aliases 2>&1 | tail -5`
Expected: FAIL (`.md` not in alias list yet)

- [ ] **Step 3: Update the aliases array**

Update each alias entry to add `.md` variant first, keeping old formats for backward compatibility:

```rust
    let aliases: &[(&str, &[&str])] = &[
        // Phase 1: Strategy
        ("topic_brief", &["goal.md"]),
        ("research_questions", &["goal.md"]),
        ("problem_tree", &["problem_tree.md", "topic_evaluation.md", "topic_evaluation.json"]),
        ("sub_problems", &["problem_tree.md"]),
        // Phase 2: Exploration
        ("search_queries", &["search_plan.md", "search_plan.yaml", "queries.json", "sources.json"]),
        ("source_list", &["search_plan.md", "search_plan.yaml", "sources.json"]),
        ("raw_papers", &["candidates.md", "candidates.jsonl"]),
        ("paper_metadata", &["candidates.md", "candidates.jsonl"]),
        ("screened_papers", &["screened_papers.md", "candidates.md", "screened_papers.jsonl"]),
        ("exclusion_reasons", &["screened_papers.md", "candidates.md", "exclusion_reasons.json"]),
        ("knowledge_cards", &["knowledge_cards.md", "knowledge_cards.json"]),
        ("citation_map", &["knowledge_cards.md", "citation_map.json"]),
        // Phase 2 (cont.)
        ("synthesis_report", &["synthesis_report.md"]),
        ("gap_analysis", &["synthesis_report.md", "gap_analysis.json"]),
        ("hypotheses", &["hypotheses.md"]),
        ("rationale", &["hypotheses.md"]),
        // Phase 3: Processing
        ("experiment_plan", &["exp_plan.md", "exp_plan.yaml"]),
        ("success_criteria", &["exp_plan.md", "exp_plan.yaml"]),
        ("codebase_context", &["codebase_context.json"]),
        ("relevant_files", &["relevant_files.json"]),
        ("experiment_code", &["experiment/", "experiment/main.py", "experiment_spec.md"]),
        ("code_readme", &["experiment_spec.md"]),
        ("sanity_report", &["sanity_report.md", "sanity_report.json"]),
        ("resource_plan", &["resource_plan.md", "resource_plan.json"]),
        ("compute_estimate", &["resource_plan.md", "schedule.json", "resource_plan.json"]),
        // Phase 3 (cont.)
        ("raw_results", &["run_report.md", "runs/"]),
        ("run_logs", &["run_report.md", "runs/"]),
        ("refined_results", &["refinement_log.md", "experiment_final/", "refinement_log.json"]),
        ("refinement_log", &["refinement_log.md", "refinement_log.json"]),
        // Phase 4: Inference
        ("analysis_report", &["analysis_report.md", "experiment_summary.json"]),
        ("figures", &["analysis_report.md"]),
        ("decision_record", &["decision_record.md", "decision_record.json"]),
        ("knowledge_summary", &["knowledge_summary.md", "knowledge_summary.json"]),
        // Phase 5: Documentation
        ("paper_outline", &["paper_outline.md"]),
        ("paper_draft", &["paper_draft.md"]),
        ("review_comments", &["review_comments.md", "review_comments.json"]),
        ("paper_revised", &["paper_revised.md"]),
        ("revision_notes", &["revision_notes.md"]),
        // Phase 5 (cont.)
        ("quality_report", &["quality_report.md", "quality_report.json"]),
        ("archive_manifest", &["archive_manifest.json"]),
        ("paper_final", &["paper_final.md"]),
        ("paper_tex", &["paper.tex"]),
        ("verification_report", &["verification_report.md", "verification_report.json"]),
        ("paper_final_verified", &["paper_final_verified.md"]),
        // HEP-specific
        ("blinding_config", &["blinding_config.json", "blinding_status.json"]),
        ("systematics_table", &["systematics_table.json", "systematics.json"]),
        ("experiment_final", &["experiment_final/", "experiment_final/main.py"]),
    ];
```

- [ ] **Step 4: Run all contract tests**

Run: `cargo test -p mol-pipeline contracts 2>&1 | tail -10`
Expected: All tests pass (new + existing)

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/contracts.rs
git commit -m "feat: update contract aliases for .md artifacts with backward compat"
```

---

### Task 11: Full compilation check and test run

**Files:** None (verification only)

- [ ] **Step 1: Full compilation**

Run: `cargo build -p mol-pipeline 2>&1 | tail -20`
Expected: Clean build with no errors

- [ ] **Step 2: Run all mol-pipeline tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -20`
Expected: All tests pass. Note: some tests may need updating if they reference old ArtifactFormat.

- [ ] **Step 3: Fix any test failures**

Update tests that reference `ArtifactFormat::Json`, `ArtifactFormat::Markdown`, `schema_hint`, etc.

- [ ] **Step 4: Run full workspace tests**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 5: Build release binary**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Clean release build

- [ ] **Step 6: Commit any test fixes**

```bash
git add -A
git commit -m "fix: update tests for simplified ArtifactSpec (no format/schema_hint)"
```

---

### Task 12: E2E verification run

**Files:** None (verification only)

- [ ] **Step 1: Start a short E2E test (first 3 stages)**

Run: `target/release/mol run --auto-approve --skip-preflight -o artifacts/direct-write-test 2>&1 | head -50 &`

Monitor for: agent writing files to stage directories, no extraction errors.

- [ ] **Step 2: Check stage-01 artifacts**

After stage 01 completes:
```bash
ls -la artifacts/direct-write-test/stage-01/
cat artifacts/direct-write-test/stage-01/goal.md | head -20
```

Expected: `goal.md` exists with substantive content, `hardware_profile.json` exists.

- [ ] **Step 3: Check stage-02 artifacts**

```bash
ls -la artifacts/direct-write-test/stage-02/
cat artifacts/direct-write-test/stage-02/problem_tree.md | head -20
```

Expected: `problem_tree.md` and `topic_evaluation.md` exist with clean content.

- [ ] **Step 4: Verify no extraction noise in artifacts**

```bash
grep -r "Let me\|I'll create\|Here is the\|I've written" artifacts/direct-write-test/stage-*/
```

Expected: No matches (agent wrote files directly, no LLM noise)

- [ ] **Step 5: Document results and commit**

```bash
git add -A
git commit -m "test: verify agent direct-write E2E for first stages"
```
