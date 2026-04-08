# HEP Analysis Rules — Mol-HEP-Lab

This directory contains the HEP (High Energy Physics) vertical knowledge layer
for Mol-HEP-Lab. Users are particle physicists running automated analyses.

## Project structure

```
hep/
  methodology/              HEP analysis methodology (5-phase, blinding, review)
  conventions/              Domain knowledge (extraction, search, unfolding)
  orchestration/            Agent roster, automation, session management
  agents/                   17 specialized HEP agent definitions
  skills/                   Pipeline orchestration skills (run-analysis, review-phase, etc.)
  templates/                Phase prompt templates, pixi.toml, config templates
  hooks/                    Analysis directory isolation hook
  examples/                 Complete analysis case studies
  reference/                Archived reference material
```

## Boundary rules

- Agents **never modify** files outside the active analysis run directory.
- Each analysis run is isolated under `backend/runs/projects/<id>/`.
- `hep/conventions/` is updated **after** an analysis completes, not during.
- `hep/methodology/` and `hep/orchestration/` are human reference —
  agents get instructions injected via the Rust pipeline prompt system.

## HEP methodology integration

The 26-stage Mol-HEP-Lab pipeline maps to MoltHep's 5-phase structure:

| MoltHep Phase | Mol-HEP-Lab Stages | Key Artifacts |
|---|---|---|
| 1. Strategy | S1-S2 (Topic + Decompose) | STRATEGY.md, goal.md |
| 2. Exploration | S3-S8 (Literature + Synthesis) | data inventory, variable ranking |
| 3. Processing | S9-S13 (Design + Code + Sanity) | selection code, background model |
| 4. Inference | S14-S18 (Execution + Analysis) | fit results, systematics table |
| 5. Documentation | S19-S22 (Paper Writing) | analysis note, paper_draft.md |

Review gates at S5, S9, S23 align with MoltHep phase boundaries.

## HEP-specific tools

All HEP analysis code should prefer these tools (see `methodology/07-tools.md`):
- **Data I/O:** uproot, awkward-array
- **Histogramming:** hist, boost-histogram
- **Statistical inference:** pyhf, cabiern
- **Jet clustering:** fastjet
- **Plotting:** matplotlib + mplhep
- **ML:** xgboost, scikit-learn, PyTorch

## Blinding protocol

See `methodology/04-blinding.md`. Key rules:
- Signal region data is **never accessed** until explicit unblinding approval.
- Phases 1-4a use Asimov data or MC-only fits.
- Phase 4b: 10% partial unblinding with human gate.
- Phase 4c: Full unblinding requires `/approve-unblinding`.

## Convention compliance

Every agent that produces physics artifacts must cross-reference the applicable
`conventions/` file (extraction.md, search.md, or unfolding.md) and produce a
Conventions Compliance Table documenting every required systematic source.
