# Mol-HEP-Lab 深度架构文档

> Autonomous Multi-Agent Research Platform for High Energy Physics
>
> 版本: 0.1.0 | 语言: Rust 2024 Edition + React 19 + TypeScript | 许可: MIT

---

## 目录

- [1. 项目概述](#1-项目概述)
- [2. 宏观架构](#2-宏观架构)
- [3. Crate 依赖拓扑](#3-crate-依赖拓扑)
- [4. Pipeline 状态机](#4-pipeline-状态机)
- [5. Agent 系统](#5-agent-系统)
- [6. LLM 集成层](#6-llm-集成层)
- [7. Prompt 工程与模板系统](#7-prompt-工程与模板系统)
- [8. 领域检测与知识链](#8-领域检测与知识链)
- [9. 代码生成引擎](#9-代码生成引擎)
- [10. 实验执行与沙箱](#10-实验执行与沙箱)
- [11. 自进化与元学习](#11-自进化与元学习)
- [12. 文献检索与新颖性评估](#12-文献检索与新颖性评估)
- [13. CLI 命令行接口](#13-cli-命令行接口)
- [14. 前端 Dashboard](#14-前端-dashboard)
- [15. 配置系统](#15-配置系统)
- [16. 产出物结构](#16-产出物结构)
- [17. 数据流全景](#17-数据流全景)

---

## 1. 项目概述

Mol-HEP-Lab 是一个**自主多智能体研究平台**，专为高能物理 (HEP) 粒子物理学家设计。
它将一个完整的科研流程——从选题、文献调研、假设生成、实验设计、代码编写、
沙箱执行、结果分析、研究决策到论文撰写与发表——编码为一条 **5 阶段 18 步骤**
的自动化 Pipeline，由 17 个专业化 AI Agent 协作驱动，在关键节点保留人工审核门控。

### 核心特征

| 特征 | 说明 |
|------|------|
| **全链路自动化** | 从研究问题到 LaTeX 论文 PDF，18 个阶段全自动推进 |
| **人机协作门控** | 3 个 Review Gate（文献筛选、实验设计、质量门）要求人工审批 |
| **闭环反馈** | PIVOT/REFINE 决策回滚机制，最多 2 次循环调整假设或实验 |
| **领域可扩展** | 知识链分层架构（hep → generic），支持 11 个研究领域 |
| **自进化** | 从失败中提取经验教训，注入未来运行的 prompt overlay |
| **多后端执行** | 本地 / Docker / SSH 远程 / Google Colab 四种实验沙箱 |
| **实时监控** | React 前端 + WebSocket 实时展示 Agent 状态、资源占用、产出物 |

### 技术栈速览

```
Backend:   Pure Rust workspace (17 crates) — Tokio async runtime
Frontend:  React 19 + TypeScript + Vite — WebSocket 双通道
LLM:       OpenAI-compatible API / Anthropic Messages API / ACP CLI Bridge
HEP Tools: uproot, awkward-array, hist, pyhf, fastjet, mplhep, xgboost
Server:    Axum (HTTP + WebSocket + 静态文件)
Template:  Tera (Jinja2 兼容) — 分层知识链模板
```

---

## 2. 宏观架构

### 系统分层

```
┌─────────────────────────────────────────────────────────────────┐
│                     用户交互层                                    │
│  ┌──────────┐  ┌───────────────────────────────────────────┐    │
│  │ mol CLI  │  │  React Dashboard (localhost:5903)         │    │
│  │ run/init │  │  5-Phase 金字塔可视化 + WebSocket 实时更新   │    │
│  │ serve    │  │  HITL 审核面板 + 日志 + 产出物浏览          │    │
│  └────┬─────┘  └────────────────┬──────────────────────────┘    │
│       │                         │ /ws/agents, /ws/resources     │
├───────┼─────────────────────────┼───────────────────────────────┤
│       │              服务层      │                               │
│  ┌────▼─────────────────────────▼──────────────────────────┐    │
│  │  Axum Server (mol-services)                             │    │
│  │  ├─ Agent Bridge WebSocket   — 编排 Pipeline 执行        │    │
│  │  ├─ Resource Monitor WS      — CPU/GPU 指标广播          │    │
│  │  ├─ Download Handler         — 产出物文件下载            │    │
│  │  └─ Static File Server       — React 构建产物            │    │
│  └────┬────────────────────────────────────────────────────┘    │
├───────┼─────────────────────────────────────────────────────────┤
│       │              编排层                                      │
│  ┌────▼────────────────────────────────────────────────────┐    │
│  │  Pipeline Runner (mol-pipeline)                         │    │
│  │  ├─ 18-Stage 状态机  — advance() 驱动状态转移             │    │
│  │  ├─ Checkpoint 原子写 — 支持断点续跑                      │    │
│  │  ├─ Contract 校验    — 每阶段 I/O 产出物契约              │    │
│  │  ├─ Gate 处理        — 自动/手动审批 + Reject 回滚        │    │
│  │  ├─ Decision 回滚    — PIVOT→假设 / REFINE→实验           │    │
│  │  └─ 跨阶段返工       — Reviewer verdict 触发上游重做      │    │
│  └────┬────────────────────────────────────────────────────┘    │
├───────┼─────────────────────────────────────────────────────────┤
│       │              执行层                                      │
│  ┌────▼────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ mol-engine  │  │mol-agents│  │mol-exper. │  │ mol-llm    │  │
│  │ 代码生成引擎 │  │专业化Agent│  │沙箱执行   │  │ LLM 抽象层  │  │
│  │ 蓝图→生成→  │  │Benchmark │  │Local/     │  │ OpenAI/    │  │
│  │ 验证→修复   │  │CodeSearch│  │Docker/    │  │ Anthropic/ │  │
│  │ →树搜索→审查│  │Figure    │  │SSH/Colab  │  │ ACP Bridge │  │
│  └─────────────┘  └──────────┘  └──────────┘  └────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│                     基础设施层                                    │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌────────────────┐ │
│  │mol-common │ │mol-config │ │mol-domains│ │mol-knowledge   │ │
│  │共享类型    │ │配置加载    │ │领域检测    │ │知识库管理       │ │
│  │Prompt引擎 │ │YAML校验   │ │知识链解析  │ │Markdown/       │ │
│  │硬件检测    │ │默认值填充  │ │Prompt适配  │ │Obsidian后端    │ │
│  └───────────┘ └───────────┘ └───────────┘ └────────────────┘ │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌────────────────┐ │
│  │mol-web    │ │mol-liter. │ │mol-evolut.│ │mol-metamol     │ │
│  │Web搜索    │ │文献检索    │ │自进化系统  │ │元学习/PRM门控  │ │
│  │Scholar    │ │arXiv/S2/  │ │经验教训    │ │技能提取        │ │
│  │PDF抽取    │ │OpenAlex   │ │时间衰减    │ │LLM-as-Judge   │ │
│  └───────────┘ └───────────┘ └───────────┘ └────────────────┘ │
│  ┌───────────┐ ┌───────────┐                                   │
│  │mol-templ. │ │mol-health │                                   │
│  │LaTeX编译  │ │健康检查    │                                   │
│  │MD→LaTeX   │ │Doctor诊断 │                                   │
│  │会议模板   │ │依赖校验    │                                   │
│  └───────────┘ └───────────┘                                   │
└─────────────────────────────────────────────────────────────────┘
```

### 运行时数据流

```
用户提交研究主题
        │
        ▼
   ┌─────────────┐     ┌──────────────────────────────────────────┐
   │ Phase 1      │     │ Phase 2                                  │
   │ Strategy     │────▶│ Exploration                              │
   │              │     │                                          │
   │ 1.1 选题     │     │ 2.1 文献搜索 ──▶ 2.2 文献筛选 [GATE]     │
   │ 1.2 分解     │     │ 2.3 知识提取 ──▶ 2.4 综合+假设生成        │
   └──────────────┘     └───────────────────┬──────────────────────┘
                                            │
                        ┌───────────────────▼──────────────────────┐
                        │ Phase 3: Execution                       │
                        │                                          │
                        │ 3.1 实验设计 [GATE] ──▶ 3.2 代码搜索      │
                        │ 3.3 代码开发 ──▶ 3.4 实验循环              │
                        └───────────────────┬──────────────────────┘
                                            │
                        ┌───────────────────▼──────────────────────┐
                        │ Phase 4: Inference                       │
                        │                                          │
                        │ 4.1 结果分析                              │
                        │ 4.2 研究决策 ─┬─ proceed ──▶ Phase 5     │
                        │              ├─ pivot ────▶ 2.4 (换假设) │
                        │              ├─ refine ───▶ 3.4 (改实验) │
                        │              └─ stop ─────▶ 终止          │
                        │ 4.3 知识总结                              │
                        └───────────────────┬──────────────────────┘
                                            │
                        ┌───────────────────▼──────────────────────┐
                        │ Phase 5: Documentation                   │
                        │                                          │
                        │ 5.1 论文大纲 ──▶ 5.2 论文撰写             │
                        │ 5.3 同行评审 ──▶ 5.4 质量门 [GATE]        │
                        │ 5.5 发表 (LaTeX + PDF + BibTeX)          │
                        └──────────────────────────────────────────┘
```

---

## 3. Crate 依赖拓扑

### 17 个 Crate 总览

| Crate | 类型 | 核心职责 | 代码量级 |
|-------|------|---------|---------|
| **mol-cli** | Binary | CLI 入口，7 个子命令 | ~700 行 |
| **mol-common** | Library | 共享类型、Prompt 引擎、硬件检测、嵌入数据 | ~3000 行 |
| **mol-config** | Library | YAML 配置加载与校验 | ~800 行 |
| **mol-llm** | Library | LLM 抽象层（OpenAI/Anthropic/ACP） | ~2500 行 |
| **mol-pipeline** | Library | 18-stage 状态机、Runner、Checkpoint、Contract | ~5000 行 |
| **mol-engine** | Library | 代码生成引擎（蓝图→生成→验证→修复→审查） | ~3000 行 |
| **mol-agents** | Library | 专业化 Agent（Benchmark/CodeSearch/Figure） | ~2000 行 |
| **mol-experiment** | Library | 实验沙箱（Local/Docker/SSH/Colab） | ~2000 行 |
| **mol-services** | Library | Axum WebSocket 服务（Agent Bridge/Resource Monitor） | ~4000 行 |
| **mol-literature** | Library | 文献检索（arXiv/S2/OpenAlex）+ 新颖性评估 | ~1500 行 |
| **mol-web** | Library | Web 搜索、Scholar 爬虫、PDF 抽取 | ~1500 行 |
| **mol-domains** | Library | 领域检测（11 域）、Profile、Prompt 适配 | ~1500 行 |
| **mol-knowledge** | Library | 知识库（Markdown/Obsidian 后端） | ~500 行 |
| **mol-templates** | Library | LaTeX 编译、MD→LaTeX 转换、会议模板 | ~1500 行 |
| **mol-metamol** | Library | 元学习、PRM 质量门控、技能提取 | ~1000 行 |
| **mol-evolution** | Library | 自进化（经验记录、时间衰减、Prompt overlay） | ~600 行 |
| **mol-health** | Library | 环境健康检查（Doctor 诊断） | ~400 行 |

### 依赖图

```
                            mol-cli (binary)
                           ╱   │   │    ╲
                          ╱    │   │     ╲
                   mol-config  │   │   mol-health
                     │    ╲    │   │      │
                     │     ╲   │   │      │
                     │   mol-pipeline ◄───┘
                     │      │    ╲
                     │      │     ╲
                     │   mol-engine  mol-services
                     │      │          │    ╲
                     │      │          │   mol-llm ◄──────────┐
                     │      │          │     │                │
                     ▼      ▼          ▼     ▼                │
                  mol-common ◄─────────────────────────┐      │
                     ▲  ▲  ▲  ▲                        │      │
                     │  │  │  │                        │      │
              ┌──────┘  │  │  └──────┐                 │      │
              │         │  │         │                 │      │
         mol-domains    │  │    mol-knowledge     mol-agents  │
              │         │  │                        │  │      │
              │    mol-web │                        │  └──────┘
              │         │  │                        │
              │  mol-literature                mol-experiment
              │         │
              │    mol-templates
              │
         mol-evolution
              │
         mol-metamol
```

### 关键外部依赖

| 功能 | Crate | 说明 |
|------|-------|------|
| 异步运行时 | `tokio` (full) | 所有 async 操作基础 |
| HTTP 服务 | `axum` + `tower-http` | WebSocket + CORS + 静态文件 |
| HTTP 客户端 | `reqwest` | LLM API 调用 |
| 序列化 | `serde` + `serde_json` + `serde_yaml` | 全局数据格式 |
| 模板引擎 | `tera` | Jinja2 兼容 Prompt 渲染 |
| Docker | `bollard` | Docker 沙箱交互 |
| SSH | `ssh2` | 远程 GPU 服务器执行 |
| GPU 监控 | `nvml-wrapper` + `sysinfo` | NVIDIA GPU / 系统资源 |
| PDF 解析 | `lopdf` | 文献 PDF 文本抽取 |
| HTML 解析 | `scraper` | Scholar 页面解析 |
| Git | `git2` | 实验仓库管理 |
| 正则 | `regex` + `LazyLock` | 代码验证、输出解析 |

---

## 4. Pipeline 状态机

### 4.1 Stage 定义

Pipeline 核心定义在 `crates/mol-pipeline/src/stages.rs`。
每个 Stage 有稳定的 `i32` 判别值，用于 checkpoint 和 wire-format 兼容。

```rust
pub enum Stage {
    // Phase 1: Strategy
    TopicInit        = 1,   // 1.1
    ProblemDecompose = 2,   // 1.2

    // Phase 2: Exploration
    LiteratureSearch    = 3,   // 2.1
    LiteratureScreen    = 4,   // 2.2 GATE
    KnowledgeExtract    = 5,   // 2.3
    SynthesisHypotheses = 6,   // 2.4

    // Phase 3: Execution
    ExperimentDesign = 7,   // 3.1 GATE
    CodebaseSearch   = 8,   // 3.2
    CodeDevelop      = 9,   // 3.3
    ExperimentCycle  = 10,  // 3.4

    // Phase 4: Inference
    ResultAnalysis   = 11,  // 4.1
    ResearchDecision = 12,  // 4.2
    KnowledgeSummary = 13,  // 4.3

    // Phase 5: Documentation
    PaperOutline = 14,  // 5.1
    PaperWrite   = 15,  // 5.2
    PeerReview   = 16,  // 5.3
    QualityGate  = 17,  // 5.4 GATE
    Publish      = 18,  // 5.5

    Discussion = 100,   // 特殊: 带外讨论
}
```

### 4.2 状态转移

每个 Stage 的生命周期由 `StageStatus` 和 `TransitionEvent` 驱动：

```
                    ┌──────────────────────────────────────────────┐
                    │              Stage 状态机                     │
                    │                                              │
   Start            │  Pending ──Start──▶ Running                  │
                    │    ▲                  │                       │
   Resume           │    │            ┌────┤                       │
                    │    │            │    │                       │
                    │  Paused ◄──Timeout──┤                       │
                    │    ▲            │    │                       │
                    │    │            │    ▼                       │
                    │  Failed ◄──Fail─┘  Succeed                  │
                    │    │                  │                       │
                    │    │            ┌─────┴─────┐                │
                    │    ▼            │           │                │
                    │  Retrying   Gate Stage?  Non-Gate            │
                    │    │            │           │                │
                    │    └──Start──▶  ▼           ▼                │
                    │           BlockedApproval  Done ──▶ next     │
                    │              │       │                       │
                    │         Approve   Reject                    │
                    │              │       │                       │
                    │              ▼       ▼                       │
                    │            Done   Rollback Target            │
                    │                   (Pending)                  │
                    └──────────────────────────────────────────────┘
```

核心转移函数 `advance()` 返回 `TransitionOutcome`：

```rust
pub struct TransitionOutcome {
    pub stage: Stage,
    pub status: StageStatus,
    pub next_stage: Option<Stage>,       // Runner 下一步执行
    pub rollback_stage: Option<Stage>,   // 仅 Reject 时非 None
    pub checkpoint_required: bool,
    pub decision: String,                // "proceed" | "block" | "retry" | "pivot"
}
```

### 4.3 Gate 系统

三个 Gate Stage 在成功后进入 `BlockedApproval` 等待人工审批：

| Gate Stage | 审批后 | Reject 回滚目标 |
|-----------|--------|----------------|
| `LiteratureScreen` (4) | → KnowledgeExtract (5) | → LiteratureSearch (3) 重新搜索 |
| `ExperimentDesign` (7) | → CodebaseSearch (8) | → SynthesisHypotheses (6) 重新假设 |
| `QualityGate` (17) | → Publish (18) | → PaperOutline (14) 重写论文 |

`auto_approve_gates` 配置可跳过人工审批，自动通过所有 Gate。

### 4.4 Decision 回滚

Stage 12 (ResearchDecision) 完成后，根据 Agent 输出的决策关键词触发回滚：

```rust
pub fn decision_rollback(decision: &str) -> Option<Stage> {
    match decision {
        "pivot"  => Some(Stage::SynthesisHypotheses),  // 丢弃假设，重新生成
        "refine" => Some(Stage::ExperimentCycle),       // 保留假设，重跑实验
        _        => None,                               // "proceed" / "stop"
    }
}
```

**安全边界：**
- `MAX_DECISION_PIVOTS = 2` — 最多回滚 2 次
- Pivot 计数持久化到 `.pivot_count` 文件，递归子 pipeline 共享计数器
- 跨阶段返工 (Cross-stage rework) 使用 `rework_history` 去重，防止无限循环

### 4.5 I/O 契约 (Contract)

每个 Stage 声明其**必需输入**和**期望输出**，在执行前/后校验：

```rust
pub struct StageContract {
    pub required_inputs: Vec<&'static str>,   // 执行前必须存在
    pub expected_outputs: Vec<&'static str>,  // 执行后应当产出
}
```

**灵活匹配规则：**
- 精确名称匹配（`"topic_brief"`）
- 文件名别名（`"topic_brief"` 可由 `goal.md` 满足）
- 跨格式回退（`.md` ↔ `.json` ↔ `.yaml`）

契约可通过 `{knowledge_root}/contracts.yaml` 进行领域级覆盖。

### 4.6 Checkpoint 与断点续跑

**原子写入模式：**
1. 创建同目录 tempfile（确保同文件系统 rename 原子性）
2. 写入 JSON 到 tempfile
3. 原子 rename temp → target
4. 出错时 best-effort 清理 tempfile

```json
{
  "last_completed_stage": 9,
  "last_completed_name": "CODE_DEVELOP",
  "run_id": "mol-20260409-192434-9e9916",
  "timestamp": "2026-04-09T19:45:12.345Z"
}
```

续跑时，`resume_from_checkpoint()` 返回 checkpoint 之后的下一个 Stage。

**Heartbeat 文件**每 30 秒更新一次，记录当前进程 ID、阶段、已用时间，
供外部哨兵监控 pipeline 存活状态。

---

## 5. Agent 系统

### 5.1 Agent 角色定义

Mol-HEP-Lab 定义了 17 个专业化 Agent，分为**主执行 (Primary)** 和**顾问 (Advisor)** 两种模式：

| Agent | 主负责阶段 | 职能 |
|-------|-----------|------|
| Lead Analyst | Strategy, Design, Analysis | 编排策略与高层决策 |
| Investigator | Search, Screen, Extract | 文献检索、筛选、知识提取 |
| Theory Scout | Synthesis | 连接理论预测，生成假设 |
| Data Explorer | (advisor) | 数据集探索，分布与相关性分析 |
| Detector Specialist | (advisor) | 探测器响应、效率、分辨率 |
| Signal Lead | Codebase, Code Develop | 信号区设计，接受度优化 |
| Background Estimator | (advisor) | 本底模型构建 |
| Systematic Source Evaluator | (advisor) | 系统不确定性编目 |
| Systematics Fitter | Experiment Cycle | 系统变化基础设施实现 |
| ML Specialist | (advisor) | 机器学习组件设计与验证 |
| Cross Checker | (advisor) | 物理原理交叉验证 |
| Critical Reviewer | (advisor) | 逻辑漏洞与方法论风险识别 |
| Constructive Reviewer | (advisor) | 改进建议与替代方案 |
| Physics Reviewer | Peer Review | 物理有效性与发表就绪评估 |
| Arbiter | Decision, Quality Gate | 争议仲裁，最终审批权 |
| Plot Validator | (advisor) | 图表质量与规范合规验证 |
| Note Writer | Outline, Write, Publish | 文档组装为发表格式 |

### 5.2 Stage → Agent 映射

定义在 `hep/agents.yaml`：

```yaml
stage_agents:
  TOPIC_INIT:          lead-analyst
  PROBLEM_DECOMPOSE:   lead-analyst
  LITERATURE_SEARCH:   investigator
  LITERATURE_SCREEN:   investigator
  CODEBASE_SEARCH:     signal-lead
  CODE_DEVELOP:        signal-lead
  EXPERIMENT_CYCLE:    systematics-fitter
  PAPER_WRITE:         note-writer
  PEER_REVIEW:         physics-reviewer
  PUBLISH:             note-writer
```

**顾问注入**（作为 `{{ advisor_roles }}` 模板变量）：

```yaml
advisors:
  EXPERIMENT_DESIGN: [detector-specialist, data-explorer]
  CODE_DEVELOP:      [background-estimator, ml-specialist, plot-validator]
  EXPERIMENT_CYCLE:  [background-estimator, systematic-source-evaluator, ml-specialist]
  PEER_REVIEW:       [critical-reviewer, constructive-reviewer]
```

### 5.3 Agent Profile 格式

每个 Agent 定义为 `hep/agents/{name}.md`，包含 YAML 前言 + Markdown 正文：

```markdown
---
name: signal-lead
description: Designs signal regions and optimizes acceptance cuts
tools: [Read, Write, Edit, Bash, Grep, Glob, WebSearch, WebFetch]
model: default
artifact_conventions: true
---

You are a Signal Lead for HEP collider analysis...

## Responsibilities
- Design signal and control regions
- Optimize event selection cuts
- Implement acceptance × efficiency calculations
...
```

### 5.4 专业化 Agent 模块

`crates/mol-agents/src/` 实现了三个独立的 Agent 编排器：

#### Benchmark Agent

4 阶段流水线：`Surveyor → Selector → Acquirer → Validator`

- 搜索 Hugging Face 和 Web 上的标准基准数据集
- 根据硬件约束（GPU 内存、时间预算）筛选
- 验证数据加载器可用性
- 输出 `BenchmarkPlan`（选定基准、数据加载代码、实验备注）

#### Code Search Agent

GitHub 代码搜索 + 模式提取 + 缓存：

- 根据研究主题生成搜索查询
- 在 GitHub 上搜索相关仓库
- 提取 API 模式、文件结构、库版本
- 缓存结果供后续 code_develop 阶段使用
- 输出 `CodeSearchResult.to_prompt_context()` 注入代码生成

#### Figure Agent

6 阶段流水线：`Decision → Planner → CodeGen → Renderer → Critic → Integrator`

- 决定需要哪些图表
- 规划每张图的规格（类型、数据源、标题）
- 生成 Python matplotlib 代码
- 在沙箱中渲染图片
- 质量审查（strict_mode 强制高标准）
- 组装最终 manifest

### 5.5 多 Agent 协作模式

**Agentic Executor 模式**（`executor.rs`中的 `execute_agentic()`）：

1. 创建阶段输出目录
2. 重置 LLM session（防止上下文泄漏）
3. 从 prior artifacts + domain knowledge + methodology 构建模板变量
4. 渲染 Prompt 模板（含 `output_spec` 告诉 Agent 写哪些文件）
5. 调用 `llm_generate()` 发送 system/user prompts
6. Agent 使用 Write 工具直接写文件到阶段目录
7. Executor 扫描磁盘检查产出物
8. **回退**：若 Agent 未写文件，使用 response 文本 + `simple_clean()` 清理

**Multi-Agent Executor**（`execute_multi_agentic()`）：

主 Agent 执行后，并行启动 Reviewer Agent：
- 每个 Reviewer 接收主产出物内容作为上下文
- 写入结构化 verdict 文件（`{reviewer_name}_verdict.json`）
- Verdict 格式：`{"verdict": "approve|rework", "upstream_rework_stage": 6}`
- 支持通过 verdict 触发跨阶段返工

### 5.6 Discussion 引擎

多轮合成讨论模式，用于多 Agent 共识构建：

```
Round 1: Perspective Analysis — 各 Agent 提出观点
Round 2: Critical Review     — 交叉批评
Round 3: Consensus           — 最终综合
```

输出 `consensus.md` 注入后续阶段的假设生成。

---

## 6. LLM 集成层

### 6.1 Provider 架构

`crates/mol-llm/src/` 实现了统一的异步 LLM 客户端，支持多后端：

```rust
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages, json_mode, model_override) -> Result<LLMResponse>;
    async fn preflight(&self) -> PreflightResult;
    async fn reset_session(&self) -> Result<()>;
    async fn set_cwd(&self, path: &Path) -> Result<()>;
    async fn spawn_instance(&self) -> Result<Box<dyn LlmProvider>>;
}
```

**三种实现：**

| Provider | 协议 | 适用场景 |
|----------|------|---------|
| `LLMClient` | OpenAI-compatible REST API | GPT-4o, DeepSeek, Qwen 等 |
| `AnthropicAdapter` | Anthropic Messages API | Claude 系列 |
| `CliProvider` (ACP) | CLI 子进程桥接 | Claude Code, Codex, Gemini |

### 6.2 LLMClient 配置

```rust
pub struct LLMClientConfig {
    pub base_url: String,
    pub primary_model: String,
    pub fallback_models: Vec<String>,     // 主模型失败时依次尝试
    pub max_tokens: u32,
    pub temperature: f64,
    pub max_retries: u32,
    pub retry_base_delay: Duration,
    pub timeout: Duration,                // Duration::ZERO = 无超时（长任务）
    pub fallback_url: Option<String>,     // MetaMol 桥接 URL
    pub fallback_api_key: Option<String>,
}
```

### 6.3 模型路由

不同模型 API 使用不同的 token 限制参数：

```rust
// 使用 max_completion_tokens
const NEW_PARAM_MODELS: &[&str] = &["o3", "o3-mini", "o4-mini", "gpt-5.4"];

// 使用 max_output_tokens (Responses API)
const RESPONSES_API_MODELS: &[&str] = &["gpt-5", "gpt-5.1", "gpt-5.2"];

// 其他模型使用 max_tokens
```

### 6.4 ACP 桥接

ACP (Agent Client Protocol) 通过 CLI 子进程与本地 AI Agent 通信：

```rust
pub struct ACPConfig {
    pub agent: String,          // "claude" | "codex" | "gemini"
    pub cwd: PathBuf,
    pub acpx_command: String,   // "acpx" 可执行文件路径
    pub session_name: String,
    pub timeout_sec: u64,
}
```

**关键行为：**
- **Session 隔离**：每个阶段切换时 kill 旧 session + 创建新 session，防止上下文泄漏
- **大 Prompt 处理**：超过 100KB 时写入临时文件再传递
- **重连逻辑**：指数退避（2s → 4s → 8s → 16s → 32s）处理 stale session
- **Preflight 检查**：验证 `acpx` 命令和 Agent 可用性

### 6.5 JSON Mode

不同 Provider 使用不同策略启用结构化 JSON 输出：

- **OpenAI**：`response_format: {"type": "json_object"}`
- **Claude/DeepSeek**：系统 prompt 前置注入 `"You MUST respond with valid JSON only..."`

### 6.6 模型回退链

```
Primary Model (e.g. claude-3.5-sonnet)
    │ 失败
    ▼
Fallback Model 1 (e.g. gpt-4o)
    │ 失败
    ▼
Fallback Model 2 (e.g. deepseek-chat)
    │ 失败
    ▼
Error — 所有模型均失败
```

每次失败后指数退避 + 重试（最多 `max_retries` 次）。

---

## 7. Prompt 工程与模板系统

### 7.1 PromptEngine

基于 Tera（Jinja2 兼容）的模板引擎，定义在 `crates/mol-common/src/prompts.rs`：

```rust
pub struct PromptEngine {
    tera: tera::Tera,
}

impl PromptEngine {
    pub fn from_directory(dir: &Path) -> Result<Self>;      // 加载 *.md/*.txt/*.yaml 模板
    pub fn render(&self, name: &str, vars: impl IntoIterator) -> Result<String>;
    pub fn render_one_shot(template: &str, vars) -> Result<String>;  // 临时模板
}
```

### 7.2 模板结构

每个阶段模板由 `---user---` 分隔符分为 system 和 user 两部分：

```markdown
{{ agent_role | default(value="") }}

You are a research director making strategic decisions...

{{ conventions | default(value="") }}

{% if principles %}
## Analysis Principles
{{ principles }}
{% endif %}

---user---

Make a strategic research decision based on the experiment results.

Topic: {{ topic }}
Analysis report: {{ analysis_report | default(value="") }}
Hypotheses: {{ hypotheses | default(value="") }}

Decision options:
- **proceed**: results are promising
- **pivot**: change analysis strategy
- **stop**: insufficient sensitivity

{{ output_spec | default(value="") }}
```

### 7.3 模板变量注入

`template_vars()` 函数全面填充上下文（`executor.rs`）：

| 变量类 | 包含内容 |
|--------|---------|
| **研究上下文** | `topic`, `analysis_type`, `domain_id` |
| **Prior Artifacts** | `goal`, `hypotheses`, `exp_plan`, `codebase_context`, `analysis_report` 等 |
| **Agent 角色** | `agent_role`（主 Agent）, `advisor_roles`（顾问列表） |
| **方法论** | `principles`, `phase_requirements`, `artifact_format`, `review_protocol` |
| **领域知识** | `conventions`（从 `conventions/{analysis_type}.md` 加载） |
| **协议** | `blinding_protocol`, `coding_standards`, `tool_standards` |
| **数据** | `datasets`（用户指定目录优先） |
| **产出规格** | `output_spec`（ArtifactSpec 列表，告诉 Agent 应写哪些文件） |

### 7.4 默认 Prompt 系统

`data/prompts.default.yaml` 提供全局后备 prompt：

```yaml
# AutoResearchClaw — Default Prompt Templates

blocks:
  compute_budget: "Use lightweight approaches..."
  pkg_hint_sandbox: "Available packages: numpy, scipy..."
  topic_constraint: "Stay focused on the research topic..."

stages:
  TOPIC_INIT:
    max_tokens: 4096
    system: |
      You are a research coordinator...
    user: |
      Research topic: {topic}
      ...
  CODE_DEVELOP:
    max_tokens: 16384
    system: |
      You are an expert ML engineer...
      {codebase_context}
    user: |
      Generate experiment code for: {topic}
      Plan: {exp_plan}
      ...
```

### 7.5 知识链模板加载

`StagePromptEngine` 按优先级从知识链加载模板：

```
hep-cepc/templates/stages/code_develop.md  ← 最高优先级
hep/templates/stages/code_develop.md       ← HEP 通用
generic/templates/stages/code_develop.md   ← 通用后备
data/prompts.default.yaml                  ← 最终后备
```

---

## 8. 领域检测与知识链

### 8.1 领域检测

`crates/mol-domains/src/detector.rs` 通过关键词优先级检测研究领域：

```rust
pub enum ResearchDomain {
    HighEnergyPhysics, MachineLearning, Physics, Chemistry,
    Biology, Economics, Mathematics, Engineering, Security,
    Robotics, Generic,
}
```

**检测优先级：** HEP (最高) → Security → Robotics → Economics → Chemistry → Biology → Physics → Math → Engineering → ML → Generic (后备)

**HEP 关键词示例：** ATLAS, CMS, LHCb, Higgs, unfolding, CLs, pyhf, uproot, awkward, mplhep, jet tagging, cross-section, luminosity, NTuple...

还支持 LLM 辅助检测 (`detect_domain_with_llm()`) 作为关键词检测的补充。

### 8.2 Domain Profile

每个领域有完整的静态 Profile：

```rust
pub struct DomainProfile {
    pub domain: ResearchDomain,
    pub domain_id: String,               // "hep_collider"
    pub display_name: String,
    pub paradigm: ExperimentParadigm,     // HepAnalysis, Comparison, etc.
    pub default_metrics: Vec<DomainMetric>,
    pub benchmarks: Vec<String>,
    pub docker_image: String,             // "researchmol/sandbox-hep:latest"
    pub pip_packages: Vec<String>,
    pub suggested_frameworks: Vec<String>,
    pub experiment_templates: Vec<String>,
    pub gpu_required: bool,
}
```

### 8.3 知识链 (KnowledgeChain)

分层文件解析系统，定义在 `crates/mol-common/src/knowledge.rs`：

```rust
pub struct KnowledgeChain {
    roots: Vec<PathBuf>,  // e.g. ["hep-cepc", "hep", "generic"]
}

impl KnowledgeChain {
    pub fn read_first(&self, rel_path: &str) -> Option<String>;  // 首个匹配
    pub fn read_all(&self, rel_path: &str) -> Vec<String>;       // 所有层级
    pub fn resolve(&self, rel_path: &str) -> Option<PathBuf>;    // 路径解析
    pub fn templates_dir(&self) -> Option<PathBuf>;
}
```

**解析示例：**
```
查找 "agents/signal-lead.md":
  hep-cepc/agents/signal-lead.md  →  未找到
  hep/agents/signal-lead.md       →  找到，返回
  generic/agents/signal-lead.md   →  不需要
```

### 8.4 Prompt 适配器

`PromptAdapter` trait 实现领域感知的 prompt 注入：

```rust
pub struct PromptBlocks {
    pub compute_budget: String,
    pub dataset_guidance: String,
    pub hp_reporting: String,
    pub code_generation_hints: String,
    pub result_analysis_hints: String,
    pub experiment_design_context: String,
    pub statistical_test_guidance: String,
    pub output_format_guidance: String,
}
```

`ChainBackedAdapter` 从 YAML 知识链文件动态加载，优于硬编码的静态 Profile。

### 8.5 Domain YAML 配置

`hep/domain.yaml` 示例：

```yaml
domain: high_energy_physics
domain_id: hep_collider
paradigm: hep_analysis
metrics:
  - name: significance
    direction: higher_better
  - name: CLs_upper_limit
    direction: lower_better
  - name: cross_section
    direction: higher_better
docker_image: "researchmol/sandbox-hep:latest"
pip_packages: [uproot, awkward, hist, pyhf, fastjet, xgboost, mplhep]
experiment_templates:
  - NTuple reading
  - staged blinding
  - pyhf workspace construction
  - CLs exclusion
  - mplhep plots
```

---

## 9. 代码生成引擎

### 9.1 架构概览

`crates/mol-engine/src/` 实现了多阶段 LLM + 沙箱循环的代码生成：

```
                     CodegenRuntime
                          │
            ┌─────────────┤
            │             │
    文件系统探测     策略选择
    (Discovery)    (Routing)
            │             │
            │      ┌──────┴──────┐
            │      │             │
            │  MolAgent      Fallback
            │  Strategy      Strategy
            │      │
            │      ▼
            │  Phase 1: 蓝图规划
            │      │
            │  Phase 2: 顺序文件生成
            │      │
            │  Phase 2.5: 硬验证门控
            │      │
            │  Phase 3: 执行中修复
            │      │
            │  Phase 4: 解树搜索 (可选)
            │      │
            │  Phase 5: 多 Agent 审查
            │      │
            ▼      ▼
       CodegenResult
       {files, strategy_name, elapsed_sec}
```

### 9.2 Phase 1: 蓝图规划

LLM 生成 YAML 蓝图，指定每个文件的结构：
- 依赖关系
- 伪代码
- 函数签名
- 生成顺序

### 9.3 Phase 2: 顺序文件生成

文件按依赖顺序逐一生成：
- 已生成代码的摘要 (CodeMem) 注入 prompt
- 防止 import 错误，提高跨文件一致性

### 9.4 Phase 2.5: 硬验证门控

使用预编译正则表达式（`LazyLock`）检测常见问题：

| 检查项 | 正则模式 | 说明 |
|--------|---------|------|
| 语法错误 | 缺少冒号、括号不匹配 | 基本 Python 语法 |
| 硬编码指标 | `RE_HARDCODED_METRIC` | `accuracy = 0.95` 等伪造 |
| 跨文件 import | import 路径解析 | 引用不存在的模块 |
| 空存根类 | 空 class body | 未实现的占位符 |
| 未定义名称 | `undefined`, `NULL`, `NONE` | 占位符值 |
| nn.Module 反模式 | forward() 中创建层 | PyTorch 最佳实践 |

检测到问题后触发**靶向 LLM 修复**，而非全文重生成。

### 9.5 Phase 3: 执行中修复

```
生成代码 → 沙箱执行 → 解析 stderr/traceback → LLM 修复 → 再执行
                                     ↑                        │
                                     └────────────────────────┘
                                        (最多 10 次迭代)
```

### 9.6 Phase 4: 解树搜索 (可选)

探索多个候选实现：
- 按 metric、runtime、无错误 评分
- 选择最高分节点
- 维护解历史用于回滚

### 9.7 Phase 5: 多 Agent 审查

独立的 Reviewer → Coder 循环：
- 安全回退：若审查拒绝，回滚到先前版本
- 防止质量退化

### 9.8 DiscoveredData — 文件系统上下文

运行时从文件系统自动探测：

```rust
pub struct DiscoveredData {
    pub checkpoint_model_index: HashMap<K, V>,  // model config JSON
    pub checkpoint_files: Vec<String>,
    pub dataset_files: Vec<String>,
    pub dataset_sample: String,                 // 前 1000 字符采样
    pub codebase_files: Vec<String>,
    pub codebase_readme: String,                // README 截断 3000 字符
}
```

---

## 10. 实验执行与沙箱

### 10.1 四种沙箱后端

`crates/mol-experiment/src/` 定义统一接口：

```rust
pub trait Sandbox: Send + Sync {
    async fn execute(&self, code: &str) -> Result<ExecutionResult>;
    async fn cleanup(&self) -> Result<()>;
}
```

| 后端 | 技术 | 隔离性 | GPU | 适用场景 |
|------|------|--------|-----|---------|
| **Local** | subprocess | 无 | 直接 | 开发调试，速度最快 |
| **Docker** | bollard crate | 容器级 | 透传 | 生产环境，安全隔离 |
| **SSH** | ssh2 crate | 远程 | 远程 GPU | GPU 集群 |
| **Colab** | Drive 文件轮询 | 云端 | Colab GPU | 无本地 GPU |

### 10.2 代码安全验证

执行前进行多层验证：

```rust
pub fn validate_code(code: &str) -> Vec<ValidationIssue> {
    // Security: 检测危险操作 (os.system, subprocess, eval)
    // Syntax: Python 语法检查
    // ImportBlacklist: 禁用 import 黑名单
    // Performance: 性能警告
    // Maintainability: 代码质量
}
```

`ValidationIssue` 带有 `Category` 和 `Severity`（Info / Warning / Error / Critical）。

### 10.3 GPU 自动选择

`experiment_run.rs` 实现智能 GPU 选择：
- 调用 `nvidia-smi` 获取 GPU 状态
- 评分公式：`0.5 × mem% + 0.5 × util%`
- 选择得分最低的 GPU（最空闲）

### 10.4 Sanity Check 运行时

`runtimes/sanity_check.rs` 实现代码冒烟测试：

**工作区准备：**
1. 复制实验文件到隔离工作区
2. 符号链接共享目录（datasets, checkpoints）

**成功检测优先级：**
1. 结构化 `sanity_verdict.json`：`{"verdict": "pass|fail"}`
2. 输出文件存在性（figures/, results/ 等）
3. 短语匹配（pass/fail 关键词列表）— 传统后备

### 10.5 迭代精炼运行时

`runtimes/iterative_refine.rs` 实现实验优化循环：

- 加载 baseline 指标（搜索 `runs/*/results.json`）
- 比较 baseline vs. 当前指标（支持 minimize/maximize 方向）
- 复制最终实验到 `experiment_final/`
- 排除符号链接和隐藏文件

### 10.6 Pipeline 级迭代

`execute_iterative_pipeline()` 在 Stage 10-11 之间运行内循环：

```
ExperimentCycle (10) → ResultAnalysis (11) → 不满意? → ExperimentCycle → ...
                                                           (最多 max_iterations 次)
```

---

## 11. 自进化与元学习

### 11.1 Evolution Store

`crates/mol-evolution/src/lib.rs` 记录每次运行的经验教训：

```rust
pub struct LessonEntry {
    pub id: String,                    // UUID v4
    pub category: LessonCategory,      // 8 类
    pub severity: LessonSeverity,      // Info(1.0) / Warning(1.5) / Critical(2.5)
    pub title: String,
    pub description: String,
    pub stage: u32,
    pub run_id: String,
    pub timestamp: DateTime<Utc>,
    pub tags: Vec<String>,
    pub applied_count: u32,
}
```

**8 个经验类别：**
Configuration, Methodology, DataHandling, ModelSelection, ExperimentDesign, Writing, CodeGeneration, ResourceManagement

### 11.2 时间衰减排序

经验按时间衰减加权排序：

```
权重 = severity_weight × stage_match_boost × time_decay

time_decay: 30 天半衰期
stage_match_boost: 直接匹配 2x
max_age: 90 天（更老的排除）
每阶段返回 top 5 经验
```

### 11.3 Prompt Overlay 注入

`get_evolution_overlay(stage)` 生成经验覆盖文本，注入未来运行的 prompt：

```
## Lessons from Previous Runs

1. [WARNING] GPU memory estimation was off by 3x...
2. [CRITICAL] Hardcoded batch size caused OOM on A100...
3. [INFO] Using mixed precision reduced training time by 40%...
```

### 11.4 失败分类启发式

根据失败 Stage 自动分类：
- Stages 3-5（文献）→ Methodology
- Stage 6（综合）→ DataHandling
- Stages 7, 9, 10（实验）→ ExperimentDesign / CodeGeneration / ResourceManagement
- Stages 14-18（写作）→ Writing

### 11.5 PRM 质量门控

`crates/mol-metamol/src/prm_gate.rs` 实现 LLM-as-Judge 多数投票：

```rust
pub struct PRMConfig {
    pub votes: usize,         // 默认 3
    pub temperature: f32,     // 默认 0.6
    pub gate_stages: Vec<u32>,  // 默认 [5, 9, 15, 20]
}

pub struct PRMResult {
    pub passed: bool,         // majority_score > 0
    pub score: f32,           // -1.0, 0.0, 或 1.0
    pub votes: Vec<f32>,
    pub feedback: String,
}
```

**门控阶段：**
- Stage 5: 文献筛选质量
- Stage 9: 实验设计严谨性
- Stage 15: PROCEED/PIVOT 决策合理性
- Stage 20: 整体论文质量

### 11.6 经验 → 技能转化

`crates/mol-metamol/src/lesson_to_skill.rs` 将关键失败转化为可复用技能：

```rust
pub struct SkillDraft {
    pub title: String,              // "arc-gpu-memory-estimation"
    pub description: String,        // 何时使用
    pub category: String,           // automation/coding/research/...
    pub stage_applicability: Vec<u32>,
    pub prompt_template: String,    // 完整 Markdown 正文
}

pub async fn convert_lessons(lessons, config) -> Vec<SkillDraft>;
pub async fn write_skill_draft(draft, skills_dir) -> Result<()>;
```

### 11.7 Stage → Skill 映射

`stage_skill_map.rs` 定义每个阶段可注入的技能：

```
Stage 1  → ["literature-search-strategy"]
Stage 6  → ["research-gap-identification", "hypothesis-formulation"]
Stage 9  → ["hardware-aware-coding", "experiment-debugging"]
Stage 15 → ["academic-writing-structure", "peer-review-methodology"]
Stage 18 → ["citation-integrity"]
```

---

## 12. 文献检索与新颖性评估

### 12.1 三大学术 API

`crates/mol-literature/src/` 支持三个文献源：

| Provider | API | 特点 |
|----------|-----|------|
| **arXiv** | REST XML | 预印本全文 |
| **Semantic Scholar** | S2 Graph API | 引用网络、嵌入 |
| **OpenAlex** | REST JSON | 机构、概念图谱 |

统一接口：

```rust
pub async fn search_papers(
    query: &str,
    provider: Provider,
    options: SearchOptions,
) -> Result<Vec<Paper>>;
```

### 12.2 新颖性评估

`novelty.rs` 评估研究假设的新颖程度：

```rust
pub struct NoveltyReport {
    pub novelty_score: f64,         // 0.0–1.0
    pub assessment: String,         // "high" | "moderate" | "low" | "critical"
    pub similar_papers: Vec<SimilarPaper>,
    pub recommendation: String,     // "proceed" | "differentiate" | "abort"
    pub search_coverage: String,    // "insufficient" | "partial" | "full"
}
```

工作流程：
1. 从假设文本提取关键词
2. 在三大 API 上搜索相似论文
3. 计算语义相似度
4. 评估新颖性得分 + 推荐行动

### 12.3 引用验证

`citation.rs` 验证论文引用的真实性（防止 AI 编造引用）：

```rust
pub struct VerificationReport {
    pub total_citations: usize,
    pub verified: usize,
    pub unverified: usize,
    pub status: Vec<VerifyStatus>,  // 逐条验证状态
}
```

### 12.4 Web 搜索与爬虫

`crates/mol-web/src/`：

- **Tavily 搜索**：主搜索引擎（API key）
- **DuckDuckGo**：后备搜索
- **Scholar 爬虫**：Google Scholar 抓取（限速 + UA 轮换）
- **HTML 爬虫**：通用网页文本提取
- **PDF 抽取**：`lopdf` 解析 PDF 元数据和文本
- **连通性检查**：网络可达性预检

---

## 13. CLI 命令行接口

### 13.1 7 个子命令

`crates/mol-cli/src/main.rs` 基于 `clap` 定义：

| 命令 | 用途 | 关键参数 |
|------|------|---------|
| `mol run` | 执行研究 Pipeline | `--topic`, `--from-stage`, `--to-stage`, `--auto-approve`, `--resume` |
| `mol init` | 交互式创建配置 | 选择 Provider (openai/openrouter/deepseek/acp) |
| `mol validate` | 校验 config.mol.yaml | 必需键、可选键、路径存在性 |
| `mol doctor` | 环境健康诊断 | Python、GPU、LLM 连通性、依赖 |
| `mol serve` | 启动统一服务器 | `--port`, `--public`, `--frontend-dir` |
| `mol report` | 生成运行报告 | 从产出物目录生成可读摘要 |
| `mol setup` | 安装可选工具 | OpenCode 等 |

### 13.2 `mol run` 执行流程

```
1. 解析配置文件 (config.mol.yaml → config.yaml → config.arc.yaml)
2. 加载 MolConfig (typed YAML)
3. 创建 LLM Provider (API → CLI → fallback)
4. LLM Preflight 连通性检查
5. 生成 Run ID: mol-{timestamp}-{6-char-hash}
6. 构建 KnowledgeChain (hep/ + generic/ + custom)
7. 构建 Executor Config (实验设置、知识链、领域)
8. 执行 Pipeline: execute_pipeline_with_llm()
9. 输出摘要 (stages completed/failed/skipped, total time)
```

### 13.3 `mol serve` 统一服务器

单 Tokio runtime 承载 3 个子系统：

```
Axum Router
├─ /ws/agents       → Agent Bridge WebSocket
├─ /ws/resources    → Resource Monitor WebSocket
├─ /download/{*path} → 产出物文件下载
└─ /*               → React 前端静态文件
```

**产出物下载安全：**
- 路径规范化 (canonicalize)
- 禁止路径遍历攻击
- 优先直接路径，后备按 newest-first 搜索

---

## 14. 前端 Dashboard

### 14.1 技术栈

```
React 19.2.4 + TypeScript 5.9.3 + Vite
WebSocket 双通道 (agents + resources)
国际化 (中文 / English)
深色/浅色主题切换
```

### 14.2 5 层金字塔可视化

前端将 18 个 Stage 映射为 5 层金字塔，配合反馈回路动画：

```
             ┌───────────────────┐
             │  Phase 1: Strategy│  (橙色 #f59e0b)
             │  Lead Analyst     │
             └───────┬───────────┘
                     │ ↓ Data Flow Arrow
         ┌───────────┴───────────────┐
         │  Phase 2: Exploration     │  (蓝色 #3b82f6)
         │  Theory Scout, Data Expl. │
         └───────────┬───────────────┘
                     │
     ┌───────────────┴───────────────────┐
     │  Phase 3: Execution               │  (绿色 #10b981)
     │  Signal Lead, BG Est., ML Spec.   │
     └───────────────┬───────────────────┘
                     │
   ┌─────────────────┴─────────────────────┐
   │  Phase 4: Inference                   │  (红色 #ef4444)
   │  Cross Checker, Systematics Fitter    │
   └─────────────────┬─────────────────────┘
                     │
 ┌───────────────────┴───────────────────────┐
 │  Phase 5: Documentation                   │  (紫色 #a855f7)
 │  Physics Reviewer, Note Writer, Plot Val. │
 └────────────────────────────────────────────┘

         ╔═══════════════════════╗
         ║  Feedback Loop Arrow  ║  Inference → Exploration
         ╚═══════════════════════╝
```

### 14.3 核心组件

| 组件 | 功能 |
|------|------|
| `ProjectPanel` | 项目创建、快速提交（Lab/Reproduce 模式）、项目列表 |
| `LayerPanel` | 单层 Agent 显示、状态指示器、日志 |
| `ResourceMonitor` | 实时 CPU/GPU 指标（利用率、内存、温度） |
| `LogPanel` | 时序日志流，按严重度着色 |
| `HumanFeedbackPanel` | HITL 聊天输入，人工反馈注入 |
| `DataShelf` | 按数据仓库浏览产出物（Knowledge/Papers/Results/Insights） |
| `DataFlowArrow` | 层间数据流动画箭头 |

### 14.4 WebSocket 消息协议

```typescript
type WSMessage =
  | { type: 'agent_update';      payload: MolAgent }
  | { type: 'artifact_produced'; payload: Artifact }
  | { type: 'log';               payload: LogEntry }
  | { type: 'stage_update';      payload: { agentId; stage; status } }
  | { type: 'resource_stats';    payload: ResourceStats }
  | { type: 'project_list';      payload: ProjectInfo[] }
  | { type: 'chat_message';      payload: ChatMessage }
  | { type: 'queue_update';      payload: QueueSummary }
```

### 14.5 Mock 模式

无后端服务时的降级演示：
- 模拟 11 个 Agent 活动
- 每 1.5–3.5 秒发射随机事件
- 生成 HEP 领域相关的日志消息
- 产出物带仓库映射

### 14.6 Vite 开发代理

```typescript
server: {
  proxy: {
    '/ws/resources': 'ws://localhost:8905/',
    '/ws/agents':    'ws://localhost:8906/',
    '/download':     'http://localhost:8906/',
  }
}
```

---

## 15. 配置系统

### 15.1 MolConfig 顶层结构

`crates/mol-config/src/types.rs` 定义全局配置：

```yaml
# config.mol.yaml

project:
  name: "Z Boson Dimuon Analysis"
  mode: FullAuto          # DocsFirst | SemiAuto | FullAuto

research:
  topic: "Dimuon invariant mass spectrum: Z boson identification..."
  domains: [hep]
  daily_paper_count: 10
  quality_threshold: 0.7
  knowledge_root: "hep"
  knowledge_chain: ["hep", "generic"]

runtime:
  timezone: "Asia/Shanghai"
  max_parallel_tasks: 4
  approval_timeout_hours: 24
  retry_limit: 3

llm:
  provider: "openai"
  base_url: "https://api.openai.com/v1"
  api_key_env: "OPENAI_API_KEY"
  primary_model: "gpt-4o"
  coding_model: "gpt-4o"
  fallback_models: ["gpt-4.1", "gpt-4o-mini"]
  timeout_sec: 300
  max_retries: 3
  acp:
    agent: "claude"
    timeout_sec: 600

experiment:
  mode: Sandbox           # Simulated | Sandbox | Docker | SshRemote | ColabDrive
  time_budget_sec: 3600
  max_iterations: 10
  metric_key: "significance"
  metric_direction: Maximize
  datasets_dir: "./data"
  sandbox:
    python_path: "python3"
    gpu_required: false
    max_memory_mb: 4096
  docker:
    memory_limit_mb: 8192
    gpu_device_id: "0"

security:
  hitl_required_stages: [4, 7, 17]
  allow_publish_without_approval: false
  redact_sensitive_logs: true

export:
  target_conference: "NeurIPS2025"
  paper_format: "latex"

notifications:
  channel: "slack"
  on_stage_fail: true
  on_gate_required: true

web_search:
  enable_scholar: true
  enable_tavily: true

metamol_bridge:
  enabled: false
```

### 15.2 配置加载顺序

```
显式 --config 参数
  → config.mol.yaml
    → config.yaml
      → config.arc.yaml
```

### 15.3 硬件检测

`crates/mol-common/src/hardware.rs`：

```rust
pub struct HardwareProfile {
    pub has_gpu: bool,
    pub gpu_type: String,      // "cuda" | "npu" | "mps" | "cpu"
    pub gpu_name: String,
    pub vram_mb: Option<u64>,
    pub tier: String,          // "high" | "limited" | "cpu_only"
    pub warning: String,
}
```

**检测优先级：**
1. NVIDIA CUDA (`nvml-wrapper` crate)
2. NVIDIA CUDA (`nvidia-smi` 子进程后备)
3. Huawei Ascend NPU (`npu-smi`)
4. Apple Silicon MPS (`sysinfo` + `uname`)
5. CPU-only 后备

### 15.4 嵌入数据

`mol-common` 通过 `include_str!` 将多个 YAML 文件编译进二进制：

| 数据 | 说明 |
|------|------|
| `SEMINAL_PAPERS_YAML` | 30+ 领域奠基论文，按关键词索引 |
| `BENCHMARK_KNOWLEDGE_YAML` | 领域基准数据集、baseline、指标 |
| `DATASET_REGISTRY_YAML` | 分层数据集（Tier 1: 预缓存, Tier 2: 可下载, Tier 3: 过大） |
| `DOCKER_PROFILES_YAML` | 领域 → Docker 镜像 + 包映射 |
| `PROMPTS_DEFAULT_YAML` | 18 个阶段的默认 prompt 模板 |

---

## 16. 产出物结构

### 16.1 目录布局

每次 Pipeline 运行产生一个独立目录：

```
artifacts/mol-{timestamp}-{hash}/
├── checkpoint.json              # 断点续跑记录
├── heartbeat.json               # 运行时心跳
├── pipeline_summary.json        # 运行摘要
├── blinding_status.json         # 盲法状态
├── .pivot_count                 # 决策回滚计数
├── .rework_history              # 返工去重记录
│
├── stage-01/                    # 1.1 Topic Init
│   ├── goal.md                  #   SMART 目标
│   ├── hardware_profile.json    #   硬件检测结果
│   └── stage_log.md             #   阶段执行日志
│
├── stage-02/                    # 1.2 Problem Decompose
│   ├── problem_tree.md
│   └── topic_evaluation.json
│
├── stage-03/                    # 2.1 Literature Search
│   ├── search_plan.yaml
│   ├── sources.json
│   └── candidates.jsonl
│
├── stage-04/                    # 2.2 Literature Screen [GATE]
│   └── screened_papers.jsonl
│
├── stage-05/                    # 2.3 Knowledge Extract
│   └── knowledge_cards.json
│
├── stage-06/                    # 2.4 Synthesis & Hypotheses
│   ├── synthesis_report.md
│   └── hypotheses.md
│
├── stage-07/                    # 3.1 Experiment Design [GATE]
│   └── exp_plan.yaml
│
├── stage-08/                    # 3.2 Codebase Search
│   └── codebase_context.md
│
├── stage-09/                    # 3.3 Code Develop
│   ├── experiment_spec.md
│   ├── experiment_code.md
│   ├── experiment/              #   生成的实验代码
│   │   ├── main.py
│   │   ├── model.py
│   │   └── utils.py
│   ├── sanity_report.json
│   └── figures/                 #   matplotlib 图表
│
├── stage-10/                    # 3.4 Experiment Cycle
│   ├── resource_plan.md
│   ├── run_report.md
│   ├── refinement_log.json
│   └── runs/
│       ├── run-001/results.json
│       └── run-002/results.json
│
├── stage-11/                    # 4.1 Result Analysis
│   └── analysis_report.md
│
├── stage-12/                    # 4.2 Research Decision
│   ├── decision_record.md
│   └── stage_log.md
│
├── stage-13/                    # 4.3 Knowledge Summary
│   └── knowledge_summary.md
│
├── stage-14/                    # 5.1 Paper Outline
│   └── paper_outline.md
│
├── stage-15/                    # 5.2 Paper Write
│   ├── paper_draft.md
│   └── paper_revised.md
│
├── stage-16/                    # 5.3 Peer Review
│   └── review_comments.md
│
├── stage-17/                    # 5.4 Quality Gate [GATE]
│   └── quality_report.md
│
└── stage-18/                    # 5.5 Publish
    ├── archive_manifest.json
    ├── paper_final.md           #   最终论文 Markdown
    ├── paper.tex                #   LaTeX 源码
    ├── paper.pdf                #   编译 PDF
    └── references.bib           #   BibTeX 参考文献
```

### 16.2 实际运行示例

以 `artifacts/mol-20260409-192434-9e9916/` 为例（Z 玻色子双缪子分析）：

**研究主题：** "Dimuon Invariant Mass Spectrum: Z Boson Identification, Background Estimation, and Signal Extraction"

**关键产出：**

- **goal.md (19KB):** 完整物理分析方案
  - 信号过程：pp → Z/γ* → μ⁺μ⁻
  - 截面：972 ± 30 pb (NNLO)
  - 本底源：DY continuum, tt̄, WW/WZ
  - 80–100 GeV 窗口期望 Z 产额：404 事件

- **experiment/ (生成代码):**
  - 事件选择（muon pT > 20 GeV, |η| < 2.4, 异号电荷）
  - 区域定义（SR: 80–100 GeV, CR-Low: 40–70 GeV, CR-High: 100–110 GeV）
  - 信号模型（Breit-Wigner ⊗ Gaussian）
  - 统计模型（pyhf HistFactory）
  - 系统不确定性（形状、尺度、分辨率）

- **figures/ (25+ 图表):** 质量谱、cutflow、拟合图、N-1 分布、ROC 曲线

- **paper_final.md (80KB):** 完整物理论文
  - 标题："Data-Driven Z Boson Signal Yield Measurement..."
  - 结果：19.2σ 显著性, μ = 0.669 ± 0.050 (stat) ± 0.032 (syst)
  - 34 条参考文献

- **paper.pdf (363KB):** 编译 PDF，含嵌入图表、方程、表格

---

## 17. 数据流全景

### 17.1 完整执行时序

```
用户: mol run --topic "Z boson dimuon analysis" --auto-approve
        │
        ▼
   ┌─ mol-cli ──────────────────────────────────────────────────────┐
   │  1. 解析 config.mol.yaml                                       │
   │  2. 创建 LLM Provider (OpenAI / Anthropic / ACP)              │
   │  3. Preflight: 验证 API 连通性                                  │
   │  4. 生成 Run ID: mol-20260409-192434-9e9916                    │
   │  5. 构建 KnowledgeChain: ["hep", "generic"]                   │
   └────┬───────────────────────────────────────────────────────────┘
        │
        ▼
   ┌─ mol-pipeline::execute_pipeline() ─────────────────────────────┐
   │                                                                 │
   │  for stage in STAGE_SEQUENCE {                                  │
   │      // 1. 检查 Checkpoint 续跑                                  │
   │      // 2. Contract 校验 (required_inputs 存在?)                 │
   │      // 3. 构建 StageContext (prior_artifacts + config)         │
   │      // 4. 启动 30s Heartbeat 循环                               │
   │      // 5. execute_stage(stage, &context)                       │
   │      //    ├─ 加载 Tera 模板 (知识链优先级)                       │
   │      //    ├─ 注入 template_vars (prior artifacts, domain, etc) │
   │      //    ├─ LLM generate (system + user prompt)               │
   │      //    ├─ Agent 写文件到 stage-XX/                           │
   │      //    └─ 扫描产出物                                        │
   │      // 6. 注册产出物到 artifact_registry                       │
   │      // 7. 原子写入 Checkpoint                                   │
   │      // 8. 写入 stage_log.md                                    │
   │      // 9. 处理 Gate / Decision / Rework                        │
   │  }                                                              │
   │                                                                 │
   │  生成 pipeline_summary.json                                     │
   └─────────────────────────────────────────────────────────────────┘
        │
        ▼
   artifacts/mol-20260409-192434-9e9916/
   ├── stage-01/ ... stage-18/
   └── pipeline_summary.json
```

### 17.2 跨阶段产出物流动

```
Stage 1 (goal.md) ──────────────────────▶ 所有后续阶段 (研究主题)
Stage 2 (problem_tree.md) ─────────────▶ Stage 3, 6 (子问题)
Stage 3 (candidates.jsonl) ────────────▶ Stage 4 (筛选输入)
Stage 5 (knowledge_cards.json) ────────▶ Stage 6 (知识综合)
Stage 6 (hypotheses.md) ───────────────▶ Stage 7, 12 (实验设计, 决策)
Stage 7 (exp_plan.yaml) ───────────────▶ Stage 9, 10 (代码生成, 实验)
Stage 8 (codebase_context.md) ─────────▶ Stage 9 (代码上下文)
Stage 9 (experiment/) ─────────────────▶ Stage 10 (实验执行)
Stage 10 (runs/results.json) ──────────▶ Stage 11 (结果分析)
Stage 11 (analysis_report.md) ─────────▶ Stage 12 (研究决策)
Stage 12 (decision_record.md) ─────────▶ Stage 13 或回滚
Stage 13 (knowledge_summary.md) ───────▶ Stage 14, 15 (论文)
Stage 15 (paper_draft.md) ────────────▶ Stage 16 (同行评审)
Stage 16 (review_comments.md) ────────▶ Stage 15 (修订)
Stage 15 (paper_revised.md) ──────────▶ Stage 17 (质量门)
Stage 17 (quality_report.md) ─────────▶ Stage 18 (发表)
```

### 17.3 决策回滚数据流

```
Stage 12 输出 decision_record.md
    │
    ├─ "proceed" → Stage 13 → Phase 5
    │
    ├─ "pivot" → 回滚到 Stage 6
    │            (丢弃旧 hypotheses.md，重新生成)
    │            (pivot_count++ ≤ MAX_DECISION_PIVOTS=2)
    │
    ├─ "refine" → 回滚到 Stage 10
    │             (保留假设，重跑实验)
    │             (pivot_count++)
    │
    └─ "stop" → Pipeline 终止
```

### 17.4 6 个共享数据仓库

前端通过 6 个逻辑仓库组织产出物：

| 仓库 | 图标 | 来源阶段 | 用途 |
|------|------|---------|------|
| **Knowledge** | 💡 | Exploration (3-6) | 文献知识 → 实验设计 |
| **Exp Design** | 🧪 | Execution (7) | 实验方案 |
| **Codebase** | 💻 | Execution (8-10) | 代码 → 推理 |
| **Results** | 📊 | Inference (11-13) | 分析结果 |
| **Insights** | 🧠 | 跨项目 | 积累知识 |
| **Papers** | 📝 | Documentation (14-18) | 论文产出 |

---

## 附录: 关键设计模式总结

| 模式 | 实现位置 | 说明 |
|------|---------|------|
| **原子 Checkpoint** | `checkpoint.rs` | tempfile + rename 防止崩溃损坏 |
| **灵活产出物名** | `contracts.rs` | 别名 + 跨格式回退处理命名变体 |
| **Verdict-First 返工** | `executor.rs` | 结构化 JSON verdict 触发上游重做 |
| **模板驱动 Prompt** | `executor.rs` + `prompts.rs` | 知识链注入丰富每阶段上下文 |
| **有界递归** | `runner.rs` | MAX_DECISION_PIVOTS=2 + 返工去重防死循环 |
| **优雅降级** | `runner.rs` | skip-noncritical + graceful-degradation 容忍部分失败 |
| **分层知识解析** | `KnowledgeChain` | specific → generic 不重复实现 |
| **多后端抽象** | `LlmProvider` / `Sandbox` | Trait 统一 API/CLI/Docker/SSH 接口 |
| **自进化** | `EvolutionStore` | 时间衰减经验注入未来 prompt |
| **嵌入数据** | `include_str!` | 离线可用，零 I/O 启动 |
| **Session 隔离** | `acp.rs` | kill + recreate 防止 LLM 上下文泄漏 |
| **硬验证门控** | `code_agent.rs` | 20+ 预编译正则检测代码缺陷 |
