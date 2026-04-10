// ===================== Pipeline Stage Definitions (Phase.Step) =====================
// Aligned with backend: crates/mol-pipeline/src/stages.rs (18 stages)

export const RCStage = {
  // Phase 1: Strategy
  TOPIC_INIT: 1,
  PROBLEM_DECOMPOSE: 2,
  // Phase 2: Exploration
  LITERATURE_SEARCH: 3,       // ← SearchStrategy + LiteratureCollect
  LITERATURE_SCREEN: 4,       // GATE
  KNOWLEDGE_EXTRACT: 5,
  SYNTHESIS_HYPOTHESES: 6,    // ← Synthesis + HypothesisGen
  // Phase 3: Execution
  EXPERIMENT_DESIGN: 7,       // GATE
  CODEBASE_SEARCH: 8,
  CODE_DEVELOP: 9,            // ← CodeGeneration + SanityCheck
  EXPERIMENT_CYCLE: 10,       // ← ResourcePlanning + ExperimentRun + IterativeRefine
  // Phase 4: Inference
  RESULT_ANALYSIS: 11,
  RESEARCH_DECISION: 12,
  KNOWLEDGE_SUMMARY: 13,
  // Phase 5: Documentation
  PAPER_OUTLINE: 14,
  PAPER_WRITE: 15,            // ← PaperDraft + PaperRevision
  PEER_REVIEW: 16,
  QUALITY_GATE: 17,           // GATE
  PUBLISH: 18,                // ← KnowledgeArchive + ExportPublish + CitationVerify
  // Special
  DISCUSSION: 100,
} as const;

export type RCStage = (typeof RCStage)[keyof typeof RCStage];

/** Phase number (1-5) that a stage belongs to. */
export function phaseOf(stage: RCStage): number {
  if (stage >= 1 && stage <= 2) return 1;
  if (stage >= 3 && stage <= 6) return 2;
  if (stage >= 7 && stage <= 10) return 3;
  if (stage >= 11 && stage <= 13) return 4;
  if (stage >= 14 && stage <= 18) return 5;
  return 0; // Discussion
}

/** 1-based step index within the phase. */
function stepOf(stage: RCStage): number {
  const offsets: Record<number, number> = { 1: 0, 2: 2, 3: 6, 4: 10, 5: 13 };
  const p = phaseOf(stage);
  return p > 0 ? stage - offsets[p] : 0;
}

/** Phase.Step label (e.g. "3.3"). */
export function phaseStepLabel(stage: RCStage): string {
  const p = phaseOf(stage);
  return p > 0 ? `${p}.${stepOf(stage)}` : '';
}

export const PHASE_NAMES: Record<number, { en: string; zh: string }> = {
  1: { en: 'Strategy', zh: '策略' },
  2: { en: 'Exploration', zh: '探索' },
  3: { en: 'Execution', zh: '执行' },
  4: { en: 'Inference', zh: '推断' },
  5: { en: 'Documentation', zh: '文档' },
};

export interface StageMeta {
  id: RCStage;
  phase: number;
  step: number;
  name: string;
  key: string;
  outputs: string[];
}

// Artifact outputs aligned with backend agent_bridge.rs::stage_outputs()
export const STAGE_META: Record<RCStage, StageMeta> = {
  // Phase 1: Strategy
  1:  { id: 1,  phase: 1, step: 1, name: '课题初始化',       key: 'TOPIC_INIT',           outputs: ['goal.md', 'hardware_profile.json'] },
  2:  { id: 2,  phase: 1, step: 2, name: '问题分解',         key: 'PROBLEM_DECOMPOSE',     outputs: ['problem_tree.md', 'topic_evaluation.json'] },
  // Phase 2: Exploration
  3:  { id: 3,  phase: 2, step: 1, name: '文献检索',         key: 'LITERATURE_SEARCH',     outputs: ['search_plan.yaml', 'sources.json', 'queries.json', 'candidates.jsonl'] },
  4:  { id: 4,  phase: 2, step: 2, name: '文献筛选',         key: 'LITERATURE_SCREEN',     outputs: ['screened_papers.jsonl', 'exclusion_reasons.json'] },
  5:  { id: 5,  phase: 2, step: 3, name: '知识提取',         key: 'KNOWLEDGE_EXTRACT',     outputs: ['knowledge_cards.json', 'citation_map.json'] },
  6:  { id: 6,  phase: 2, step: 4, name: '综合与假设',       key: 'SYNTHESIS_HYPOTHESES',  outputs: ['synthesis_report.md', 'gap_analysis.json', 'hypotheses.md'] },
  // Phase 3: Execution
  7:  { id: 7,  phase: 3, step: 1, name: '实验设计',         key: 'EXPERIMENT_DESIGN',     outputs: ['exp_plan.yaml'] },
  8:  { id: 8,  phase: 3, step: 2, name: '代码库检索',       key: 'CODEBASE_SEARCH',       outputs: ['codebase_context.md'] },
  9:  { id: 9,  phase: 3, step: 3, name: '代码开发',         key: 'CODE_DEVELOP',          outputs: ['experiment/', 'experiment_spec.md', 'sanity_report.json'] },
  10: { id: 10, phase: 3, step: 4, name: '实验循环',         key: 'EXPERIMENT_CYCLE',      outputs: ['resource_plan.json', 'runs/', 'refinement_log.json', 'experiment_final/'] },
  // Phase 4: Inference
  11: { id: 11, phase: 4, step: 1, name: '结果分析',         key: 'RESULT_ANALYSIS',       outputs: ['analysis_report.md', 'experiment_summary.json'] },
  12: { id: 12, phase: 4, step: 2, name: '研究决策',         key: 'RESEARCH_DECISION',     outputs: ['decision_record.json'] },
  13: { id: 13, phase: 4, step: 3, name: '知识归纳',         key: 'KNOWLEDGE_SUMMARY',     outputs: ['knowledge_summary.md'] },
  // Phase 5: Documentation
  14: { id: 14, phase: 5, step: 1, name: '论文大纲',         key: 'PAPER_OUTLINE',         outputs: ['paper_outline.md'] },
  15: { id: 15, phase: 5, step: 2, name: '论文写作',         key: 'PAPER_WRITE',           outputs: ['paper_draft.md', 'paper_revised.md', 'revision_notes.md'] },
  16: { id: 16, phase: 5, step: 3, name: '同行评审',         key: 'PEER_REVIEW',           outputs: ['review_comments.md', 'critical_review.md'] },
  17: { id: 17, phase: 5, step: 4, name: '质量门控',         key: 'QUALITY_GATE',          outputs: ['quality_report.md', 'plot_validation.md', 'rendering_review.md'] },
  18: { id: 18, phase: 5, step: 5, name: '发布',             key: 'PUBLISH',               outputs: ['archive_manifest.json', 'paper_final.md', 'paper.tex'] },
  // Special
  100:{ id: 100,phase: 0, step: 0, name: '沟通讨论',         key: 'DISCUSSION',            outputs: ['discussion_transcript.md', 'consensus_synthesis.md'] },
};

// ===================== Phase Layer Definitions =====================

export const AgentLayer = {
  STRATEGY: 'strategy',
  EXPLORATION: 'exploration',
  EXECUTION: 'execution',
  INFERENCE: 'inference',
  DOCUMENTATION: 'documentation',
} as const;

export type AgentLayer = (typeof AgentLayer)[keyof typeof AgentLayer];

export interface LayerMeta {
  name: string;
  color: string;
  desc: string;
  phase: number;
  stages: RCStage[];
}

export const LAYER_META: Record<AgentLayer, LayerMeta> = {
  [AgentLayer.STRATEGY]: {
    name: 'Phase 1 · Strategy',
    color: '#f59e0b',
    desc: 'Topic initialization and problem decomposition',
    phase: 1,
    stages: [1, 2],
  },
  [AgentLayer.EXPLORATION]: {
    name: 'Phase 2 · Exploration',
    color: '#3b82f6',
    desc: 'Literature discovery, synthesis, and hypothesis generation',
    phase: 2,
    stages: [3, 4, 5, 6, 100],
  },
  [AgentLayer.EXECUTION]: {
    name: 'Phase 3 · Execution',
    color: '#10b981',
    desc: 'Experiment design, code development, and execution',
    phase: 3,
    stages: [7, 8, 9, 10],
  },
  [AgentLayer.INFERENCE]: {
    name: 'Phase 4 · Inference',
    color: '#ef4444',
    desc: 'Result analysis, research decision, and knowledge summary',
    phase: 4,
    stages: [11, 12, 13],
  },
  [AgentLayer.DOCUMENTATION]: {
    name: 'Phase 5 · Documentation',
    color: '#a855f7',
    desc: 'Paper writing, review, and publication',
    phase: 5,
    stages: [14, 15, 16, 17, 18],
  },
};

export const ALL_LAYERS: readonly AgentLayer[] = [
  AgentLayer.STRATEGY,
  AgentLayer.EXPLORATION,
  AgentLayer.EXECUTION,
  AgentLayer.INFERENCE,
  AgentLayer.DOCUMENTATION,
];

// ===================== Shared Data Repositories =====================

export const RepoId = {
  KNOWLEDGE: 'knowledge',
  EXP_DESIGN: 'exp_design',
  CODEBASE: 'codebase',
  RESULTS: 'results',
  INSIGHTS: 'insights',
  PAPERS: 'papers',
} as const;

export type RepoId = (typeof RepoId)[keyof typeof RepoId];

export interface RepoMeta {
  name: string;
  icon: string;
  desc: string;
  fromLayer: AgentLayer;
  toLayer: AgentLayer | null;
  artifacts: string[];
}

export const REPO_META: Record<RepoId, RepoMeta> = {
  [RepoId.KNOWLEDGE]: {
    name: 'Idea 仓库',
    icon: '💡',
    desc: '文献卡片、知识综合、研究假设',
    fromLayer: AgentLayer.EXPLORATION,
    toLayer: AgentLayer.EXECUTION,
    artifacts: ['goal.md', 'problem_tree.md', 'screened_papers.jsonl', 'knowledge_cards.json', 'synthesis_report.md', 'hypotheses.md'],
  },
  [RepoId.EXP_DESIGN]: {
    name: '实验设计仓库',
    icon: '🧪',
    desc: '实验方案、代码库检索',
    fromLayer: AgentLayer.EXECUTION,
    toLayer: AgentLayer.EXECUTION,
    artifacts: ['exp_plan.yaml', 'codebase_context.md'],
  },
  [RepoId.CODEBASE]: {
    name: '代码仓库',
    icon: '💻',
    desc: '实验代码、规格说明',
    fromLayer: AgentLayer.EXECUTION,
    toLayer: AgentLayer.INFERENCE,
    artifacts: ['experiment/', 'experiment_spec.md'],
  },
  [RepoId.RESULTS]: {
    name: '结果仓库',
    icon: '📊',
    desc: '实验结果、分析报告、决策',
    fromLayer: AgentLayer.INFERENCE,
    toLayer: null,
    artifacts: ['runs/', 'analysis_report.md', 'experiment_summary.json', 'decision_record.json'],
  },
  [RepoId.INSIGHTS]: {
    name: '知识库',
    icon: '🧠',
    desc: '跨项目研究结论、洞察、后续方向',
    fromLayer: AgentLayer.INFERENCE,
    toLayer: AgentLayer.EXPLORATION,
    artifacts: ['knowledge_summary.md'],
  },
  [RepoId.PAPERS]: {
    name: '论文仓库',
    icon: '📝',
    desc: '论文大纲、初稿、评审、修订稿',
    fromLayer: AgentLayer.DOCUMENTATION,
    toLayer: null,
    artifacts: ['paper_outline.md', 'paper_draft.md', 'review_comments.md', 'paper_revised.md', 'paper_final.md'],
  },
};

export const ALL_REPOS: readonly RepoId[] = [
  RepoId.KNOWLEDGE,
  RepoId.INSIGHTS,
  RepoId.PAPERS,
];

// ===================== Backend Layer Translation =====================
// Backend uses: idea, experiment, coding, execution, writing
// Frontend uses: strategy, exploration, execution, inference, documentation
// This mapping normalizes backend layer strings to frontend AgentLayer values.

const BACKEND_LAYER_MAP: Record<string, AgentLayer> = {
  'idea': AgentLayer.EXPLORATION,        // Backend merges Strategy+Exploration into "idea"
  'experiment': AgentLayer.EXECUTION,    // Backend "experiment" = ExperimentDesign gate
  'coding': AgentLayer.EXECUTION,        // Backend "coding" = CodebaseSearch + CodeDevelop + ExperimentCycle
  'execution': AgentLayer.INFERENCE,     // Backend "execution" = ResultAnalysis + ResearchDecision + KnowledgeSummary
  'writing': AgentLayer.DOCUMENTATION,   // Backend "writing" = PaperOutline..Publish
  // Frontend names pass through
  'strategy': AgentLayer.STRATEGY,
  'exploration': AgentLayer.EXPLORATION,
  'processing': AgentLayer.EXECUTION,    // Legacy compat
  'inference': AgentLayer.INFERENCE,
  'documentation': AgentLayer.DOCUMENTATION,
};

/** Normalize a backend layer string to a frontend AgentLayer. */
export function normalizeLayer(backendLayer: string): AgentLayer {
  return BACKEND_LAYER_MAP[backendLayer] ?? AgentLayer.EXPLORATION;
}

// ===================== Agent & Runtime Types =====================

export type AgentStatus = 'idle' | 'working' | 'error' | 'done' | 'waiting_discussion' | 'discussing' | 'waiting_human_review';
export type StageStatus = 'pending' | 'running' | 'completed' | 'failed' | 'skipped' | 'waiting' | 'discussing';

export interface MolAgent {
  id: string;
  name: string;
  layer: AgentLayer;
  status: AgentStatus;
  currentStage: RCStage | null;
  currentTask: string;
  stageProgress: Record<number, StageStatus>;
  runId: string;
  projectId?: string;
  roleTag?: string;
}

export interface Artifact {
  id: string;
  repoId: RepoId;
  projectId: string;
  filename: string;
  producedBy: string;
  timestamp: number;
  size: string;
  status: 'fresh' | 'stale' | 'error';
  content?: string;
  stage?: number;
}

// Artifact labels aligned with backend stage_outputs()
export const ARTIFACT_LABELS: Record<string, { icon: string; zh: string; en: string }> = {
  // Phase 1
  'goal.md':                  { icon: '🎯', zh: '研究目标', en: 'Research Goal' },
  'hardware_profile.json':    { icon: '🖥️', zh: '硬件检测', en: 'Hardware Profile' },
  'problem_tree.md':          { icon: '🌳', zh: '问题分解树', en: 'Problem Tree' },
  'topic_evaluation.json':    { icon: '📋', zh: '课题评估', en: 'Topic Evaluation' },
  // Phase 2
  'search_plan.yaml':         { icon: '🔍', zh: '检索策略', en: 'Search Plan' },
  'sources.json':             { icon: '📡', zh: '数据源', en: 'Data Sources' },
  'queries.json':             { icon: '🔎', zh: '检索查询', en: 'Search Queries' },
  'candidates.jsonl':         { icon: '📚', zh: '候选文献', en: 'Candidate Papers' },
  'candidates.md':            { icon: '📚', zh: '候选文献', en: 'Candidate Papers' },
  'screened_papers.jsonl':    { icon: '✅', zh: '筛选文献', en: 'Screened Papers' },
  'exclusion_reasons.json':   { icon: '❌', zh: '排除原因', en: 'Exclusion Reasons' },
  'knowledge_cards.json':     { icon: '🗂️', zh: '知识卡片', en: 'Knowledge Cards' },
  'knowledge_cards.md':       { icon: '🗂️', zh: '知识卡片', en: 'Knowledge Cards' },
  'citation_map.json':        { icon: '🔗', zh: '引用图谱', en: 'Citation Map' },
  'synthesis_report.md':      { icon: '🧬', zh: '知识综合报告', en: 'Synthesis Report' },
  'gap_analysis.json':        { icon: '🔍', zh: '研究空白分析', en: 'Gap Analysis' },
  'hypotheses.md':            { icon: '💡', zh: '研究假设', en: 'Research Hypotheses' },
  // Phase 3
  'exp_plan.yaml':            { icon: '🧪', zh: '实验方案', en: 'Experiment Plan' },
  'exp_plan.md':              { icon: '🧪', zh: '实验方案', en: 'Experiment Plan' },
  'codebase_context.md':      { icon: '🔗', zh: '代码库上下文', en: 'Codebase Context' },
  'codebase_context.json':    { icon: '🔗', zh: '代码库上下文', en: 'Codebase Context' },
  'experiment/':              { icon: '💻', zh: '实验代码', en: 'Experiment Code' },
  'experiment_spec.md':       { icon: '📋', zh: '实验规格', en: 'Experiment Spec' },
  'sanity_report.json':       { icon: '🔬', zh: '冒烟测试报告', en: 'Sanity Report' },
  'sanity_report.md':         { icon: '🔬', zh: '冒烟测试报告', en: 'Sanity Report' },
  'resource_plan.json':       { icon: '📅', zh: '资源计划', en: 'Resource Plan' },
  'runs/':                    { icon: '▶️', zh: '运行结果', en: 'Run Results' },
  'refinement_log.json':      { icon: '🔄', zh: '迭代日志', en: 'Refinement Log' },
  'experiment_final/':        { icon: '🏁', zh: '最终实验', en: 'Final Experiment' },
  'run_report.md':            { icon: '📊', zh: '运行报告', en: 'Run Report' },
  'experiment_code.md':       { icon: '💻', zh: '实验代码', en: 'Experiment Code' },
  // Phase 4
  'analysis_report.md':       { icon: '📊', zh: '结果分析', en: 'Result Analysis' },
  'experiment_summary.json':  { icon: '📈', zh: '实验摘要', en: 'Experiment Summary' },
  'decision_record.json':     { icon: '🧭', zh: '研究决策', en: 'Research Decision' },
  'decision_record.md':       { icon: '🧭', zh: '研究决策', en: 'Research Decision' },
  'knowledge_summary.md':     { icon: '🧠', zh: '知识归纳', en: 'Knowledge Summary' },
  // Phase 5
  'paper_outline.md':         { icon: '📝', zh: '论文大纲', en: 'Paper Outline' },
  'paper_draft.md':           { icon: '📄', zh: '论文初稿', en: 'Paper Draft' },
  'paper_revised.md':         { icon: '✍️', zh: '论文修订稿', en: 'Paper Revised' },
  'revision_notes.md':        { icon: '📝', zh: '修订说明', en: 'Revision Notes' },
  'review_comments.md':       { icon: '👁️', zh: '评审意见', en: 'Review Comments' },
  'critical_review.md':       { icon: '🔍', zh: '批判性评审', en: 'Critical Review' },
  'quality_report.md':        { icon: '✅', zh: '质量报告', en: 'Quality Report' },
  'plot_validation.md':       { icon: '📊', zh: '图表验证', en: 'Plot Validation' },
  'rendering_review.md':      { icon: '🖨️', zh: '排版审查', en: 'Rendering Review' },
  'archive_manifest.json':    { icon: '📦', zh: '归档清单', en: 'Archive Manifest' },
  'paper_final.md':           { icon: '📄', zh: '最终论文', en: 'Final Paper' },
  'paper.tex':                { icon: '📄', zh: 'LaTeX 源码', en: 'LaTeX Source' },
  'paper.pdf':                { icon: '📄', zh: '论文 PDF', en: 'Paper PDF' },
  // Special
  'discussion_transcript.md':    { icon: '💬', zh: '讨论记录', en: 'Discussion Transcript' },
  'consensus_synthesis.md':      { icon: '🤝', zh: '共识综合', en: 'Consensus Synthesis' },
  'pre_discussion_syntheses.md': { icon: '📋', zh: '讨论前综合', en: 'Pre-discussion Syntheses' },
};

export interface LogEntry {
  id: string;
  agentId: string;
  agentName: string;
  layer: AgentLayer;
  stage: RCStage | null;
  message: string;
  level: 'info' | 'success' | 'warning' | 'error';
  timestamp: number;
}

// ===================== Resource Monitoring =====================

export interface GpuInfo {
  id: number;
  name: string;
  utilization: number;
  memUsed: number;
  memTotal: number;
  temperature: number;
}

export interface ResourceStats {
  cpuPercent: number;
  memUsed: number;
  memTotal: number;
  gpus: GpuInfo[];
  acceleratorLabel?: string;
  timestamp: number;
}

// ===================== Task Queues =====================

export interface QueueSummary {
  name: string;
  total: number;
  pending: number;
  assigned: number;
  completed: number;
}

export type QueueMap = Record<string, QueueSummary>;

// ===================== Human Feedback =====================

export interface ChatMessage {
  id: string;
  role: 'user' | 'system';
  content: string;
  targetLayer?: string;
  timestamp: number;
}

// ===================== Project Management =====================

export type ProjectStatus = 'running' | 'queued' | 'completed' | 'interrupted' | 'new';

export interface ProjectInfo {
  projectId: string;
  status: ProjectStatus;
  lastCompletedStage: number;
  lastCompletedName: string;
  firstStage: number;
  totalStages: number;
  timestamp: string;
  topic: string;
  configPath: string;
  intervention?: string;
}

// ===================== WebSocket Protocol =====================

export type WSMessage =
  | { type: 'agent_update'; payload: MolAgent }
  | { type: 'artifact_produced'; payload: Artifact }
  | { type: 'log'; payload: LogEntry }
  | { type: 'stage_update'; payload: { agentId: string; stage: RCStage; status: StageStatus } }
  | { type: 'resource_stats'; payload: ResourceStats }
  | { type: 'queue_update'; payload: QueueMap }
  | { type: 'chat_message'; payload: ChatMessage }
  | { type: 'project_list'; payload: ProjectInfo[] }
  | { type: 'download_url'; payload: { projectId: string; filename: string; url: string } }
  | { type: 'system'; payload: { message: string } };

// ===================== App State =====================

export interface AppState {
  agents: MolAgent[];
  artifacts: Artifact[];
  logs: LogEntry[];
  queues: QueueMap;
  chatMessages: ChatMessage[];
  projects: ProjectInfo[];
  selectedProjectId: string | null;
  resources: ResourceStats | null;
  resConnected: boolean;
  connected: boolean;
  mockMode: boolean;
}
