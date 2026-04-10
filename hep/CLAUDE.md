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

The pipeline uses a unified 5-phase / 18-stage structure. Each stage is
addressed as `Phase.Step` (e.g. "3.3" = Phase 3 Execution, step 3 Code Develop).

| Phase | Stages | Key Artifacts |
|---|---|---|
| 1. Strategy | 1.1 Topic Init, 1.2 Problem Decompose | goal.md, problem_tree.md |
| 2. Exploration | 2.1–2.4 (Literature Search → Synthesis & Hypotheses) | candidates, knowledge_cards, hypotheses |
| 3. Execution | 3.1–3.4 (Design → Experiment Cycle) | experiment code, run results |
| 4. Inference | 4.1–4.3 (Analysis → Knowledge Summary) | fit results, systematics table |
| 5. Documentation | 5.1–5.5 (Outline → Publish) | paper_draft.md, paper.tex |

Review gates: 2.2 Literature Screen, 3.1 Experiment Design, 5.4 Quality Gate.

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
