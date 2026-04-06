# Mol-HEP-Lab Rust Migration — Handoff Document

**Date:** 2026-04-06  
**Status:** Phase 1 in progress, Phase 2-7 planned

---

## What Was Done This Session

### 1. Deep Audit of Python vs Rust Parity
- Read every Python file (~51K LOC across 210 files) and every Rust file (~25K LOC across 120 files)
- Found Rust is **much more complete than initially thought** (60-95% per crate)
- The main gap is: **26 pipeline stage implementations are all stubs** in `mol-pipeline/src/executor.rs`

### 2. Comprehensive Implementation Plan Written
- **Location:** `docs/superpowers/plans/2026-04-06-rust-full-migration.md`
- 7 phases, 35 tasks, ~13,600 lines of new Rust code estimated
- Plan was reviewed by code-reviewer agent, 3 critical issues fixed:
  - Added 3 missing Python modules (experiment_schema, prompt_adapter, web/agent)
  - Added Discussion stage to Phase 4
  - Fixed data file migration ordering (before Python deletion)

### 3. Git Repository Initialized
- Initial commit: `4296294` on `master` branch
- All existing code committed as baseline

### 4. Task 1.1 (Rust Branding) Dispatched
- Running in background — replaces ResearchClaw/ClawAI/MetaClaw/OpenClaw references in Rust code with Mol-HEP-Lab

---

## What Needs To Be Done Next

### Immediate Next Steps (in order)

1. **Check Task 1.1 completion** — verify branding agent finished, run spec review
2. **Execute Task 1.2** — Update Docker image refs and showcase markdown files
3. **Execute Phase 2 tasks (2.1-2.10)** — 10 utility module ports, can be done sequentially

### Execution Method

Use **Subagent-Driven Development** (skill: `superpowers:subagent-driven-development`):
- Fresh agent per task
- Two-stage review after each: spec compliance, then code quality
- Plan file has full task descriptions with code examples

To continue, tell the new session:
```
继续执行 docs/superpowers/plans/2026-04-06-rust-full-migration.md 的计划。
使用 subagent-driven development。从 Task 1.2 开始（如果 1.1 已完成），或检查 1.1 状态。
```

### Task Status Overview

| Task ID | Phase | Status |
|---------|-------|--------|
| #2 (1.1) | Branding - Rust | in_progress (background agent) |
| #3 (1.2) | Branding - Config | pending |
| #4-13 (2.1-2.10) | Utilities | pending |
| #14-17 (3.1-3.4) | Agent Runtimes | pending (depends on 2.1) |
| #18-28 (4.1-4.11) | Pipeline Stages | pending (depends on Phase 3) |
| #29-31 (5.1-5.3) | Code Agent + Bridges | pending (depends on Phase 2) |
| #32 (6.1) | Services | pending |
| #33-36 (7.1-7.4) | Verification + Deletion | pending (depends on ALL) |

### Dependency Chain
```
Phase 1 ───────────── can start now
Phase 2 ───────────── can start now  
Phase 3 ───────────── after Task 2.1 (adapters)
Phase 4 ───────────── after Tasks 2.8-2.10 + Phase 3
Phase 5 ───────────── after Phase 2
Phase 6 ───────────── can start now
Phase 7 ───────────── after ALL above
```

---

## Key Architecture Notes for Next Session

### Rust Crate Completion Status (verified by reading actual source)

| Crate | Real Status | Key Info |
|-------|------------|----------|
| mol-engine | **95%** | turn_loop.rs (783 lines, fully working), all 6 tools implemented in tools/executor.rs (523 lines), codegen strategies done |
| mol-agents | **85%** | benchmark/figure orchestrators complete, code_searcher submodules need filling |
| mol-experiment | **90%** | docker.rs (503 lines, GPU/NPU), ssh.rs (405 lines), colab.rs (346 lines), validation.rs (405 lines) — ALL real |
| mol-pipeline | **Framework 100%, stages 0%** | executor.rs line 141-198: all 26 stages call `stub_stage()` |
| mol-services | **80%** | agent_bridge.rs is 28K+ lines (mostly complete) |
| All others | **90-100%** | Production-ready |

### Python Source Reference Paths
- Main package: `backend/agent/researchclaw/`
- Pipeline executor: `backend/agent/researchclaw/pipeline/executor.py` (9,832 lines — the big one)
- Agent runtimes: `backend/agent/researchclaw/pipeline/{sanity_check,experiment_run,iterative_refine,result_analysis}/runtime.py`
- Code agent: `backend/agent/researchclaw/pipeline/code_agent.py` (1,397 lines)
- Tests: `backend/agent/tests/`

### Stage Execution Pattern (how stubs need to be replaced)

Each stub in `executor.rs` looks like:
```rust
Stage::TopicInit => stub_stage(stage, &["topic_brief", "research_questions"]).await,
```

Replace with actual implementation that:
1. Reads prior artifacts via `read_prior_artifact(run_dir, filename)`
2. Calls LLM via `chat_with_prompt()` with domain-specific prompts
3. Falls back to template if no LLM
4. Writes output artifacts to `stage_dir`
5. Returns `StageResult` with status and artifact list

### Branding Mapping
- "ResearchClaw" → "Mol-HEP-Lab" / "mol"
- "Claw AI Lab" → "Mol-HEP-Lab"
- "MetaClaw" → "MetaMol"
- "OpenClaw" → "OpenMol"
- Docker: "researchclaw/sandbox-*" → "molheplab/sandbox-*"
- Cache: ".researchclaw_cache" → ".molheplab_cache"

---

## Memory Notes

- User wants **100% functional parity** — every Python feature must work identically in Rust
- User wants **Mol-HEP-Lab branding** throughout
- User prefers **全栈 Rust** — minimal non-Rust code acceptable only where necessary (TeX templates, frontend TS/CSS)
- The `backend/` Python directory should be **completely deleted** after verification
- User communicates in Chinese, understands English technical terms
