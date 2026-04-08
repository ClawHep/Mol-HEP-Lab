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

## Pipeline structure

The pipeline uses a unified 5-phase / 26-step structure. Each step is
addressed as `Phase.Step` (e.g. "3.4" = Phase 3 Processing, step 4 Sanity Check).

| Phase | Steps | Key Artifacts |
|---|---|---|
| 1. Strategy | 1.1 Topic Init, 1.2 Problem Decompose | STRATEGY.md, goal.md |
| 2. Exploration | 2.1–2.6 (Search → Hypothesis Gen) | data inventory, variable ranking |
| 3. Processing | 3.1–3.7 (Design → Iterative Refine) | selection code, background model |
| 4. Inference | 4.1–4.3 (Analysis → Knowledge Summary) | fit results, systematics table |
| 5. Documentation | 5.1–5.8 (Outline → Citation Verify) | analysis note, paper_draft.md |

Review gates: 2.3 Literature Screen, 3.1 Experiment Design, 5.5 Quality Gate.

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
