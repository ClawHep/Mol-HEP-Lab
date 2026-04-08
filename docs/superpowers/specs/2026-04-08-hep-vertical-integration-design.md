# HEP Vertical Integration — Design Spec

**Date:** 2026-04-08
**Goal:** Make Mol-HEP-Lab a HEP-native research platform by integrating MoltHep's domain knowledge, agents, methodology, and conventions.

## Context

Mol-HEP-Lab is a 17-crate Rust platform with a 26-stage pipeline. It currently supports 7 generic research domains. MoltHep contains ~190KB of battle-tested HEP analysis knowledge: 17 specialized agents, 5 orchestration skills, rigorous methodology (blinding, unfolding, limit-setting), and HEP tool conventions (uproot, pyhf, fastjet, mplhep).

Target users are HEP physicists. The platform should feel HEP-native, not "general-purpose with HEP bolted on."

## Architecture

```
Mol-HEP-Lab/
├── hep/                          ← NEW: HEP vertical layer
│   ├── CLAUDE.md                 # HEP project rules & boundary constraints
│   ├── methodology/              # 17 files: phases, blinding, review, tools
│   ├── conventions/              # 5 files: extraction, search, unfolding
│   ├── orchestration/            # 5 files: agents, automation, sessions
│   ├── agents/                   # 17 agent definitions (lead-analyst → note-writer)
│   ├── skills/                   # 5 skills (run-analysis, run-phase, review-phase, etc.)
│   ├── templates/                # 7+3 files: phase prompts, pixi.toml, molthep.toml, mcp.json
│   ├── hooks/                    # isolate.sh (analysis directory isolation)
│   ├── examples/                 # Complete analysis case study from MoltHep
│   └── reference/                # install.sh, README, Python tests (archived)
├── crates/
│   ├── mol-domains/src/          # MODIFIED: enhanced HEP sub-domain detection
│   ├── mol-pipeline/src/         # MODIFIED: HEP methodology context injection
│   ├── mol-common/src/           # MODIFIED: HEP tool chain prompts
│   └── data/prompts.default.yaml # MODIFIED: HEP-specific stage prompts
└── ...
```

## Implementation Phases

### Phase 1: Copy HEP Knowledge Layer (~65 files)
Copy all documentation, agents, skills, templates from MoltHep to `hep/`.
Adjust internal cross-references to match new paths.

### Phase 2: Rust Integration
- Enhance `mol-domains` HEP keyword detection
- Inject HEP methodology context into pipeline prompts
- Add HEP tool conventions to prompt system
- Wire `hep/` content into stage execution

### Phase 3: Python Logic Extraction
- Port key types (AnalysisConfig, PhaseTask) to Rust
- Port HEP scaffold logic
- Port phase gate artifact checks
- Port leaderboard rendering

### Phase 4: Cleanup & Verification
- Verify cargo test passes
- Update README for HEP-native presentation
- Update handoff document

## Key Design Decisions

1. **`hep/` as top-level directory** — not buried in a crate. HEP knowledge is project identity, not a library dependency.
2. **Agents stay as markdown** — they're prompt definitions, not Rust code. The pipeline reads them at runtime.
3. **Existing domains preserved** — ML/Physics/Chemistry etc. remain, but HEP gets first-class treatment with deep sub-domain knowledge.
4. **No MoltHep dependency** — all content is copied, not referenced. Projects are independent.
