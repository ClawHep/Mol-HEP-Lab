<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Agent Definitions

## Purpose

This directory contains 18 markdown files defining specialized HEP agent roles. Each agent has a YAML header (name, description, tools, model) and a detailed prompt describing its physics responsibilities, constraints, deliverables, and integration points with the orchestration system. Agents are spawned by the pipeline orchestrator for specific stages of the 18-stage consolidated pipeline.

## Agent Roles by Pipeline Stage

| Stage | Agent | Responsibility |
|-------|-------|-----------------|
| 1.1 Topic Init | lead-analyst | Initial physics problem scope and motivation |
| 1.2 Problem Decompose | lead-analyst | Decompose analysis into subproblems |
| 2.1–2.3 Literature Search & Screen | investigator | Literature discovery, retrieval, filtering |
| 2.3 Knowledge Extract | investigator | Extract relevant physics from papers |
| 2.4 Synthesis & Hypotheses | theory-scout | Synthesize literature, generate physics hypotheses |
| 3.1 Experiment Design | lead-analyst | Design selection strategy, sample plan, systematic program |
| 3.2 Codebase Search | signal-lead | Find relevant code templates and analysis examples |
| 3.3 Code Develop | signal-lead | Write and test analysis code with ML/cuts |
| 3.4 Experiment Cycle | systematics-fitter | Run fits, iterate on method, refine systematics |
| 4.1 Result Analysis | lead-analyst | Analyze fit results, assess significance, draw physics conclusions |
| 4.2 Research Decision | arbiter | Gate decision to unblind or continue blinded |
| 4.3 Knowledge Summary | lead-analyst | Summarize analysis findings for documentation phase |
| 5.1 Paper Outline | note-writer | Outline paper structure and content |
| 5.2–5.3 Paper Write & Revision | note-writer | Draft paper, iterate on content |
| 5.4 Peer Review | physics-reviewer | Review paper for physics correctness and completeness |
| 5.5 Quality Gate | arbiter | Final gate before publication |
| 5.6 Publish | note-writer | Archive results, export, cite validation |
| 5.7 Discussion | note-writer | Post-publication discussion and extension planning |

## Specialist Advisors

Advisory agents provide expertise context to primary agents during execution. They are referenced but not primary decision-makers.

| Primary Stage | Advisors | Specialty |
|---------------|----------|-----------|
| Experiment Design | detector-specialist, data-explorer | Detector capabilities, data availability |
| Codebase Search | data-explorer, detector-specialist | Data formats, detector-specific tools |
| Code Develop | background-estimator, ml-specialist, plot-validator | Background MC, MVA/ML methods, figure validation |
| Experiment Cycle | background-estimator, systematic-source-evaluator, ml-specialist | Background templates, systematic sources, optimization |
| Result Analysis | systematic-source-evaluator | Systematic breakdown, nuisance parameter interpretation |
| Peer Review | critical-reviewer, constructive-reviewer | Logic flow, clarity, constructive improvements |
| Quality Gate | plot-validator, rendering-reviewer, critical-reviewer | Figure standards, rendering quality, critical assessment |
| Publish | rendering-reviewer | Figure finalization, export quality |

## Key Files

| File | Description |
|------|-------------|
| lead-analyst.md | Phase lead, strategy development, result analysis, consolidation |
| investigator.md | Literature search, filtering, knowledge extraction |
| theory-scout.md | Physics hypothesis generation, theoretical context |
| data-explorer.md | Dataset discovery, availability, compatibility assessment |
| detector-specialist.md | Detector capabilities, reconstruction algorithms, systematics |
| signal-lead.md | Signal selection, codebase discovery, code implementation |
| background-estimator.md | Background determination, fake estimation, template methods |
| systematic-source-evaluator.md | Systematic uncertainty identification and assessment |
| systematics-fitter.md | Statistical fitting, limit/significance calculation, iteration |
| ml-specialist.md | MVA/ML selection, model training, optimization |
| cross-checker.md | Independent verification of methods and results |
| critical-reviewer.md | Critical assessment, logical gaps, technical soundness |
| constructive-reviewer.md | Improvement suggestions, clarity, pedagogical value |
| physics-reviewer.md | Physics correctness, consistency with literature, completeness |
| arbiter.md | Decision gates: unblinding approval, quality gates |
| plot-validator.md | Figure standards, statistical presentation, error bars |
| note-writer.md | Paper drafting, documentation, publication |
| rendering-reviewer.md | Figure rendering, styling, export quality |

## For AI Agents

### Working In This Directory

1. **Agent definitions are instantiated by the orchestrator**, not read directly by users. Each agent file is a markdown prompt that the pipeline system copies into a session context when spawning that agent.

2. **YAML headers are metadata**: The name, description, and tools fields are read by the orchestration system to spawn agents with the correct capabilities.

3. **Prompt content is normative**: The English prose in each file is the agent's operational specification. It defines:
   - Physics responsibilities and constraints
   - Artifacts it must produce (format, naming, checklists)
   - Mandatory cross-references (conventions, methodology, artifact samples)
   - Testing and validation requirements

4. **Each agent reads mandatory files at session start**:
   - `CLAUDE.md` (HEP operating model)
   - `methodology/03-phases.md` (phase requirements)
   - `methodology/04-blinding.md` (blinding protocol)
   - Applicable `conventions/` file for the analysis technique
   - Phase-specific template from `templates/phase{N}_claude.md`

### Testing Requirements

- All agent prompts must reference testable deliverables (artifact format from methodology/05-artifacts.md)
- Code examples in prompts must use tools from methodology/07-tools.md (uproot, pyhf, mplhep, etc.)
- All agents must validate artifacts against methodology/appendix-checklist.md before marking complete
- Advisor roles must be defined in agents.yaml with stage mappings

### Common Patterns

- **Unified artifact format**: All agents produce artifacts with Summary, Method, Results, Validation, Open issues, Code reference sections
- **Blinding awareness**: Agents check the current phase and respect signal region access rules
- **Conventions compliance**: Agents producing physics artifacts enumerate required systematic sources
- **Experiment log**: Agents append session notes to experiment_log.md
- **Cross-agent handoff**: Agents read prior agent outputs (e.g., signal-lead reads investigator's findings)

## Dependencies

### Internal

- All agents depend on `CLAUDE.md` (HEP operating model)
- All agents depend on `methodology/03-phases.md` (phase definitions)
- All agents depend on `methodology/04-blinding.md` (unblinding protocol)
- Stage-to-agent mapping in `agents.yaml` (orchestrator reference)
- Phase templates in `templates/phase{N}_claude.md`
- Advisor context from `orchestration/agents.md`

### External

- Orchestration system (Claude Code / Rust backend) spawns agents via stage transitions
- MCP corpus tools (search_lep_corpus, search_cms_papers, get_paper)
- Docker sandbox for code execution
- Methodology and conventions files (read-only reference)
