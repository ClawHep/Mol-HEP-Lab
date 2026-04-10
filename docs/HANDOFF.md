# Handoff Document — E2E Pipeline (2026-04-10)

> 18-stage pipeline 全部跑通。本文档记录架构决策、已修复问题、未修复问题和下一步。
> 详细 friction log 见 `docs/e2e-friction-log.md`。

## 当前状态

- **E2E Run #2**: 全部 18/18 stages 完成（含 resume），总耗时 ~4 小时
- **Run ID**: `mol-20260409-192434-9e9916`
- **Topic**: Dimuon invariant mass spectrum analysis using CMS open data
- **Artifacts**: `artifacts/mol-20260409-192434-9e9916/` (stage-01 ~ stage-18)
- **Binary**: `target/release/mol` 已编译，包含所有 fix
- **Paper**: `paper_final.md` (81KB), `paper.tex` (70KB) — PDF 待编译

## 架构决策

1. **Agent artifacts 统一 .md** — Agent 写 markdown，JSON 仅用于程序生成文件。`read_prior_artifact()` 有 .md↔.json 双向 fallback。
2. **Review stages 无 rework loop** — PeerReview/QualityGate 用 `execute_multi_agentic()`，不用 `_with_rework()`。Rework 是 review-of-review，逻辑错误（#35）。
3. **Structured verdict JSON** — Reviewer 写 `{name}_verdict.json`，用 `check_upstream_rework()` 解析，支持跨 stage rework。
4. **Cross-stage rework** — `retry_from_stage` on StageResult，Runner 通过递归 sub-run 处理，bounded by MAX_DECISION_PIVOTS。
5. **Archive manifest 在 agentic step 之后重建** — 确保 paper_final.md 等 agent 产出的文件被收录。
6. **Strip `<thinking>` blocks** — `strip_thinking_blocks()` 清理 LLM 思维链泄漏。

## 已修复 (E2E Run #1 + #2 + post-run)

| # | Issue | Fix | Files |
|---|-------|-----|-------|
| 26 | LiteratureScreen 不能解析 markdown candidates | markdown bullet-list parser fallback + ArtifactSpec→.jsonl | phase2.rs |
| 27 | domain 检测用 domains[] 而非 knowledge_root | 优先用 knowledge_root | run.rs |
| 29 | sanity_check_max_iterations config 未传递 | config wiring | run.rs |
| 31 | `[thinking]` blocks 泄漏到 artifacts | strip_thinking_blocks() | executor.rs, phase3.rs |
| 32 | Cross-stage rework 机制 | verdict JSON + retry_from_stage | executor.rs, runner.rs, phase5.rs |
| 34 | Agent 产出 .md 但 Spec 声明 .json | 双向 fallback | executor.rs |
| 35 | PeerReview/QualityGate rework loop 逻辑错误 | 改为 execute_multi_agentic | phase5.rs |
| 36 | Sanity check loop 发送相同 prompt | 加入上轮 verdict 反馈 | phase3.rs |
| 38 | paper_final.md 不在 archive manifest | manifest 移到 agentic step 之后 | phase5.rs |
| 39 | PaperWrite revision 编造数据 | code-aware revision (注入 experiment_code + run_report) + prompt guard anti-fabrication 规则 | paper_write.md (hep+generic), peer_review.md |
| 40 | Archive manifest 在 artifacts 生成前构建 | 同 #38 | phase5.rs |
| 41 | Publish 不收集上游 figures/references.bib | figures symlink + bib 复制移到 tex_path 检查之前 | phase5.rs |
| 16 | Frontend/backend stage 不匹配 | 前端 26→18 stage 对齐 + layer normalization + artifact name 对齐 | types.ts, i18n, mock.ts, App.tsx, LogPanel.tsx |

## 未修复 — Must Fix

### #37 长时间运行后 Publish 崩溃（MEDIUM）

Pipeline 跑 ~3.5 小时后 Stage 18 进程静默退出。Resume 成功。需调查 ACP (node/acpx) 长时间运行稳定性。  
文件: `agent_bridge.rs`

## 未修复 — Should Fix

| # | Issue | 文件 |
|---|-------|------|
| 34 | Stage 10-13 产出 .md 但 ArtifactSpec 声明 .json（fallback 生效但不干净）| 各 phase stages_impl |

## 未修复 — Noted (设计问题)

| # | Issue |
|---|-------|
| 28 | Stage 9 (CODE_DEVELOP) 耗时 40+ 分钟（3 phase 设计如此） |
| 32 | PeerReview rework 只能改论文不能改代码（#39 的上游根因） |
| 33 | PeerReview 之前 50+ 分钟（#35 已修复，现在 ~10 分钟） |

## PDF 编译

```bash
# 1. 安装缺失的 LaTeX 包
sudo /usr/local/texlive/2026basic/bin/universal-darwin/tlmgr install multirow bm lineno

# 2. 编译
export PATH="/usr/local/texlive/2026basic/bin/universal-darwin:$PATH"
cd artifacts/mol-20260409-192434-9e9916/stage-18
pdflatex -interaction=nonstopmode paper.tex
bibtex paper
pdflatex -interaction=nonstopmode paper.tex
pdflatex -interaction=nonstopmode paper.tex
# → paper.pdf
```

---

## 下次会话怎么说

```
继续 E2E pipeline 优化。看 docs/HANDOFF.md。
剩余: #37 (Publish 长时间崩溃), #34 (ArtifactSpec .json vs .md)。
可以跑一次 E2E 验证 #39/#41/#16 的修复效果。
```
