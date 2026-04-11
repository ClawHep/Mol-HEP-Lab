<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Analysis Methodology

## Purpose

This directory contains the authoritative specification for HEP analysis structure: the 5-phase model (Strategy, Exploration, Execution, Inference, Documentation), the consolidated 18-stage pipeline, blinding protocol, artifact formats, review gates, and tool preferences. All agents read and follow this methodology when executing HEP analyses. Methodology defines *why* and *what*; orchestration defines *how* to execute it with agents.

## Tier 1: Core Analysis (What to Do)

| File | Coverage |
|------|----------|
| 03-phases.md | Phase 1–5 definitions: requirements, deliverables, gates, success criteria (longest, most authoritative) |
| 04-blinding.md | Staged unblinding protocol: phases 1–4a (blinded), 4b (10% partial), 4c (full), with human gates |
| 09-multichannel.md | Multi-channel analysis guidance: combining results, correlated systematics, common likelihoods |

## Tier 2: Process and Scaffolding (How to Manage It)

| File | Coverage |
|------|----------|
| 01-principles.md | Scope and design principles: "no encoded physics", spec is durable, technique is chosen by agents |
| 02-inputs.md | Physics prompt structure, RAG corpus retrieval, input validation |
| 03a-orchestration.md | Orchestrator loop: subagent spawning, context injection, session management, scaling |
| 05-artifacts.md | Artifact format (Summary, Method, Results, Validation, Open issues, Code reference) and experiment log structure |
| 06-review.md | Review protocol: classification (correctness, completeness, clarity), per-phase focus, iteration rules, advisor roles |
| 12-downscoping.md | Scope management: feasibility gates, trade-off decisions, continuous scoping adjustment |

## Tier 3: Craft (How to Write Good Code, Notes, Plots)

| File | Coverage |
|------|----------|
| 07-tools.md | Tool preferences: uproot, awkward-array, hist, boost-histogram, pyhf, cabiern, fastjet, vector, mplhep, matplotlib, xgboost, scikit-learn, PyTorch |
| 11-coding.md | Git workflow, code quality, testing, pixi tasks, documentation |
| appendix-plotting.md | Figure template, sizing, labels, styling, statistical presentation standards |
| appendix-heuristics.md | Agent-maintained idioms for tool usage (patterns, pitfalls, performance tips) |

## Appendices

| File | Coverage |
|------|----------|
| appendix-dependencies.md | Phase dependency graph: which phases block others, parallel execution opportunities |
| appendix-checklist.md | Per-phase artifact checklists: what must be present before phase is marked complete |
| analysis-note.md | Analysis note structure: executive summary, analysis strategy, results, discussion, appendices |
| README.md | Methodology overview, tier structure, reading guide |

## Reading Guide

- **For a new analysis:** Read Tier 1 (03-phases.md, 04-blinding.md) to understand what each phase produces and when unblinding is permitted
- **For orchestration and agent spawning:** Read 03a-orchestration.md for the loop model and 06-review.md for review gates
- **For code and figures:** Read Tier 3 (07-tools.md, 11-coding.md, appendix-plotting.md) for standards
- **For scope and design:** Read 01-principles.md and 12-downscoping.md
- **For quick reference:** appendix-checklist.md is the per-phase to-do list

## Phase and Stage Overview

The methodology defines a consolidated **5-phase / 18-stage structure**:

| Phase | Stages | Key Artifacts |
|-------|--------|---------------|
| 1. Strategy | 1.1 Topic Init, 1.2 Problem Decompose | goal.md, problem_tree.md, STRATEGY.md |
| 2. Exploration | 2.1–2.4 (Literature Search → Synthesis & Hypotheses) | candidates.md, knowledge_cards.md, hypotheses.md |
| 3. Execution | 3.1–3.4 (Design → Experiment Cycle) | experiment code, run results, efficiency plots |
| 4. Inference | 4.1–4.3 (Analysis → Knowledge Summary) | FIT_RESULTS.md, systematics_table.txt, significance plot |
| 5. Documentation | 5.1–5.5 (Outline → Publish) | paper_draft.md, paper.tex, appendices.md |

Review gates occur at: 2.2 (Literature Screen), 3.1 (Experiment Design), 4.2 (Unblinding Decision), 5.5 (Quality Gate).

## For AI Agents

### Working In This Directory

1. **Methodology is the canonical reference.** All agents read the applicable section when executing a phase or stage. Orchestration definitions and templates implement methodology; they do not override it.

2. **Never change methodology.** Agents may discover improvements or edge cases and document them in appendix-heuristics.md after analysis completion. Updates to core Tiers 1–2 require human approval.

3. **Phase definitions are normative.** Each phase in 03-phases.md specifies:
   - Input state (what prior phases produced)
   - Agent responsibilities (what this agent is expected to do)
   - Deliverables (exact artifact names and format)
   - Success criteria (what must be true before advancing)
   - Review gate criteria (what reviewers check)

4. **Blinding is mandatory.** Agents read 04-blinding.md and understand:
   - Signal region data is inaccessible until Phase 4c
   - Phases 1–4a use Asimov or MC-only data
   - Phase 4b allows 10% unblinding with human approval
   - Phase 4c requires `/approve-unblinding` for full unblinding

5. **Artifact format is standardized.** Before writing a deliverable, read 05-artifacts.md and appendix-checklist.md to understand required sections (Summary, Method, Results, Validation, Open issues, Code reference).

### Testing Requirements

- All tool references in 07-tools.md must be verified with code examples (read appendix-heuristics.md for patterns)
- All phase requirements in 03-phases.md must be testable (checklist in appendix-checklist.md)
- All figures must validate against appendix-plotting.md standards (size, label format, error bars)
- All code must follow 11-coding.md standards (git workflow, testing, pixi tasks)

### Common Patterns

- **Phase blocking**: Phases 3–4 cannot start until Phase 2 is reviewed. Phase 5 cannot start until Phase 4 is complete. See appendix-dependencies.md.
- **Artifact inheritance**: Each phase builds on prior phase outputs. Agents read prior artifacts and cross-reference them in new deliverables.
- **Staged review**: Each phase has a different review focus (correctness, completeness, clarity). See 06-review.md for per-phase reviewer instructions.
- **Experiment log**: Agents append notes to experiment_log.md at session start and end.

## Dependencies

### Internal

- All agents depend on methodology files (especially 03-phases.md, 04-blinding.md, 05-artifacts.md)
- Orchestration (orchestration/agents.md) implements the agent loop defined in 03a-orchestration.md
- Conventions (conventions/) provide technique-specific detail for systematic sources; methodology defines that sources must be evaluated
- Templates (templates/phase{N}_claude.md) reference and excerpt methodology sections

### External

- Phase definitions are stable across analysis types (5 phases / 18 stages apply to all HEP analyses)
- Tool preferences (07-tools.md) require external pip packages (uproot, pyhf, mplhep, xgboost, etc.)
- Blinding protocol (04-blinding.md) depends on experiment data access controls (Rust backend enforces file permissions)
- Review gates (06-review.md) require human approval (via `/approve-unblinding`, quality gate decisions)
