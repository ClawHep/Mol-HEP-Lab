import { AgentLayer, STAGE_META, LAYER_META, RepoId } from './types';
import type { MolAgent, WSMessage, Artifact, RCStage as RCStageT } from './types';

let counter = 0;
const uid = () => `m-${++counter}-${Date.now()}`;
const pick = <T>(arr: T[]): T => arr[Math.floor(Math.random() * arr.length)];

function makeAgent(id: string, name: string, layer: AgentLayer, runId: string): MolAgent {
  const stages = LAYER_META[layer].stages;
  const progress: Record<number, 'pending'> = {};
  for (const s of stages) progress[s] = 'pending';
  return { id, name, layer, status: 'idle', currentStage: null, currentTask: '', stageProgress: progress, runId };
}

export const INITIAL_AGENTS: MolAgent[] = [
  makeAgent('P1-01', 'Lead Analyst',           AgentLayer.STRATEGY,      'run-001'),
  makeAgent('P2-01', 'Theory Scout',           AgentLayer.EXPLORATION,   'run-001'),
  makeAgent('P2-02', 'Data Explorer',          AgentLayer.EXPLORATION,   'run-002'),
  makeAgent('P3-01', 'Signal Lead',            AgentLayer.PROCESSING,    'run-001'),
  makeAgent('P3-02', 'Background Estimator',   AgentLayer.PROCESSING,    'run-002'),
  makeAgent('P3-03', 'ML Specialist',          AgentLayer.PROCESSING,    'run-003'),
  makeAgent('P4-01', 'Cross Checker',          AgentLayer.INFERENCE,     'run-001'),
  makeAgent('P4-02', 'Systematics Fitter',     AgentLayer.INFERENCE,     'run-002'),
  makeAgent('P5-01', 'Physics Reviewer',       AgentLayer.DOCUMENTATION, 'run-001'),
  makeAgent('P5-02', 'Note Writer',            AgentLayer.DOCUMENTATION, 'run-002'),
  makeAgent('P5-03', 'Plot Validator',         AgentLayer.DOCUMENTATION, 'run-003'),
];

const TASK_DETAILS: Record<number, string[]> = {
  1:  ['解析研究课题，生成 SMART 目标...', '检测硬件环境 (GPU/CPU)...'],
  2:  ['将课题分解为子问题树...', '确定优先研究方向...'],
  3:  ['规划检索策略，确定数据源...', '生成关键词查询组合...'],
  4:  ['调用 OpenAlex API 检索论文...', '从 Semantic Scholar 收集引用...', '扫描 arXiv 最新预印本...'],
  5:  ['基于相关性和质量筛选文献...', '评估 shortlist 覆盖度... [GATE]'],
  6:  ['从筛选论文提取知识卡片...', '结构化关键发现...'],
  7:  ['聚类研究主题，识别研究空白...', '综合多论文结论...'],
  8:  ['生成可证伪假设...', '多 agent 辩论评估假设...'],
  9:  ['设计实验方案 (YAML)...', '确定 baseline 和评估指标... [GATE]', '规划消融实验矩阵...'],
  11: ['估算 GPU 需求和运行时间...', '生成调度计划 schedule.json...'],
  10: ['生成实验核心代码...', 'AST 验证代码正确性...', '适配硬件环境...', '编写评估脚本...'],
  12: ['在沙箱中执行实验代码...', '监控 NaN/Inf 检查...', '提交训练任务至集群...'],
  13: ['编辑-运行-评估循环...', 'LLM 修复失败用例...', '收敛性评估...'],
  14: ['分析实验指标...', '生成可视化图表...', '多 agent 结果评审...'],
  15: ['做出研究决策: PROCEED / PIVOT / REFINE...', '记录决策历史...'],
};

const LOG_TEMPLATES: Record<string, Array<{ msg: string; level: 'info' | 'success' | 'warning' | 'error' }>> = {
  [AgentLayer.STRATEGY]: [
    { msg: 'Parsing research topic, generating SMART objectives...', level: 'info' },
    { msg: 'Problem decomposed into 4 sub-questions', level: 'success' },
    { msg: 'HEP domain detected: routing to HEP pipeline', level: 'info' },
  ],
  [AgentLayer.EXPLORATION]: [
    { msg: 'Found 12 relevant papers via INSPIRE-HEP', level: 'info' },
    { msg: 'Literature screen passed: 12 → 8 shortlisted', level: 'success' },
    { msg: 'Knowledge cards extracted: 8 cards', level: 'success' },
    { msg: 'INSPIRE API rate limit, retrying...', level: 'warning' },
    { msg: 'Synthesis identified 3 unexplored signal regions', level: 'info' },
    { msg: 'Generated 2 falsifiable hypotheses', level: 'success' },
    { msg: 'arXiv connection timeout', level: 'error' },
  ],
  [AgentLayer.PROCESSING]: [
    { msg: 'Experiment design: 3 signal regions + 5 control regions', level: 'success' },
    { msg: 'Generated pyhf workspace with 12 systematic sources', level: 'info' },
    { msg: 'Sanity check: NaN detected in BDT output, fixing...', level: 'warning' },
    { msg: 'Experiment design GATE approved', level: 'success' },
    { msg: 'uproot I/O: processing 2.4M events from nanoAOD', level: 'info' },
    { msg: 'Background model fit converged (chi2/ndf = 1.12)', level: 'success' },
    { msg: 'fastjet clustering: anti-kT R=0.4 completed', level: 'info' },
  ],
  [AgentLayer.INFERENCE]: [
    { msg: 'CLs upper limit: mu < 0.85 at 95% CL', level: 'success' },
    { msg: 'Observed significance: 2.3 sigma (expected: 1.8)', level: 'info' },
    { msg: 'Decision: PROCEED → results meet publication threshold', level: 'success' },
    { msg: 'Decision: PIVOT → insufficient sensitivity, revise selection', level: 'warning' },
    { msg: 'Systematic uncertainty breakdown generated', level: 'info' },
  ],
  [AgentLayer.DOCUMENTATION]: [
    { msg: 'Paper outline generated with 8 sections', level: 'success' },
    { msg: 'mplhep figures rendered: 12 plots', level: 'info' },
    { msg: 'Peer review: 2 A-items, 3 B-items, 1 C-item', level: 'warning' },
    { msg: 'Quality gate PASSED', level: 'success' },
    { msg: 'Citation verification: all 34 refs valid', level: 'success' },
    { msg: 'LaTeX package exported', level: 'info' },
  ],
};

function stageToRepo(stage: RCStageT): RepoId | null {
  if (stage <= 8) return RepoId.KNOWLEDGE;
  if (stage === 9 || stage === 11) return RepoId.EXP_DESIGN;
  if (stage === 10) return RepoId.CODEBASE;
  if (stage >= 12) return RepoId.RESULTS;
  return null;
}

export function createMockMessageGenerator(onMessage: (msg: WSMessage) => void): () => void {
  const intervals: number[] = [];
  const agents = [...INITIAL_AGENTS];
  const agentMap = new Map(agents.map((a) => [a.id, { ...a }]));

  const emitAgentActivity = () => {
    const agent = pick(agents);
    const state = agentMap.get(agent.id)!;
    const layerStages = LAYER_META[agent.layer].stages;
    const roll = Math.random();

    if (roll < 0.5 && state.status !== 'working') {
      const stage = pick(layerStages);
      state.status = 'working';
      state.currentStage = stage;
      state.currentTask = pick(TASK_DETAILS[stage] || ['处理中...']);
      state.stageProgress[stage] = 'running';
    } else if (roll < 0.75 && state.currentStage) {
      state.stageProgress[state.currentStage] = 'completed';
      state.status = 'done';
      state.currentTask = '';

      const repo = stageToRepo(state.currentStage);
      if (repo) {
        const outputs = STAGE_META[state.currentStage].outputs;
        const file = pick(outputs);
        const artifact: Artifact = {
          id: uid(),
          repoId: repo,
          projectId: agent.runId,
          filename: file,
          producedBy: agent.name,
          timestamp: Date.now(),
          size: `${(Math.random() * 100 + 1).toFixed(1)} KB`,
          status: 'fresh',
        };
        onMessage({ type: 'artifact_produced', payload: artifact });
      }

      const nextStage = state.currentStage;
      state.currentStage = null;
      onMessage({
        type: 'stage_update',
        payload: { agentId: agent.id, stage: nextStage, status: 'completed' },
      });
    } else if (roll < 0.85 && state.currentStage) {
      state.stageProgress[state.currentStage] = 'failed';
      state.status = 'error';
      state.currentTask = '执行失败，等待重试...';
    } else {
      state.status = 'idle';
      state.currentStage = null;
      state.currentTask = '';
    }

    agentMap.set(agent.id, { ...state });
    onMessage({ type: 'agent_update', payload: { ...state } });

    if (state.status !== 'idle') {
      const templates = LOG_TEMPLATES[agent.layer];
      const tmpl = state.status === 'error'
        ? templates.find((t) => t.level === 'error') || pick(templates)
        : pick(templates);
      onMessage({
        type: 'log',
        payload: {
          id: uid(),
          agentId: agent.id,
          agentName: agent.name,
          layer: agent.layer,
          stage: state.currentStage,
          message: tmpl.msg,
          level: tmpl.level,
          timestamp: Date.now(),
        },
      });
    }
  };

  intervals.push(
    window.setInterval(emitAgentActivity, 1500 + Math.random() * 2000),
  );

  setTimeout(emitAgentActivity, 300);
  setTimeout(emitAgentActivity, 800);

  return () => intervals.forEach(clearInterval);
}
