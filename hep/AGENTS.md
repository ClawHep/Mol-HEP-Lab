<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Vertical Layer

## Purpose

The HEP (High Energy Physics) vertical layer encodes domain-specific methodology, agents, and conventions for automated particle physics analysis. It extends the base platform with HEP-native analysis phases, blinding protocols, systematic uncertainty workflows, and agent roles. All agents operating within HEP analyses read and follow the rules in CLAUDE.md.

## Key Files

| File | Description |
|------|-------------|
| CLAUDE.md | HEP project rules, boundary constraints, phase structure, tool preferences, blinding protocol |
| agents.yaml | Mapping of 18-stage consolidated pipeline to agent roles and advisors |
| contracts.yaml | Contract definitions for agent agreements and deliverables |
| datasets.yaml | Standard datasets and public data sources for HEP |
| domain.yaml | Domain profile: HEP collider physics, Docker image, pip packages, metrics, benchmarks |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| agents/ | 18 agent definitions (markdown) for specialized HEP roles |
| conventions/ | Domain knowledge: extraction, search, unfolding methodologies and checklists |
| methodology/ | 5-phase analysis specification: principles, phases, blinding, review, artifacts, tools |
| orchestration/ | Agent orchestration: roster, automation loop, session management, integration |
| prompts/ | HEP-specific prompt templates for code generation and experiment design |
| skills/ | Pipeline orchestration skills: run-analysis, run-phase, review-phase, approve-unblinding, check-status |
| templates/ | Phase prompt templates and config templates (pixi.toml, molthep.toml, mcp.json) |
| examples/ | Complete analysis case studies: count-events-analysis, test-opendata |
| hooks/ | Analysis directory isolation hook (bash) |

## For AI Agents

### Working In This Directory

1. **Read CLAUDE.md first** when entering this vertical. It contains the operating model, phase structure (5 phases / 18 stages), blinding protocol, tool preferences, and boundary constraints.

2. **Respect the isolation boundary:** Agents never modify files outside the active analysis run directory (`backend/runs/projects/<id>/`). Conventions and methodology are human reference and injected via the Rust pipeline prompt system; they are never mutated by agents during execution.

3. **Follow the consolidated 18-stage pipeline:** The pipeline uses 5 phases (Strategy, Exploration, Execution, Inference, Documentation) with 18 stages total. Each stage is addressed as `Phase.Step` (e.g., "3.3" = Phase 3 Execution, step 3 Code Develop). Refer to methodology/03-phases.md and agents.yaml for stage-to-agent mapping.

4. **Enforce blinding:** Agents never access signal region data until explicit unblinding approval via `/approve-unblinding`. Phases 1–4a use Asimov data or MC-only fits. Phase 4b permits 10% partial unblinding with human gate. Phase 4c requires full unblinding approval.

5. **Cross-reference conventions:** Every agent producing physics artifacts must read the applicable conventions/ file (extraction.md, search.md, or unfolding.md) and produce a Conventions Compliance Table documenting every required systematic source.

### Testing Requirements

- All code examples in agent prompts must be tested with uproot, awkward-array, hist, pyhf, and mplhep.
- All shell commands (pixi, git, bash) must be verified to work in the analysis directory.
- All agents must test their deliverables against the checklists in methodology/appendix-checklist.md before producing final artifacts.

### Common Patterns

- **Artifact naming:** `{ARTIFACT}_{session_name}_{timestamp}.md` (e.g., `STRATEGY_phase1_20260412_143021.md`)
- **No overwrites:** Create new files alongside previous versions; do not mutate prior artifacts.
- **Experiment log:** Read and append to `experiment_log.md` at session start and end.
- **RAG corpus queries:** Use MCP tools to query literature (search_lep_corpus, search_cms_papers, get_paper).
- **Figure standards:** All plots must use mplhep with experiment style (ATLAS, CMS, LHCb). See methodology/appendix-plotting.md.

## Dependencies

### Internal

- **To Methodology**: agents.yaml → agents.md → methodology/ (phases, blinding, review, tools, conventions)
- **To Orchestration**: agents.yaml → orchestration/agents.md (prompt templates for spawning subagents)
- **To Templates**: Templates reference phase-specific CLAUDE.md files that agents read at runtime

### External

- **Rust pipeline system** (researchmol backend): Injects prompt context and agent definitions
- **MCP corpus tools**: Literature search (search_lep_corpus, search_cms_papers, get_paper)
- **Docker sandbox** (researchmol/sandbox-hep:latest): Executes analysis code with HEP tools (uproot, pyhf, mplhep, xgboost)
- **ROOT / NTuple data**: User-supplied input files in analysis directory
