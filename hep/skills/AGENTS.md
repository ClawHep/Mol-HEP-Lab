<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP Pipeline Orchestration Skills

## Purpose

This directory contains executable skill files that implement specific HEP analysis pipeline operations. Skills are user-invocable or orchestration-invoked commands that manage analysis state, run phases, review work, and gate critical decisions. Each skill is a markdown file with a YAML header and an instruction prompt that the pipeline system executes.

## Key Skills

| Skill | User-Invocable | Purpose |
|-------|-----------------|---------|
| run-analysis | Yes | Initialize and run the full automated analysis pipeline from physics prompt to publication |
| run-phase | No | Execute a single phase (orchestration-invoked) |
| review-phase | No | Run review gates for a completed phase (orchestration-invoked) |
| approve-unblinding | Yes | Human approval gate for Phase 4c unblinding (after Phase 4a expected results) |
| check-status | Yes | Query current analysis progress, phase state, pending reviews, bottlenecks |

## Skill Signatures

### run-analysis

```yaml
name: run-analysis
description: Initialize and run the full automated HEP analysis pipeline
user-invocable: true
```

**Arguments:** Physics prompt (inline text or `.md` file path), optional analysis config (`.yaml` file path)

**Responsibilities:**
1. Parse physics prompt and config
2. Scaffold analysis directory structure
3. Read methodology (phases, blinding, review)
4. Dispatch agents through 18-stage pipeline
5. Manage phase transitions and review gates
6. Track experiment log and artifacts
7. Coordinate with human gates (unblinding approval)

### run-phase

```yaml
name: run-phase
description: Execute a single phase with agent dispatch
user-invocable: false
```

**Arguments:** Phase number (1–5), agent name, context (methodology, prior artifacts)

**Responsibilities:**
1. Load current analysis state
2. Inject phase template into agent context
3. Spawn primary + advisor agents
4. Wait for deliverables
5. Append to experiment_log.md
6. Transition to review if gate present

### review-phase

```yaml
name: review-phase
description: Run review tier for completed phase
user-invocable: false
```

**Arguments:** Phase number, artifact path, review tier (correctness / completeness / clarity)

**Responsibilities:**
1. Spawn reviewer agents (physics, critical, constructive, plot-validator, rendering)
2. Aggregate review feedback
3. Classify issues (A / B / C)
4. Produce arbiter decision (PASS / ITERATE / ESCALATE)
5. Record decision in experiment_log.md

### approve-unblinding

```yaml
name: approve-unblinding
description: Human approval gate for Phase 4c signal region unblinding
user-invocable: true
```

**Arguments:** Analysis ID, human decision (approve / reject), optional justification

**Responsibilities:**
1. Verify Phase 4a (expected) results are complete and reviewed
2. Check for human gate readiness (arbiter decision)
3. Update analysis config: `unblinding_approved: true`
4. Log decision and timestamp to experiment_log.md
5. Signal orchestrator to proceed to Phase 4c

### check-status

```yaml
name: check-status
description: Query analysis state and progress
user-invocable: true
```

**Arguments:** Analysis ID

**Output:** JSON status report with:
- Current phase and stage
- Completed phases + review decisions
- Pending reviews (tier, reviewer, deadline)
- Bottlenecks (blocked stages, failed gates)
- Recent experiment log entries
- Artifact locations and timestamps

## For AI Agents

### Working In This Directory

1. **Skills are executable instructions:** Each skill file contains a YAML header and a prompt. The orchestration system reads the header to understand invocation, then executes the prompt as an agent instruction.

2. **YAML header metadata:**
   - `name`: Unique skill identifier (kebab-case)
   - `description`: One-sentence summary
   - `user-invocable`: Whether a user can directly invoke this skill (vs. orchestration-only)

3. **Skill execution:** When invoked, the skill's prompt is injected into an orchestrator agent context along with:
   - Analysis ID and current state
   - Methodology references
   - Prior artifacts and experiment log
   - Any user-provided arguments

4. **State mutation:** Skills modify analysis state (experiment_log.md, analysis_config.yaml). Mutations are logged with timestamps.

5. **Error handling:** Skills must gracefully handle missing analyses, invalid states, incomplete phases, and user input errors.

### Testing Requirements

- All skills must be testable with mock analysis directories
- run-analysis must scaffold valid directory structure
- run-phase must read phase templates from `../templates/`
- review-phase must spawn reviewer agents and aggregate feedback
- approve-unblinding must verify Phase 4a completion before allowing unblinding
- check-status must parse experiment_log.md and return valid JSON

### Common Patterns

- **State queries:** Read experiment_log.md (append-only log) and latest artifacts
- **State mutations:** Append timestamped entries to experiment_log.md before mutating analysis_config.yaml
- **Gating logic:** Check preconditions (prior phase completion, review PASS decisions) before allowing transitions
- **User validation:** Prompt for confirmation on destructive operations (unblinding approval)
- **Rollback safety:** Log all decisions; support analysis restart from any phase with state reconstruction

## Dependencies

### Internal

- All skills depend on `../methodology/03-phases.md` (phase definitions)
- All skills depend on `../methodology/04-blinding.md` (unblinding protocol)
- All skills depend on `../methodology/06-review.md` (review tier definitions)
- All skills depend on `../orchestration/agents.md` (agent spawn templates)
- All skills depend on `../templates/phase{N}_claude.md` (phase context)

### External

- Rust pipeline backend (researchmol): Manages analysis directories, enforces permissions
- Claude Code orchestrator: Spawns agents, aggregates reviews
- MCP tools: Literature corpus queries (search_lep_corpus, get_paper)
- Docker sandbox: Executes analysis code
- Git: Version control for analysis code and artifacts
