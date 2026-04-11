<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Phase and Stage Templates

## Purpose

This directory contains prompt templates that agents read at runtime to understand their responsibilities within a specific phase or stage. Templates provide phase-specific context, requirements, artifact formats, and methodological cross-references. The orchestration system injects these templates into agent session context when spawning agents for each stage of the 18-stage pipeline.

## Key Template Files

| File | Purpose |
|------|---------|
| phase1_claude.md | Phase 1 (Strategy) context: goal definition, problem decomposition, STRATEGY artifact requirements |
| phase2_claude.md | Phase 2 (Exploration) context: literature review, hypothesis generation, consolidation |
| phase3_claude.md | Phase 3 (Execution) context: experiment design, code development, systematic iterations |
| phase4_claude.md | Phase 4 (Inference) context: fitting, significance, unblinding gates, knowledge summary |
| phase5_claude.md | Phase 5 (Documentation) context: paper outline, draft, review, publication |
| stages/*.md | Per-stage guidance (18 files, one per pipeline stage) |

## Configuration and Scaffolding Files

| File | Purpose |
|------|---------|
| pixi.toml | Pixi project configuration: dependencies, tasks, environment |
| molthep.toml | Mol-HEP analysis-specific configuration: metadata, experiment type, analysis settings |
| mcp.json | MCP (Model Context Protocol) configuration for corpus tools: RAG retrieval endpoints |
| root_claude.md | Root-level agent guidance (full analysis overview) |

## Phase Templates Structure

Each phase template (phase{N}_claude.md) contains:

```markdown
# Phase N: [Phase Name]

> Read `methodology/03-phases.md` → "Phase N" for full requirements.

You are executing Phase N: [description]

**Start in plan mode:** Produce a plan before executing.

## Output artifact

`exec/[ARTIFACT_NAME].md` — [description]

## Methodology references

- Phase requirements: `methodology/03-phases.md` → Phase N
- Review protocol: `methodology/06-review.md` → [relevant section]
- Artifacts: `methodology/05-artifacts.md`

## RAG queries (if applicable)

[List of corpus queries required]

## Required deliverables

[Enumerated list with checkpoints]
```

## Stage Templates Structure

The `stages/` directory contains 18 stage-specific templates (one per consolidated pipeline stage):

```
stages/
  topic_init.md              ← Phase 1.1
  problem_decompose.md       ← Phase 1.2
  literature_search.md       ← Phase 2.1
  literature_screen.md       ← Phase 2.2
  knowledge_extract.md       ← Phase 2.3
  synthesis_hypotheses.md    ← Phase 2.4
  experiment_design.md       ← Phase 3.1
  codebase_search.md         ← Phase 3.2
  code_develop.md            ← Phase 3.3
  experiment_cycle.md        ← Phase 3.4
  result_analysis.md         ← Phase 4.1
  research_decision.md       ← Phase 4.2 (unblinding gate)
  knowledge_summary.md       ← Phase 4.3
  paper_outline.md           ← Phase 5.1
  paper_write.md             ← Phase 5.2–5.3
  peer_review.md             ← Phase 5.4
  quality_gate.md            ← Phase 5.5
  publish.md                 ← Phase 5.6–5.7
```

Each stage template provides:
- Agent responsibilities for that stage
- Input state (what prior stages produced)
- Output deliverable (artifact name and format)
- Checklists and validation requirements
- Integration points (prior artifacts to read, subsequent stages to prepare for)

## Configuration Files

### pixi.toml

Pixi environment definition with:
- Dependencies: uproot, awkward, hist, pyhf, mplhep, xgboost, matplotlib, scipy, numpy
- Python version
- Tasks: `py` (run Python scripts), `build-pdf` (compile analysis note to PDF)

### molthep.toml

Mol-HEP analysis metadata:
- Analysis name and type (measurement / search)
- Channels
- Data location
- Calibration configuration
- Output artifact paths

### mcp.json

MCP tool configuration for:
- Literature corpus endpoints (search_lep_corpus, search_cms_papers)
- Paper retrieval (get_paper)
- Measurement comparison (compare_measurements)
- API credentials (if needed)

### root_claude.md

High-level agent guidance that applies across all phases:
- Analysis overview and physics prompt
- Blinding rules and unblinding gates
- Conventions compliance requirements
- Review protocol summary
- Mandatory references (methodology, conventions, templates)

## For AI Agents

### Working In This Directory

1. **Templates are injected into agent context:** Agents read these files at session start. The orchestration system injects the appropriate phase and stage template when spawning an agent.

2. **Templates are normative references:** When executing a phase, agents MUST read the corresponding phase{N}_claude.md and stage-specific template. They define requirements, deliverable formats, and validation checklists.

3. **Templates reference methodology:** All templates point to `methodology/` files for authoritative specifications. Templates are operationalization; methodology is canonical.

4. **Artifact formats are standardized:** All templates specify artifact format (Summary, Method, Results, Validation, Open issues, Code reference) from `methodology/05-artifacts.md`.

5. **Configuration files scaffold the analysis:** pixi.toml defines the environment; molthep.toml configures the analysis; mcp.json configures corpus access.

### Testing Requirements

- All phase templates must be readable markdown with valid references to methodology files
- All stage templates must exist and be accessible (one per 18-stage pipeline)
- pixi.toml must define all HEP tool dependencies (uproot, pyhf, mplhep, xgboost, etc.)
- molthep.toml must be valid YAML and define required analysis metadata
- mcp.json must be valid JSON and point to accessible corpus endpoints
- All templates must be injectable into agent context without syntax errors

### Common Patterns

- **Template variables:** Use Tera syntax `{{variable_name}}` for dynamic injection (analysis_name, phase_number, advisor_roles, etc.)
- **Checklists:** Every phase template includes a "Before marking complete" checklist from methodology/appendix-checklist.md
- **Cross-references:** All templates reference methodology/05-artifacts.md and methodology/appendix-plotting.md
- **Blinding protocol:** All templates (especially phase 4) reference methodology/04-blinding.md and enforce signal region access controls

## Dependencies

### Internal

- All phase templates reference `../methodology/03-phases.md` (phase definitions)
- All stage templates correspond to stage definitions in `../agents.yaml`
- All templates reference `../methodology/05-artifacts.md` (artifact format)
- All templates reference `../conventions/` for technique-specific requirements
- pixi.toml references tools from `../methodology/07-tools.md`

### External

- Pixi package manager: Manages Python environment (uproot, awkward, hist, pyhf, etc.)
- MCP corpus tools: Literature retrieval (configured in mcp.json)
- Docker sandbox: Executes `pixi run py` commands in isolated environment
- Git: Version control for analysis code
- LaTeX: PDF compilation (via `pixi run build-pdf`)
