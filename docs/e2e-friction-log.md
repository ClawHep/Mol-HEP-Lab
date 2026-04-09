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
| 04 LITERATURE_COLLECT | ~300s+ | candidates.jsonl | ⚠️ narrative text, not JSONL |
| 05 LITERATURE_SCREEN | ~1s | screened_papers.jsonl (empty), exclusion_reasons.json | ⚠️ cascading failure |
| 06 KNOWLEDGE_EXTRACT | running | — | In progress |
| 07-26 | — | — | Pending... |

Pipeline is running end-to-end via ACP→Claude CLI with fresh sessions.

---

## Friction Point #5: candidates.jsonl contains narrative text instead of JSONL

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/executor.rs:381` (strip_markdown_fences)
**Problem**: Claude returns a long narrative response with embedded code blocks instead of
raw JSONL. `strip_markdown_fences()` only handled the case where the entire response was
wrapped in a single code fence. When the response is narrative text with embedded fences,
the structured data was never extracted.
**Expected**: `candidates.jsonl` should contain one JSON object per line.
**Fix**: Rewrote `strip_markdown_fences()` to handle embedded fences — extracts the
largest fenced block when the response isn't a single top-level fence.

**Status**: FIXED

---

## Friction Point #6: screened_papers.jsonl empty due to cascading failure

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/stages_impl/phase2.rs:225`
**Problem**: `execute_literature_screen()` iterates `candidates.jsonl` lines, tries
`serde_json::from_str()` on each. When candidates.jsonl is narrative text, 0 lines parse.
The fallback (`screened.is_empty() && candidates_text.is_empty()`) doesn't trigger because
candidates_text is non-empty (it contains narrative). Result: empty screened_papers.jsonl.
**Expected**: Fallback should trigger when 0 valid JSON lines are parsed.
**Fix**: Changed condition to `screened.is_empty() && excluded.is_empty()` so fallback
triggers whenever no JSON lines were successfully parsed, regardless of raw text content.

**Status**: FIXED

---

## Friction Point #7: User-provided data not wired to experiment stages

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/executor.rs`, `crates/mol-pipeline/src/runner.rs`,
  `crates/mol-cli/src/commands/run.rs`
**Problem**: Users configure `experiment.datasets_dir` in config.mol.yaml pointing to their
own data files, but the pipeline had no mechanism to:
1. Make that directory available in the experiment workspace
2. Tell the LLM "use ONLY these files" instead of public datasets
Literature collection is independent and runs as normal regardless.
**Expected**: If `datasets_dir` is set, experiment stages see **only** user data. If not set,
they see public datasets from `datasets.yaml`.
**Fix**:
1. Added `datasets_dir` field to pipeline's `MolConfig` struct, wired from `experiment.datasets_dir`.
2. Runner symlinks `datasets_dir` → `{run_dir}/datasets/` at pipeline start.
3. Template variable `{{ datasets }}` lists local file names with "use ONLY these files"
   when `datasets_dir` is set; falls back to `datasets.yaml` (public URLs) when empty.
4. CLI: added `--data <dir>` flag (`-d` short) — overrides config, per-experiment.

**Status**: FIXED

---

## Friction Point #8: LLM returns narrative text instead of JSON for structured artifacts

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/executor.rs`, `crates/mol-pipeline/src/stages_impl/phase2.rs`
**Problem**: Stages that produce `.json` / `.jsonl` artifacts call `llm_generate()` with
`json_mode: true`, but ACP/CLI providers ignore `json_mode` (the underlying agent manages
its own output format). Result: Claude returns narrative markdown with tables instead of
valid JSON. `strip_markdown_fences` can extract code-fenced JSON, but when the LLM writes
no code fences at all, there's nothing to extract.
**Expected**: Structured artifacts should always be valid JSON/JSONL.
**Fix**: Two-pass active extraction pattern (model-agnostic):
1. **Pass 1**: Call LLM with original prompt — let it respond in any format (narrative OK).
2. **Check**: Try to parse as JSON / extract via `extract_json_block()` (brace-matching).
3. **Pass 2** (if needed): Send a focused extraction prompt: "from the analysis above,
   output ONLY valid JSON". This is a trivial format-conversion task that works across
   all models, because the hard reasoning is already done in Pass 1.
4. **Upgraded to agentic executor** (`execute_agentic`): each stage declares `ArtifactSpec`
   (filename, format, description, schema_hint). The executor runs Phase 1 (free analysis),
   then Phase 2 (per-artifact focused extraction with schema + validation + retry).
5. Converted stages: LITERATURE_COLLECT, KNOWLEDGE_EXTRACT, SYNTHESIS, HYPOTHESIS_GEN.
6. Helper functions: `is_valid_json()`, `collect_json_lines()`, `extract_json_block()`,
   `validate_artifact_format()`, `strip_llm_preamble()`.

**Status**: FIXED — architecture now model-agnostic; remaining stages can adopt `execute_agentic` incrementally

---

## Friction Point #9: ACP agent plan/task tracking leaks into artifacts

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/executor.rs` (strip_llm_noise, strip_llm_preamble)
**Problem**: When using ACP→Claude CLI, the agent returns internal task tracking
artifacts like `[plan]` blocks (`- [in_progress]`, `- [completed]`), session
meta-commentary ("Good. The conventions and methodology docs exist..."), and
artifact summaries ("Three artifacts produced:"). These leak into markdown
artifacts (e.g., `goal.md`) because:
1. `strip_llm_noise` didn't handle `[plan]` blocks
2. `strip_llm_preamble` didn't handle "Good."/"Perfect."/"Understood." patterns
3. Phase 1 analysis wasn't passed through `strip_llm_noise` before Phase 2 extraction
**Expected**: Artifacts should contain only the actual content, no agent internals.
**Fix**:
1. Added `[plan]` block stripping to `strip_llm_noise()`
2. Extended `strip_llm_preamble()` with more ACP chatter patterns
3. Added `strip_llm_noise()` call between Phase 1 and Phase 2 in `execute_agentic()`

**Status**: FIXED

---

## Friction Point #10: YAML artifact extraction fails — agent returns meta-commentary

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/executor.rs` (extract_artifact, ArtifactFormat::Yaml)
**Problem**: When the agentic Phase 2 extraction requests a YAML artifact from the
ACP agent, Claude returns meta-commentary ("The pipeline is requesting raw YAML...
the file already exists at...") instead of actual YAML content. JSON extraction
works correctly (sources.json, queries.json both valid), but YAML extraction fails.
**Expected**: `search_plan.yaml` should contain a valid YAML search plan.
**Fix**: TODO

**Status**: INVESTIGATING
