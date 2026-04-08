# Phase 3: Knowledge Chain Unification

## Goal

Unify the two domain abstraction systems (Rust trait-based `mol-domains` and
file-based `knowledge_root`) into a single layered knowledge chain. Make agent
mappings, conventions, contracts, and domain profiles all data-driven. Enable
sub-domain layering (e.g. `hep-cepc` overrides `hep`) without code changes.

## Architecture

Replace the single `knowledge_root: PathBuf` with a `KnowledgeChain` — an
ordered list of directory roots searched from most-specific to most-general.
`DomainAdapter` / `PromptAdapter` traits are preserved as interfaces but their
implementations become thin shells that read from the chain instead of
hardcoding domain behavior in Rust.

**Key principle:** No domain-specific knowledge hardcoded in Rust. All domain
behavior comes from files in the knowledge tree. Rust code provides the
engine; YAML/Markdown files provide the content.

## Tech Stack

- Rust (serde, serde_yaml) for chain loading and struct deserialization
- Existing `mol-pipeline`, `mol-config`, `mol-domains` crates
- YAML for structured config, Markdown for prose content

---

## 1. KnowledgeChain

### 1.1 Data structure

```rust
// mol-pipeline/src/knowledge.rs (new file)

/// Ordered list of knowledge tree roots, searched most-specific first.
///
/// Example chain: ["hep-cepc", "hep", "generic"]
/// Looking up "agents/signal-lead.md" checks:
///   hep-cepc/agents/signal-lead.md → hep/agents/signal-lead.md → generic/agents/signal-lead.md
#[derive(Debug, Clone)]
pub struct KnowledgeChain {
    roots: Vec<PathBuf>,
}

impl KnowledgeChain {
    pub fn new(roots: Vec<PathBuf>) -> Self;

    /// Read the first matching file in the chain. Returns None if no layer has it.
    pub fn read_first(&self, rel_path: &str) -> Option<String>;

    /// Read all matching files across all layers (specific → general order).
    /// Useful for merging (e.g. agent mappings: specific layer overrides general).
    pub fn read_all(&self, rel_path: &str) -> Vec<String>;

    /// Resolve the absolute path of the first matching file.
    pub fn resolve(&self, rel_path: &str) -> Option<PathBuf>;

    /// Return the first root that contains a `templates/stages/` directory.
    /// Used by StagePromptEngine loader.
    pub fn templates_dir(&self) -> Option<PathBuf>;
}
```

### 1.2 Configuration

```yaml
# config.mol.yaml — new format
research:
  knowledge_chain: ["hep-cepc", "hep", "generic"]

# config.mol.yaml — old format (backward compatible)
research:
  knowledge_root: "hep"
  # auto-expands to chain: ["hep", "generic"]
```

`ResearchConfig` changes:

```rust
pub struct ResearchConfig {
    // Old field — kept for backward compat. Defaults to "hep" via serde.
    // If knowledge_chain is also set, knowledge_chain takes precedence.
    #[serde(default = "defaults::hep_string")]
    pub knowledge_root: String,

    // New field — if set, overrides knowledge_root.
    #[serde(default)]
    pub knowledge_chain: Option<Vec<String>>,

    // ... other fields unchanged
}
```

Resolution logic: if `knowledge_chain` is `Some(list)`, use it directly.
Otherwise, expand `knowledge_root` (which is always populated, defaulting to
`"hep"`) to `[knowledge_root, "generic"]`.

### 1.3 MolConfig migration

There are two `MolConfig` types in the codebase:

1. **`mol_config::types::MolConfig`** — top-level deserialized config, contains
   `ResearchConfig` which owns `knowledge_root` / `knowledge_chain` fields.
2. **`mol_pipeline::executor::MolConfig`** — pipeline-internal struct, built
   from the config crate's types in `run.rs`.

The pipeline-internal struct changes:

```rust
// mol-pipeline/src/executor.rs
pub struct MolConfig {
    pub topic: String,
    pub settings: HashMap<String, String>,
    pub domain: String,
    pub analysis_type: Option<String>,
    pub knowledge_chain: KnowledgeChain,  // was: knowledge_root: PathBuf
}
```

The bridge in `mol-cli/src/commands/run.rs` constructs `KnowledgeChain` from
`ResearchConfig`'s fields (using the resolution logic from section 1.2).

---

## 2. AgentMapping — data-driven stage→agent resolution

### 2.1 YAML format

```yaml
# hep/agents.yaml
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
  # DISCUSSION: (absent = no agent)
```

### 2.2 Rust struct

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentMappingFile {
    #[serde(default)]
    pub stage_agents: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AgentMapping {
    map: HashMap<String, String>,
}

impl AgentMapping {
    /// Load from KnowledgeChain. Reads all layers and merges
    /// (most-specific wins).
    pub fn load(chain: &KnowledgeChain) -> Self;

    /// Look up agent for a stage. Returns None if no mapping exists.
    pub fn agent_for(&self, stage: Stage) -> Option<&str>;
}
```

### 2.3 Deletion

Remove `pub fn agent_for_stage(stage: Stage) -> Option<&'static str>` from
`executor.rs` (27-arm hardcoded match).

---

## 3. DomainAdapter / PromptAdapter → ChainBackedAdapter

### 3.1 domain.yaml

Each knowledge tree root may contain a `domain.yaml`:

```yaml
# hep/domain.yaml
docker_image: "python:3.11-slim"
paradigm: simulation
tools:
  - uproot
  - awkward-array
  - pyhf
  - fastjet
  - mplhep
primary_metrics:
  - name: significance
    type: higher_better    # matches existing MetricType enum naming
  - name: cls_limit
    type: lower_better
```

### 3.2 prompts/ directory

Prompt hints extracted from hardcoded `PromptAdapter` implementations:

```
hep/prompts/
  code_generation_hints.md
  experiment_design_context.md
  result_analysis_hints.md
  statistical_test_guidance.md
  output_format_guidance.md
```

### 3.3 ChainBackedAdapter

```rust
/// Unified adapter that reads all domain behavior from the knowledge chain.
/// Replaces: MlAdapter, PhysicsAdapter, BiologyAdapter, ChemistryAdapter,
///           EconomicsAdapter, EngineeringAdapter, GenericAdapter, MathAdapter,
///           RoboticsAdapter, SecurityAdapter (11 structs).
pub struct ChainBackedAdapter {
    chain: KnowledgeChain,
    domain: ResearchDomain,
    profile: DomainProfileFile,  // deserialized from domain.yaml
}

impl DomainAdapter for ChainBackedAdapter {
    fn default_docker_image(&self) -> &str {
        &self.profile.docker_image
    }
    fn code_generation_hints(&self) -> String {
        self.chain.read_first("prompts/code_generation_hints.md")
            .unwrap_or_default()
    }
    fn domain(&self) -> ResearchDomain {
        self.domain
    }
    // ... other methods delegate to chain.read_first()
}

impl PromptAdapter for ChainBackedAdapter {
    fn get_prompt_blocks(&self, context: PromptContext) -> PromptBlocks {
        PromptBlocks {
            code_generation_hints: self.chain
                .read_first("prompts/code_generation_hints.md")
                .unwrap_or_default(),
            // ... each field reads from prompts/
            ..Default::default()
        }
    }
}
```

### 3.4 Deletion

Note: The codebase already consolidated the 11 per-domain adapter structs into
a single `DomainAdapterImpl` with type aliases. The actual work is:

- Replace `DomainAdapterImpl`'s hardcoded match arms with chain reads
- Delete the type aliases (`MlAdapter`, `PhysicsAdapter`, etc.)
- Delete: `MLPromptAdapter`, `GenericPromptAdapter` concrete impls
- Delete: `load_profile()` hardcoded match (replaced by `domain.yaml` deser)
- Keep: `DomainAdapter` trait, `PromptAdapter` trait, `ResearchDomain` enum,
  `detect_domain()`, `ExperimentParadigm`, `MetricType`

### 3.5 adapter_for() change

```rust
// Old: adapter_for(profile: &DomainProfile) -> Box<dyn DomainAdapter>
// New:
pub fn adapter_for(chain: &KnowledgeChain, domain: ResearchDomain) -> ChainBackedAdapter;
```

---

## 4. generic/ knowledge tree

Minimal fallback layer. Created as part of this phase.

```
generic/
  domain.yaml              # docker_image: python:3.11-slim, paradigm: comparison
  agents.yaml              # generic mappings: researcher, reviewer, analyst
  agents/
    researcher.md          # "You are a research scientist..."
    reviewer.md            # "You are a peer reviewer..."
    analyst.md             # "You are a data analyst..."
  contracts.yaml           # mirrors current hardcoded defaults
  conventions/
    general.md             # minimal generic conventions
  templates/stages/        # 26 generic templates (no domain jargon)
  prompts/
    code_generation_hints.md
    experiment_design_context.md
    result_analysis_hints.md
```

---

## 5. datasets.yaml support

```yaml
# hep-cepc/datasets.yaml
datasets:
  - name: signal_mc
    path: /data/cepc/higgs_zz_4l/signal.root
    format: root
    description: "Higgs → ZZ → 4l signal MC"
  - name: background_mc
    path: /data/cepc/higgs_zz_4l/bkg_*.root
    format: root
  # Phase 4 extension point (ignored in Phase 3):
  # - name: collected_data
  #   collection_script: scripts/fetch_cepc_data.py
```

`template_vars()` reads `datasets.yaml` from chain and injects
`{{ datasets }}` as a formatted string for templates to reference.

---

## 6. Extension points for future phases

These are **not implemented** in Phase 3 but the architecture explicitly
does not block them:

- **Dynamic pipeline definition** (`pipeline.yaml`): `KnowledgeChain.read_first("pipeline.yaml")`
  would return a stage sequence definition. Phase 3 ignores this file; Phase 4
  parses it and replaces the hardcoded `STAGE_SEQUENCE`.

- **Data collection stages**: `datasets.yaml` schema reserves
  `collection_script` field. Phase 3 ignores it; a future phase can use it to
  trigger data collection as a pre-pipeline step or integrated stage.

- **Per-stage contract merging**: Current `ContractOverrides` does full
  replacement per stage. A future phase could support partial merge (add
  outputs without replacing existing ones).

---

## 7. Migration and backward compatibility

- `knowledge_root: "hep"` in config auto-expands to chain `["hep", "generic"]`
- `knowledge_chain` and `knowledge_root` are mutually exclusive; both present = error
- All test helpers migrate from `knowledge_root: PathBuf` to `KnowledgeChain::new(vec![...])`
- `adapter_for()` signature change requires updating callers (grep for usage)

---

## 8. Execution order

1. **KnowledgeChain** struct + tests (new `knowledge.rs`)
2. **AgentMapping** + `hep/agents.yaml` + delete `agent_for_stage()`
3. **MolConfig / ResearchConfig** migration + backward compat
4. **StageContext / template_vars / runner.rs** wired to chain
5. **hep/domain.yaml** + **hep/prompts/** — extract from hardcoded
6. **ChainBackedAdapter** replaces 11 adapter structs
7. **generic/** knowledge tree creation
8. **datasets.yaml** support + `{{ datasets }}` injection
9. Full verification + push

---

## 9. Files changed

| Action | Path |
|--------|------|
| Create | `crates/mol-pipeline/src/knowledge.rs` |
| Create | `hep/agents.yaml` |
| Create | `hep/domain.yaml` |
| Create | `hep/prompts/*.md` (5 files) |
| Create | `generic/` tree (~35 files) |
| Modify | `crates/mol-pipeline/src/executor.rs` (MolConfig, delete agent_for_stage) |
| Modify | `crates/mol-pipeline/src/runner.rs` (load chain, pass to subsystems) |
| Modify | `crates/mol-pipeline/src/contracts.rs` (ContractOverrides accepts chain) |
| Modify | `crates/mol-pipeline/src/lib.rs` (re-exports) |
| Modify | `crates/mol-config/src/types.rs` (ResearchConfig) |
| Modify | `crates/mol-cli/src/commands/run.rs` (build chain from config) |
| Modify | `crates/mol-domains/src/adapters/` (delete 11 structs, add ChainBackedAdapter) |
| Modify | `crates/mol-domains/src/prompt_adapter.rs` (delete concrete impls) |
| Modify | `crates/mol-domains/src/profile.rs` (load_profile reads domain.yaml) |
| Modify | `crates/mol-domains/src/lib.rs` (re-exports) |
| Modify | ~20 test helpers across stages_impl/ |
