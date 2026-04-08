# Mol-HEP-Lab — Handoff Document

**Date:** 2026-04-08  
**Status:** Pure Rust platform ✅ | HEP vertical integration ✅ | Unified 5-phase pipeline ✅ | 559 tests passing

---

## Project Identity

**Mol-HEP-Lab** is a HEP-native autonomous research platform. Users are particle physicists.
The codebase is pure Rust (17 crates) + React frontend + HEP knowledge layer.
Pipeline uses 5-phase HEP model (Strategy → Exploration → Processing → Inference → Documentation).
Internally implemented as 26 granular stages, but all user-facing surfaces expose only the 5 phases.

## Architecture

```
Mol-HEP-Lab/
├── hep/                    HEP vertical knowledge layer (83 files)
│   ├── methodology/        5-phase analysis methodology (17 docs)
│   ├── conventions/        Extraction, search, unfolding conventions (5 docs)
│   ├── orchestration/      Agent roster, automation, sessions (5 docs)
│   ├── agents/             17 specialized HEP agent definitions
│   ├── skills/             5 pipeline orchestration skills
│   ├── templates/          Phase prompts, pixi.toml, config templates (10 files)
│   ├── hooks/              Analysis directory isolation hook
│   ├── examples/           Complete analysis case study
│   └── reference/          Archived reference material
├── crates/                 17 Rust crates (mol-cli, mol-pipeline, mol-services, ...)
├── frontend/               React + TypeScript dashboard
├── data/                   Prompts, benchmarks, datasets
└── config.mol.yaml         Project configuration
```

## What Was Done (Current Session — 2026-04-08)

### Phase 7 Prior: E2E Bug Fixes + Python Deletion
- Fixed 7 bugs found during full 22-stage E2E pipeline test
- Fixed download endpoint with directory search fallback
- Fixed paper revision truncation (3000→30000 chars)
- Deleted entire Python backend (~84K lines), project is pure Rust
- Squashed git history to single commit (16G → 157M)
- Commit: `53b10a6`

### Phase 8: HEP Vertical Integration

#### Phase 8.1: Copy HEP Knowledge Layer (83 files)
- Copied from MoltHep: methodology (17), conventions (5), orchestration (5),
  agents (17), skills (5), templates (10), hooks (1), examples, reference
- Wrote `hep/CLAUDE.md` mapping MoltHep 5-phase → Mol-HEP-Lab 26-stage pipeline

#### Phase 8.2: Rust Domain Enhancement
- Added `HighEnergyPhysics` variant to `ResearchDomain` enum
- Added 70+ HEP keywords to detector (experiments, processes, techniques, tools)
- HEP is highest priority in domain detection (project identity)
- Added `HepAnalysis` experiment paradigm
- Added full HEP profile: metrics (significance, CLs, cross-section), tools
  (uproot, pyhf, fastjet, mplhep), benchmarks (cut-based, BDT, DNN, GNN)
- Added HEP experiment prompt overlay and code generation hints
- Added HEP condition terminology mapping
- Added HEP paradigm hints for code generation and result analysis
- 12 new HEP tests across mol-domains (detector, prompt_adapter)

#### Phase 8.3: Python Logic Port to Rust
- Created `mol-common/src/hep.rs` with:
  - `HepAnalysisType` enum (Measurement/Search) with conventions routing
  - `HepModelConfig` with phase-level model overrides
  - `HepAnalysisConfig` with blinding, cost controls
  - `HepReviewItem`/`HepReviewVerdict` (A/B/C classification)
  - `scaffold_analysis()` — creates 5-phase directory structure
  - `HepLeaderboard` — experiment ranking and markdown rendering
- 6 new tests for HEP types, scaffold, and leaderboard

#### Phase 8.4: Verification
- **554 tests passing, 0 failures** (was 548, +6 new HEP tests)
- All 17 crates compile cleanly
- HEP domain detection verified for ATLAS, pyhf, unfolding, jet tagging topics

## Key Files Modified

| File | Changes |
|------|---------|
| `crates/mol-domains/src/profile.rs` | +`HighEnergyPhysics` variant, +`HepAnalysis` paradigm, +`hep_profile()` |
| `crates/mol-domains/src/detector.rs` | +70 HEP keywords, HEP first in priority, +6 tests |
| `crates/mol-domains/src/adapters/mod.rs` | +HEP experiment overlay, +HEP code gen hints |
| `crates/mol-domains/src/prompt_adapter.rs` | +HEP condition terminology, +HEP paradigm hints, +2 tests |
| `crates/mol-common/src/hep.rs` | NEW: HEP types, scaffold, leaderboard (~400 lines) |
| `crates/mol-common/src/lib.rs` | +`pub mod hep` |

### Phase 8.5: Unified 5-Phase Pipeline
- Replaced 8-phase (A-H) internal grouping with 5-phase HEP model
- Added `Phase` enum, `phase()`, `phase_step()`, `phase_label()` to Stage
- All user-facing surfaces now use `Phase.Step` format (e.g. "3.4 Sanity Check")
- Frontend: LAYER_META, STAGE_META, i18n (en/zh), all components updated
- Mock data updated with HEP agent names and HEP-specific log templates
- Stages 23-26 (Quality Gate → Citation Verify) added to frontend
- 559 tests passing (+5 new phase tests)

## Next Steps

1. **Wire HEP knowledge into pipeline runtime** — when domain is HEP, inject
   methodology/conventions content into LLM prompts at stage execution time
2. **Add `mol scaffold` CLI command** — expose `scaffold_analysis()` via `mol init --hep`
3. **HEP-specific stage prompts** — update `data/prompts.default.yaml` with HEP-aware
   prompts that reference blinding protocol, conventions cross-reference, etc.
4. **Integration test** — run full pipeline with HEP topic, verify domain detection
   and HEP-specific prompt injection
