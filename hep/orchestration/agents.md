<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Agent Orchestration

## Purpose

This directory defines the operational implementation of the methodology spec for Claude Code agent systems. Methodology (`../methodology/`) defines *why* and *what*; orchestration defines *how* to execute it with agents. It includes the agent roster and prompt launch templates, automation pseudocode, session isolation model, and integration with MCP corpus tools and experiment scaffolding.

## Key Files

| File | Description |
|------|-------------|
| agents.md | Agent roster (18 agents with models), phase-to-agent mapping, review tiers, execution + review + advisor launch templates |
| automation.md | Orchestration loop pseudocode: dispatch, monitor, review, decision, archive, transition; regression handling; example flows |
| sessions.md | Session naming conventions, directory layout (ASCII tree), isolation model, concurrency rules |
| integration.md | RAG corpus setup (MCP tools), Claude Code team mapping, adaptation to other HEP systems |
| README.md | Overview, relationship to methodology and templates |

## Orchestration Loop

The orchestrator is a state machine that cycles through the 18-stage pipeline:

```
Loop:
  1. Read State        ← Load experiment_log.md, prior artifacts
  2. Dispatch          ← Spawn primary + advisor agents for current stage
  3. Monitor           ← Wait for deliverables, collect logs
  4. Review            ← Spawn reviewer agents (correctness → completeness → clarity)
  5. Decide            ← PASS (advance) / ITERATE (re-execute) / ESCALATE (human)
  6. Archive           ← Append to experiment_log.md, move artifacts to phase directory
  7. Transition        ← Advance pipeline state, repeat
```

Review gates (stages 2.2, 3.1, 4.2, 5.5) require explicit human approval before advancing.

## Agent Roster

**Execution:** 11 agents (lead-analyst, theory-scout, data-explorer, detector-specialist, signal-lead, ml-specialist, background-estimator, systematic-source-evaluator, systematics-fitter, cross-checker, note-writer)

**Review:** 6 agents (physics-reviewer, critical-reviewer, constructive-reviewer, rendering-reviewer, plot-validator, arbiter)

**Support:** 1 agent (investigator, regression analysis)

See `agents.md` for full roster with models, phase mappings, and launch templates.

## Session Model

Each analysis is isolated in `backend/runs/projects/<project_id>/analyses/{analysis_name}/`:

```
analyses/{analysis_name}/
  prompt.md                    ← Physics prompt
  analysis_config.yaml         ← Config (channels, data location)
  experiment_log.md            ← Append-only session history
  conventions/ → (symlink)     ← hep/conventions
  
  exec/                        ← Output artifacts
    STRATEGY_*.md, HYPOTHESES_*.md, FIT_RESULTS_*.md, etc.
  
  phase1/, phase2/, phase3/, phase4/, phase5/
    review/                    ← Review feedback per phase
    artifacts/                 ← Finalized artifacts
```

Agents never modify files outside the analysis directory. Conventions and methodology are read-only reference (symlinked or injected).

## For AI Agents

### Working In This Directory

1. **Orchestration implements methodology:** When spawning an agent, you implement the methodology spec (03-phases.md, 04-blinding.md, 06-review.md). Never deviate from the spec.

2. **Agent prompts are injected dynamically:** Templates in agents.md are copied and customized when spawning subagents. Do not hardcode; reference this directory.

3. **Review tier order is fixed:** Correctness → Completeness → Clarity. Do not skip tiers or reorder them.

4. **Session isolation is strict:** Agents read/write only within their active analysis directory. Conventions and methodology are symlinked in as read-only reference.

5. **Experiment log is the session record:** Append all significant events (dispatch, deliverable, review feedback, decisions) to experiment_log.md.

### Testing Requirements

- All agent templates in agents.md must have valid YAML headers (name, description, tools, model)
- All session directory paths must be creatable and writable (test mkdir, cd, file ops)
- All integration points (MCP tools, Docker sandbox, git) must be testable
- The automation loop must gracefully handle common failures (timeout, review rejection, missing data)

### Common Patterns

- **Agent dispatch:** Inject phase template (`templates/phase{N}_claude.md`), methodology references, prior artifacts, context
- **Deliverable naming:** `{ARTIFACT}_{session_name}_{timestamp}.md` (e.g., `STRATEGY_phase1_20260412_143021.md`)
- **Review aggregation:** Collect 3–6 reviewer outputs; tally votes; majority rules or escalate splits
- **Error recovery:** On timeout/failure, log the failure, optionally retry with reduced scope, or escalate to human
- **Context injection:** Use Tera template syntax `{{variable_name}}` in agent prompts

## Dependencies

### Internal

- **To Methodology:** orchestration/ implements 03-phases.md, 03a-orchestration.md, 06-review.md, 05-artifacts.md
- **To Agents:** agents.md references all 18 agent definitions from `../agents/`
- **To Templates:** phase templates (`../templates/phase{N}_claude.md`) are injected into agent context
- **To Conventions:** symlinked into analysis directories; agents read them at Phase 1 and Phase 4a

### External

- **Rust pipeline backend:** Spawns orchestrator, manages session directories, enforces data access controls
- **MCP corpus tools:** `search_lep_corpus`, `search_cms_papers`, `get_paper`, `compare_measurements`
- **Docker sandbox:** researchmol/sandbox-hep:latest; executes analysis code with HEP tools
- **Claude Code team infrastructure:** Spawns subagents, manages session state, aggregates reviews
- **Git:** Tracks analysis code and results in analysis directory
