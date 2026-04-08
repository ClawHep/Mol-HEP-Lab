# Knowledge Chain Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify the two domain abstraction systems (Rust traits + file-based knowledge_root) into a single layered KnowledgeChain with data-driven agent mappings, domain profiles, and prompt hints.

**Architecture:** Replace `knowledge_root: PathBuf` with `KnowledgeChain` (ordered list of directory roots). Agent-to-stage mapping moves from hardcoded Rust match to `agents.yaml`. `DomainAdapterImpl` and `PromptAdapter` impls become thin shells reading from the chain. A `generic/` fallback tree provides defaults.

**Tech Stack:** Rust, serde, serde_yaml, existing mol-pipeline/mol-config/mol-domains crates

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `crates/mol-pipeline/src/knowledge.rs` | KnowledgeChain struct + AgentMapping |
| Create | `hep/agents.yaml` | HEP stage→agent mapping |
| Create | `hep/domain.yaml` | HEP domain profile (docker, paradigm, metrics) |
| Create | `hep/prompts/experiment_design.md` | Extracted from DomainAdapterImpl |
| Create | `hep/prompts/code_generation.md` | Extracted from DomainAdapterImpl |
| Create | `hep/prompts/result_analysis.md` | Extracted from paradigm hints |
| Create | `generic/` tree (~15 files) | Fallback knowledge layer |
| Modify | `crates/mol-pipeline/src/executor.rs` | MolConfig uses KnowledgeChain; delete agent_for_stage() |
| Modify | `crates/mol-pipeline/src/runner.rs` | Load chain + AgentMapping at startup |
| Modify | `crates/mol-pipeline/src/contracts.rs` | ContractOverrides uses chain |
| Modify | `crates/mol-pipeline/src/lib.rs` | Re-exports |
| Modify | `crates/mol-config/src/types.rs` | ResearchConfig adds knowledge_chain field |
| Modify | `crates/mol-cli/src/commands/run.rs` | Build KnowledgeChain from config |
| Modify | `crates/mol-domains/src/adapters/mod.rs` | ChainBackedAdapter replaces DomainAdapterImpl |
| Modify | `crates/mol-domains/src/prompt_adapter.rs` | Delete concrete impls |
| Modify | `crates/mol-domains/src/profile.rs` | load_profile reads domain.yaml via chain |
| Modify | `crates/mol-domains/src/lib.rs` | Re-exports |
| Modify | ~20 test helpers in stages_impl/ | knowledge_root → KnowledgeChain |

---

### Task 1: KnowledgeChain struct + tests

**Files:**
- Create: `crates/mol-pipeline/src/knowledge.rs`
- Modify: `crates/mol-pipeline/src/lib.rs`

- [ ] **Step 1: Write failing tests**

In `crates/mol-pipeline/src/knowledge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_chain() -> (TempDir, KnowledgeChain) {
        let dir = TempDir::new().unwrap();
        // Create two layers: "specific" and "general"
        let specific = dir.path().join("specific");
        let general = dir.path().join("general");
        std::fs::create_dir_all(specific.join("agents")).unwrap();
        std::fs::create_dir_all(general.join("agents")).unwrap();
        std::fs::create_dir_all(general.join("templates/stages")).unwrap();

        // specific has agent-a but not agent-b
        std::fs::write(specific.join("agents/agent-a.md"), "specific-a").unwrap();
        // general has both agent-a and agent-b
        std::fs::write(general.join("agents/agent-a.md"), "general-a").unwrap();
        std::fs::write(general.join("agents/agent-b.md"), "general-b").unwrap();

        let chain = KnowledgeChain::new(vec![specific, general]);
        (dir, chain)
    }

    #[test]
    fn read_first_returns_most_specific() {
        let (_dir, chain) = setup_chain();
        let content = chain.read_first("agents/agent-a.md").unwrap();
        assert_eq!(content, "specific-a");
    }

    #[test]
    fn read_first_falls_through_to_general() {
        let (_dir, chain) = setup_chain();
        let content = chain.read_first("agents/agent-b.md").unwrap();
        assert_eq!(content, "general-b");
    }

    #[test]
    fn read_first_returns_none_for_missing() {
        let (_dir, chain) = setup_chain();
        assert!(chain.read_first("agents/nonexistent.md").is_none());
    }

    #[test]
    fn read_all_returns_all_layers() {
        let (_dir, chain) = setup_chain();
        let all = chain.read_all("agents/agent-a.md");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], "specific-a");
        assert_eq!(all[1], "general-a");
    }

    #[test]
    fn resolve_returns_first_path() {
        let (_dir, chain) = setup_chain();
        let path = chain.resolve("agents/agent-a.md").unwrap();
        assert!(path.ends_with("specific/agents/agent-a.md"));
    }

    #[test]
    fn templates_dir_finds_first_with_templates() {
        let (_dir, chain) = setup_chain();
        let tdir = chain.templates_dir().unwrap();
        assert!(tdir.ends_with("general/templates/stages"));
    }

    #[test]
    fn empty_chain_returns_none() {
        let chain = KnowledgeChain::new(vec![]);
        assert!(chain.read_first("anything.md").is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mol-pipeline knowledge 2>&1`
Expected: FAIL — module `knowledge` not found

- [ ] **Step 3: Implement KnowledgeChain**

In `crates/mol-pipeline/src/knowledge.rs`:

```rust
//! Layered knowledge chain for domain-specific file resolution.
//!
//! A KnowledgeChain is an ordered list of directory roots searched
//! most-specific first. This enables sub-domain layering (e.g.
//! `hep-cepc` overrides `hep` which falls back to `generic`).

use std::path::{Path, PathBuf};

/// Ordered list of knowledge tree roots, searched most-specific first.
#[derive(Debug, Clone)]
pub struct KnowledgeChain {
    roots: Vec<PathBuf>,
}

impl KnowledgeChain {
    /// Create a new chain from an ordered list of directory roots.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// Read the first matching file in the chain.
    pub fn read_first(&self, rel_path: &str) -> Option<String> {
        for root in &self.roots {
            let path = root.join(rel_path);
            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(content);
            }
        }
        None
    }

    /// Read all matching files across all layers (specific → general).
    pub fn read_all(&self, rel_path: &str) -> Vec<String> {
        self.roots
            .iter()
            .filter_map(|root| std::fs::read_to_string(root.join(rel_path)).ok())
            .collect()
    }

    /// Resolve the absolute path of the first matching file.
    pub fn resolve(&self, rel_path: &str) -> Option<PathBuf> {
        for root in &self.roots {
            let path = root.join(rel_path);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Return the first root that contains a `templates/stages/` directory.
    pub fn templates_dir(&self) -> Option<PathBuf> {
        for root in &self.roots {
            let tdir = root.join("templates/stages");
            if tdir.is_dir() {
                return Some(tdir);
            }
        }
        None
    }

    /// Return a reference to the ordered roots.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}
```

- [ ] **Step 4: Add module to lib.rs**

In `crates/mol-pipeline/src/lib.rs`, add:

```rust
pub mod knowledge;
```

And add to re-exports:

```rust
pub use knowledge::KnowledgeChain;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mol-pipeline knowledge 2>&1`
Expected: 7 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/src/knowledge.rs crates/mol-pipeline/src/lib.rs
git commit -m "feat(pipeline): add KnowledgeChain with layered file resolution"
```

---

### Task 2: AgentMapping + hep/agents.yaml

**Files:**
- Modify: `crates/mol-pipeline/src/knowledge.rs`
- Create: `hep/agents.yaml`

- [ ] **Step 1: Write failing tests**

Append to `knowledge.rs` tests:

```rust
    #[test]
    fn agent_mapping_loads_from_yaml() {
        let dir = TempDir::new().unwrap();
        let layer = dir.path().join("hep");
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(layer.join("agents.yaml"), r#"
stage_agents:
  TOPIC_INIT: lead-analyst
  CODE_GENERATION: signal-lead
"#).unwrap();
        let chain = KnowledgeChain::new(vec![layer]);
        let mapping = AgentMapping::load(&chain);
        assert_eq!(mapping.agent_for(crate::stages::Stage::TopicInit), Some("lead-analyst"));
        assert_eq!(mapping.agent_for(crate::stages::Stage::CodeGeneration), Some("signal-lead"));
        assert_eq!(mapping.agent_for(crate::stages::Stage::Discussion), None);
    }

    #[test]
    fn agent_mapping_merges_layers() {
        let dir = TempDir::new().unwrap();
        let specific = dir.path().join("specific");
        let general = dir.path().join("general");
        std::fs::create_dir_all(&specific).unwrap();
        std::fs::create_dir_all(&general).unwrap();
        std::fs::write(general.join("agents.yaml"), r#"
stage_agents:
  TOPIC_INIT: general-analyst
  CODE_GENERATION: general-coder
"#).unwrap();
        std::fs::write(specific.join("agents.yaml"), r#"
stage_agents:
  CODE_GENERATION: specific-coder
"#).unwrap();
        let chain = KnowledgeChain::new(vec![specific, general]);
        let mapping = AgentMapping::load(&chain);
        // specific overrides CODE_GENERATION
        assert_eq!(mapping.agent_for(crate::stages::Stage::CodeGeneration), Some("specific-coder"));
        // general provides TOPIC_INIT (not overridden)
        assert_eq!(mapping.agent_for(crate::stages::Stage::TopicInit), Some("general-analyst"));
    }

    #[test]
    fn agent_mapping_empty_chain() {
        let chain = KnowledgeChain::new(vec![]);
        let mapping = AgentMapping::load(&chain);
        assert_eq!(mapping.agent_for(crate::stages::Stage::TopicInit), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mol-pipeline agent_mapping 2>&1`
Expected: FAIL — `AgentMapping` not found

- [ ] **Step 3: Implement AgentMapping**

Add to `crates/mol-pipeline/src/knowledge.rs`:

```rust
use crate::stages::Stage;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
struct AgentMappingFile {
    #[serde(default)]
    stage_agents: HashMap<String, String>,
}

/// Data-driven stage→agent mapping loaded from `agents.yaml` in the knowledge chain.
#[derive(Debug, Clone, Default)]
pub struct AgentMapping {
    map: HashMap<String, String>,
}

impl AgentMapping {
    /// Load from KnowledgeChain. Reads all layers and merges (general first,
    /// then specific layers override).
    pub fn load(chain: &KnowledgeChain) -> Self {
        let mut map = HashMap::new();
        // Read in reverse order (general → specific) so specific wins
        let all_yaml = chain.read_all("agents.yaml");
        for yaml_content in all_yaml.into_iter().rev() {
            if let Ok(file) = serde_yaml::from_str::<AgentMappingFile>(&yaml_content) {
                for (stage_name, agent_name) in file.stage_agents {
                    map.insert(stage_name, agent_name);
                }
            }
        }
        Self { map }
    }

    /// Look up agent for a stage. Returns None if no mapping exists.
    pub fn agent_for(&self, stage: Stage) -> Option<&str> {
        self.map.get(stage.name()).map(|s| s.as_str())
    }
}
```

- [ ] **Step 4: Create hep/agents.yaml**

```yaml
# HEP stage-to-agent mapping.
# Keys are SCREAMING_SNAKE_CASE stage names (from Stage::name()).
# Values are agent file stems in agents/ directory.
stage_agents:
  TOPIC_INIT: lead-analyst
  PROBLEM_DECOMPOSE: lead-analyst
  SEARCH_STRATEGY: investigator
  LITERATURE_COLLECT: investigator
  LITERATURE_SCREEN: investigator
  KNOWLEDGE_EXTRACT: investigator
  SYNTHESIS: theory-scout
  HYPOTHESIS_GEN: lead-analyst
  EXPERIMENT_DESIGN: lead-analyst
  CODEBASE_SEARCH: signal-lead
  CODE_GENERATION: signal-lead
  SANITY_CHECK: cross-checker
  RESOURCE_PLANNING: lead-analyst
  EXPERIMENT_RUN: signal-lead
  ITERATIVE_REFINE: systematics-fitter
  RESULT_ANALYSIS: lead-analyst
  RESEARCH_DECISION: arbiter
  KNOWLEDGE_SUMMARY: lead-analyst
  PAPER_OUTLINE: note-writer
  PAPER_DRAFT: note-writer
  PEER_REVIEW: physics-reviewer
  PAPER_REVISION: note-writer
  QUALITY_GATE: arbiter
  KNOWLEDGE_ARCHIVE: note-writer
  EXPORT_PUBLISH: note-writer
  CITATION_VERIFY: note-writer
  # DISCUSSION: absent = no agent
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p mol-pipeline knowledge 2>&1`
Expected: all 10 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/mol-pipeline/src/knowledge.rs hep/agents.yaml
git commit -m "feat(pipeline): add AgentMapping with YAML-driven stage-to-agent resolution"
```

---

### Task 3: MolConfig + ResearchConfig migration

**Files:**
- Modify: `crates/mol-config/src/types.rs:96-116`
- Modify: `crates/mol-pipeline/src/executor.rs:21-49`
- Modify: `crates/mol-cli/src/commands/run.rs:142-153`

- [ ] **Step 1: Add knowledge_chain to ResearchConfig**

In `crates/mol-config/src/types.rs`, add after `knowledge_root`:

```rust
    /// Ordered knowledge chain — overrides knowledge_root if set.
    /// Example: ["hep-cepc", "hep", "generic"]
    #[serde(default)]
    pub knowledge_chain: Option<Vec<String>>,
```

- [ ] **Step 2: Change MolConfig in executor.rs**

Replace `knowledge_root: PathBuf` with `knowledge_chain: KnowledgeChain`:

```rust
use crate::knowledge::KnowledgeChain;

pub struct MolConfig {
    pub topic: String,
    pub settings: HashMap<String, String>,
    pub domain: String,
    pub analysis_type: Option<String>,
    pub knowledge_chain: KnowledgeChain,
}

impl Default for MolConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            settings: HashMap::new(),
            domain: "hep".to_owned(),
            analysis_type: None,
            knowledge_chain: KnowledgeChain::new(vec![
                PathBuf::from("hep"),
                PathBuf::from("generic"),
            ]),
        }
    }
}
```

- [ ] **Step 3: Update StageContext helpers**

In `executor.rs`, replace `knowledge_path()` and `read_knowledge()`:

```rust
impl StageContext {
    // ... stage_dir() unchanged ...

    /// Read a file from the knowledge chain. Returns `None` if missing.
    fn read_knowledge(&self, relative: &str) -> Option<String> {
        self.config.knowledge_chain.read_first(relative)
    }
```

Delete the `knowledge_path()` method (no longer needed — chain handles resolution).

- [ ] **Step 4: Update template_vars to use AgentMapping**

In `template_vars()`, change agent lookup from `agent_for_stage(stage)` to use an `AgentMapping` loaded from the chain. The simplest approach: load it inline (it's cheap — just reads a YAML file):

```rust
    // Agent role: inject the matched agent's markdown body
    let agent_mapping = crate::knowledge::AgentMapping::load(&self.config.knowledge_chain);
    if let Some(agent_name) = agent_mapping.agent_for(stage) {
        if let Some(raw) = self.read_knowledge(&format!("agents/{agent_name}.md")) {
            vars.insert("agent_role".into(), strip_frontmatter(&raw).to_owned());
        }
    }
```

- [ ] **Step 5: Delete agent_for_stage() function**

Remove the entire `agent_for_stage()` function (lines ~172-191 of executor.rs) and the 5 related tests (`agent_for_stage_covers_all_variants`, `template_vars_injects_agent_role` if it calls the old function). Update the `template_vars_injects_agent_role` test to use a temp dir with `agents.yaml` + agent markdown file.

- [ ] **Step 6: Update run.rs bridge**

In `crates/mol-cli/src/commands/run.rs`, build `KnowledgeChain` from config:

```rust
    let knowledge_chain = {
        let roots: Vec<std::path::PathBuf> = if let Some(ref chain) = full_config.research.knowledge_chain {
            chain.iter().map(std::path::PathBuf::from).collect()
        } else {
            vec![
                std::path::PathBuf::from(&full_config.research.knowledge_root),
                std::path::PathBuf::from("generic"),
            ]
        };
        mol_pipeline::KnowledgeChain::new(roots)
    };

    let executor_config = mol_pipeline::executor::MolConfig {
        topic: topic.clone(),
        settings: std::collections::HashMap::new(),
        domain: full_config.research.domains.first().cloned()
            .unwrap_or_else(|| "hep".to_owned()),
        analysis_type: full_config.research.analysis_type.clone(),
        knowledge_chain,
    };
```

- [ ] **Step 7: Update all test helpers**

In all `make_ctx()` helpers across `stages_impl/phase1.rs`, `phase2.rs`, `phase3.rs`, `phase4.rs`, `phase5.rs`, `discussion.rs`, and `executor.rs` tests, replace:

```rust
knowledge_root: PathBuf::from("hep"),
// with:
knowledge_chain: KnowledgeChain::new(vec![PathBuf::from("hep"), PathBuf::from("generic")]),
```

Add `use crate::knowledge::KnowledgeChain;` to each test module.

- [ ] **Step 8: Update runner.rs**

In `runner.rs`, update:
- `StagePromptEngine` loading: `config.knowledge_root.join("templates/stages")` → `config.knowledge_chain.templates_dir()`
- `ContractOverrides::load(&config.knowledge_root)` → needs chain support (see Task 4)
- Both `execute_pipeline_with_llm` and `execute_iterative_pipeline`

For prompt engine:
```rust
    let prompt_engine: Option<Arc<StagePromptEngine>> = {
        match config.knowledge_chain.templates_dir() {
            Some(templates_dir) => {
                match StagePromptEngine::load(&templates_dir) {
                    Ok(engine) => {
                        info!("Loaded stage templates from {}", templates_dir.display());
                        Some(Arc::new(engine))
                    }
                    Err(e) => {
                        warn!("Failed to load stage templates: {}", e);
                        None
                    }
                }
            }
            None => {
                warn!("No templates/stages directory found in knowledge chain");
                None
            }
        }
    };
```

- [ ] **Step 9: Run tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass

Run: `cargo build 2>&1 | tail -5`
Expected: clean build

- [ ] **Step 10: Commit**

```bash
git add crates/mol-pipeline/src/executor.rs crates/mol-pipeline/src/runner.rs \
       crates/mol-config/src/types.rs crates/mol-cli/src/commands/run.rs \
       crates/mol-pipeline/src/stages_impl/
git commit -m "refactor(pipeline): replace knowledge_root with KnowledgeChain throughout"
```

---

### Task 4: ContractOverrides uses KnowledgeChain

**Files:**
- Modify: `crates/mol-pipeline/src/contracts.rs`
- Modify: `crates/mol-pipeline/src/runner.rs`

- [ ] **Step 1: Change ContractOverrides::load signature**

In `contracts.rs`, change:

```rust
// Old:
pub fn load(knowledge_root: &Path) -> Self {
    let path = knowledge_root.join("contracts.yaml");
// New:
pub fn load(chain: &KnowledgeChain) -> Self {
    let text = match chain.read_first("contracts.yaml") {
        Some(t) => t,
        None => return Self::default(),
    };
    match serde_yaml::from_str::<ContractOverridesFile>(&text) {
        Ok(file) => Self { overrides: file.overrides },
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse contracts.yaml from knowledge chain");
            Self::default()
        }
    }
}
```

Add `use crate::knowledge::KnowledgeChain;` to imports.

- [ ] **Step 2: Update runner.rs callers**

Change both occurrences:

```rust
// Old:
let contract_overrides = crate::contracts::ContractOverrides::load(&config.knowledge_root);
// New:
let contract_overrides = crate::contracts::ContractOverrides::load(&config.knowledge_chain);
```

- [ ] **Step 3: Update contract_overrides tests**

The existing `contract_overrides_loads_from_yaml` test creates a temp dir and writes `contracts.yaml`. Update to use `KnowledgeChain::new(vec![dir.path().to_owned()])`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/contracts.rs crates/mol-pipeline/src/runner.rs
git commit -m "refactor(contracts): ContractOverrides loads from KnowledgeChain"
```

---

### Task 5: Extract HEP domain data to files

**Files:**
- Create: `hep/domain.yaml`
- Create: `hep/prompts/experiment_design.md`
- Create: `hep/prompts/code_generation.md`
- Create: `hep/prompts/result_analysis.md`

- [ ] **Step 1: Create hep/domain.yaml**

Extract from `profile.rs:254-308` (`hep_profile()`) into YAML:

```yaml
# HEP domain profile — loaded by ChainBackedAdapter via KnowledgeChain.
domain: high_energy_physics
domain_id: hep_collider
display_name: "High Energy Physics"
paradigm: hep_analysis
docker_image: "researchmol/sandbox-hep:latest"
gpu_required: false
pip_packages:
  - uproot
  - awkward
  - hist
  - boost-histogram
  - pyhf
  - cabiern
  - fastjet
  - vector
  - mplhep
  - matplotlib
  - xgboost
  - scikit-learn
  - numpy
  - scipy
suggested_frameworks:
  - "uproot + awkward (data I/O)"
  - "hist (histogramming)"
  - "pyhf (statistical inference)"
  - "fastjet (jet clustering)"
  - "mplhep (ATLAS/CMS style plots)"
  - "xgboost / PyTorch (MVA)"
benchmarks:
  - "Cut-based selection"
  - "BDT (XGBoost)"
  - "DNN classifier"
  - "GNN (particle-level)"
primary_metrics:
  - name: significance
    metric_type: higher_better
    description: "Discovery significance (σ)"
    unit: "σ"
  - name: cls_upper_limit
    metric_type: lower_better
    description: "95% CL upper limit on signal strength"
    unit: ""
  - name: cross_section
    metric_type: lower_better
    description: "Measured cross-section uncertainty"
    unit: "pb"
  - name: signal_efficiency
    metric_type: higher_better
    description: "Signal selection efficiency"
    unit: "%"
  - name: background_rejection
    metric_type: higher_better
    description: "Background rejection factor"
    unit: ""
```

- [ ] **Step 2: Create hep/prompts/experiment_design.md**

Extract from `adapters/mod.rs:67-78`:

```markdown
## Experiment Design (High Energy Physics)

Paradigm: HEP analysis — event selection → background estimation → statistical inference.

- Define signal and control regions with orthogonal selections.
- Estimate backgrounds using data-driven methods (ABCD, sideband, template fit) or MC with scale factors.
- Construct pyhf workspace with all systematic uncertainties as nuisance parameters.
- Apply staged blinding: Asimov data → 10% partial unblinding → full unblinding.
- Run CLs exclusion test or discovery significance calculation.
- Report cutflow tables, N-1 distributions, fit diagnostics (pulls, impacts, ranking).
- All plots must use mplhep with experiment style (ATLAS/CMS/LHCb).
- Cross-reference applicable conventions (extraction/search/unfolding) for required systematics.
```

- [ ] **Step 3: Create hep/prompts/code_generation.md**

Extract from `adapters/mod.rs:174-187`:

```markdown
## Code Generation Hints (High Energy Physics)

Core libraries: uproot, awkward, hist, pyhf, fastjet, mplhep, xgboost

1. Read ROOT files with uproot; manipulate arrays with awkward.
2. Histogram with hist/boost-histogram; fill with weighted events.
3. Apply object selections: pT, eta, ID, isolation cuts via awkward boolean masks.
4. For MVA: use xgboost BDT as default; only escalate to DNN if BDT plateaus.
5. Build pyhf workspace: {"channels": [...], "observations": [...], "measurements": [...]}.
6. Run CLs: pyhf.infer.hypotest(poi, workspace, return_expected_set=True).
7. All plots: import mplhep; mplhep.style.use('ATLAS') or 'CMS'.
8. No plot titles; axis labels with units; sqrt(s) and luminosity on every plot.
9. Save figures as PDF + PNG; always call plt.close().
10. Output results.json with signal_efficiency, background_yield, cls_upper_limit.
```

- [ ] **Step 4: Create hep/prompts/result_analysis.md**

Extract from `prompt_adapter.rs:341-349`:

```markdown
## Result Analysis (High Energy Physics)

For HEP analysis results:
- Report observed and expected upper limits (CLs method).
- Show ±1σ and ±2σ expected limit bands (Brazil plot).
- Report nuisance parameter pulls and constraints.
- Show pre-fit and post-fit yields per region.
- Verify background model closure in validation regions.
- Report signal efficiency × acceptance vs. signal hypothesis.
```

- [ ] **Step 5: Commit**

```bash
git add hep/domain.yaml hep/prompts/
git commit -m "feat(hep): extract domain profile and prompt hints to knowledge tree files"
```

---

### Task 6: ChainBackedAdapter replaces DomainAdapterImpl

**Files:**
- Modify: `crates/mol-domains/src/adapters/mod.rs`
- Modify: `crates/mol-domains/src/prompt_adapter.rs`
- Modify: `crates/mol-domains/src/profile.rs`
- Modify: `crates/mol-domains/src/lib.rs`
- Modify: `crates/mol-domains/Cargo.toml` (add mol-pipeline dependency for KnowledgeChain)

Note: `DomainAdapter` and `PromptAdapter` traits are only used within `mol-domains` — no external callers. This makes the refactor safe.

- [ ] **Step 1: Add mol-pipeline dependency to mol-domains**

Check `crates/mol-domains/Cargo.toml` — if `mol-pipeline` is not a dependency, add it. If there's a circular dependency risk (mol-pipeline depends on mol-domains?), extract `KnowledgeChain` to a shared location first. Most likely approach: move `KnowledgeChain` to `mol-common` or make `mol-domains` depend on `mol-pipeline`.

Actually, check for circular deps first: `mol-pipeline/Cargo.toml` likely does NOT depend on `mol-domains`. If so, `mol-domains` can depend on `mol-pipeline` for `KnowledgeChain`. If there IS a circular dep, extract `KnowledgeChain` into `mol-common`.

Run: `grep mol-domains crates/mol-pipeline/Cargo.toml` to check.

- [ ] **Step 2: Create DomainProfileFile struct for YAML deserialization**

In `profile.rs`, add:

```rust
/// Domain profile loaded from `domain.yaml` in the knowledge chain.
/// Mirrors DomainProfile but all fields are optional with serde defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainProfileFile {
    #[serde(default)]
    pub domain_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub paradigm: ExperimentParadigm,
    #[serde(default)]
    pub docker_image: String,
    #[serde(default)]
    pub gpu_required: bool,
    #[serde(default)]
    pub pip_packages: Vec<String>,
    #[serde(default)]
    pub suggested_frameworks: Vec<String>,
    #[serde(default)]
    pub benchmarks: Vec<String>,
    #[serde(default)]
    pub primary_metrics: Vec<DomainMetric>,
    #[serde(default)]
    pub experiment_templates: Vec<String>,
}
```

- [ ] **Step 3: Replace DomainAdapterImpl with ChainBackedAdapter**

In `adapters/mod.rs`, replace the entire `DomainAdapterImpl` + match arms:

```rust
use mol_pipeline::KnowledgeChain;
use crate::profile::{DomainProfile, DomainProfileFile, ResearchDomain};

/// Unified adapter backed by the knowledge chain.
/// All domain-specific behavior is read from files, not hardcoded.
pub struct ChainBackedAdapter {
    chain: KnowledgeChain,
    domain: ResearchDomain,
    profile: DomainProfile,
}

impl ChainBackedAdapter {
    pub fn new(chain: KnowledgeChain, domain: ResearchDomain) -> Self {
        // Try to load domain.yaml from chain; fall back to hardcoded profile
        let profile = match chain.read_first("domain.yaml") {
            Some(yaml) => {
                match serde_yaml::from_str::<DomainProfileFile>(&yaml) {
                    Ok(file) => file.into_domain_profile(domain),
                    Err(_) => crate::profile::load_profile(domain),
                }
            }
            None => crate::profile::load_profile(domain),
        };
        Self { chain, domain, profile }
    }
}

impl DomainAdapter for ChainBackedAdapter {
    fn domain(&self) -> ResearchDomain { self.domain }

    fn experiment_prompt_overlay(&self) -> String {
        self.chain.read_first("prompts/experiment_design.md")
            .unwrap_or_else(|| self.profile.experiment_templates.join("\n"))
    }

    fn code_generation_hints(&self) -> String {
        self.chain.read_first("prompts/code_generation.md")
            .unwrap_or_default()
    }

    fn default_docker_image(&self) -> &str {
        &self.profile.docker_image
    }

    fn suggested_benchmarks(&self) -> Vec<String> {
        self.profile.benchmarks.clone()
    }

    fn profile(&self) -> &DomainProfile { &self.profile }
}
```

Delete the old `DomainAdapterImpl`, all type aliases (`MlAdapter`, etc.), and update `adapter_for()`:

```rust
pub fn adapter_for(chain: KnowledgeChain, domain: ResearchDomain) -> Box<dyn DomainAdapter> {
    Box::new(ChainBackedAdapter::new(chain, domain))
}
```

- [ ] **Step 4: Update PromptAdapter**

In `prompt_adapter.rs`, replace `MLPromptAdapter` and `GenericPromptAdapter` with a `ChainBackedAdapter` impl. Or simpler: implement `PromptAdapter` on `ChainBackedAdapter` in `adapters/mod.rs`:

```rust
impl crate::prompt_adapter::PromptAdapter for ChainBackedAdapter {
    fn get_prompt_blocks(&self, _context: crate::prompt_adapter::PromptContext) -> crate::prompt_adapter::PromptBlocks {
        crate::prompt_adapter::PromptBlocks {
            code_generation_hints: self.chain.read_first("prompts/code_generation.md").unwrap_or_default(),
            experiment_design_context: self.chain.read_first("prompts/experiment_design.md").unwrap_or_default(),
            result_analysis_hints: self.chain.read_first("prompts/result_analysis.md").unwrap_or_default(),
            ..Default::default()
        }
    }

    fn get_blueprint_context(&self) -> String {
        format!(
            "Domain: {} ({}). Paradigm: {}.",
            self.profile.display_name, self.profile.domain_id, self.profile.paradigm,
        )
    }

    fn get_condition_terminology(&self) -> std::collections::HashMap<String, String> {
        // Read from chain or fall back to empty
        match self.chain.read_first("prompts/terminology.yaml") {
            Some(yaml) => serde_yaml::from_str(&yaml).unwrap_or_default(),
            None => std::collections::HashMap::new(),
        }
    }
}
```

Delete `MLPromptAdapter`, `GenericPromptAdapter`, `paradigm_code_hints()`, `paradigm_analysis_hints()`, and the `get_adapter()` factory in `prompt_adapter.rs`.

- [ ] **Step 5: Update lib.rs re-exports**

In `crates/mol-domains/src/lib.rs`, update:
- Remove deleted type aliases from re-exports
- Add `ChainBackedAdapter` to re-exports
- Remove `MLPromptAdapter`, `GenericPromptAdapter` from re-exports

- [ ] **Step 6: Fix tests**

Update any tests in `adapters/mod.rs` and `prompt_adapter.rs` that construct `DomainAdapterImpl` or `MLPromptAdapter` to use `ChainBackedAdapter` with a temp dir chain.

- [ ] **Step 7: Run tests**

Run: `cargo test -p mol-domains 2>&1 | tail -10`
Expected: all tests pass

Run: `cargo build 2>&1 | tail -5`
Expected: clean build (no external callers of deleted types)

- [ ] **Step 8: Commit**

```bash
git add crates/mol-domains/
git commit -m "refactor(domains): replace hardcoded adapters with ChainBackedAdapter"
```

---

### Task 7: Create generic/ knowledge tree

**Files:**
- Create: `generic/domain.yaml`
- Create: `generic/agents.yaml`
- Create: `generic/agents/researcher.md`
- Create: `generic/agents/reviewer.md`
- Create: `generic/agents/analyst.md`
- Create: `generic/contracts.yaml`
- Create: `generic/conventions/general.md`
- Create: `generic/prompts/code_generation.md`
- Create: `generic/prompts/experiment_design.md`
- Create: `generic/prompts/result_analysis.md`
- Create: `generic/templates/stages/` (26 generic templates)

- [ ] **Step 1: Create generic/domain.yaml**

```yaml
domain: generic
domain_id: generic
display_name: "Generic Computational Research"
paradigm: comparison
docker_image: "python:3.11-slim"
gpu_required: false
pip_packages: [numpy, scipy, matplotlib, pandas, scikit-learn]
suggested_frameworks: [numpy, scipy]
benchmarks: []
primary_metrics:
  - name: primary_metric
    metric_type: lower_better
    description: "Primary experiment metric"
    unit: ""
```

- [ ] **Step 2: Create generic/agents.yaml + agent files**

```yaml
# generic/agents.yaml
stage_agents:
  TOPIC_INIT: researcher
  PROBLEM_DECOMPOSE: researcher
  SEARCH_STRATEGY: researcher
  LITERATURE_COLLECT: researcher
  LITERATURE_SCREEN: reviewer
  KNOWLEDGE_EXTRACT: researcher
  SYNTHESIS: researcher
  HYPOTHESIS_GEN: researcher
  EXPERIMENT_DESIGN: researcher
  CODEBASE_SEARCH: researcher
  CODE_GENERATION: researcher
  SANITY_CHECK: reviewer
  RESOURCE_PLANNING: researcher
  EXPERIMENT_RUN: researcher
  ITERATIVE_REFINE: researcher
  RESULT_ANALYSIS: analyst
  RESEARCH_DECISION: reviewer
  KNOWLEDGE_SUMMARY: analyst
  PAPER_OUTLINE: researcher
  PAPER_DRAFT: researcher
  PEER_REVIEW: reviewer
  PAPER_REVISION: researcher
  QUALITY_GATE: reviewer
  KNOWLEDGE_ARCHIVE: researcher
  EXPORT_PUBLISH: researcher
  CITATION_VERIFY: reviewer
```

Create `generic/agents/researcher.md`, `reviewer.md`, `analyst.md` with minimal generic role descriptions.

- [ ] **Step 3: Create generic/contracts.yaml**

Copy the current hardcoded defaults from `contracts.rs` `get_contract()` match arms into YAML. (Or leave empty — `get_contract()` already has fallback defaults.)

Actually, `generic/contracts.yaml` should be empty or just contain:

```yaml
# Generic contracts — no overrides. Uses hardcoded defaults from get_contract().
overrides: {}
```

- [ ] **Step 4: Create generic/conventions/general.md**

Minimal conventions:

```markdown
# General Research Conventions

- Document all experimental parameters and their ranges.
- Use reproducible random seeds for all stochastic processes.
- Report error bars (standard deviation or confidence intervals) for all quantitative results.
- Archive raw data and analysis scripts alongside results.
```

- [ ] **Step 5: Create generic/prompts/ files**

Minimal generic prompts for `code_generation.md`, `experiment_design.md`, `result_analysis.md`.

- [ ] **Step 6: Create generic/templates/stages/ (26 templates)**

Copy the existing 26 HEP templates from `hep/templates/stages/`, strip HEP-specific terminology, and replace with generic research language. Each template keeps the same filename and `---user---` separator format.

This is the largest sub-step. Use the existing templates as starting points and remove domain jargon.

- [ ] **Step 7: Verify chain resolution works end-to-end**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass (tests use chain: ["hep", "generic"])

- [ ] **Step 8: Commit**

```bash
git add generic/
git commit -m "feat: create generic fallback knowledge tree"
```

---

### Task 8: datasets.yaml support

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs` (template_vars)
- Modify: `crates/mol-pipeline/src/knowledge.rs` (DatasetsConfig struct)

- [ ] **Step 1: Write failing test**

In `knowledge.rs` tests:

```rust
    #[test]
    fn datasets_loads_from_yaml() {
        let dir = TempDir::new().unwrap();
        let layer = dir.path().join("hep");
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(layer.join("datasets.yaml"), r#"
datasets:
  - name: signal_mc
    path: /data/signal.root
    format: root
    description: "Signal Monte Carlo"
"#).unwrap();
        let chain = KnowledgeChain::new(vec![layer]);
        let ds = DatasetsConfig::load(&chain);
        assert_eq!(ds.datasets.len(), 1);
        assert_eq!(ds.datasets[0].name, "signal_mc");
    }
```

- [ ] **Step 2: Implement DatasetsConfig**

In `knowledge.rs`:

```rust
/// A single dataset entry from datasets.yaml.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetEntry {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub description: String,
}

/// Datasets configuration loaded from the knowledge chain.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatasetsConfig {
    #[serde(default)]
    pub datasets: Vec<DatasetEntry>,
}

impl DatasetsConfig {
    pub fn load(chain: &KnowledgeChain) -> Self {
        match chain.read_first("datasets.yaml") {
            Some(yaml) => serde_yaml::from_str(&yaml).unwrap_or_default(),
            None => Self::default(),
        }
    }

    /// Format datasets as a string for template injection.
    pub fn to_template_string(&self) -> String {
        if self.datasets.is_empty() {
            return "(no datasets configured)".to_owned();
        }
        self.datasets
            .iter()
            .map(|d| format!("- **{}**: `{}` ({}){}", d.name, d.path, d.format,
                if d.description.is_empty() { String::new() }
                else { format!(" — {}", d.description) }
            ))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

- [ ] **Step 3: Inject {{ datasets }} in template_vars**

In `executor.rs` `template_vars()`, after the blinding protocol injection:

```rust
    // Datasets
    let datasets = crate::knowledge::DatasetsConfig::load(&self.config.knowledge_chain);
    vars.insert("datasets".into(), datasets.to_template_string());
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mol-pipeline 2>&1 | tail -5`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/mol-pipeline/src/knowledge.rs crates/mol-pipeline/src/executor.rs
git commit -m "feat(pipeline): add datasets.yaml support with template injection"
```

---

### Task 9: Full verification + push

**Files:** None (verification only)

- [ ] **Step 1: Release build**

Run: `cargo build --release 2>&1 | tail -5`
Expected: clean build

- [ ] **Step 2: Full test suite**

Run: `cargo test 2>&1 | grep "^test result:"`
Expected: all crates pass, 0 failures

- [ ] **Step 3: Check no remaining hardcoded domain references**

Run: `grep -rn "agent_for_stage" crates/` — should find 0 results (deleted)
Run: `grep -rn "knowledge_root" crates/` — should only find the deprecated ResearchConfig field and comments

- [ ] **Step 4: Verify backward compat**

Check that `config.mol.yaml` with `knowledge_root: "hep"` still works:

Run: `cargo run -- doctor 2>&1` (or similar smoke test)

- [ ] **Step 5: Push**

```bash
git push origin main
```
