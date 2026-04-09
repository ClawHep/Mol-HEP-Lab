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

**Status**: INVESTIGATING (less critical after agent-direct-write migration)

---

## Friction Point #11: `[thinking]` blocks leak into fallback artifacts

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/executor.rs` (`simple_clean()`)
**Problem**: When agent doesn't write a file via Write tool, `execute_agentic()` falls
back to writing the raw LLM response as the artifact. This response includes `[thinking]`
blocks (Claude's internal reasoning), polluting the artifact with non-content text.
Observed in `stage-12/sanity_report.md` during E2E run.
**Expected**: Artifacts should contain only the actual content.
**Fix**: Added `[thinking]...[/thinking]` block stripping to `simple_clean()`.

**Status**: FIXED

---

## Friction Point #12: No `stage_log.md` for traceability

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/runner.rs`
**Problem**: Completed stages have no execution log. Only heartbeat.json and
checkpoint.json are updated. When debugging pipeline issues, there's no per-stage
record of timing, artifacts, status, and decision.
**Expected**: Each stage should produce a `stage_log.md` documenting what happened.
**Fix**: Added `write_stage_log()` in runner.rs, called after heartbeat write.
Produces markdown table with stage name, phase, status, decision, elapsed time,
and artifact list with file sizes.

**Status**: FIXED

---

## Friction Point #13: SANITY_CHECK doesn't run code or iterate

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs`
**Problem**: `execute_sanity_check()` only does multi-agent LLM review (cross-checker
+ plot-validator). It never actually runs the experiment code, doesn't use the
`runtimes/sanity_check` workspace/fix-loop infrastructure, and can't fix issues.
The sanity_check runtime has all the tools (prepare_workspace, check_success,
copy_fixes_back) but they were never wired into the stage executor.
**Expected**: Sanity check should: prepare workspace → run code → detect errors →
fix iteratively → copy fixes back → THEN do multi-agent review.
**Fix**: Rewrote `execute_sanity_check()` to use the full iterative runtime before
falling through to multi-agent review.

**Status**: FIXED

---

## Friction Point #14: Review findings are write-only (no rework loop)

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs`, `executor.rs`
**Problem**: Multi-agent reviewers (plot-validator, critical-reviewer, etc.) write
review reports but pipeline never reads them back. If a reviewer finds CRITICAL
issues, the pipeline just moves to the next stage — no feedback → fix → re-review.
**Expected**: After multi-agent review, check reports for CRITICAL/FAIL findings.
If found, feed back to primary agent to fix, then re-review (bounded loop).
**Fix**: Implemented `execute_multi_agentic_with_rework()` in executor.rs — generic rework
wrapper that runs multi-agent review, scans reviewer artifacts for trigger phrases
(CRITICAL, FAIL, MUST FIX, etc.), feeds findings back to primary agent, and re-reviews.
Bounded by `max_rework` parameter (set to 2 for all stages).
Wired into: SANITY_CHECK, PEER_REVIEW, QUALITY_GATE.

**Status**: FIXED

---

## Friction Point #15: CODE_GENERATION agent runs full experiment

**Severity**: LOW (positive surprise)
**File**: Observed in E2E stage-11
**Problem**: The CODE_GENERATION agent (stage 11) not only writes code but also
executes it, generating 29 files including figures (PDF+PNG), pyhf workspaces,
and run reports. This goes beyond the intended scope (code writing only), but
the outputs are high quality (Z resonance fits, Brazil bands, cutflow plots).
**Impact**: EXPERIMENT_RUN (stage 14) may be partially redundant. Consider whether
to constrain CODE_GENERATION or embrace the agent's initiative.
**Fix**: No fix needed — document as expected agentic behavior. EXPERIMENT_RUN
will still run the "official" experiment with proper resource tracking.

**Status**: NOTED

---

## Friction Point #16: Front-end / Back-end artifact name mismatch

**Severity**: HIGH
**File**: `frontend/src/types.ts`, `crates/mol-services/src/agent_bridge.rs`
**Problem**: Pipeline produces artifacts with different names than frontend expects.
Examples: pipeline writes `synthesis_report.md` but frontend expects `synthesis.md`;
pipeline writes `analysis_report.md` but frontend expects `analysis.md`.
Also: agent_bridge.rs `stage_outputs()` and `stage_to_layer()` are missing stages
23-26 entirely. New multi-agent artifacts (plot_validation.md, critical_review.md,
etc.) have no frontend registration.
**Fix**: TODO — align after backend stabilizes. See full mismatch table in
conversation log dated 2026-04-09.

**Status**: TODO (deferred until backend stable)

---

## Friction Point #17: Claude Code context window exhaustion during E2E monitoring

**Severity**: HIGH
**File**: N/A (tooling limitation)
**Problem**: When using Claude Code to monitor a long-running pipeline E2E test, the
conversation context fills up with repeated heartbeat checks, artifact reads, large code
diffs, and friction-point analysis. The session hits the context limit and requires a
"continue from summary" restart, losing fine-grained state (exact line numbers being
edited, in-progress rework reasoning, uncommitted code changes).
Two prior sessions were lost this way during the 2026-04-09 E2E run:
1. First session: exhausted after implementing multi-agent parallel execution + stage_log +
   iterative sanity check + monitoring stages 1-12
2. Second session: exhausted after implementing rework loop + resuming pipeline at stage 14
**Expected**: Monitoring should not consume context so aggressively.
**Mitigation**:
1. Use `run_in_background` for pipeline monitoring instead of `sleep N && check`
2. Commit code changes frequently before context fills
3. Record friction points immediately in docs/ rather than holding them in context
4. Use the friction log as durable memory instead of relying on conversation state

**Status**: MITIGATED (process change, not a code fix)

---

## Friction Point #18: Pipeline resume generates new run_id, confusing

**Severity**: LOW
**File**: `crates/mol-cli/src/commands/run.rs`
**Problem**: When resuming with `--resume --output <existing-dir>`, the pipeline prints a
NEW run_id (`mol-20260409-165938-9e9916`) even though it writes to the old directory.
Heartbeat shows the new run_id but checkpoint shows the old one. This is cosmetically
confusing but functionally correct — resume works, stages continue from checkpoint.
**Expected**: Resume should reuse the original run_id, or at least log clearly that
it's continuing a prior run.
**Fix**: Minor — cosmetic only, not blocking.

**Status**: NOTED
