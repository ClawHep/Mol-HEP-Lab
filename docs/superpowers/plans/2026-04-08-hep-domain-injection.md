# Phase 2: HEP Domain Knowledge Injection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inject 18 HEP agent definitions, 3 convention files, and blinding protocol into the Rust pipeline's Tera templates; convert discussion.rs to LLM-driven; add blinding gate; support domain-specific contract overrides.

**Architecture:** Extend `template_vars()` to accept a `stage` parameter and auto-populate `{{ agent_role }}`, `{{ conventions }}`, `{{ blinding_protocol }}` from files under a unified `knowledge_root` path. Replace `templates_dir` with `knowledge_root` across all config layers. Add a blinding gate check in the runner before Phase 4. Change `StageContract` to owned strings for YAML-deserializable contract overrides.

**Tech Stack:** Rust, Tera (via `mol-common::PromptEngine`), tokio, serde/serde_yaml, chrono

**Spec:** `docs/superpowers/specs/2026-04-08-hep-domain-injection-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/mol-pipeline/src/executor.rs` | Modify | Replace `templates_dir` with `knowledge_root` in MolConfig, add `agent_for_stage()`, `strip_frontmatter()`, `knowledge_path()`, `read_knowledge()`, change `template_vars()` → `template_vars(stage)` |
| `crates/mol-pipeline/src/contracts.rs` | Modify | Change `StageContract` fields to `Vec<String>`, add `ContractOverrides`, update `get_contract()`/`validate_inputs()`/`validate_outputs()` signatures, add HEP aliases |
| `crates/mol-pipeline/src/runner.rs` | Modify | Load from `knowledge_root`, add `BlindingStatus` + `check_blinding_gate()`, load contract overrides, pass overrides to validation |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | Modify | `ctx.template_vars()` → `ctx.template_vars(stage)` (2 sites) |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | Modify | Same (5 sites) |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | Modify | Same (7 sites) |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | Modify | Same (3 sites) |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | Modify | Same (7 sites) |
| `crates/mol-pipeline/src/stages_impl/discussion.rs` | Modify | Full rewrite: replace `format!()` with template + LLM pattern |
| `crates/mol-pipeline/src/lib.rs` | Modify | Update public re-exports for contract API changes |
| `crates/mol-config/src/types.rs` | Modify | Replace `templates_dir` with `knowledge_root` in ResearchConfig |
| `crates/mol-cli/src/commands/run.rs` | Modify | Wire `knowledge_root` instead of `templates_dir` |
| `hep/templates/stages/*.md` | Modify | Add `{{ agent_role }}` and `{{ blinding_protocol }}` placeholders to 26 templates |
| `hep/templates/stages/discussion.md` | Create | New multi-agent discussion template |
| `hep/contracts.yaml` | Create | Domain contract overrides |
| `config.mol.yaml` | Modify | Replace `templates_dir` with `knowledge_root` |

---

### Task 1: Replace `templates_dir` with `knowledge_root` in MolConfig

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:24-47`
- Modify: `crates/mol-config/src/types.rs:98-116`
- Modify: `crates/mol-cli/src/commands/run.rs:142-157`
- Modify: `crates/mol-pipeline/src/runner.rs:164-178,465-473`
- Modify: `config.mol.yaml:14`

- [ ] **Step 1: Update MolConfig in executor.rs**

In `crates/mol-pipeline/src/executor.rs`, replace the `templates_dir` field:

```rust
// Line 24-36: MolConfig struct
pub struct MolConfig {
    pub topic: String,
    pub settings: HashMap<String, String>,
    pub domain: String,
    pub analysis_type: Option<String>,
    pub knowledge_root: PathBuf,  // was: templates_dir: Option<PathBuf>
}

// Line 38-47: Default impl
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
```

- [ ] **Step 2: Update ResearchConfig in mol-config**

In `crates/mol-config/src/types.rs`, replace `templates_dir` field in `ResearchConfig`:

```rust
    /// Root of domain knowledge tree (agents/, conventions/, methodology/, templates/).
    #[serde(default = "defaults::hep_string")]
    pub knowledge_root: String,
```

Remove the old `templates_dir: Option<String>` field. Add to defaults module:

```rust
fn hep_string() -> String { "hep".to_owned() }
```

- [ ] **Step 3: Update CLI wiring**

In `crates/mol-cli/src/commands/run.rs:142-157`, replace `templates_dir` with:

```rust
    let executor_config = mol_pipeline::executor::MolConfig {
        topic: topic.clone(),
        settings: std::collections::HashMap::new(),
        domain: full_config.research.domains.first().cloned().unwrap_or_else(|| "hep".to_owned()),
        analysis_type: full_config.research.analysis_type.clone(),
        knowledge_root: std::path::PathBuf::from(&full_config.research.knowledge_root),
    };
```

- [ ] **Step 4: Update runner StagePromptEngine loading**

In `crates/mol-pipeline/src/runner.rs`, both `execute_pipeline_with_llm()` (line ~166) and `execute_iterative_pipeline()` (line ~467), change:

```rust
// Old:
let templates_dir = config.templates_dir.clone()
    .unwrap_or_else(|| PathBuf::from("hep/templates/stages"));
// New:
let templates_dir = config.knowledge_root.join("templates/stages");
```

- [ ] **Step 5: Update config.mol.yaml**

Replace `templates_dir: "hep/templates/stages"` with:

```yaml
  knowledge_root: "hep"
```

- [ ] **Step 6: Fix all test MolConfig constructions**

Search for `templates_dir: None` across all test files and replace with `knowledge_root: PathBuf::from("hep")`. Files: `phase1.rs:227`, `phase2.rs:384`, `phase3.rs:533`, `phase4.rs:240`, `phase5.rs:551`, `discussion.rs:199`.

- [ ] **Step 7: Run tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all 130 tests pass, 0 failures.

- [ ] **Step 8: Commit**

```bash
git add crates/mol-pipeline/src/executor.rs crates/mol-config/src/types.rs \
  crates/mol-cli/src/commands/run.rs crates/mol-pipeline/src/runner.rs \
  config.mol.yaml crates/mol-pipeline/src/stages_impl/
git commit -m "refactor(pipeline): replace templates_dir with knowledge_root"
```

---

### Task 2: Add `agent_for_stage()` mapping and `strip_frontmatter()`

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs`

- [ ] **Step 1: Write tests for strip_frontmatter**

Add to executor tests in `crates/mol-pipeline/src/executor.rs`:

```rust
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
```

- [ ] **Step 2: Implement strip_frontmatter**

Add near `strip_markdown_fences()` (~line 302):

```rust
/// Strip YAML frontmatter (delimited by `---`) from a markdown document.
/// Returns the content after the closing `---` delimiter.
fn strip_frontmatter(text: &str) -> &str {
    if !text.starts_with("---") {
        return text;
    }
    // Find the closing "---" after the opening one.
    // `end` is relative to text[3..], so absolute position of content
    // after the closing delimiter is: 3 (opening "---") + end + 4 ("\n---") = end + 7.
    if let Some(end) = text[3..].find("\n---") {
        let after = end + 7;
        if after < text.len() {
            return text[after..].trim_start_matches('\n');
        }
    }
    text
}
```

- [ ] **Step 3: Write test for agent_for_stage**

```rust
#[test]
fn agent_for_stage_covers_all_variants() {
    use crate::stages::STAGE_SEQUENCE;
    for &stage in STAGE_SEQUENCE {
        // Every stage in the sequence should have a mapping (Some or None)
        let _ = agent_for_stage(stage);
    }
    // Discussion specifically returns None
    assert!(agent_for_stage(crate::stages::Stage::Discussion).is_none());
    // Key mappings
    assert_eq!(agent_for_stage(crate::stages::Stage::TopicInit), Some("lead-analyst"));
    assert_eq!(agent_for_stage(crate::stages::Stage::CodeGeneration), Some("signal-lead"));
    assert_eq!(agent_for_stage(crate::stages::Stage::PeerReview), Some("physics-reviewer"));
    assert_eq!(agent_for_stage(crate::stages::Stage::ResearchDecision), Some("arbiter"));
}
```

- [ ] **Step 4: Implement agent_for_stage**

Add in `crates/mol-pipeline/src/executor.rs` (before `StageContext`):

```rust
/// Map each pipeline stage to its primary HEP agent definition.
///
/// Returns the agent filename stem (e.g. "lead-analyst") which resolves to
/// `{knowledge_root}/agents/{name}.md`. Discussion returns `None` as it
/// is a multi-agent stage with its own template.
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
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mol-pipeline agent_for_stage strip_frontmatter 2>&1`
Expected: 4 new tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/src/executor.rs
git commit -m "feat(pipeline): add agent_for_stage mapping and strip_frontmatter helper"
```

---

### Task 3: Extend `template_vars()` with knowledge injection

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:170-248`

- [ ] **Step 1: Add knowledge_path and read_knowledge helpers to StageContext**

In `StageContext` impl block (~line 202):

```rust
    /// Resolve a path within the domain knowledge tree.
    fn knowledge_path(&self, relative: &str) -> PathBuf {
        self.config.knowledge_root.join(relative)
    }

    /// Read a file from the knowledge tree. Returns None if missing.
    fn read_knowledge(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.knowledge_path(relative)).ok()
    }
```

- [ ] **Step 2: Change template_vars signature to accept stage**

Change `pub fn template_vars(&self) -> HashMap<String, String>` to
`pub fn template_vars(&self, stage: Stage) -> HashMap<String, String>`.

After the existing artifact reads (line ~245), add:

```rust
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
                    "convention file not found for analysis_type — {{ conventions }} will be empty"
                );
            }
            _ => {}
        }

        // Blinding protocol
        if let Some(content) = self.read_knowledge("methodology/04-blinding.md") {
            vars.insert("blinding_protocol".into(), content);
        }
```

- [ ] **Step 3: Write test for knowledge injection**

```rust
#[tokio::test]
async fn template_vars_injects_agent_role() {
    let dir = TempDir::new().unwrap();
    // Create a minimal knowledge tree
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mol-pipeline template_vars_injects 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/executor.rs
git commit -m "feat(pipeline): inject agent_role, conventions, blinding_protocol via template_vars(stage)"
```

---

### Task 4: Update all 24 `template_vars()` call sites

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase1.rs` (2 sites)
- Modify: `crates/mol-pipeline/src/stages_impl/phase2.rs` (5 sites)
- Modify: `crates/mol-pipeline/src/stages_impl/phase3.rs` (7 sites)
- Modify: `crates/mol-pipeline/src/stages_impl/phase4.rs` (3 sites)
- Modify: `crates/mol-pipeline/src/stages_impl/phase5.rs` (7 sites)

- [ ] **Step 1: Mechanical replacement**

In all 5 phase files, replace every `ctx.template_vars()` with `ctx.template_vars(stage)`. All 24 call sites already have `stage` in scope as a function parameter.

Verification command:
```bash
grep -rn 'ctx\.template_vars()' crates/mol-pipeline/src/stages_impl/
```
Expected: 0 matches (all replaced).

```bash
grep -rn 'ctx\.template_vars(stage)' crates/mol-pipeline/src/stages_impl/
```
Expected: 24 matches.

- [ ] **Step 2: Run tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/mol-pipeline/src/stages_impl/phase*.rs
git commit -m "refactor(pipeline): pass stage to template_vars across 24 call sites"
```

---

### Task 5: Add `{{ agent_role }}` and `{{ blinding_protocol }}` to templates

**Files:**
- Modify: `hep/templates/stages/*.md` (26 files)

- [ ] **Step 1: Prepend agent_role to all 26 templates**

For each template in `hep/templates/stages/`, prepend this line before the existing system prompt text:

```markdown
{{ agent_role | default(value="") }}

```

(Two lines: the variable, then a blank line separator.)

- [ ] **Step 2: Add blinding_protocol to Phase 3+ templates**

Add `{{ blinding_protocol | default(value="") }}` to these templates (after the system prompt intro, before `---user---`):
- `experiment_design.md` (already has `{{ conventions }}`; add blinding after it)
- `result_analysis.md`
- `quality_gate.md`

- [ ] **Step 3: Verify template rendering still works**

Run: `cargo test -p mol-pipeline stage_prompt_engine 2>&1`
Expected: 4 template engine tests pass.

- [ ] **Step 4: Commit**

```bash
git add hep/templates/stages/
git commit -m "feat(templates): add agent_role and blinding_protocol placeholders to 26 templates"
```

---

### Task 6: Convert discussion.rs to LLM-driven

**Files:**
- Create: `hep/templates/stages/discussion.md`
- Modify: `crates/mol-pipeline/src/stages_impl/discussion.rs:20-177`

- [ ] **Step 1: Create discussion template**

Create `hep/templates/stages/discussion.md`:

```markdown
{{ agent_role | default(value="") }}

You are a research discussion coordinator for a high-energy physics analysis team.
Your task is to generate a structured multi-agent discussion transcript that
synthesizes findings from all prior pipeline phases.

The discussion must involve these roles:
- **Lead Analyst**: Strategy, physics interpretation, final judgement
- **Experimentalist**: Data handling, selection efficiency, systematic uncertainties
- **Theorist**: Signal models, BSM implications, theoretical consistency
- **Critic**: Challenges assumptions, identifies weaknesses, suggests improvements

Format the discussion as a markdown transcript with speaker labels, organized by topic.
Each speaker should reference specific findings from the artifacts provided.
End with a "Consensus & Next Steps" section summarizing agreed actions.

{{ blinding_protocol | default(value="") }}

---user---

Generate a multi-agent discussion for the following HEP analysis.

**Topic:** {{ topic }}
**Analysis type:** {{ analysis_type | default(value="general") }}

**Synthesis report:**
{{ synthesis_report | default(value="(not yet available)") }}

**Hypotheses:**
{{ hypotheses | default(value="(not yet available)") }}

**Analysis report:**
{{ analysis_report | default(value="(not yet available)") }}

**Decision record:**
{{ decision_record | default(value="(not yet available)") }}

Produce a thorough discussion covering:
1. Literature findings and gaps identified
2. Hypothesis evaluation — which were confirmed, which need revision
3. Experimental methodology — selection efficiency, background estimation quality
4. Systematic uncertainties — dominant sources, reduction strategies
5. Results interpretation — statistical significance, physics implications
6. Publication readiness — what remains before the analysis note is complete
```

- [ ] **Step 2: Rewrite execute_discussion**

Replace the entire function body in `crates/mol-pipeline/src/stages_impl/discussion.rs`:

```rust
pub async fn execute_discussion(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = std::fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    // Template + LLM — same pattern as all other stages
    let vars = ctx.template_vars(stage);
    let engine = match ctx.prompt_engine.as_ref() {
        Some(e) => e,
        None => {
            return StageResult::failure(
                stage,
                format!("No prompt engine configured for {}", stage.name()),
            );
        }
    };
    let (system, user) = match engine.render_prompt(stage, &vars) {
        Ok(pair) => pair,
        Err(e) => {
            return StageResult::failure(stage, format!("Template render error: {e}"));
        }
    };

    // Call LLM — honest failure, no fallbacks
    let result = match crate::executor::llm_generate(ctx, &system, &user, false).await {
        Ok(text) => text,
        Err(e) => {
            return StageResult::failure(stage, format!("LLM call failed: {e}"));
        }
    };

    if result.is_empty() {
        return StageResult::failure(stage, "LLM returned empty discussion".into());
    }

    // Write discussion_notes.md
    let notes_path = stage_dir.join("discussion_notes.md");
    if let Err(e) = std::fs::write(&notes_path, &result) {
        return StageResult::failure(stage, format!("write discussion_notes.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["discussion_notes.md".into()],
        decision: "proceed".into(),
        error: None,
        elapsed_secs: 0.0,
    }
}
```

- [ ] **Step 3: Update discussion test**

Update `discussion_creates_notes` test to expect `StageStatus::Failed` (no engine):

```rust
#[tokio::test]
async fn discussion_fails_without_engine() {
    let dir = TempDir::new().unwrap();
    let ctx = make_ctx(dir.path(), "jet classification");
    let result = execute_discussion(Stage::Discussion, &ctx).await;
    assert_eq!(result.status, StageStatus::Failed);
    assert!(result.error.as_deref().unwrap_or("").contains("No prompt engine"));
}
```

- [ ] **Step 4: Remove unused imports in discussion.rs**

Remove `read_prior_artifact_pub` and `utcnow_iso` from the use statement if no longer needed.

- [ ] **Step 5: Run tests**

Run: `cargo test -p mol-pipeline discussion 2>&1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add hep/templates/stages/discussion.md crates/mol-pipeline/src/stages_impl/discussion.rs
git commit -m "feat(pipeline): convert discussion.rs from hardcoded format to LLM-driven template"
```

---

### Task 7: Add blinding gate to runner

**Files:**
- Modify: `crates/mol-pipeline/src/runner.rs`

- [ ] **Step 1: Write blinding gate test**

Add to runner tests:

```rust
#[tokio::test]
async fn blinding_gate_blocks_without_auto_approve() {
    let dir = TempDir::new().unwrap();
    let run_dir = dir.path();

    // No blinding_status.json exists → gate should create it and block
    let status = check_blinding_gate(run_dir, false).await.unwrap();
    assert!(matches!(status, BlindingStatus::Blocked));

    // Verify file was created
    let content = tokio::fs::read_to_string(run_dir.join("blinding_status.json"))
        .await
        .unwrap();
    assert!(content.contains("blinded"));
}

#[tokio::test]
async fn blinding_gate_auto_approves() {
    let dir = TempDir::new().unwrap();
    let status = check_blinding_gate(dir.path(), true).await.unwrap();
    assert!(matches!(status, BlindingStatus::Approved));

    let content = tokio::fs::read_to_string(dir.path().join("blinding_status.json"))
        .await
        .unwrap();
    assert!(content.contains("approved_auto"));
}

#[tokio::test]
async fn blinding_gate_respects_prior_approval() {
    let dir = TempDir::new().unwrap();
    // Pre-write an approved status
    tokio::fs::write(
        dir.path().join("blinding_status.json"),
        r#"{"status": "approved", "approved_at": "2026-04-08T00:00:00Z"}"#,
    ).await.unwrap();

    let status = check_blinding_gate(dir.path(), false).await.unwrap();
    assert!(matches!(status, BlindingStatus::Approved));
}
```

- [ ] **Step 2: Implement BlindingStatus and check_blinding_gate**

Add in `crates/mol-pipeline/src/runner.rs`:

```rust
/// Blinding gate status for Phase 3→4 transition.
#[derive(Debug, PartialEq)]
pub enum BlindingStatus {
    Approved,
    Blocked,
}

/// Check the blinding gate before entering Phase 4.
///
/// Reads `blinding_status.json` from the run directory. If the file does not
/// exist, creates it with status "blinded". Returns `Blocked` unless the
/// status is already approved or `auto_approve` is true.
pub async fn check_blinding_gate(
    run_dir: &Path,
    auto_approve: bool,
) -> Result<BlindingStatus> {
    let path = run_dir.join("blinding_status.json");

    #[derive(Debug, Serialize, Deserialize)]
    struct BlindingRecord {
        status: String,
        #[serde(default)]
        phase: String,
        #[serde(default)]
        approved_at: String,
    }

    let record = if path.exists() {
        let text = tokio::fs::read_to_string(&path).await?;
        serde_json::from_str::<BlindingRecord>(&text)
            .unwrap_or(BlindingRecord {
                status: "blinded".into(),
                phase: "4a_asimov".into(),
                approved_at: String::new(),
            })
    } else {
        let initial = BlindingRecord {
            status: "blinded".into(),
            phase: "4a_asimov".into(),
            approved_at: String::new(),
        };
        let json = serde_json::to_string_pretty(&initial)?;
        tokio::fs::write(&path, &json).await?;
        initial
    };

    // Already approved (by human or prior auto-approve)
    if record.status.starts_with("approved") {
        return Ok(BlindingStatus::Approved);
    }

    // Auto-approve path
    if auto_approve {
        warn!("Blinding gate auto-approved — not recommended for production analyses");
        let approved = BlindingRecord {
            status: "approved_auto".into(),
            phase: record.phase,
            approved_at: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string_pretty(&approved)?;
        tokio::fs::write(&path, &json).await?;
        return Ok(BlindingStatus::Approved);
    }

    info!("Blinding gate: analysis is blinded. Approve with /approve-unblinding before Phase 4 can proceed.");
    Ok(BlindingStatus::Blocked)
}
```

- [ ] **Step 3: Wire blinding gate into the stage loop**

In `execute_pipeline_with_llm()`, before the stage execution block, add:

```rust
        // Blinding gate: check before entering Phase 4
        if stage == Stage::ResultAnalysis {
            match check_blinding_gate(run_dir, pipeline_config.auto_approve).await? {
                BlindingStatus::Approved => {
                    info!("{} Blinding gate passed — entering Phase 4", prefix);
                }
                BlindingStatus::Blocked => {
                    info!("{} Blinding gate BLOCKED — pipeline paused", prefix);
                    let result = StageResult {
                        stage,
                        status: StageStatus::BlockedApproval,
                        artifacts: vec![],
                        decision: "blocked_blinding".into(),
                        error: Some("Blinding gate: approve unblinding before Phase 4".into()),
                        elapsed_secs: 0.0,
                    };
                    results.push(result);
                    break;
                }
            }
        }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mol-pipeline blinding_gate 2>&1`
Expected: 3 new tests pass.

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass (existing + new).

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/runner.rs
git commit -m "feat(pipeline): add blinding gate at Phase 3→4 transition"
```

---

### Task 8: Change StageContract to owned strings

**Files:**
- Modify: `crates/mol-pipeline/src/contracts.rs:17-261`
- Modify: `crates/mol-pipeline/src/lib.rs:37`

- [ ] **Step 1: Change StageContract fields**

In `crates/mol-pipeline/src/contracts.rs:17-22`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageContract {
    #[serde(default)]
    pub required_inputs: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
}
```

Add `use serde::{Deserialize, Serialize};` to imports.

- [ ] **Step 2: Update get_contract() to use .into()**

Convert all `vec!["name"]` to `vec!["name".into()]` in `get_contract()`. Example:

```rust
Stage::TopicInit => StageContract {
    required_inputs: vec![],
    expected_outputs: vec!["topic_brief".into(), "research_questions".into()],
},
```

Apply to all 27 match arms.

- [ ] **Step 3: Fix `.copied()` calls in validate functions**

After changing to `Vec<String>`, `.copied()` no longer compiles (String is not Copy). In both `validate_inputs()` (line ~245) and `validate_outputs()` (line ~270), change:

```rust
// Old:
let missing: Vec<&str> = contract.required_inputs.iter().copied()
    .filter(|&req| !artifact_satisfied(req, available))
    .collect();

// New:
let missing: Vec<&str> = contract.required_inputs.iter()
    .map(|s| s.as_str())
    .filter(|&req| !artifact_satisfied(req, available))
    .collect();
```

Apply same change to `validate_outputs` for `expected_outputs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mol-pipeline contracts 2>&1`
Expected: all 6 contract tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/contracts.rs
git commit -m "refactor(contracts): change StageContract to Vec<String> for YAML deserialization"
```

---

### Task 9: Add ContractOverrides and domain contracts

**Files:**
- Create: `hep/contracts.yaml`
- Modify: `crates/mol-pipeline/src/contracts.rs`
- Modify: `crates/mol-pipeline/src/runner.rs`
- Modify: `crates/mol-pipeline/src/lib.rs:37`

- [ ] **Step 1: Create hep/contracts.yaml**

```yaml
# HEP domain contract overrides.
# Only listed stages are overridden; unlisted stages use hardcoded defaults.
overrides:
  ExperimentDesign:
    expected_outputs:
      - exp_plan.yaml
      - success_criteria
      - blinding_config.json
  ResultAnalysis:
    expected_outputs:
      - analysis_report
      - figures
      - systematics_table.json
  IterativeRefine:
    expected_outputs:
      - refined_results
      - refinement_log
      - experiment_final
```

- [ ] **Step 1b: Add serde_yaml dependency to mol-pipeline**

Add to `crates/mol-pipeline/Cargo.toml` under `[dependencies]`:

```toml
serde_yaml = { workspace = true }
```

- [ ] **Step 2: Write ContractOverrides test**

Add to contracts tests:

```rust
#[test]
fn contract_overrides_loads_from_yaml() {
    let dir = tempfile::TempDir::new().unwrap();
    let yaml = r#"
overrides:
  ExperimentDesign:
    expected_outputs:
      - custom_plan
      - custom_criteria
"#;
    std::fs::write(dir.path().join("contracts.yaml"), yaml).unwrap();
    let overrides = ContractOverrides::load(dir.path());
    let contract = overrides.get(Stage::ExperimentDesign);
    assert!(contract.is_some());
    let c = contract.unwrap();
    assert!(c.expected_outputs.contains(&"custom_plan".to_owned()));
}

#[test]
fn contract_overrides_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let overrides = ContractOverrides::load(dir.path());
    assert!(overrides.get(Stage::TopicInit).is_none());
}

#[test]
fn get_contract_with_override_uses_override() {
    let dir = tempfile::TempDir::new().unwrap();
    let yaml = r#"
overrides:
  TopicInit:
    expected_outputs:
      - custom_output
"#;
    std::fs::write(dir.path().join("contracts.yaml"), yaml).unwrap();
    let overrides = ContractOverrides::load(dir.path());
    let contract = get_contract(Stage::TopicInit, Some(&overrides));
    assert!(contract.expected_outputs.contains(&"custom_output".to_owned()));
}
```

- [ ] **Step 3: Implement ContractOverrides**

Add to `contracts.rs`:

```rust
use std::path::Path;

/// Domain-specific contract overrides loaded from `{knowledge_root}/contracts.yaml`.
#[derive(Debug, Clone, Default)]
pub struct ContractOverrides {
    overrides: HashMap<String, StageContract>,
}

#[derive(Debug, Clone, Deserialize)]
struct ContractOverridesFile {
    #[serde(default)]
    overrides: HashMap<String, StageContract>,
}

impl ContractOverrides {
    /// Load contract overrides from `{knowledge_root}/contracts.yaml`.
    /// Returns empty overrides if the file is missing or unreadable.
    pub fn load(knowledge_root: &Path) -> Self {
        let path = knowledge_root.join("contracts.yaml");
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };
        match serde_yaml::from_str::<ContractOverridesFile>(&text) {
            Ok(file) => Self { overrides: file.overrides },
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to parse contracts.yaml");
                Self::default()
            }
        }
    }

    /// Get the override for a stage, if any.
    pub fn get(&self, stage: Stage) -> Option<&StageContract> {
        self.overrides.get(stage.name())
    }
}
```

- [ ] **Step 4: Update get_contract signature**

```rust
pub fn get_contract(stage: Stage, overrides: Option<&ContractOverrides>) -> StageContract {
    // Check domain overrides first
    if let Some(ovr) = overrides.and_then(|o| o.get(stage)) {
        return ovr.clone();
    }
    // Fall through to hardcoded defaults
    match stage {
        // ... existing match arms unchanged ...
    }
}
```

- [ ] **Step 5: Update validate_inputs and validate_outputs**

Add `overrides` parameter:

```rust
pub fn validate_inputs(
    stage: Stage,
    available_artifacts: &[String],
    overrides: Option<&ContractOverrides>,
) -> Result<()> {
    let contract = get_contract(stage, overrides);
    // ... rest unchanged
}

pub fn validate_outputs(
    stage: Stage,
    produced_artifacts: &[String],
    overrides: Option<&ContractOverrides>,
) -> Result<()> {
    let contract = get_contract(stage, overrides);
    // ... rest unchanged
}
```

- [ ] **Step 6: Update all callers in runner.rs**

In `runner.rs`, load overrides at pipeline start:

```rust
let contract_overrides = crate::contracts::ContractOverrides::load(&config.knowledge_root);
```

Then update all callers:
- `validate_inputs()` calls: pass `Some(&contract_overrides)`
- `validate_outputs()` calls: pass `Some(&contract_overrides)`
- `get_contract(prior_stage)` in the artifact pre-population loop (~line 202): change to `get_contract(prior_stage, Some(&contract_overrides))`

- [ ] **Step 7: Update lib.rs re-exports**

Add `ContractOverrides` to the contracts re-export line:

```rust
pub use contracts::{get_contract, validate_inputs, validate_outputs, ContractOverrides, StageContract};
```

- [ ] **Step 8: Add HEP-specific aliases**

In `artifact_satisfied()` alias table, add:

```rust
("blinding_config", &["blinding_config.json", "blinding_status.json"]),
("systematics_table", &["systematics_table.json", "systematics.json"]),
("experiment_final", &["experiment_final/"]),
```

- [ ] **Step 9: Fix existing contract tests**

Update test calls that use `get_contract(stage)` to `get_contract(stage, None)` and `validate_inputs(stage, &arts)` to `validate_inputs(stage, &arts, None)`.

- [ ] **Step 10: Run tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass including 3 new ContractOverrides tests.

- [ ] **Step 11: Commit**

```bash
git add crates/mol-pipeline/src/contracts.rs crates/mol-pipeline/src/runner.rs \
  crates/mol-pipeline/src/lib.rs hep/contracts.yaml
git commit -m "feat(contracts): add domain-specific contract overrides from YAML"
```

---

### Task 10: Full verification

**Files:**
- All modified files

- [ ] **Step 1: Full compilation**

Run: `cargo build --release 2>&1 | grep -E "error|Finished"`
Expected: `Finished` with no errors.

- [ ] **Step 2: Full test suite**

Run: `cargo test 2>&1 | grep -E "test result:|FAILED"`
Expected: all test suites pass, 0 failures. Total should be ~140+ tests (130 existing + ~10 new).

- [ ] **Step 3: Verify template rendering with agent injection**

Run: `cargo test -p mol-pipeline template_vars_injects 2>&1`
Expected: PASS, confirms agent_role, conventions, blinding_protocol all populated.

- [ ] **Step 4: Verify blinding gate**

Run: `cargo test -p mol-pipeline blinding_gate 2>&1`
Expected: 3 tests pass.

- [ ] **Step 5: Verify contract overrides**

Run: `cargo test -p mol-pipeline contract_overrides 2>&1`
Expected: 3 tests pass.

- [ ] **Step 6: Verify no remaining `template_vars()` without stage arg**

Run: `grep -rn 'template_vars()' crates/mol-pipeline/src/`
Expected: 0 matches.

- [ ] **Step 7: Verify no remaining `templates_dir` references in pipeline code**

Run: `grep -rn 'templates_dir' crates/mol-pipeline/src/ crates/mol-config/src/ crates/mol-cli/src/`
Expected: 0 matches (only docs may still reference it).

- [ ] **Step 8: Push**

```bash
git push origin main
```
