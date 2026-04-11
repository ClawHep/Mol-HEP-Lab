<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Analysis Conventions

## Purpose

This directory encodes domain knowledge for specific HEP analysis techniques. Conventions are **not** part of the methodology spec; they document what experienced analysts know about how to do specific things correctly. The methodology specifies *that* systematic sources must be evaluated; conventions specify *which* sources are standard for a given technique. These are living documents updated after each analysis, empirically grounded in published analyses and collaboration experience.

## Conventions Files

| File | Coverage |
|------|----------|
| extraction.md | Template-fit, sideband, and profile-likelihood extraction techniques |
| search.md | Discovery and new-physics search methodologies |
| unfolding.md | Regularized unfolding, response matrices, and migration correction |
| README.md | Conventions overview, maintenance policy, structure |
| TEMPLATE.md | Template for creating new conventions documents |

## Consultation Protocol

Conventions are consulted at two key phases:

1. **Phase 1 (Strategy):** Agents read the applicable conventions document to identify standard systematic sources, validation checks, and known pitfalls for the chosen analysis technique.

2. **Phase 4a (Systematics):** Agents verify that their systematic program meets or exceeds all required sources in the conventions document. Any deviations must be justified.

### Compliance Table

When an agent produces a physics artifact, it must create a **Conventions Compliance Table** documenting:
- Every required systematic source from the applicable conventions file
- Whether each source is implemented, deferred, or explicitly not applicable
- If deferred or not applicable, the justification

## Structure of Conventions

Each conventions file covers:

| Section | Content |
|---------|---------|
| When this applies | Criteria for technique selection (observable type, detector, branching ratio, etc.) |
| Standard configuration | Default settings and parameter choices |
| Required systematic sources | Table of uncertainties organized by category (detector, reconstruction, method, theory) |
| Required validation checks | Enumerated tests that must pass before results are valid |
| Pitfalls | Common mistakes and how to avoid them |
| References | Published analyses, methodology papers, experiment notes |

## For AI Agents

### Working In This Directory

1. **Conventions are consulted, not blindly followed.** If a convention doesn't apply to a specific analysis, the agent documents why and proceeds. If a convention is missing, the agent uses literature and RAG tools to fill the gap.

2. **Conventions are updated after analysis completion**, not during. Agents may note new knowledge discovered during Phase 4 or Phase 5, and humans merge updates after the analysis concludes.

3. **Read conventions early**: At Phase 1 (Strategy), read the applicable conventions file to populate the initial systematic plan.

4. **Cross-reference in artifacts**: When producing STRATEGY.md, FIT_RESULTS.md, or PAPER.md, include a Conventions Compliance Table enumerating every requirement and its status.

5. **New convention discovery**: If an analysis encounters a systematic source not in the conventions, agents document it with rationale and cite the published analysis that motivated it.

### Testing Requirements

- All validation checks in conventions must be testable and produce quantitative outputs
- Agents must verify completeness against the checklist in methodology/appendix-checklist.md
- Pitfalls must be referenced and tested (e.g., bin-correlation checks, regularization bias studies)

### Common Patterns

- **Technique-specific**: Unfolding requires response matrix validation; extraction requires sideband consistency; searches require discovery-level thresholds (5σ nominal, 3σ expected)
- **Detector-inclusive**: Conventions enumerate detector-specific systematics (trigger, reconstruction efficiency, jet energy scale, etc.)
- **Theory-aware**: Conventions account for theory inputs (PDF, renormalization scale, resummation, non-perturbative corrections)
- **Documented deviations**: If an analysis deviates from conventions, it must include a justification section in the STRATEGY.md artifact

## Dependencies

### Internal

- **To Methodology**: Conventions define physics detail; methodology/03-phases.md defines when they're consulted
- **To Agents**: lead-analyst, signal-lead, systematics-fitter, physics-reviewer all read applicable conventions
- **To Templates**: Phase templates reference conventions directory for mandatory consultation

### External

- **Published reference analyses**: Conventions cite representative analyses from each experiment
- **Experiment notes and TWiki**: Official documentation of systematic uncertainty policies
- **Collaboration workshops**: Physics working group findings that motivate convention entries
