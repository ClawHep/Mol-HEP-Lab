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

**Status**: FIXED — Originally in `strip_llm_noise()` which was later refactored into
`simple_clean()`. Re-verified 2026-04-10: `[plan]` block stripping and ACP chatter
patterns ("Good.", "Perfect.", "Understood.") re-added to `simple_clean()`.

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

---

## Friction Point #19: Pipeline stages over-fragmented — inner loops split across stages

**Severity**: HIGH (architecture)
**File**: `crates/mol-pipeline/src/stages.rs`, all `stages_impl/` files
**Problem**: The 26-stage pipeline splits natural feedback loops into separate stages,
breaking the tight iteration cycle. Examples:
- CODE_GENERATION (11) produces code+figures, but SANITY_CHECK (12) is a separate stage
  that checks them — the agent can't fix-and-rerun in place.
- EXPERIMENT_RUN (14) and ITERATIVE_REFINE (15) are the same loop: run→check→fix→rerun.
- PAPER_DRAFT (20) and PAPER_REVISION (22) are writing→editing within one document.
- Three trivial cleanup stages at the end (24+25+26) don't justify separation.

**Expected**: Stages with internal feedback loops should be single stages with the loop
inside. Review/gate stages remain independent.

**Fix**: Merge 26 stages → 18 stages:

```
Phase 1: Strategy (2)
  1. TOPIC_INIT
  2. PROBLEM_DECOMPOSE

Phase 2: Exploration (4)
  3. LITERATURE_SEARCH        ← merge SearchStrategy + LiteratureCollect
  4. LITERATURE_SCREEN        (GATE)
  5. KNOWLEDGE_EXTRACT
  6. SYNTHESIS_HYPOTHESES     ← merge Synthesis + HypothesisGen

Phase 3: Execution (4)
  7. EXPERIMENT_DESIGN        (GATE)
  8. CODEBASE_SEARCH
  9. CODE_DEVELOP             ← merge CodeGeneration + SanityCheck
                                (write code → run → check figures → fix → rerun loop)
  10. EXPERIMENT_CYCLE        ← merge ResourcePlanning + ExperimentRun + IterativeRefine
                                (plan resources → run → iterate → converge)

Phase 4: Inference (3)
  11. RESULT_ANALYSIS
  12. RESEARCH_DECISION
  13. KNOWLEDGE_SUMMARY

Phase 5: Documentation (5)
  14. PAPER_OUTLINE
  15. PAPER_WRITE             ← merge PaperDraft + PaperRevision
                                (write → revise cycle, PeerReview triggers revision)
  16. PEER_REVIEW
  17. QUALITY_GATE            (GATE)
  18. PUBLISH                 ← merge KnowledgeArchive + ExportPublish + CitationVerify
```

**Merge rationale per group:**
- CODE_DEVELOP: agent writes code, runs it, sees figures, fixes issues in-place — one loop
- EXPERIMENT_CYCLE: resource estimation → run → check → refine is one convergence loop
- LITERATURE_SEARCH: strategy + execution are one continuous action
- SYNTHESIS_HYPOTHESES: synthesize knowledge → generate hypotheses is continuous reasoning
- PAPER_WRITE: draft + revision is the same writing loop
- PUBLISH: three lightweight cleanup steps → one finalization stage

**Not merged (independent roles):**
- TOPIC_INIT vs PROBLEM_DECOMPOSE: different goals (define vs decompose)
- PEER_REVIEW vs QUALITY_GATE: content review vs publication gate
- RESULT_ANALYSIS vs RESEARCH_DECISION: analysis vs decision (continue/stop/pivot)
- LITERATURE_SCREEN: explicit gate stage

**Implementation scope:**
1. Redefine `Stage` enum (18 variants + Discussion)
2. Merge stage_impl functions (inner loops stay inside merged stage)
3. Update contracts.rs, agents.yaml, all 26 templates → 18 templates
4. Update frontend stage mappings (deferred with #16)
5. Existing artifact dirs: stage-01..stage-18 (renumber)

**Status**: DONE — 18-stage enum, executors, contracts, templates all implemented.
Cleanup completed 2026-04-10: removed 28 orphaned templates (14 × 2 dirs),
fixed stale stage refs in stage_skill_map.rs, agent_bridge.rs, validate.rs.

---

## Friction Point #20: hep/ ↔ pipeline alignment audit (2026-04-10)

**Severity**: MEDIUM (cleanup)
**Files**: Multiple
**Problem**: After 26→18 stage consolidation, several files still referenced the old
26-stage pipeline numbering or contained orphaned artifacts.
**Findings & Fixes**:
1. `stage_skill_map.rs:task_type_for_stage()` — referenced stages 19-23 → FIXED (remapped to 18-stage)
2. `agent_bridge.rs:repo_for_stage()` — referenced stages 19-22 → FIXED (remapped to 18-stage)
3. `mol-config/validate.rs` — allowed stages 1-23 → FIXED (now 1-18)
4. 14 orphaned template files × 2 dirs (hep/ + generic/) → REMOVED
5. `hep/skills/*.md` — stale `src/methodology/` paths → FIXED to `hep/methodology/`
6. `hep/skills/` — confirmed REFERENCE-ONLY (not loaded by pipeline); domain knowledge
   already captured in hep/methodology/, hep/agents/, hep/prompts/

**Verified working**:
- All 19 templates (18 stages + Discussion) present with `---user---` delimiter
- All 18 `execute_*` functions present and dispatched
- All 18 agents loaded via agents.yaml
- All 3 domain prompts loaded via chain_backed.rs
- domain.yaml and datasets.yaml loaded
- Full `cargo test` passes

**Status**: FIXED

---

## Friction Point #21: Agent flexibility audit — four rigidity fixes (2026-04-10)

**Severity**: MEDIUM-HIGH (agent effectiveness)
**Files**: stages.rs, executor.rs, sanity_check.rs, phase3.rs
**Problem**: Deep audit of executor/runner found four patterns limiting agent effectiveness:

### Fix 21a: Decision rollback disabled (MAX_DECISION_PIVOTS = 0)
**File**: `crates/mol-pipeline/src/stages.rs:331`
**Was**: `MAX_DECISION_PIVOTS: u32 = 0` — ResearchDecision stage could signal "pivot" or
"refine" but pipeline ignored it and always forced PROCEED.
**Fix**: Changed to `MAX_DECISION_PIVOTS: u32 = 2` — pipeline now responds to decision
signals: "pivot" → rollback to SynthesisHypotheses, "refine" → rollback to ExperimentCycle.
Bounded to 2 loops to prevent infinite recursion.

### Fix 21b: Agent extra artifacts silently discarded
**File**: `crates/mol-pipeline/src/executor.rs` (execute_agentic)
**Was**: Only spec-declared files were registered. If agent produced supplementary files
(extra figures, intermediate analysis, supporting data), they were invisible to pipeline.
**Fix**: After checking declared specs, scan `stage_dir` for any additional files the agent
wrote. Register them as "Extra artifact discovered" in the result. Excludes hidden files
and stage_log.md.

### Fix 21c: Sanity check success detection too brittle
**File**: `crates/mol-pipeline/src/runtimes/sanity_check.rs`
**Was**: FAIL_PHRASES included overly broad terms ("fail", "error", "cannot") that
false-positive on normal text ("without failure", "error handling", "cannot be improved").
PASS_PHRASES missed common agent phrasings ("0 failures", "ran successfully").
**Fix**: Removed overly broad fail phrases, added precise error patterns (traceback,
syntax error, segmentation fault, etc.). Added 10 new pass phrases. Added
`check_outputs_exist()` for structural file-based success detection.

### Fix 21d: ExperimentCycle rigid 3-sub-phase structure
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs`
**Was**: Three sequential `execute_agentic()` calls (plan → run → refine), each resetting
the LLM session. Agent couldn't skip planning if trivial, couldn't iterate organically,
couldn't adjust workflow based on intermediate results.
**Fix**: Single `execute_agentic()` call with all expected artifacts declared together.
Agent decides its own workflow (plan, run, iterate, converge) in one session. Post-processing
creates runs/ and experiment_final/ directories from whatever the agent produced.

**Also fixed**: #9 [plan] block stripping re-added to `simple_clean()` — was lost during
`strip_llm_noise()` → `simple_clean()` refactor.

**Status**: FIXED — all tests pass

---

## Audit: Serial vs Parallel design (2026-04-10)

**Conclusion**: Current serial pipeline is correct for research workflows.

The dependency graph shows almost every stage requires output from its immediate predecessor.
Only CodebaseSearch (stage 8) could theoretically run in parallel with early CodeDevelop setup,
but the gain is ~5% at most. Gate stages (LiteratureScreen, ExperimentDesign, QualityGate)
are hard sync points by design (blinding/safety protocol).

The fan-out/fan-in parallelism in `execute_multi_agentic()` (reviewer agents via JoinSet)
is correctly placed: primary work is serial (epistemic dependency), review is parallel
(independent evaluations). This matches Claude Code's Agent tool pattern.

**Not changed**: Pipeline remains serial. No DAG scheduler needed.

---

## Friction Point #22: Deep flow simulation — Phase 2 edge cases (2026-04-10)

**Fix A**: LiteratureScreen candidates filename mismatch
**File**: `crates/mol-pipeline/src/stages_impl/phase2.rs:63-65`
**Problem**: LiteratureSearch (2.1) produces `candidates.md` but LiteratureScreen (2.2) only
read `candidates.jsonl`. If the agent wrote markdown instead of JSONL, screening silently
saw zero candidates and fell through to fake-data fallback.
**Fix**: Try both `candidates.jsonl` and `candidates.md` via `or_else()`.

**Fix B**: Missing Phase 2 artifacts in template_vars
**File**: `crates/mol-pipeline/src/executor.rs:256-259`
**Problem**: KnowledgeExtract template couldn't access screened_papers or candidates because
they weren't in the `artifact_files` lookup list.
**Fix**: Added `candidates`, `screened_papers`, `exclusion_reasons` to artifact_files.

**Fix C**: "stop" decision ignored by runner
**File**: `crates/mol-pipeline/src/runner.rs:427-438`
**Problem**: ResearchDecision could signal "stop" (research concluded/infeasible) but the
runner only handled "pivot" and "refine" via `decision_rollback()`. "stop" was treated as
"proceed", continuing into Phase 5 despite the agent saying to stop.
**Fix**: Added explicit "stop" handling before rollback check: break pipeline with "stop" decision.

**Fix D**: LiteratureScreen silent fake-data fallback
**File**: `crates/mol-pipeline/src/stages_impl/phase2.rs:103-165`
**Problem**: When no JSON lines could be parsed from candidates, the code silently inserted a
fabricated placeholder paper. Downstream stages would analyse fake data without knowing it.
**Fix**: Added intelligent JSON extraction from narrative text (strip markdown fences, try
JSON array parsing, line-by-line JSON object parsing). Degraded placeholder is last resort
only, marked with `"_degraded": true` and `"screening_status": "included_degraded"`.

**Status**: ALL FIXED — tests pass

---

## Friction Point #23: Archive manifest + template_vars completeness (2026-04-10)

**Fix F**: Publish archive manifest wrong filenames
**File**: `crates/mol-pipeline/src/stages_impl/phase5.rs:183-192`
**Problem**: Archive manifest listed `exp_plan.yaml`, `knowledge_cards.json`,
`knowledge_summary.json` — but the pipeline actually produces `exp_plan.md`,
`knowledge_cards.md`, `knowledge_summary.md`. `find_prior_file_pub()` couldn't find
these, leaving the archive manifest nearly empty.
**Fix**: Corrected all filenames to match actual stage outputs. Also added `problem_tree.md`
and `decision_record.md` which were missing entirely.

**Fix G**: template_vars() missing 12 artifacts
**File**: `crates/mol-pipeline/src/executor.rs:260-285`
**Problem**: Templates couldn't access `hardware_profile.json`, `sources.json`, `queries.json`,
`citation_map.json`, `relevant_files.json`, `experiment_code.md`, `plot_validation.md`,
`refinement_log.md`, `critical_review.md`, `constructive_review.md`, `rendering_review.md`,
`verification_report.md` — all produced by stages but not registered for template rendering.
**Fix**: Added all 12 missing artifacts to the `artifact_files` lookup array.

**Status**: ALL FIXED — tests pass

---

## Friction Point #24: Deep flow simulation — runner + Phase 3 edge cases (2026-04-10)

**Fix I**: pivot_count resets on recursion — MAX_DECISION_PIVOTS not globally enforced
**File**: `crates/mol-pipeline/src/runner.rs:220,445-466`
**Problem**: `execute_pipeline()` initializes `pivot_count = 0` as a local variable.
When ResearchDecision triggers a rollback, the code recursively calls `execute_pipeline()`
which creates a fresh `pivot_count = 0`. This means MAX_DECISION_PIVOTS (2) is enforced
per recursion level, not globally — theoretically allowing unbounded recursive pivots.
**Fix**: Persist pivot_count to `.pivot_count` file in run_dir. Each call reads the file
at startup and writes it back when incrementing. Recursive calls share the global counter.

**Fix J**: CodebaseSearch hardcoded HEP frameworks for all domains
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs:60-68`
**Problem**: When `codebases_dir` is not configured (common case), CodebaseSearch writes
hardcoded HEP-specific tools (uproot, awkward-array, hist, mplhep) regardless of the
configured domain. If domain is "ml" or "physics", the agent receives misleading context.
**Fix**: Added domain-aware match on `ctx.config.domain` with specific frameworks/patterns/
dependencies for "hep", "ml"/"ai", "physics"/"astro"/"cosmology", and a generic fallback.

**Fix K**: REWORK_TRIGGERS false positives on "critical"
**File**: `crates/mol-pipeline/src/executor.rs:828-838`
**Problem**: REWORK_TRIGGERS included bare "critical" and "fail" which could match positive
statements like "critical contribution" or "fails to disappoint". Also "blocking" could
match "blocking systematic uncertainty" (a physics term, not a bug).
**Fix**: Replaced loose triggers with specific compound phrases: "critical issue",
"critical flaw", "critical error", "critical bug", "critical problem", "blocking issue",
"blocking flaw", "fails to", "incorrect". Retained "must fix", "severity: a", "not ready",
"reject" as they are unambiguous.

**Status**: ALL FIXED — tests pass

---

## Deep Flow Audit Summary (2026-04-10)

Verified correct behavior for:
- Phase 1: GPU/memory detection graceful on macOS; stage_dir guaranteed before hardware write
- Phase 2: `read_prior_artifact()` backward-compat is one-way (.md→.json only);
  LiteratureScreen `or_else` fallback correctly handles both candidates formats;
  strip_markdown_fences returns text as-is when no fences exist; degraded placeholder
  only used as absolute last resort with `_degraded` flag
- Phase 3: sanity_check module functions all verified present and correct;
  session reset between CodeDevelop phases prevents context leakage;
  ExperimentCycle post-processing handles empty .py dirs gracefully
- Phase 4: `extract_decision_from_md()` lowercases first — case-insensitive;
  defaults to "proceed" when no keyword found (correct)
- Phase 5: PaperWrite resets LLM session between draft and revision (by design);
  Publish archive_manifest gracefully handles missing files (only archives what exists);
  all 19 templates have `---user---` delimiter and `{{ output_spec }}`
- Contracts: alias table covers all actual artifact filenames; validate_inputs/outputs
  are hard errors (not warnings); HEP overrides load correctly from knowledge chain
- Runner: blinding gate is file-based state machine (correct for auto-approve mode);
  stage_log.md correctly excluded from artifact discovery

---

## Friction Point #25: hep/ 垂直经验死文件 (2026-04-10)

**Problem**: 全面审计 hep/ 目录发现多个高价值文件有代码存在但管线未加载。

**Fix L**: 接入 `hep/prompts/*.md` 到管线
**Files**: `executor.rs` + 4 个 stage templates
**Problem**: `hep/prompts/experiment_design.md`、`code_generation.md`、`result_analysis.md`
包含 HEP 领域核心知识（pyhf workflow、uproot 用法、CLs 测试等）。`mol-domains` crate
有加载代码（`ChainBackedAdapter`），但 `mol-pipeline` 不依赖 `mol-domains`，从未调用。
**Fix**: 在 `template_vars()` 中通过 `read_knowledge("prompts/...")` 加载为
`experiment_design_hints`、`code_generation_hints`、`result_analysis_hints` 变量。
在 `experiment_design.md`、`code_develop.md`、`experiment_cycle.md`、`result_analysis.md`
模板中添加 `{% if xxx_hints %}` 引用块。

**Fix M**: 接入 `hep/methodology/03a-orchestration.md` 和 `appendix-dependencies.md`
**File**: `executor.rs:423-432`
**Problem**: 03a-orchestration 包含 agent 架构模式和 session 隔离原则，
appendix-dependencies 包含 Phase 依赖图。均未被管线加载。
**Fix**: 在 `template_vars()` 中加载为 `orchestration` 和 `dependency_graph` 变量。
模板可选引用（`{% if orchestration %}` 等），不强制。

**Remaining NOT LOADED (by design)**:
- `hep/CLAUDE.md` — 人类参考文档，非管线输入
- `hep/methodology/README.md` — 目录说明
- `hep/conventions/README.md`, `TEMPLATE.md` — 元文档
- `hep/orchestration/*.md` — 描述"人用 Claude Code 手动跑"的模式，Rust 管线已内化
- `hep/skills/*.md` — Claude Code slash-command，非管线
- `hep/hooks/isolate.sh` — 外部 shell 集成
- `hep/templates/mcp.json`, `molthep.toml`, `pixi.toml` — 环境配置模板
- `hep/examples/`, `hep/reference/` — 参考材料

**Status**: FIXED — tests pass

---

## E2E Run #2 (2026-04-10): Full 18-stage dimuon pipeline via ACP

Config: `config.mol.yaml`, provider=acp, datasets_dir=hep/examples/test-opendata/data (CMS dimuon 14MB CSV)
Command: `mol run --config config.mol.yaml --auto-approve --skip-preflight -d hep/examples/test-opendata/data`
Run ID: `mol-20260409-192434-9e9916`

### Stage progress tracker

| Stage | Name | Time | Artifacts | Quality |
|-------|------|------|-----------|---------|
| 01 | TOPIC_INIT | 214s | goal.md (19KB), hardware_profile.json, experiment_log.md | ✅ Excellent — conventions compliance table, reference survey, 64% OS/36% SS data stats |
| 02 | PROBLEM_DECOMPOSE | ~120s | problem_tree.md (13KB), topic_evaluation.md (7KB) | ✅ 7 sub-problems: object def, selection, bkg estimation, signal model, systematics, stat inference, validation |
| 03 | LITERATURE_SEARCH | ~300s | search_plan.md (9KB), sources.json (4KB), queries.json (8KB), candidates.md (36KB) | ⚠️ Content excellent but format wrong: .md instead of .jsonl |
| 04 | LITERATURE_SCREEN | 0s | screened_papers.jsonl (577B), exclusion_reasons.json (88B) | ❌ Degraded fallback — couldn't parse candidates.md, inserted fake placeholder |
| 05 | KNOWLEDGE_EXTRACT | ~300s | knowledge_cards.md (27KB), citation_map.json (6KB) | ✅ Despite degraded input, agent used seed papers from search_plan. 10+ cards with INSPIRE keys |
| 06 | SYNTHESIS_HYPOTHESES | ~240s | synthesis_report.md (20KB), hypotheses.md (16KB), experiment_log.md | ✅ 6 hypotheses: Z yield, DNN vs cuts, bkg closure, resolution, bkg-only exclusion, xsec consistency |
| 07 | EXPERIMENT_DESIGN | ~420s | exp_plan.md (41KB) | ✅ Massive experiment plan with full conventions compliance, 13 systematic sources |
| 08 | CODEBASE_SEARCH | ~30s | codebase_context.json, relevant_files.json | ⚠️ Generic frameworks (numpy, scipy) instead of HEP tools (uproot, pyhf) — domain not wired |
| 09 | CODE_DEVELOP | >1200s | experiment/main.py (1540 lines), 10 figure pairs, run_report.json, workspace.json, sanity_report.json | ✅✅ Outstanding — agent wrote, ran, produced full analysis with CMS-style plots, pyhf fit, signal injection |
| 10-18 | — | — | — | Pipeline still running... |

---

## Friction Point #26: LiteratureScreen can't parse structured markdown candidates

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/stages_impl/phase2.rs:56-187`
**Problem**: LiteratureSearch agent produces `candidates.md` with rich structured markdown
(`- **paper_id:** CMS-SMP-10-008`, `- **relevance_score:** 0.95`). LiteratureScreen's
parser only understands JSONL and JSON code blocks. The fallback markdown fence extraction
finds nothing because the data is in bullet-list format, not code fences. Falls to degraded
placeholder (1 fake paper), corrupting the entire Phase 2 knowledge chain.
**Impact**: Downstream KnowledgeExtract, Synthesis, and Hypotheses stages work from degraded
input. They compensate well (using search_plan seed papers and domain knowledge), but lose
the 30+ real candidates the agent collected.
**Fix (applied)**:
1. Changed LiteratureSearch ArtifactSpec from `candidates.md` to `candidates.jsonl` with
   explicit "MUST be valid JSON, one object per line" instruction.
2. Added markdown bullet-list parser as fallback in LiteratureScreen: detects `### N.` headers
   as entry boundaries and `- **key:** value` lines as fields. Parses paper_id, title, authors,
   year, relevance_score, tags, etc. Correctly handles numeric fields (year as int, relevance_score
   as float).
3. Updated executor.rs artifact lookup: `("candidates", "candidates.jsonl")`.
**Status**: FIXED — rebuild successful

---

## Friction Point #27: CodebaseSearch domain detection broken — knowledge_root ignored

**Severity**: MEDIUM
**File**: `crates/mol-cli/src/commands/run.rs:163-168`
**Problem**: `executor_config.domain` was set from `research.domains.first()` (YAML array),
which contains research categories like `"deep-learning"`. Config has `domains: ["deep-learning", "physics"]`
so domain = `"deep-learning"` — doesn't match any of the CodebaseSearch patterns ("hep", "ml", "physics").
Falls to generic defaults (numpy, scipy, pandas, matplotlib).
Meanwhile, `knowledge_root: "hep"` correctly identifies the domain but wasn't used.
**Expected**: Domain should derive from `knowledge_root` when available, since that's the actual
domain directory structure the pipeline uses.
**Fix (applied)**: Changed domain derivation to prefer `knowledge_root` over `domains.first()`:
```rust
domain: {
    let kr = &full_config.research.knowledge_root;
    if !kr.is_empty() { kr.clone() }
    else { full_config.research.domains.first().cloned().unwrap_or("hep".into()) }
},
```
**Status**: FIXED — rebuild successful

---

## Friction Point #28: Stage 9 (CODE_DEVELOP) extremely long — 3-phase LLM calls

**Severity**: MEDIUM (expected behavior, but worth noting)
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs:100-233`
**Problem**: CODE_DEVELOP has 3 sequential LLM phases:
1. Agentic code generation (agent writes + runs code) — ~10 min
2. Iterative sanity check (up to 5 LLM calls checking code) — ~5-10 min
3. Multi-agent review with rework (2 reviewers + possible rework) — ~5-10 min

Total: 20-30 minutes per stage. With ACP (2-4 min per LLM call), this is the pipeline's
bottleneck. Not a bug — the thorough checking is valuable — but users should expect this stage
to take significantly longer than others.
**Mitigation**: Consider adding progress logging between phases so users know which sub-phase
is running. Currently only the heartbeat timer is visible.
**Status**: NOTED — no fix needed, but progress logging would improve UX

---

## Friction Point #29: config.mol.yaml `sanity_check_max_iterations: 100` not wired

**Severity**: LOW
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs:182-184`, `config.mol.yaml`
**Problem**: The YAML config has `sanity_check_max_iterations: 100` under `experiment:` section,
but the code reads from `ctx.config.settings` HashMap which is always empty (never populated
from experiment config). The setting has no effect — default of 5 is always used.
**Expected**: Config should flow through to the executor.
**Fix**: Either wire `experiment.sanity_check_max_iterations` into `MolConfig.settings` during
config construction in run.rs, or add a dedicated field to `MolConfig`.
**Status**: ✅ FIXED — wired `experiment.sanity_check_max_iterations`, `max_iterations`,
`time_budget_sec`, and `codebases_dir` into `MolConfig.settings` HashMap in run.rs

---

## Friction Point #30: candidates.md vs candidates.jsonl backward compat

**Severity**: LOW
**File**: `crates/mol-pipeline/src/executor.rs:259`, `read_prior_artifact_pub()`
**Problem**: `read_prior_artifact` has backward-compat: when looking for `.md`, it falls back
to `.json/.yaml/.jsonl`. But the reverse is NOT true — looking for `.jsonl` does NOT fall back
to `.md`. After Fix #26, the artifact lookup changed to `candidates.jsonl`, which means:
- New runs: agent writes `.jsonl` → found directly
- Old runs: agent wrote `.md` → NOT found via `.jsonl` lookup
The LiteratureScreen has explicit `or_else` for both formats, so this is handled at the stage
level. But `template_vars()` would miss `candidates.md` if looking up as `candidates.jsonl`.
**Fix**: Already mitigated — LiteratureScreen handles both. Template vars for candidates are
less critical (downstream stages use screened_papers, not raw candidates).
**Status**: NOTED — mitigated at stage level

---

## Friction Point #31: `[thinking]` blocks leak into sanity_summary.md

**Severity**: LOW
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs:234`, `sanity_summary.md`
**Problem**: The `last_response` from `llm_generate()` may include `<thinking>...</thinking>`
blocks from the LLM. These are written verbatim into `sanity_summary.md`, polluting the artifact
with internal reasoning that users shouldn't see.
**Fix**: Strip `<thinking>...</thinking>` blocks from `last_response` before writing to file.
Could use `strip_frontmatter()` or a dedicated `strip_thinking_blocks()` utility.
**Status**: ✅ FIXED — added `strip_thinking_blocks()` to executor.rs, used in phase3.rs sanity_summary write

---

## Friction Point #32: Stage 16 (PeerReview) rework cannot fix upstream issues

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/executor.rs:1010-1073`
**Problem**: When reviewers find issues requiring upstream changes (e.g. "background model
spread is 55%, need different fit strategy"), the rework loop only re-calls the current stage's
primary agent. The agent can rewrite text/analysis but cannot re-run experiments from Stage 9/10.
Example: Stage 16 reviewer said "not ready for publication" due to physics issues in the
background model — but rework can only revise the paper, not the experiment code.
**Expected**: Cross-stage rework (e.g. reviewer triggers re-run of Stage 9 CodeDevelop).
**Workaround**: The primary agent addresses issues by reframing/adding caveats in the paper,
which is the right thing for genuinely paper-level issues. For experiment-level issues, a
human would need to manually restart from the relevant stage.
**Status**: ✅ FIXED — added cross-stage rework: `retry_from_stage` field on StageResult,
`upstream_rework_stage` field in verdict JSON, `check_upstream_rework()` in executor,
`collect_review_feedback()` in runner. Review findings copied to upstream stage dir.
Runner recursively re-runs from target stage. Bounded by MAX_DECISION_PIVOTS.

---

## Friction Point #33: Stage 16 rework takes 20+ min (reviewer×2 + rework + re-review)

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/executor.rs:1005-1073`, stage-16
**Problem**: The `execute_multi_agentic_with_rework()` loop for PeerReview involves:
1. Primary agent generates paper (~5 min) — already done in Stage 15
2. 2-3 reviewers run in parallel (~5 min)
3. `check_review_findings()` detects issues → rework triggered
4. Primary agent rewrites paper with feedback (~5 min)
5. Re-run all reviewers (~5 min)
Total: 20+ min for a single rework round. With max_rework > 1, could be 40+ min.
Observed: Stage 16 ran 24+ min with paper_revised.md growing from 58KB to 81KB.
**Mitigation options**:
- Run only the reviewer(s) that flagged issues in re-review, not all
- Add early-exit if rework produces minor changes only
- Cap PeerReview to max_rework=1 (current default may be higher)
**Status**: ✅ FIXED by #35 — PeerReview/QualityGate switched to plain execute_multi_agentic (no rework loop)

---

## Friction Point #34: Stages 10-13 produce .md instead of declared .json artifacts

**Severity**: LOW
**File**: Stage output directories (stage-11, stage-12, stage-13)
**Problem**: The pipeline declares these stages should produce JSON artifacts:
- Stage 11: `analysis_report.md` (expected: could be .json for structured data)
- Stage 12: `decision_record.md` (contract says `decision_record.json`)
- Stage 13: `knowledge_summary.md` (contract says `knowledge_summary.json`)
The ACP agent writes .md files instead of .json. Downstream stages that look up
`decision_record.json` via `read_prior_artifact` won't find the .md version unless
the fallback chain handles it.
**Fix**: Either update contracts to expect .md, or add .md→.json fallback in artifact lookup,
or make ArtifactSpec descriptions more explicit about requiring JSON format.
**Status**: ✅ FIXED — `read_prior_artifact()` now tries all extensions (.md, .json, .yaml, .yml, .jsonl)
bidirectionally for any recognized extension, not just .md→.json

---

## Friction Point #35: PeerReview/QualityGate rework loop is review-of-review (logic error)

**Severity**: HIGH
**File**: `crates/mol-pipeline/src/stages_impl/phase5.rs:88,126`
**Problem**: PeerReview used `execute_multi_agentic_with_rework()` with the primary agent
being a reviewer (writes `review_comments.md`) and two more reviewers checking that review.
When rework triggers, it asks the primary reviewer to revise its own review based on
reviewer-of-reviewer feedback — this is pointless. Same issue in QualityGate. Combined with
`REWORK_TRIGGERS_FALLBACK` phrases ("not ready", "critical issue") that almost always appear
in review text, rework was triggered every time, adding 15+ min per round × max_rework=2.
**Fix**: Changed both to `execute_multi_agentic()` (parallel reviews, no rework loop).
Reviewers produce independent assessments without recursive revision.
**Status**: ✅ FIXED

---

## Friction Point #36: Sanity check loop sends identical prompt every iteration

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/stages_impl/phase3.rs:199-226`
**Problem**: The sanity check while-loop calls `llm_generate()` with the exact same
`system_prompt` and `user_prompt` on every iteration. If iteration 1 fails, iteration 2
with identical input will likely fail the same way — just wasting LLM calls until max_iterations.
**Fix**: Feed the previous attempt's verdict/response back into the next iteration's prompt,
so the agent knows what failed and can fix it:
- If verdict JSON exists: include verdict file contents
- Fallback: include last response text
**Status**: ✅ FIXED

---

## Friction Point #37: Publish stage (18) crash — process dies silently

**Severity**: HIGH
**File**: Runner / ACP session management
**Problem**: Pipeline process (PID 34514) died during Stage 18 (Publish) after a total
runtime of ~3.5 hours. The archive_manifest.json was written (code-driven, no LLM needed)
but the agentic part (paper_final, references verification, LaTeX) never completed.
Likely cause: ACP session instability after long rework-heavy run (Stage 16 peer review
used max_rework=2 with 40+ min of back-and-forth).
**Mitigation**: Session reset at the start of each `execute_agentic()` call should help.
May also need ACP process-level health check.
**Status**: TODO — needs investigation

---

## Friction Point #38: paper_final.md not produced by any stage

**Severity**: MEDIUM
**File**: `crates/mol-pipeline/src/stages_impl/phase5.rs`
**Problem**: The Publish stage archive_manifest expects `paper_final.md` but:
- Stage 15 (PaperWrite) produces `paper_draft.md` + `paper_revised.md`
- Stage 16 (PeerReview) updates `paper_revised.md` (81KB post-rework)
- No stage produces `paper_final.md`
The archive correctly finds 9/10 expected artifacts but misses the paper entirely.
**Fix**: Moved archive manifest build to AFTER the agentic step (which produces paper_final.md).
Added `paper_revised.md` as fallback in the manifest artifact list. Removed redundant pre-agentic
manifest build that could never find paper_final.md.
**Status**: FIXED (phase5.rs)

---

## Friction Point #39: PaperWrite revision fabricates numbers not in code

**Severity**: HIGH (design issue)
**File**: `crates/mol-pipeline/src/stages_impl/phase5.rs` (PaperWrite + PeerReview)
**Problem**: Stage 16 (PeerReview) correctly identifies code-paper inconsistencies (e.g.,
closure test pull=0.00, 4 NPs vs. 8 claimed, missing figures). However, Stage 15
PaperWrite's revision sub-phase "fixes" review comments by modifying paper text WITHOUT
access to or ability to modify the experiment code. Result: paper claims results (Bernstein
polynomial fits, acceptance normsys, 8-NP workspace) that the code never produced.
Stage 17 QualityGate correctly catches this as "fabricated results" (score 3/10).
**Root cause**: Revision agent can only edit text, not code. It interprets "fix the issue"
as "change the number in the paper" rather than "this cannot be fixed at this stage."
**Potential fix**: (1) Revision prompt should explicitly forbid inventing numbers not in
code artifacts. (2) Revision should flag unresolvable issues as "requires code change"
rather than fabricating fixes. (3) Consider rework loop back to CODE_DEVELOP for
code-paper inconsistencies.
**Status**: NOTED — design issue for future iteration

---

## E2E Run #2 Final Results (2026-04-10)

**17 of 18 stages completed** (Stage 18 crashed)
**Total runtime: ~3.5 hours** (03:24 → ~07:15 CST)

### Per-stage timing

| Stage | Name | Time | Key Artifact(s) |
|-------|------|------|-----------------|
| 01 | TOPIC_INIT | 3.5m | goal.md (19KB) |
| 02 | PROBLEM_DECOMPOSE | 2m | problem_tree.md (13KB) |
| 03 | LITERATURE_SEARCH | 5m | candidates.md (36KB), queries.json |
| 04 | LITERATURE_SCREEN | <1s | screened_papers.jsonl (degraded) |
| 05 | KNOWLEDGE_EXTRACT | 5m | knowledge_cards.md (27KB) |
| 06 | SYNTHESIS_HYPOTHESES | 4m | hypotheses.md (16KB), synthesis_report.md (20KB) |
| 07 | EXPERIMENT_DESIGN | 7m | exp_plan.md (41KB) |
| 08 | CODEBASE_SEARCH | <1m | codebase_context.json |
| 09 | CODE_DEVELOP | 40m | main.py (1540L), 10 figure pairs, pyhf workspace |
| 10 | EXPERIMENT_CYCLE | 20m | run_report.md, refinement_log.md, 11 figures |
| 11 | RESULT_ANALYSIS | 5m | analysis_report.md (29KB) |
| 12 | RESEARCH_DECISION | 4m | decision_record.md — PROCEED |
| 13 | KNOWLEDGE_SUMMARY | 4m | knowledge_summary.md (26KB) |
| 14 | PAPER_OUTLINE | 4m | paper_outline.md (31KB) |
| 15 | PAPER_WRITE | 30m | paper_draft.md (46KB), paper_revised.md (58KB), references.bib |
| 16 | PEER_REVIEW | 50m | review + rework (paper grew 58KB→81KB) |
| 17 | QUALITY_GATE | 10m | quality_report.md, plot_validation.md |
| 18 | PUBLISH | 20m (resumed) | paper_final.md (81KB), paper.tex (70KB), verification_report.md |

### Key observations:
1. Bottleneck stages: CODE_DEVELOP (40m) and PEER_REVIEW (50m) — multi-phase LLM + rework
2. Rework validated: PeerReview caught real physics issue (55% bkg spread), paper revised
3. Agent autonomy: 1540-line analysis with CMS-style plots, pyhf fit, staged unblinding
4. Domain knowledge: conventions compliance, HEP tools (pyhf, mplhep), proper citations
5. Decision system: ResearchDecision correctly chose PROCEED with 5/6 hypotheses confirmed
6. QualityGate (Stage 17): scored 3/10, correctly caught fabricated results (paper-code divergence)
7. Publish (Stage 18): completed on resume, 19-artifact archive with FAIR compliance metadata

### Resume run (Stages 17-18):
- Resumed from checkpoint at Stage 17 using rebuilt binary (fixes #26, #27, #35)
- Stage 17 (QualityGate): 10.6 min, multi-agent review (arbiter + plot-validator + rendering-reviewer)
- Stage 18 (Publish): 19.7 min, archive manifest + paper_final.md + paper.tex + verification_report
- Total resume time: ~30 min
- **All 18 stages now completed successfully**

---

### Friction Point #39: PaperWrite revision fabricates results (code-paper divergence)

**Severity**: HIGH — caught by QualityGate, but indicates architectural issue
**Stage**: PaperWrite (15) / QualityGate (17)
**Description**: When PeerReview (Stage 16) identifies issues with code-produced numbers,
PaperWrite's revision step changes the *text* to address findings without modifying the
underlying code. This leads to fabricated results: the paper claims numbers that no code
produces. QualityGate correctly caught 13 Category A issues including:
- Code produces closure test pull=0.00; paper claims 0.03±0.14
- Code has 4 NPs; paper describes 8 with fabricated impacts
- 3 referenced figures don't exist on disk
**Root cause**: PaperWrite revision has no mechanism to modify experiment code — it can only
edit paper text. When a reviewer says "add Bernstein polynomial", the revision adds text
about Bernstein polynomials rather than updating main.py.
**Fix**: Requires architectural change — either (a) PaperWrite gets code-write capability
for the revision step, or (b) rework triggers experiment re-execution, or (c) revision
is explicitly instructed to only report what the code actually produces.
**Status**: FIXED — code-aware revision: injected experiment_code + run_report as read-only context into paper_write.md (hep+generic) and peer_review.md templates; added 5-rule anti-fabrication prompt guard; UNRESOLVABLE marker for code-level issues

### Friction Point #40: Publish archive_manifest built before artifacts exist

**Severity**: LOW (self-corrected by agent)
**Stage**: Publish (18)
**Description**: In `execute_publish()`, sub-phase 1 builds archive_manifest.json by
scanning prior stage dirs for specific filenames (including `paper_final.md`). This runs
BEFORE sub-phase 2 (the agentic step) which actually produces `paper_final.md`. The
code-generated manifest found only 9/10 artifacts. However, the agent then overwrote the
manifest with a richer 19-artifact version.
**Root cause**: archive_manifest code in phase5.rs:212-247 runs before execute_agentic.
**Fix**: Move archive_manifest generation AFTER execute_agentic, or rebuild it at the end.
**Status**: FIXED (phase5.rs — archive manifest moved after execute_agentic, paper_revised.md added as fallback)

---

## Handoff: Unfixed Issues (2026-04-10)

### Must Fix (HIGH severity, unfixed)

**#37 — Publish stage crash on long runs**
- **What**: Pipeline process (PID) dies silently during Stage 18 after ~3.5 hours of total runtime.
- **Root cause**: Likely ACP session instability after extended rework-heavy run. Session reset per-stage helps but may not be sufficient for very long runs.
- **Workaround**: Resume from checkpoint works. Pipeline completed on second attempt.
- **Fix**: Add ACP health check / keepalive, or investigate node process memory under long sessions.
- **Files**: ACP session management in `crates/mol-services/src/agent_bridge.rs`

### Should Fix (MEDIUM severity, unfixed)

**#34 — Stages 10-13 produce .md instead of declared .json artifacts**
- Agent writes markdown even when ArtifactSpec requests .json. Fallback parsing handles it but adds complexity.
- **Fix**: Stronger prompt enforcement, or accept .md as primary format and parse structured data from it.

### Noted (design issues, no immediate fix needed)

**#28 — Stage 9 (CODE_DEVELOP) takes 40+ minutes**: 3-phase structure (write + run + sanity) is inherently slow. Not a bug.

**#32 — PeerReview rework cannot fix upstream issues**: Rework loop at Stage 16 can only revise the paper, not the experiment code. Related to #39.

**#33 — PeerReview takes 50+ minutes with rework**: Already mitigated by removing rework loop from PeerReview (#35 fix). Now runs ~10 min as parallel review only.

### Fixed in this session

| # | Issue | Fix |
|---|-------|-----|
| 26 | LiteratureScreen can't parse markdown candidates | Added markdown bullet-list parser fallback + changed ArtifactSpec to request .jsonl |
| 27 | Domain detection uses domains[] instead of knowledge_root | Prefer knowledge_root in run.rs |
| 35 | PeerReview/QualityGate rework loop reviews own review | Changed to plain execute_multi_agentic (no rework) |
| 38 | paper_final.md not in archive manifest | Moved manifest build after agentic step, added paper_revised.md fallback |
| 39 | PaperWrite revision fabricates numbers | Code-aware revision: injected experiment_code+run_report into paper_write+peer_review templates + anti-fabrication prompt guard |
| 40 | Archive manifest built before artifacts exist | Same fix as #38 |
| 41 | Publish doesn't collect upstream figures/bib | Moved figures symlink + bib copy before tex_path check in phase5.rs |
