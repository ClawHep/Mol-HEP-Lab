# Mol-HEP-Lab — Handoff Document

**Date:** 2026-04-09  
**Status:** 26-stage pipeline E2E tested | 23/26 stages pass | 150 Rust tests | Critical fixes pending

---

## Project Identity

**Mol-HEP-Lab** is a HEP-native autonomous research platform. Users are particle physicists.
Pure Rust (17 crates) + React frontend + HEP knowledge layer.
Pipeline: 5-phase HEP model (Strategy → Exploration → Processing → Inference → Documentation),
internally 26 granular stages.

## Architecture

```
Mol-HEP-Lab/
├── hep/                    HEP vertical knowledge layer (83 files)
│   ├── methodology/        5-phase analysis methodology (17 docs)
│   ├── conventions/        Extraction, search, unfolding conventions (5 docs)
│   ├── orchestration/      Agent roster, automation, sessions (5 docs)
│   ├── agents/             17 specialized HEP agent definitions
│   ├── skills/             5 pipeline orchestration skills
│   ├── templates/          Phase prompts, pixi.toml, config templates (10 files)
│   ├── hooks/              Analysis directory isolation hook
│   ├── examples/           Complete analysis case study
│   └── reference/          Archived reference material
├── crates/                 17 Rust crates (mol-cli, mol-pipeline, mol-services, ...)
├── frontend/               React + TypeScript dashboard
├── data/                   Prompts, benchmarks, datasets
└── config.mol.yaml         Project configuration
```

---

## LLM 调用架构

### 两种使用方式

```
方式 1：用户通过前端/CLI 配置
  用户 → 前端 / mol run → pipeline 自动 spawn LLM 调用

方式 2：用户让自己的 LLM 当大脑
  用户 ↔ Claude Code CLI → LLM 调 mol run 或直接调各 stage
```

两种方式底层一样。不管上面是人在点按钮还是 LLM 在发指令。

### ACP

ACP = `acpx` 命令行工具用的协议，让 Rust 代码程序化调用 Claude Code CLI。

```
用户直接用 CLI：  用户 ↔ Claude Code CLI
mol run 自动跑：  mol run (Rust) → spawn acpx 子进程 → Claude Code CLI → 返回结果
                                   ↑ ACP 桥接层
```

ACP 只是管道，不限制能力。用户手动操作不涉及 ACP。

### LLM Provider 层

`mol-llm` crate 支持多种 provider，pipeline 不关心底层：

| Provider | 底层 | 适用场景 |
|----------|------|---------|
| **CLI/ACP** | `acpx` → Claude Code | 本地开发，无需 API key |
| **API 直连** | HTTP 请求到 Anthropic/OpenAI | 生产部署，高吞吐 |

并行多智能体 = 同时发 N 个 LLM 调用。所有 provider 均支持。

---

## What Was Done (2026-04-08)

### Phase 8: HEP Vertical Integration
- Copied 83 HEP knowledge files from MoltHep
- Added `HighEnergyPhysics` domain variant + 70 keywords + HEP profile
- Ported Python HEP types to Rust (`mol-common/src/hep.rs`)
- Unified 5-phase pipeline model (replaced 8-phase A-H grouping)
- 559 tests passing

## What Was Done (2026-04-09)

### Agentic Stage Migration

All 25 LLM-driven stages migrated to `execute_agentic` two-phase pattern:
- **Phase 1**: Free-form LLM analysis (no format constraint)
- **Phase 2**: Per-artifact focused extraction (schema hint + validation + retry)

Each stage declares `ArtifactSpec` (filename, format, description, schema_hint).
2 stages are code-driven (no LLM): `literature_screen`, `knowledge_archive`.

### E2E Pipeline Test Results

Ran full 26-stage pipeline with dimuon CMS open data topic.
Pipeline reached stage 23/26 and completed (17 success, 1 fail, 9 cascade-skip on old binary;
resume from stage 18 with new binary reached stage 23+).

**Stage-by-stage results (latest run):**

| Stage | Name | Time | Artifacts | Quality |
|-------|------|------|-----------|---------|
| 01 | TOPIC_INIT | 46s | goal.md, hardware_profile.json | ✅ JSON clean, MD has ACP noise |
| 02 | PROBLEM_DECOMPOSE | 92s | problem_tree.md, topic_evaluation.json | ✅ JSON clean, MD has ACP noise |
| 03 | SEARCH_STRATEGY | 187s | search_plan.yaml, sources.json, queries.json | ✅ All clean (YAML fix works) |
| 04 | LITERATURE_COLLECT | 625s | candidates.jsonl | ⚠️ ACP preamble + embedded JSONL |
| 05 | LITERATURE_SCREEN | <1s | screened_papers.jsonl, exclusion_reasons.json | ✅ Code-driven |
| 06 | KNOWLEDGE_EXTRACT | 167s | knowledge_cards.json, citation_map.json | ✅ JSON clean (was `[1]`, fixed) |
| 07 | SYNTHESIS | 615s | synthesis_report.md, gap_analysis.json | ✅ JSON clean, MD has ACP noise |
| 08 | HYPOTHESIS_GEN | 248s | hypotheses.md | ⚠️ ACP noise |
| 09 | EXPERIMENT_DESIGN | 536s | exp_plan.yaml | ⚠️ YAML wrapped in ACP narrative |
| 10 | CODEBASE_SEARCH | <1s | codebase_context.json, relevant_files.json | ✅ Fast-path (no codebases) |
| 11 | CODE_GENERATION | 908s | experiment_spec.md, experiment_code.md, experiment/ | ⚠️ Code mixed with ACP noise |
| 12 | SANITY_CHECK | 148s | sanity_report.json | ✅ JSON clean |
| 13 | RESOURCE_PLANNING | 299s | resource_plan.json, schedule.json | ✅ JSON clean |
| 14 | EXPERIMENT_RUN | 1193s | run_report.json, runs/ | ❌ LLM hallucinated results, no real execution |
| 15 | ITERATIVE_REFINE | 638s | refinement_log.json, refined_code.md | ✅ JSON clean |
| 16 | RESULT_ANALYSIS | 316s | analysis_report.md, experiment_summary.json | ✅ JSON clean, MD has ACP noise |
| 17 | RESEARCH_DECISION | 146s | decision_record.json | ✅ JSON clean |
| 18 | KNOWLEDGE_SUMMARY | 411s | knowledge_summary.json | ✅ JSON clean (was failing, fixed) |
| 19 | PAPER_OUTLINE | 347s | paper_outline.md | ⚠️ ACP noise |
| 20 | PAPER_DRAFT | — | paper_draft.md | ⚠️ ACP noise |
| 21 | PEER_REVIEW | — | review_comments.json | ⚠️ ACP noise in JSON |
| 22 | PAPER_REVISION | — | paper_revised.md, revision_notes.md | Running |
| 23-26 | QUALITY_GATE → CITATION_VERIFY | — | — | Pending |

### Friction Points Discovered & Fixed

| # | Issue | Severity | Status |
|---|-------|----------|--------|
| 1 | Provider priority ignores explicit config | HIGH | ✅ Fixed |
| 3 | ACP session state leaks between runs | HIGH | ✅ Fixed |
| 4 | Thinking blocks not stripped | MEDIUM | ✅ Fixed |
| 5 | Embedded code fence extraction | HIGH | ✅ Fixed |
| 6 | Cascading empty artifact fallback | HIGH | ✅ Fixed |
| 7 | User data directory not wired | MEDIUM | ✅ Fixed |
| 8 | LLM returns narrative instead of JSON | HIGH | ✅ Fixed (agentic pattern) |
| 9 | ACP `[plan]` block noise | MEDIUM | ✅ Fixed |
| 10 | YAML extraction returns meta-commentary | HIGH | ✅ Fixed (`is_structured_yaml`) |
| 11 | ACP session context leaks between stages | HIGH | ✅ Fixed (`reset_session`) |
| 12 | Markdown artifacts contain ACP meta-commentary | HIGH | ✅ Fixed (`clean_artifact_output`) |
| 13 | CODEBASE_SEARCH hangs 16+ min | MEDIUM | ✅ Fixed (fast-path when no codebases) |
| 14 | JSON `[1]` empty artifact | HIGH | ✅ Fixed (`is_meaningful_json`) |
| 15 | ACP reconnect insufficient (3→6 attempts + backoff) | MEDIUM | ✅ Fixed |
| 16 | Extraction prompt contains filename → ACP reads disk | HIGH | ✅ Fixed |
| 17 | Phase 2 results not cleaned | HIGH | ✅ Fixed (`strip_llm_noise` on all paths) |
| 18 | `strip_llm_preamble` was dead code | CRITICAL | ✅ Fixed (wired via `clean_artifact_output`) |
| 19 | `is_acp_chatter` missing patterns | HIGH | ✅ Fixed (expanded to 20+ patterns) |

### Code Review Findings (8 noise leakage paths)

Full audit by code-reviewer agent. All CRITICAL/HIGH fixed:
1. ✅ Error fallback now goes through `clean_artifact_output`
2. ✅ `strip_llm_preamble` wired into output chain
3. ✅ Last-resort result goes through `clean_artifact_output`
4. ✅ Markdown validation checks first 5 + last 3 lines
5. ✅ `is_acp_chatter` expanded with 20+ patterns
6. MEDIUM: `strip_markdown_fences` in `llm_generate` may destroy Phase 1 context — known, mitigated by `clean_artifact_output`
7. MEDIUM: Raw YAML detector edge case — low risk
8. LOW: `llm_generate_json/jsonl` return raw on failure — latent

---

## CRITICAL: Pending Fixes (Next Session)

### 1. 实验代码未真正执行（最高优先级）

**问题**：Stage 14 (EXPERIMENT_RUN) 只让 LLM "想象" 运行结果，没有真正执行 Python 代码。
合并前两个管线都能产出图，合并后不行。

**根因**：`execute_experiment_run` 调 `execute_agentic()`（纯 LLM），
但 `crates/mol-experiment/` 已有完整 sandbox 执行器（Local/Docker/SSH/Colab），只是没接入 pipeline。

**修复**：
```
Stage 11 (CODE_GENERATION) → 生成 experiment/main.py
Stage 14 (EXPERIMENT_RUN) → 用 mol-experiment sandbox 真正执行 python3 main.py
                           → 产出 plots/*.png + runs/run_report.json（真实数据）
Stage 15 (ITERATIVE_REFINE) → 看真实结果，修改代码，重新执行
```

关键文件：
- `crates/mol-experiment/src/runner.rs` — `ExperimentRunner::run_experiment(code)` 已实现
- `crates/mol-experiment/src/sandbox.rs` — `LocalSandbox` 执行 Python 子进程
- `crates/mol-pipeline/src/stages_impl/phase3.rs` — `execute_experiment_run` 需要改写

### 2. Artifact 格式合理化

**问题**：所有 stage 都套了 `ArtifactSpec` + Phase 2 JSON extraction，对 markdown/code 是过度设计。
ACP agent 在 Phase 2 提取 markdown 时总是去读磁盘而非从 prompt 提取。

**修复方向**：
```
结构化数据（评分、配置、决策） → JSON/YAML + Phase 2 validation（保持现状）
文档内容（目标、分析、论文）   → Markdown，Phase 1 直接存，不走 Phase 2
代码（实验脚本）               → .py 文件，直接存
图表（plots）                  → .png/.pdf，由 Python 实际执行产出
```

这意味着 `execute_agentic` 需要支持混合模式：
- 有 `ArtifactSpec` 的 artifact → Phase 2 提取
- 没有 `ArtifactSpec` 的 artifact → Phase 1 结果直接清洗存储

### 3. Markdown 噪声彻底解决

即使 `clean_artifact_output` + `strip_llm_preamble` 已接入，
ACP agent 返回的 markdown 仍可能有内嵌噪声（非前缀/后缀）。
根本解决方案是上面的 #2——markdown 不走 Phase 2 提取。

### 4. 完整 E2E 重测

所有修复完成后，需要用新 binary 完整跑一次 26-stage pipeline，验证：
- 每个 stage 产物干净无噪声
- 实验代码真正执行并产出图表
- 论文包含真实数据和图表引用

---

## Multi-Agent Upgrade Plan（摩擦点修复后执行）

### 保持单智能体（20 个 stage）

| Phase | Stage | 角色 | 原因 |
|---|---|---|---|
| 1.1 | TOPIC_INIT | lead-analyst | 定义目标需要一致视角 |
| 1.2 | PROBLEM_DECOMPOSE | lead-analyst | 分解需要连贯逻辑 |
| 2.1 | SEARCH_STRATEGY | investigator | 策略必须统一 |
| 2.3 | LITERATURE_SCREEN | (代码驱动) | 无 LLM |
| 3.1 | EXPERIMENT_DESIGN | lead-analyst | 实验设计需要一致性 |
| 3.2 | CODEBASE_SEARCH | signal-lead | 机械搜索任务 |
| 3.3 | CODE_GENERATION | signal-lead | 代码必须连贯 |
| 3.4 | SANITY_CHECK | cross-checker | 通过/不通过 |
| 3.5 | RESOURCE_PLANNING | lead-analyst | 单视角估算 |
| 3.6 | EXPERIMENT_RUN | signal-lead | 执行，非讨论 |
| 3.7 | ITERATIVE_REFINE | systematics-fitter | 迭代循环 |
| 4.1 | RESULT_ANALYSIS | lead-analyst | 一致的结果解读 |
| 4.2 | RESEARCH_DECISION | arbiter | 单裁判决策 |
| 4.3 | KNOWLEDGE_SUMMARY | lead-analyst | 总结 |
| 5.1 | PAPER_OUTLINE | note-writer | 结构连贯性 |
| 5.2 | PAPER_DRAFT | note-writer | 统一文风 |
| 5.4 | PAPER_REVISION | note-writer | 回应评审意见 |
| 5.6 | KNOWLEDGE_ARCHIVE | (代码驱动) | 无 LLM |
| 5.7 | EXPORT_PUBLISH | note-writer | 格式转换 |
| 5.8 | CITATION_VERIFY | note-writer | 机械验证 |

### 升级为多智能体并行（5 个 stage）

| Stage | 并行角色 | 原因 |
|---|---|---|
| LITERATURE_COLLECT | investigator + theory-scout + detector-specialist | 多角度搜索 |
| KNOWLEDGE_EXTRACT | investigator + theory-scout + detector-specialist | 多层面提取 |
| SYNTHESIS | theory-scout + lead-analyst | 理论 vs 实验综合 |
| PEER_REVIEW | physics/critical/constructive 3个 reviewer | 多人评审 |
| DISCUSSION | lead-analyst + theory-scout + physics-reviewer + cross-checker | 多角色讨论 |

实现：`execute_multi_agent()` — Phase 1 并行发 N 个 LLM 调用，merge 后走 Phase 2。
配置：`agents.yaml` 向后兼容（字符串=单agent，`mode: multi`=多agent）。
优先级：PEER_REVIEW → LITERATURE_COLLECT + DISCUSSION → KNOWLEDGE_EXTRACT + SYNTHESIS

---

## Key Files Modified (2026-04-09)

| File | Changes |
|------|---------|
| `crates/mol-pipeline/src/executor.rs` | `execute_agentic` session reset, `clean_artifact_output`, `strip_llm_preamble` wired, `is_meaningful_json`, `is_structured_yaml`, `is_acp_chatter` expanded, extraction prompt without filenames |
| `crates/mol-pipeline/src/stages_impl/phase1.rs` | Migrated TOPIC_INIT, PROBLEM_DECOMPOSE to agentic |
| `crates/mol-pipeline/src/stages_impl/phase2.rs` | Migrated SEARCH_STRATEGY to agentic |
| `crates/mol-pipeline/src/stages_impl/phase3.rs` | Migrated 7 stages to agentic, CODEBASE_SEARCH fast-path |
| `crates/mol-pipeline/src/stages_impl/phase4.rs` | Migrated 3 stages to agentic |
| `crates/mol-pipeline/src/stages_impl/phase5.rs` | Migrated 7 stages to agentic |
| `crates/mol-pipeline/src/stages_impl/discussion.rs` | Migrated to agentic |
| `crates/mol-llm/src/provider.rs` | Added `reset_session()` to `LlmProvider` trait |
| `crates/mol-llm/src/acp.rs` | `MAX_RECONNECTS` 2→5, exponential backoff |
| `docs/e2e-friction-log.md` | Friction points #9-#19 documented |
| `docs/superpowers/HANDOFF.md` | This document |
