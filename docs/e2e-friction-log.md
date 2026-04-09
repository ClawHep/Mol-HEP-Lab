# E2E Pipeline Friction Log

Date: 2026-04-09

## Test Setup
- Config: `config.mol.yaml` with `provider: "acp"`, `agent: "claude"`
- Topic: HEP jet classification (from existing config)
- Mode: `--auto-approve --skip-preflight --skip-noncritical`

---

## Friction Point #1: Provider priority ignores explicit config

**Severity**: HIGH
**File**: `crates/mol-llm/src/provider.rs:114-188`
**Problem**: `create_provider()` checks for API keys in env BEFORE checking `provider: "acp"`.
If user has `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in their env (common for developers),
the pipeline silently routes to API mode even when config explicitly says `provider: "acp"`.
**Expected**: User's explicit `provider` setting should take priority.
**Fix**: Check `config.llm.provider` first. If it explicitly says "acp", skip API key detection.

**Status**: FIXED — `create_provider()` now checks `config.llm.provider` first;
`"acp"` goes directly to CLI provider, `"api"` to API, `"none"` disables LLM.
Auto-detect only kicks in when provider field is empty/unrecognized.

---

## Friction Point #2: No progress feedback during ACP LLM calls

**Severity**: MEDIUM
**File**: `crates/mol-llm/src/acp.rs`
**Problem**: ACP calls take 1-5 minutes per stage. During this time the user sees
no terminal output. Only heartbeat.json updates (not visible in terminal).
**Expected**: Periodic "still waiting for LLM response..." log messages.
**Fix**: Add a background progress ticker in the ACP invoke method.

**Status**: DEFERRED (not blocking, heartbeat.json works for monitoring)

---

## Friction Point #3: ACP session state leaks between runs

**Severity**: HIGH
**File**: `crates/mol-llm/src/acp.rs:192` (ensure_session)
**Problem**: `goal.md` contained meta-commentary ("Both files have already been created...")
instead of actual research goal. `sessions ensure` reused an existing session with prior
context, causing Claude to think the work was already done.
**Expected**: Each pipeline run starts with clean LLM context.
**Fix**: Changed `ensure_session()` to always close+recreate session (`sessions close` then
`sessions new`), falling back to `sessions ensure` only if `new` fails.

**Status**: FIXED — After fix: TOPIC_INIT dropped from 331s to 52s, correct content.

---

## Friction Point #4: Thinking blocks not fully stripped from artifacts

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/executor.rs:318` (strip_llm_noise)
**Problem**: `goal.md` still contained `[thinking]...[/thinking]` blocks in output.
The original `strip_llm_noise` only handled simple cases (single `\n\n` delimiter).
**Expected**: All thinking blocks stripped from artifact content.
**Fix**: Rewrote `strip_llm_noise` to handle multi-line `[thinking]...[/thinking]` and
`<thinking>...</thinking>` blocks, including unclosed blocks.

**Status**: FIXED

---

## E2E Run Results (after fixes)

| Stage | Time | Artifacts | Status |
|-------|------|-----------|--------|
| 01 TOPIC_INIT | 51.8s | goal.md, hardware_profile.json | ✅ |
| 02 PROBLEM_DECOMPOSE | 49.0s | problem_tree.md, topic_evaluation.json | ✅ |
| 03 SEARCH_STRATEGY | 248.4s | search_plan.yaml, sources.json, queries.json | ✅ |
| 04 LITERATURE_COLLECT | ~300s+ | candidates.jsonl | ✅ (running) |
| 05-26 | — | — | In progress... |

Pipeline is running end-to-end via ACP→Claude CLI with fresh sessions.
