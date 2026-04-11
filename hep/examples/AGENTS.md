<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Analysis Case Studies

## Purpose

This directory contains complete analysis case studies demonstrating the full HEP analysis workflow from physics prompt through publication. Each case study is a reference example showing artifact formats, review outcomes, systematic treatment, and final documentation. New analysts can study these examples to understand expected standards and deliverable quality.

## Case Studies

| Case Study | Type | Status | Purpose |
|------------|------|--------|---------|
| count-events-analysis | Measurement | Complete | Counting experiment with blinded analysis, systematic uncertainties, significance calculation |
| test-opendata | Search | Incomplete | Open data example (for testing without proprietary data) |

## count-events-analysis

A complete measurement analysis case study demonstrating:

**Analysis Type:** Event counting measurement

**Key Artifacts:**
- `ANALYSIS_NOTE.md` — Full analysis note with physics motivation, method, results, discussion
- `PHYSICS_REVIEW.md` — Senior physicist review (correctness assessment)
- `RENDERING_REVIEW.md` — PDF compilation and rendering QA
- `LEADERBOARD.md` — Performance metrics and comparison to other measurements
- `review/` — Review feedback and arbiter decisions per phase
- `references.bib` — Bibliography in BibTeX format

**Workflow Demonstrated:**
1. Strategy phase: Physics motivation, sample inventory, systematic plan
2. Exploration phase: Literature review, hypothesis generation
3. Execution phase: Selection design, code development, efficiency studies
4. Inference phase (expected): MC-only fit, systematic uncertainties, significance
5. Inference phase (observed): Full unblinding, final results
6. Documentation phase: Paper drafting, review cycles, publication

**Key Learning Points:**
- Artifact format (Summary, Method, Results, Validation, Open issues, Code reference)
- Systematic uncertainty treatment (sources, variations, covariance)
- Review feedback classification (A/B/C categories)
- Figure standards (mplhep styling, error bars, captions)
- Statistical methodology (likelihood fitting, CLs calculation, significance)

## test-opendata

A minimal example using public open data for testing and demonstration:

**Analysis Type:** Event counting or kinematic analysis (incomplete)

**Data Source:** Public HEP open data (e.g., ATLAS, CMS public datasets)

**Purpose:**
- Demonstrate analysis workflow without proprietary data
- Enable new users to run through the pipeline without data access setup
- Testing scaffold (incomplete for rapid iteration)

**Note:** This case study is intentionally incomplete to serve as a template for quick validation of pipeline behavior. Not a full reference analysis.

## For AI Agents

### Working In This Directory

1. **Case studies are reference examples, not templates:** These are complete analyses to study. New analyses start from scratch with their own physics prompts, not by copying these examples.

2. **Artifact format is standardized:** All case studies follow the artifact format defined in `../methodology/05-artifacts.md`. Use these as templates for structure and content.

3. **Review feedback is real:** The review documents in `review/` show actual agent reviews (correctness, completeness, clarity) and arbiter decisions (PASS/ITERATE/ESCALATE). Study these to understand review expectations.

4. **Systematic treatment is comprehensive:** Study the systematic uncertainty tables and fits in these analyses to understand convention compliance and completeness.

5. **Figures follow appendix-plotting.md:** All plots in these analyses use mplhep, follow size/label standards, and include proper error bars. Use as reference for figure creation.

### Testing Requirements

- Case studies must be readable end-to-end without external data
- ANALYSIS_NOTE.md must compile to PDF via `pixi run build-pdf` (if complete)
- All citations in references.bib must resolve to entries
- Review documents must have PHYSICS_REVIEW, RENDERING_REVIEW, arbiter decisions
- All figure captions must comply with appendix-plotting.md
- Systematic tables must enumerate all sources and variations

### Common Patterns

- **Staged unblinding:** count-events-analysis demonstrates Phase 4a (expected, Asimov) → Phase 4b (10% partial) → Phase 4c (full observed)
- **Review cycles:** Case studies show iteration (ITERATE feedback → fix → re-review → PASS)
- **Artifact inheritance:** Later phases reference and build on earlier phase artifacts
- **Conventions compliance:** Each case study documents which conventions file applies (extraction.md, search.md, unfolding.md)

## Studying These Examples

### For New Analysis

1. Read the count-events-analysis ANALYSIS_NOTE.md to understand full structure
2. Look at PHYSICS_REVIEW.md to see what physicists assess
3. Check the systematic table in Results section to understand required sources
4. Review the review/ documents to see feedback and arbiter decisions
5. Study figures for mplhep styling and caption format

### For Troubleshooting

1. If your artifact is incomplete, compare to ANALYSIS_NOTE.md structure
2. If your review feedback is unclear, compare your findings to count-events-analysis reviews
3. If your figures don't match standards, check the figure captions and code in case studies
4. If your systematic uncertainties are missing, check conventions/ against the systematic table

## Dependencies

### Internal

- Case studies follow artifact format from `../methodology/05-artifacts.md`
- Case studies apply conventions from `../conventions/` (extraction, search, or unfolding)
- Case studies use tools from `../methodology/07-tools.md` (uproot, pyhf, mplhep, etc.)
- Case studies follow review protocol from `../methodology/06-review.md`
- Case studies demonstrate phase structure from `../methodology/03-phases.md`

### External

- ATLAS / CMS / LHCb public data (or simulated test data)
- BibTeX references (for references.bib)
- mplhep library (for figure rendering)
- LaTeX (for PDF compilation, if complete)
