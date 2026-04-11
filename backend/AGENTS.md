<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Backend

## Purpose

This directory contains runtime execution artifacts for the Mol-HEP-Lab pipeline. It holds pipeline run queues, checkpoints, and execution metadata generated during `mol run` operations. **These files are auto-generated and should never be manually edited.**

## Key Directories

| Directory | Purpose |
|-----------|---------|
| `runs/` | Parent directory for all pipeline run artifacts |
| `runs/queues/` | Queue state and staged execution tasks (internal use only) |

## For AI Agents

### Reading Runtime Artifacts

**SAFE**: You may READ and ANALYZE files here for debugging:
- Inspect checkpoint files to understand pipeline state
- Review logs from failed stages
- Extract intermediate results for analysis
- Verify artifact consistency

**FORBIDDEN**: Do NOT modify files here manually:
- No direct edits to run state files
- No deletion of queues or checkpoints
- No renaming or moving artifacts
- All mutations must flow through the pipeline state machine

### Understanding Run Structure

Each completed run creates artifacts like:

```
backend/runs/
└── mol-<date>-<time>-<hash>/
    ├── stage-01_topic_formulation/
    │   ├── output.md
    │   └── metadata.json
    ├── stage-02_literature_retrieval/
    │   ├── candidates.jsonl
    │   └── metadata.json
    ├── ...
    ├── stage-18_publish/
    │   ├── paper.pdf
    │   ├── paper.tex
    │   ├── references.bib
    │   └── metadata.json
    ├── pipeline_state.json
    ├── checkpoints.json
    └── run_log.txt
```

### Debugging Runtime Issues

If a pipeline fails:

1. **Check the run log**:
   ```bash
   cat backend/runs/mol-<run_id>/run_log.txt | tail -100
   ```

2. **Inspect the pipeline state**:
   ```bash
   cat backend/runs/mol-<run_id>/pipeline_state.json | jq '.current_stage'
   ```

3. **Find the failing stage**:
   ```bash
   ls -lh backend/runs/mol-<run_id>/stage-*/
   # Check which stage has the most recent timestamp but no output
   ```

4. **Resume from checkpoint**:
   ```bash
   mol run --resume mol-<run_id>
   ```

### Cleanup

To remove old runs (optional, after verification they're complete):

```bash
# List all runs (oldest first)
ls -1t backend/runs/ | tail -10

# CAREFUL: Only delete after confirming completion
# rm -rf backend/runs/mol-<old-run-id>
```

**Do not delete runs that are currently in progress.**

## Dependencies

### Internal
- **mol-cli**: Reads/writes checkpoints here during execution
- **mol-pipeline**: Manages state files and stage artifacts
- **mol-web**: May serve artifacts via HTTP for download
- **mol-services**: May clean up old run directories (if configured)

### External
- File system (POSIX-compatible directory structure)
- JSON format for metadata

## File Formats

### Pipeline State File (`pipeline_state.json`)

```json
{
  "run_id": "mol-20260409-192434-9e9916",
  "start_time": "2026-04-09T19:24:34Z",
  "current_stage": 18,
  "stages_completed": [1, 2, 3, ..., 18],
  "gates_passed": ["literature_screening", "experiment_design"],
  "decision_pivots": 0,
  "status": "completed" | "in_progress" | "paused" | "failed"
}
```

### Checkpoint File (`checkpoints.json`)

```json
{
  "stage_number": 5,
  "stage_name": "experiment_design",
  "timestamp": "2026-04-09T21:30:15Z",
  "outputs": {
    "experiment_plan": "stage-05_experiment_design/experiment_plan.json",
    "code": "stage-05_experiment_design/code.py"
  }
}
```

### Stage Metadata (`stage-XX_*/metadata.json`)

```json
{
  "stage_number": 5,
  "stage_name": "experiment_design",
  "agent": "ExperimentDesignAgent",
  "status": "completed" | "in_progress" | "failed",
  "started_at": "2026-04-09T21:15:00Z",
  "completed_at": "2026-04-09T21:30:00Z",
  "duration_seconds": 900,
  "llm_calls": 3,
  "tokens_used": {"prompt": 5000, "completion": 3000},
  "artifacts": ["experiment_plan.json", "code.py"]
}
```

## Relationships

- **mol-cli** (`serve`, `run`, `resume` commands) — creates and manages runs
- **mol-pipeline** (state machine) — advances through stages, writes checkpoints
- **frontend** — may display run history and real-time progress
- **docs/** — may reference specific run IDs in HANDOFF.md or friction logs

## Safety

**This directory should be treated as volatile**:
- Runs can be resumed, paused, or restarted
- Old runs can be archived or deleted (after confirmation they're complete)
- The pipeline system automatically manages cleanup (configurable in config.mol.yaml)

**Backup important runs**:
```bash
# Before deletion, archive significant runs
tar -czf archive/mol-<run_id>.tar.gz backend/runs/mol-<run_id>/
```

---

See `../AGENTS.md` for root-level context.
See `../docs/HANDOFF.md` for debugging guidance on specific run failures.
See `crates/mol-cli/src/commands/resume.rs` for resume implementation.
