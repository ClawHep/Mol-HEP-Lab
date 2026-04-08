//! Agent Bridge — WebSocket service for MolAgent orchestration.
//!
//! Ports the Python `agent_bridge.py` to Rust using axum + DashMap.

use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    response::IntoResponse,
    routing::get,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Child,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use std::sync::RwLock;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const DISCUSSION_STAGE: u32 = 100;

/// Maps stage number → layer name.
pub fn stage_to_layer(stage: u32) -> Option<&'static str> {
    match stage {
        1..=8 => Some("idea"),
        9 => Some("experiment"),
        10..=13 => Some("coding"),
        14..=18 => Some("execution"),
        19..=22 => Some("writing"),
        _ => None,
    }
}

/// Stage numbers per layer.
pub fn layer_stages(layer: &str) -> &'static [u32] {
    match layer {
        "idea" => &[1, 2, 3, 4, 5, 6, 7, 100, 8],
        "experiment" => &[9],
        "coding" => &[10, 11, 12, 13],
        "execution" => &[14, 15, 16, 17, 18],
        "writing" => &[19, 20, 21, 22],
        _ => &[],
    }
}

/// (first_stage, last_stage) inclusive for a layer.
pub fn layer_range(layer: &str) -> (u32, u32) {
    match layer {
        "idea" => (1, 8),
        "experiment" => (9, 9),
        "coding" => (10, 13),
        "execution" => (14, 18),
        "writing" => (19, 22),
        _ => (1, 22),
    }
}

pub fn stage_name(stage: u32) -> &'static str {
    match stage {
        1 => "TOPIC_INIT",
        2 => "PROBLEM_DECOMPOSE",
        3 => "SEARCH_STRATEGY",
        4 => "LITERATURE_COLLECT",
        5 => "LITERATURE_SCREEN",
        6 => "KNOWLEDGE_EXTRACT",
        7 => "SYNTHESIS",
        8 => "HYPOTHESIS_GEN",
        9 => "EXPERIMENT_DESIGN",
        10 => "CODEBASE_SEARCH",
        11 => "CODE_GENERATION",
        12 => "SANITY_CHECK",
        13 => "RESOURCE_PLANNING",
        14 => "EXPERIMENT_RUN",
        15 => "ITERATIVE_REFINE",
        16 => "RESULT_ANALYSIS",
        17 => "RESEARCH_DECISION",
        18 => "KNOWLEDGE_SUMMARY",
        19 => "PAPER_OUTLINE",
        20 => "PAPER_DRAFT",
        21 => "PEER_REVIEW",
        22 => "PAPER_REVISION",
        23 => "QUALITY_GATE",
        24 => "KNOWLEDGE_ARCHIVE",
        25 => "EXPORT_PUBLISH",
        26 => "CITATION_VERIFY",
        100 => "DISCUSSION",
        _ => "UNKNOWN",
    }
}

pub fn stage_outputs(stage: u32) -> &'static [&'static str] {
    match stage {
        1 => &["goal.md", "hardware_profile.json"],
        2 => &["problem_tree.md"],
        3 => &["search_plan.yaml", "sources.json", "queries.json"],
        4 => &["candidates.jsonl"],
        5 => &["shortlist.jsonl"],
        6 => &["cards/"],
        7 => &["synthesis.md"],
        8 => &["hypotheses.md"],
        9 => &["exp_plan.yaml"],
        10 => &["codebase_candidates.json"],
        11 => &["experiment/", "experiment_spec.md"],
        12 => &["sanity_report.json"],
        13 => &["schedule.json"],
        14 => &["runs/"],
        15 => &["refinement_log.json", "experiment_final/"],
        16 => &["analysis.md", "experiment_summary.json", "charts/"],
        17 => &["decision.md"],
        18 => &["knowledge_entry.json"],
        19 => &["outline.md"],
        20 => &["paper_draft.md"],
        21 => &["reviews.md"],
        22 => &["paper_revised.md", "latex_package.zip"],
        _ => &[],
    }
}

/// Artifacts to display on the DataShelf.
pub fn is_display_artifact(name: &str) -> bool {
    matches!(
        name,
        "hypotheses.md"
            | "knowledge_entry.json"
            | "paper_revised.md"
            | "latex_package.zip"
            | "analysis.md"
            | "charts/"
            | "decision.md"
            | "exp_plan.yaml"
            | "experiment/"
            | "experiment_spec.md"
            | "experiment_final/"
    )
}

pub fn repo_for_stage(stage: u32) -> &'static str {
    match stage {
        1..=8 => "knowledge",
        9 => "exp_design",
        10..=13 => "codebase",
        14..=17 => "results",
        18 => "insights",
        19..=22 => "papers",
        _ => "knowledge",
    }
}

/// Queue name that feeds into a target layer.
pub fn layer_input_queue(layer: &str) -> Option<&'static str> {
    match layer {
        "idea" => Some("execution_feedback"),
        "experiment" => Some("idea_to_experiment"),
        "coding" => Some("experiment_to_coding"),
        "execution" => Some("coding_to_execution"),
        "writing" => Some("execution_to_writing"),
        _ => None,
    }
}

/// Queue name that a completing layer feeds into.
pub fn layer_output_queue(layer: &str) -> Option<&'static str> {
    match layer {
        "idea" => Some("idea_to_experiment"),
        "experiment" => Some("experiment_to_coding"),
        "coding" => Some("coding_to_execution"),
        "execution" => Some("execution_to_writing"),
        "writing" => Some("execution_feedback"),
        _ => None,
    }
}

pub fn queue_layers(queue: &str) -> Option<(&'static str, &'static str)> {
    match queue {
        "idea_to_experiment" => Some(("idea", "experiment")),
        "experiment_to_coding" => Some(("experiment", "coding")),
        "coding_to_execution" => Some(("coding", "execution")),
        "execution_to_writing" => Some(("execution", "writing")),
        "execution_feedback" => Some(("execution", "idea")),
        _ => None,
    }
}

pub fn all_queue_names() -> &'static [&'static str] {
    &[
        "init_to_idea",
        "idea_to_experiment",
        "experiment_to_coding",
        "coding_to_execution",
        "execution_to_writing",
        "execution_feedback",
    ]
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn uid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_json(path: &Path, data: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(data).unwrap_or_default();
    std::fs::write(path, text)
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A unit of work assigned to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub run_dir: String,
    pub config_path: String,
    pub source_layer: String,
    pub target_layer: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub assigned_at: i64,
    #[serde(default)]
    pub completed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Pending,
    Assigned,
    Completed,
    Failed,
}

/// File-backed FIFO task queue.
#[derive(Debug)]
pub struct TaskQueue {
    pub name: String,
    pub path: PathBuf,
    pub tasks: Vec<Task>,
}

impl TaskQueue {
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self { name: name.into(), path, tasks: Vec::new() }
    }

    pub fn load(&mut self) {
        if let Some(val) = read_json(&self.path) {
            if let Ok(tasks) = serde_json::from_value::<Vec<Task>>(val) {
                self.tasks = tasks;
            }
        }
    }

    pub fn save(&self) {
        if let Ok(val) = serde_json::to_value(&self.tasks) {
            let _ = write_json(&self.path, &val);
        }
    }

    pub fn push(&mut self, task: Task) {
        self.tasks.push(task);
        self.save();
    }

    pub fn peek_pending(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| t.status == TaskStatus::Pending)
    }

    pub fn assign(&mut self, task_id: &str, agent_id: &str) -> Option<Task> {
        let idx = self
            .tasks
            .iter()
            .position(|t| t.id == task_id && t.status == TaskStatus::Pending)?;
        self.tasks[idx].status = TaskStatus::Assigned;
        self.tasks[idx].assigned_to = Some(agent_id.to_string());
        self.tasks[idx].assigned_at = now_ms();
        let task = self.tasks[idx].clone();
        self.save();
        Some(task)
    }

    pub fn complete(&mut self, task_id: &str) {
        for t in &mut self.tasks {
            if t.id == task_id {
                t.status = TaskStatus::Completed;
                t.completed_at = now_ms();
                self.save();
                return;
            }
        }
    }

    pub fn fail(&mut self, task_id: &str) {
        for t in &mut self.tasks {
            if t.id == task_id {
                t.status = TaskStatus::Failed;
                self.save();
                return;
            }
        }
    }

    pub fn pending_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Pending).count()
    }

    pub fn summary(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "total": self.tasks.len(),
            "pending": self.tasks.iter().filter(|t| t.status == TaskStatus::Pending).count(),
            "assigned": self.tasks.iter().filter(|t| t.status == TaskStatus::Assigned).count(),
            "completed": self.tasks.iter().filter(|t| t.status == TaskStatus::Completed).count(),
        })
    }
}

/// Stored when an agent is waiting for human approval at a layer boundary.
#[derive(Debug, Clone)]
pub struct PendingTransition {
    pub output_queue: String,
    pub project_id: String,
    pub run_dir: String,
    pub config_path: String,
    pub topic: String,
    pub source_layer: String,
}

/// Tracks a group of L1 agents in a discussion.
#[derive(Debug)]
pub struct DiscussionGroup {
    pub project_id: String,
    pub topic: String,
    pub config_path: String,
    pub agent_ids: Vec<String>,
    pub run_dirs: HashMap<String, String>,
    pub completed_s7: HashSet<String>,
    pub completed_s8: HashSet<String>,
    pub best_agent_id: String,
    pub status: DiscussionStatus,
    pub discussion_process: Option<Child>,
    pub discussion_output_dir: String,
    pub is_cross_project: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscussionStatus {
    Gathering,
    Waiting,
    Discussing,
    Done,
}

impl DiscussionGroup {
    pub fn all_ready(&self) -> bool {
        self.completed_s7.len() >= self.agent_ids.len() && self.agent_ids.len() >= 2
    }

    pub fn all_s8_done(&self) -> bool {
        self.completed_s8.len() >= self.agent_ids.len() && self.agent_ids.len() >= 2
    }

    pub fn synthesis_dirs(&self) -> Vec<String> {
        self.agent_ids
            .iter()
            .filter_map(|aid| {
                let rd = self.run_dirs.get(aid)?;
                Some(format!("{}/stage-07", rd))
            })
            .collect()
    }
}

/// GPU allocation manager.
#[derive(Debug)]
pub struct GpuAllocator {
    pub total_gpus: u32,
    pub gpus_per_project: u32,
    pub assignments: HashMap<String, Vec<u32>>,
    occupied: HashSet<u32>,
}

impl GpuAllocator {
    pub fn new(total_gpus: u32, gpus_per_project: u32) -> Self {
        Self {
            total_gpus,
            gpus_per_project,
            assignments: HashMap::new(),
            occupied: HashSet::new(),
        }
    }

    pub fn available_count(&self) -> u32 {
        self.total_gpus.saturating_sub(self.occupied.len() as u32)
    }

    pub fn can_allocate(&self) -> bool {
        self.available_count() >= self.gpus_per_project
    }

    pub fn allocate(&mut self, project_id: &str) -> Option<Vec<u32>> {
        if let Some(existing) = self.assignments.get(project_id) {
            return Some(existing.clone());
        }
        if !self.can_allocate() {
            return None;
        }
        let free: Vec<u32> = (0..self.total_gpus)
            .filter(|i| !self.occupied.contains(i))
            .take(self.gpus_per_project as usize)
            .collect();
        self.occupied.extend(free.iter().copied());
        self.assignments.insert(project_id.to_string(), free.clone());
        Some(free)
    }

    pub fn release(&mut self, project_id: &str) -> Vec<u32> {
        let gpus = self.assignments.remove(project_id).unwrap_or_default();
        for g in &gpus {
            self.occupied.remove(g);
        }
        gpus
    }

    pub fn summary(&self) -> Value {
        serde_json::json!({
            "total": self.total_gpus,
            "perProject": self.gpus_per_project,
            "free": self.available_count(),
            "assignments": self.assignments,
        })
    }
}

/// A MolAgent process.
#[derive(Debug)]
pub struct MolAgent {
    pub id: String,
    pub name: String,
    pub base_name: String,
    pub layer: String,
    pub run_id: String,
    pub run_dir: String,
    pub config_path: String,
    pub project_id: String,
    pub status: AgentStatus,
    pub current_stage: Option<u32>,
    pub current_task: String,
    pub assigned_task_id: Option<String>,
    pub stage_progress: HashMap<u32, String>,
    pub role_tag: String,
    pub process: Option<Child>,
    pub prev_heartbeat: Value,
    pub prev_checkpoint: Value,
    pub known_artifacts: HashSet<String>,
    pub topic: String,
    // flags
    pub is_idea_factory: bool,
    pub is_idea_factory_s7_only: bool,
    pub is_discussion_s8: bool,
    pub idea_factory_batch_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    #[default]
    Idle,
    Working,
    Error,
    Done,
    WaitingDiscussion,
    Discussing,
    /// Agent paused at a layer boundary, waiting for human review/approval.
    WaitingHumanReview,
}

impl MolAgent {
    pub fn new(name: impl Into<String>, layer: impl Into<String>) -> Self {
        let name = name.into();
        let layer = layer.into();
        let stage_progress: HashMap<u32, String> = layer_stages(&layer)
            .iter()
            .map(|&s| (s, "pending".to_string()))
            .collect();
        Self {
            id: format!("L-{}", uid()),
            base_name: name.clone(),
            name,
            layer,
            run_id: String::new(),
            run_dir: String::new(),
            config_path: String::new(),
            project_id: String::new(),
            status: AgentStatus::Idle,
            current_stage: None,
            current_task: String::new(),
            assigned_task_id: None,
            stage_progress,
            role_tag: String::new(),
            process: None,
            prev_heartbeat: Value::Null,
            prev_checkpoint: Value::Null,
            known_artifacts: HashSet::new(),
            topic: String::new(),
            is_idea_factory: false,
            is_idea_factory_s7_only: false,
            is_discussion_s8: false,
            idea_factory_batch_id: None,
        }
    }

    pub fn to_frontend(&self) -> Value {
        let progress: HashMap<String, String> = self
            .stage_progress
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "layer": self.layer,
            "runId": self.run_id,
            "status": format!("{:?}", self.status).to_lowercase()
                       .replace("waitingdiscussion", "waiting_discussion")
                       .replace("waitinghumanreview", "waiting_human_review"),
            "currentStage": self.current_stage,
            "currentTask": self.current_task,
            "stageProgress": progress,
            "projectId": self.project_id,
            "roleTag": self.role_tag,
        })
    }

    pub fn reset_idle(&mut self) {
        self.assigned_task_id = None;
        self.project_id = String::new();
        self.status = AgentStatus::Idle;
        self.current_task = "Waiting for task...".to_string();
        self.run_id = String::new();
        self.run_dir = String::new();
        self.config_path = String::new();
        self.current_stage = None;
        self.stage_progress = HashMap::new();
        self.role_tag = String::new();
        self.process = None;
        self.name = self.base_name.clone();
        self.topic = String::new();
        self.is_idea_factory = false;
        self.is_idea_factory_s7_only = false;
        self.is_discussion_s8 = false;
        self.idea_factory_batch_id = None;
    }
}

// ---------------------------------------------------------------------------
// BridgeState
// ---------------------------------------------------------------------------

/// Shared state for the agent bridge. Uses DashMap for lock-free concurrent
/// read access to agents and queues from the WebSocket poll loop.
pub struct BridgeState {
    /// All registered agents keyed by agent id.
    pub agents: DashMap<String, MolAgent>,
    /// Task queues keyed by queue name.
    pub queues: DashMap<String, TaskQueue>,
    /// Connected WebSocket clients for broadcast. Sender half of broadcast channel.
    pub broadcast_tx: broadcast::Sender<String>,
    /// GPU allocation state (mutex-protected for mutation).
    pub gpu_allocator: std::sync::Mutex<GpuAllocator>,
    /// Discussion groups keyed by group/project id.
    pub discussion_groups: DashMap<String, DiscussionGroup>,
    /// Agents waiting for a discussion peer keyed by agent id.
    pub discussion_waiting: DashMap<String, String>,
    /// Fail counters per project id.
    pub fail_counts: DashMap<String, u32>,
    /// Lab mode: project_id → expected agent count for discussion batches.
    pub lab_batches: DashMap<String, usize>,
    /// Agents waiting for human approval: agent_id → pending transition info.
    pub pending_transitions: DashMap<String, PendingTransition>,
    // Configuration
    pub python_path: String,
    pub agent_package_dir: String,
    pub runs_base_dir: String,
    pub auto_loop: bool,
    pub discussion_mode: RwLock<bool>,
    pub discussion_rounds: RwLock<u32>,
    pub discussion_models: RwLock<Vec<String>>,
    pub idea_factory_topic: RwLock<String>,
    pub idea_factory_config: RwLock<String>,
    pub idea_factory_remaining: RwLock<i64>,
    pub idea_factory_produced: AtomicU64,
    pub poll_counter: AtomicU64,
}

impl BridgeState {
    pub fn new(
        python_path: String,
        agent_package_dir: String,
        runs_base_dir: String,
        total_gpus: u32,
        gpus_per_project: u32,
        auto_loop: bool,
        discussion_mode: bool,
        discussion_rounds: u32,
        discussion_models: Vec<String>,
    ) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(4096);
        let queues: DashMap<String, TaskQueue> = DashMap::new();
        let queues_dir = PathBuf::from(&runs_base_dir).join("queues");
        let _ = std::fs::create_dir_all(&queues_dir);
        for &queue_name in all_queue_names() {
            let queue_path = queues_dir.join(queue_name);
            let mut q = TaskQueue::new(queue_name, queue_path);
            q.load(); // restore any persisted tasks from disk
            queues.insert(queue_name.to_string(), q);
        }
        Arc::new(Self {
            agents: DashMap::new(),
            queues,
            broadcast_tx: tx,
            gpu_allocator: std::sync::Mutex::new(GpuAllocator::new(total_gpus, gpus_per_project)),
            discussion_groups: DashMap::new(),
            discussion_waiting: DashMap::new(),
            fail_counts: DashMap::new(),
            lab_batches: DashMap::new(),
            pending_transitions: DashMap::new(),
            python_path,
            agent_package_dir,
            runs_base_dir,
            auto_loop,
            discussion_mode: RwLock::new(discussion_mode),
            discussion_rounds: RwLock::new(discussion_rounds),
            discussion_models: RwLock::new(discussion_models),
            idea_factory_topic: RwLock::new(String::new()),
            idea_factory_config: RwLock::new(String::new()),
            idea_factory_remaining: RwLock::new(0),
            idea_factory_produced: AtomicU64::new(0),
            poll_counter: AtomicU64::new(0),
        })
    }

    pub fn projects_dir(&self) -> PathBuf {
        PathBuf::from(&self.runs_base_dir).join("projects")
    }

    pub fn queues_dir(&self) -> PathBuf {
        PathBuf::from(&self.runs_base_dir).join("queues")
    }

    /// Broadcast a JSON message to all connected clients.
    pub fn broadcast_msg(&self, msg: Value) {
        if let Ok(s) = serde_json::to_string(&msg) {
            let _ = self.broadcast_tx.send(s);
        }
    }

    /// Broadcast a list of JSON messages.
    pub fn broadcast_msgs(&self, msgs: Vec<Value>) {
        for msg in msgs {
            self.broadcast_msg(msg);
        }
    }
}

// ---------------------------------------------------------------------------
// Message builders
// ---------------------------------------------------------------------------

pub fn msg_agent_update(agent: &MolAgent) -> Value {
    serde_json::json!({ "type": "agent_update", "payload": agent.to_frontend() })
}

pub fn msg_stage_update(agent_id: &str, stage: u32, status: &str) -> Value {
    serde_json::json!({
        "type": "stage_update",
        "payload": { "agentId": agent_id, "stage": stage, "status": status }
    })
}

pub fn msg_artifact(
    repo_id: &str,
    filename: &str,
    agent_name: &str,
    size: &str,
    project_id: &str,
    content: &str,
    stage: Option<u32>,
) -> Value {
    let mut payload = serde_json::json!({
        "id": uid(),
        "repoId": repo_id,
        "projectId": project_id,
        "filename": filename,
        "producedBy": agent_name,
        "timestamp": now_ms(),
        "size": size,
        "status": "fresh",
    });
    if !content.is_empty() {
        payload["content"] = Value::String(content.to_string());
    }
    if let Some(s) = stage {
        payload["stage"] = Value::Number(s.into());
    }
    serde_json::json!({ "type": "artifact_produced", "payload": payload })
}

pub fn msg_log(
    agent_id: &str,
    agent_name: &str,
    layer: &str,
    stage: Option<u32>,
    message: &str,
    level: &str,
) -> Value {
    serde_json::json!({
        "type": "log",
        "payload": {
            "id": uid(),
            "agentId": agent_id,
            "agentName": agent_name,
            "layer": layer,
            "stage": stage,
            "message": message,
            "level": level,
            "timestamp": now_ms(),
        }
    })
}

pub fn msg_log_agent(agent: &MolAgent, message: &str, level: &str) -> Value {
    msg_log(&agent.id, &agent.name, &agent.layer, agent.current_stage, message, level)
}

pub fn msg_log_sys(message: &str, level: &str) -> Value {
    msg_log("system", "系统", "idea", None, message, level)
}

pub fn msg_queue_update(state: &BridgeState) -> Value {
    let mut payload = serde_json::Map::new();
    for entry in state.queues.iter() {
        payload.insert(entry.key().clone(), entry.value().summary());
    }
    serde_json::json!({ "type": "queue_update", "payload": payload })
}

pub fn msg_project_list(projects: Vec<Value>) -> Value {
    serde_json::json!({ "type": "project_list", "payload": projects })
}

pub fn msg_feedback_ack(_message_id: &str, content: &str, target_layer: &str) -> Value {
    serde_json::json!({
        "type": "chat_message",
        "payload": {
            "id": format!("sys-{}", uid()),
            "role": "system",
            "content": content,
            "timestamp": now_ms(),
            "targetLayer": target_layer,
        }
    })
}

pub fn msg_system(message: &str) -> Value {
    serde_json::json!({ "type": "system", "payload": { "message": message } })
}

// ---------------------------------------------------------------------------
// File monitoring / artifact helpers
// ---------------------------------------------------------------------------

fn extract_artifact_summary(path: &Path, filename: &str) -> String {
    const NO_CONTENT: &[&str] = &["paper_revised.md", "paper_draft.md", "outline.md", "reviews.md"];
    if NO_CONTENT.contains(&filename) {
        return String::new();
    }

    if path.is_dir() {
        let count = std::fs::read_dir(path)
            .map(|rd| rd.count())
            .unwrap_or(0);
        return format!("{count} files");
    }

    let text = match std::fs::read_to_string(path) {
        Ok(t) if !t.trim().is_empty() => t,
        _ => return String::new(),
    };

    if filename.ends_with(".json") {
        if let Ok(val) = serde_json::from_str::<Value>(&text) {
            if let Some(obj) = val.as_object() {
                let keys: Vec<&str> = obj.keys().map(|k| k.as_str()).take(4).collect();
                return format!("keys: {}", keys.join(", "));
            }
            if let Some(arr) = val.as_array() {
                return format!("{} entries", arr.len());
            }
        }
    }

    if filename.ends_with(".jsonl") {
        let count = text.lines().count();
        return format!("{count} entries");
    }

    // For .md and .yaml: return first non-empty line, truncated
    let first = text
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first.chars().take(200).collect()
}

fn sync_completed_stages(
    agent: &mut MolAgent,
    run_dir: &Path,
    layer_range_val: (u32, u32),
    done_up_to: u32,
) -> Vec<Value> {
    let mut messages = Vec::new();
    let (range_start, range_end) = layer_range_val;
    for s in range_start..=done_up_to.min(range_end) {
        if agent.stage_progress.get(&s).map(|v| v == "completed").unwrap_or(false) {
            continue;
        }
        if stage_to_layer(s).is_none() {
            continue;
        }
        agent.stage_progress.insert(s, "completed".to_string());
        messages.push(msg_stage_update(&agent.id, s, "completed"));
        messages.push(msg_log_agent(agent, &format!("{} completed", stage_name(s)), "success"));

        let stage_dir = run_dir.join(format!("stage-{s:02}"));
        if stage_dir.is_dir() {
            for &expected in stage_outputs(s) {
                let key = format!("{s}:{expected}");
                if agent.known_artifacts.contains(&key) {
                    continue;
                }
                let artifact_path = stage_dir.join(expected.trim_end_matches('/'));
                if !artifact_path.exists() {
                    continue;
                }
                if !is_display_artifact(expected) {
                    continue;
                }
                agent.known_artifacts.insert(key);
                let size = if artifact_path.is_dir() {
                    "dir".to_string()
                } else {
                    format!(
                        "{:.1} KB",
                        artifact_path.metadata().map(|m| m.len()).unwrap_or(0) as f64 / 1024.0
                    )
                };
                let content = extract_artifact_summary(&artifact_path, expected);
                messages.push(msg_artifact(
                    repo_for_stage(s),
                    expected,
                    &agent.name,
                    &size,
                    &agent.project_id,
                    &content,
                    Some(s),
                ));
            }
        }
    }
    messages
}

// ---------------------------------------------------------------------------
// Agent polling
// ---------------------------------------------------------------------------

pub fn poll_agent(agent: &mut MolAgent) -> Vec<Value> {
    let mut messages = Vec::new();
    let run_dir = PathBuf::from(&agent.run_dir);
    if run_dir.as_os_str().is_empty() || !run_dir.exists() {
        return messages;
    }

    let lr = if agent.is_idea_factory_s7_only {
        (7u32, 7u32)
    } else {
        layer_range(&agent.layer)
    };

    // Only read files if process is running
    let process_running = agent
        .process
        .as_mut()
        .map(|p| p.try_wait().ok().flatten().is_none())
        .unwrap_or(false);

    if process_running {
        if let Some(hb) = read_json(&run_dir.join("heartbeat.json")) {
            if hb != agent.prev_heartbeat {
                if let Some(new_stage) = hb.get("last_stage").and_then(|v| v.as_u64()) {
                    let new_stage = new_stage as u32;
                    let old_stage = agent.current_stage;
                    if old_stage != Some(new_stage)
                        && stage_to_layer(new_stage).is_some()
                        && new_stage >= lr.0
                        && new_stage <= lr.1
                    {
                        agent.current_stage = Some(new_stage);
                        agent.current_task =
                            format!("Stage {new_stage}: {}", stage_name(new_stage));
                        agent.status = AgentStatus::Working;
                        if agent.stage_progress.get(&new_stage).map(|v| v != "completed").unwrap_or(true) {
                            agent.stage_progress.insert(new_stage, "running".to_string());
                        }
                        messages.push(msg_agent_update(agent));
                        messages.push(msg_stage_update(&agent.id, new_stage, "running"));
                        messages.push(msg_log_agent(
                            agent,
                            &format!("Starting {}", stage_name(new_stage)),
                            "info",
                        ));
                    }
                }
                agent.prev_heartbeat = hb;
            }
        }

        if let Some(cp) = read_json(&run_dir.join("checkpoint.json")) {
            if cp != agent.prev_checkpoint {
                let done_up_to = cp
                    .get("last_completed_stage")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                messages.extend(sync_completed_stages(agent, &run_dir, lr, done_up_to));
                agent.prev_checkpoint = cp;

                if let Some(cs) = agent.current_stage {
                    if done_up_to >= cs && done_up_to < lr.1 {
                        let next = done_up_to + 1;
                        if stage_to_layer(next).is_some() && next >= lr.0 && next <= lr.1 {
                            agent.current_stage = Some(next);
                            agent.current_task =
                                format!("Stage {next}: {}", stage_name(next));
                            agent.stage_progress.insert(next, "running".to_string());
                            messages.push(msg_agent_update(agent));
                            messages.push(msg_stage_update(&agent.id, next, "running"));
                            messages.push(msg_log_agent(
                                agent,
                                &format!("Starting {}", stage_name(next)),
                                "info",
                            ));
                        }
                    }
                }
            }
        }
    }

    // Check if process exited
    let exit_status = agent.process.as_mut().and_then(|p| p.try_wait().ok().flatten());
    if let Some(status) = exit_status {
        // Final checkpoint read
        if let Some(cp) = read_json(&run_dir.join("checkpoint.json")) {
            let done_up_to = cp
                .get("last_completed_stage")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            messages.extend(sync_completed_stages(agent, &run_dir, lr, done_up_to));
        }

        if status.success() {
            agent.status = AgentStatus::Done;
            agent.current_task = String::new();
            agent.current_stage = None;
            messages.push(msg_agent_update(agent));
            messages.push(msg_log_agent(
                agent,
                &format!("Layer task completed (project={})", agent.project_id),
                "success",
            ));
        } else {
            let code = status.code().unwrap_or(-1);
            agent.status = AgentStatus::Error;
            agent.current_task = format!("exit code={code}");
            messages.push(msg_agent_update(agent));
            messages.push(msg_log_agent(
                agent,
                &format!("Process exited abnormally (code={code})"),
                "error",
            ));
        }
        agent.process = None;
    }

    messages
}

// ---------------------------------------------------------------------------
// Agent lifecycle
// ---------------------------------------------------------------------------

/// Create a new agent and register it in the state.
/// Returns the agent id.
fn create_agent(state: &BridgeState, name: &str, layer: &str) -> String {
    let agent = MolAgent::new(name, layer);
    let id = agent.id.clone();
    state.agents.insert(id.clone(), agent);
    id
}

/// Create a default pool of agents so tasks submitted immediately have workers.
///
/// Creates one agent per pipeline layer: idea, experiment, coding, execution,
/// writing.  Without these, `schedule_idle_agents` would find no idle workers
/// and tasks would queue up indefinitely.
pub fn create_default_agents(state: &Arc<BridgeState>) {
    let defaults = [
        ("Researcher-1", "idea"),
        ("Experiment-1", "experiment"),
        ("Coder-1", "coding"),
        ("Runner-1", "execution"),
        ("Writer-1", "writing"),
    ];
    for (name, layer) in &defaults {
        create_agent(state, name, layer);
    }
    info!("Created {} default agents", defaults.len());
}

fn assign_task_to_agent(agent: &mut MolAgent, task: &Task) {
    agent.project_id = task.project_id.clone();
    agent.run_dir = task.run_dir.clone();
    agent.run_id = task.project_id.clone();
    agent.config_path = task.config_path.clone();
    agent.assigned_task_id = Some(task.id.clone());
    agent.topic = task.topic.clone();
    agent.status = AgentStatus::Working;
    let stages = layer_stages(&agent.layer);
    agent.stage_progress = stages.iter().map(|&s| (s, "pending".to_string())).collect();
    agent.current_stage = stages.first().copied();
    agent.current_task = format!("Preparing [{}]", task.project_id);
    agent.prev_heartbeat = Value::Null;
    agent.prev_checkpoint = Value::Null;
    agent.known_artifacts = HashSet::new();

    // Extract role tag from topic pattern "[RoleName] topic"
    if let Some(rest) = task.topic.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            agent.role_tag = rest[..end].to_string();
        }
    }
}

pub fn launch_agent_for_task(
    state: &BridgeState,
    agent: &mut MolAgent,
    task: &Task,
    disc_mode: bool,
) -> Vec<Value> {
    let mut messages = Vec::new();
    assign_task_to_agent(agent, task);

    // Check reproduce mode
    let meta = read_project_meta(&task.run_dir);
    let is_reproduce = meta
        .as_ref()
        .and_then(|m| m.get("mode"))
        .and_then(|v| v.as_str())
        == Some("reproduce");

    let (mut fs, ts) = if disc_mode && agent.layer == "idea" && !is_reproduce {
        (1u32, 7u32)
    } else {
        layer_range(&agent.layer)
    };

    // Checkpoint-aware resume
    let cp = read_json(&PathBuf::from(&task.run_dir).join("checkpoint.json"));
    if let Some(ref cp) = cp {
        let last_done = cp.get("last_completed_stage").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let resume_stage = last_done + 1;

        if disc_mode && agent.layer == "idea" && !is_reproduce && last_done >= ts {
            // S1-S7 already done — skip straight to discussion/S8
            for s in fs..=ts {
                agent.stage_progress.insert(s, "completed".to_string());
            }
            messages.push(msg_log_agent(
                agent,
                &format!("S1-S7 already completed (checkpoint={last_done}), skipping to discussion/S8"),
                "info",
            ));
            // We can't call skip_discussion_proceed_s8 here (borrow issues) — caller handles it
            return messages;
        }

        if fs <= resume_stage && resume_stage <= ts {
            for s in fs..resume_stage {
                if stage_to_layer(s).is_some() {
                    agent.stage_progress.insert(s, "completed".to_string());
                }
            }
            fs = resume_stage;
            agent.current_stage = Some(fs);
            agent.current_task = format!("Resuming from checkpoint → {}", stage_name(fs));
            messages.push(msg_agent_update(agent));
            messages.push(msg_log_agent(
                agent,
                &format!("Checkpoint resume: starting from {}", stage_name(fs)),
                "info",
            ));
        }
    }

    let cmd_result = build_agent_cmd(state, agent, &task.config_path, &task.run_dir, fs, ts, &task.topic);
    match cmd_result {
        Err(e) => {
            agent.status = AgentStatus::Error;
            agent.current_task = format!("Launch failed: {e}");
            messages.push(msg_agent_update(agent));
            messages.push(msg_log_agent(agent, &format!("Launch failed: {e}"), "error"));
        }
        Ok(child) => {
            let pid = child.id();
            agent.process = Some(child);
            agent.current_task = format!("Project {} · PID={pid}", task.project_id);
            messages.push(msg_agent_update(agent));
            messages.push(msg_log_agent(
                agent,
                &format!("Task [{}] launched S{fs}→S{ts} (PID={pid})", task.project_id),
                "info",
            ));
        }
    }

    messages
}

fn build_agent_cmd(
    state: &BridgeState,
    agent: &MolAgent,
    config_path: &str,
    run_dir: &str,
    from_stage: u32,
    to_stage: u32,
    topic: &str,
) -> anyhow::Result<Child> {
    use std::process::{Command, Stdio};
    let log_path = PathBuf::from(run_dir).join(format!("agent_{}.log", agent.id));
    let log_file = std::fs::File::create(&log_path)?;

    // Prefer Rust `mol` binary if available, otherwise fall back to Python
    let mol_bin = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.join("mol")))
        .filter(|p| p.exists());

    let mut cmd = if let Some(ref mol_path) = mol_bin {
        let mut c = Command::new(mol_path);
        c.arg("run")
            .arg("--config").arg(config_path)
            .arg("--output").arg(run_dir)
            .arg("--from-stage").arg(stage_name(from_stage))
            .arg("--to-stage").arg(stage_name(to_stage))
            .arg("--auto-approve")
            .arg("--skip-preflight");
        c
    } else {
        let mut c = Command::new(&state.python_path);
        c.arg("-m")
            .arg("researchclaw")
            .arg("run")
            .arg("--config").arg(config_path)
            .arg("--output").arg(run_dir)
            .arg("--from-stage").arg(stage_name(from_stage))
            .arg("--to-stage").arg(stage_name(to_stage))
            .arg("--auto-approve")
            .arg("--skip-preflight");
        c
    };
    cmd.current_dir(&state.agent_package_dir)
        .stdout(log_file.try_clone()?)
        .stderr(Stdio::from(log_file))
        .env("PYTHONUNBUFFERED", "1");

    if !topic.is_empty() {
        cmd.arg("--topic").arg(topic);
    }

    Ok(cmd.spawn()?)
}

pub fn stop_agent(agent: &mut MolAgent) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(ref mut proc) = agent.process {
        let _ = proc.kill();
        let _ = proc.wait();
    }
    agent.process = None;
    agent.status = AgentStatus::Idle;
    agent.current_task = String::new();
    agent.current_stage = None;
    agent.assigned_task_id = None;
    messages.push(msg_agent_update(agent));
    messages.push(msg_log_agent(agent, "Agent stopped", "warning"));
    messages
}

// ---------------------------------------------------------------------------
// Project meta helpers
// ---------------------------------------------------------------------------

fn save_project_meta(run_dir: &str, project_id: &str, config_path: &str, topic: &str, mode: &str) {
    let meta_path = PathBuf::from(run_dir).join("project_meta.json");
    if meta_path.exists() {
        return;
    }
    let meta = serde_json::json!({
        "project_id": project_id,
        "config_path": config_path,
        "topic": topic,
        "mode": mode,
        "created_at": now_ms(),
    });
    let _ = write_json(&meta_path, &meta);
}

pub fn read_project_meta(run_dir: &str) -> Option<Value> {
    read_json(&PathBuf::from(run_dir).join("project_meta.json"))
}

fn determine_resume_target(run_dir: &str) -> Option<(String, u32)> {
    let cp = read_json(&PathBuf::from(run_dir).join("checkpoint.json"))?;
    let last_done = cp.get("last_completed_stage").and_then(|v| v.as_u64())? as u32;
    if last_done == 0 {
        return None;
    }
    let next_stage = last_done + 1;
    if next_stage > 22 {
        return None;
    }
    let target_layer = stage_to_layer(next_stage)?;
    Some((target_layer.to_string(), next_stage))
}

// ---------------------------------------------------------------------------
// Project lifecycle
// ---------------------------------------------------------------------------

pub fn submit_new_project(
    state: &BridgeState,
    project_id: &str,
    config_path: &str,
    topic: &str,
    mode: &str,
    disc_mode: bool,
) -> Vec<Value> {
    let mut messages = Vec::new();

    let run_dir_path = state.projects_dir().join(project_id);
    let _ = std::fs::create_dir_all(&run_dir_path);
    // Use absolute path so child processes find it regardless of cwd
    let run_dir = run_dir_path.canonicalize().unwrap_or(run_dir_path).to_string_lossy().into_owned();
    save_project_meta(&run_dir, project_id, config_path, topic, mode);

    if disc_mode {
        messages.push(msg_log_sys(
            &format!("New project [{project_id}] cross-project discussion mode: 1 agent, discuss after S7"),
            "info",
        ));
    }

    if let Some((target_layer, next_stage)) = determine_resume_target(&run_dir) {
        let queue_name = layer_input_queue(&target_layer).unwrap_or("init_to_idea");
        let source_layer = match target_layer.as_str() {
            "idea" => "init",
            "experiment" => "idea",
            "coding" => "experiment",
            "execution" => "coding",
            "writing" => "execution",
            _ => "init",
        };
        let task = Task {
            id: format!("task-{}", uid()),
            project_id: project_id.to_string(),
            run_dir: run_dir.clone(),
            config_path: config_path.to_string(),
            topic: topic.to_string(),
            source_layer: source_layer.to_string(),
            target_layer: target_layer.clone(),
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: now_ms(),
            assigned_at: 0,
            completed_at: 0,
        };
        if let Some(mut q) = state.queues.get_mut(queue_name) {
            q.push(task);
        } else {
            warn!("Queue '{queue_name}' not found — task for project [{project_id}] dropped!");
            messages.push(msg_log_sys(&format!("ERROR: queue '{queue_name}' missing, task dropped"), "error"));
        }
        messages.push(msg_log_sys(
            &format!("Project [{project_id}] checkpoint detected → resuming from {} (Stage {next_stage})", stage_name(next_stage)),
            "success",
        ));
    } else {
        let task = Task {
            id: format!("task-{}", uid()),
            project_id: project_id.to_string(),
            run_dir: run_dir.clone(),
            config_path: config_path.to_string(),
            topic: topic.to_string(),
            source_layer: "init".to_string(),
            target_layer: "idea".to_string(),
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: now_ms(),
            assigned_at: 0,
            completed_at: 0,
        };
        if let Some(mut q) = state.queues.get_mut("init_to_idea") {
            q.push(task);
        } else {
            warn!("Queue 'init_to_idea' not found — task for project [{project_id}] dropped!");
            messages.push(msg_log_sys("ERROR: queue 'init_to_idea' missing, task dropped", "error"));
        }
        messages.push(msg_log_sys(
            &format!("New project [{project_id}] added to research queue"),
            "info",
        ));
    }

    messages.push(msg_queue_update(state));
    messages
}

pub fn list_all_projects(state: &BridgeState) -> Vec<Value> {
    let projects_dir = state.projects_dir();
    if !projects_dir.exists() {
        return Vec::new();
    }

    let running_ids: HashSet<String> = state
        .agents
        .iter()
        .filter_map(|entry| {
            let a = entry.value();
            if !a.project_id.is_empty() && (a.process.is_some() || a.status == AgentStatus::WaitingHumanReview || a.status == AgentStatus::WaitingDiscussion || a.status == AgentStatus::Discussing) {
                Some(a.project_id.clone())
            } else {
                None
            }
        })
        .collect();

    let queued_ids: HashSet<String> = state
        .queues
        .iter()
        .flat_map(|entry| {
            entry
                .value()
                .tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::Assigned)
                .filter(|t| !t.project_id.is_empty())
                .map(|t| t.project_id.clone())
                .collect::<Vec<_>>()
        })
        .collect();

    let mut result = Vec::new();
    let mut entries: Vec<_> = match std::fs::read_dir(&projects_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('_'))
            .collect(),
        Err(_) => return result,
    };
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let project_id = entry.file_name().to_string_lossy().into_owned();
        let proj_dir = entry.path();

        let meta = read_json(&proj_dir.join("project_meta.json"));

        // Try project root checkpoint, then best sub-run (Lab mode)
        let cp = read_json(&proj_dir.join("checkpoint.json")).or_else(|| {
            let mut best_stage = 0u64;
            let mut best_cp: Option<Value> = None;
            if let Ok(rd) = std::fs::read_dir(&proj_dir) {
                for sub in rd.filter_map(|e| e.ok()) {
                    if sub.file_name().to_string_lossy().starts_with("run-") {
                        if let Some(sub_cp) = read_json(&sub.path().join("checkpoint.json")) {
                            let stage = sub_cp.get("last_completed_stage")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            if stage > best_stage {
                                best_stage = stage;
                                best_cp = Some(sub_cp);
                            }
                        }
                    }
                }
            }
            best_cp
        });

        let last_stage = cp.as_ref().and_then(|c| c.get("last_completed_stage")).and_then(|v| v.as_u64()).unwrap_or(0);
        let last_name = cp.as_ref().and_then(|c| c.get("last_completed_name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let timestamp = cp.as_ref().and_then(|c| c.get("timestamp")).and_then(|v| v.as_str()).unwrap_or("").to_string();

        let has_waiting_agent = state.agents.iter().any(|e| {
            let a = e.value();
            a.project_id == project_id && a.status == AgentStatus::WaitingHumanReview
        });

        let status = if last_stage >= 22 {
            "completed"
        } else if has_waiting_agent {
            "waiting_human_review"
        } else if running_ids.contains(&project_id) {
            "running"
        } else if queued_ids.contains(&project_id) {
            "queued"
        } else if last_stage > 0 {
            "interrupted"
        } else {
            "new"
        };

        let topic = meta.as_ref().and_then(|m| m.get("topic")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let config_path = meta.as_ref().and_then(|m| m.get("config_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let intervention = meta.as_ref().and_then(|m| m.get("intervention")).and_then(|v| v.as_str()).unwrap_or("").to_string();

        result.push(serde_json::json!({
            "projectId": project_id,
            "status": status,
            "lastCompletedStage": last_stage,
            "lastCompletedName": last_name,
            "firstStage": 1,
            "totalStages": 22,
            "timestamp": timestamp,
            "topic": topic,
            "configPath": config_path,
            "intervention": intervention,
        }));
    }
    result
}

fn pause_project(state: &BridgeState, project_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let mut stopped = 0u32;
    for mut entry in state.agents.iter_mut() {
        let agent = entry.value_mut();
        if agent.project_id == project_id {
            if let Some(ref mut p) = agent.process {
                let _ = p.kill();
                let _ = p.wait();
            }
            agent.reset_idle();
            messages.push(msg_agent_update(agent));
            stopped += 1;
        }
    }

    let mut removed = 0u32;
    for mut entry in state.queues.iter_mut() {
        let q = entry.value_mut();
        let before = q.tasks.len();
        q.tasks.retain(|t| t.project_id != project_id);
        removed += (before - q.tasks.len()) as u32;
        if removed > 0 {
            q.save();
        }
    }

    messages.push(msg_log_sys(
        &format!("Project [{project_id}] paused (stopped {stopped} agents, removed {removed} queued tasks)"),
        "warning",
    ));
    messages
}

fn delete_project(state: &BridgeState, project_id: &str) -> Vec<Value> {
    let mut messages = pause_project(state, project_id);

    {
        let released = state.gpu_allocator.lock().unwrap_or_else(|e| e.into_inner()).release(project_id);
        if !released.is_empty() {
            messages.push(msg_log_sys(&format!("GPU {released:?} released (project deleted)"), "info"));
        }
    }

    state.fail_counts.remove(project_id);
    state.discussion_waiting.retain(|_, v| v.as_str() != project_id);
    state.discussion_groups.remove(project_id);
    state.lab_batches.remove(project_id);

    let proj_dir = state.projects_dir().join(project_id);
    if proj_dir.exists() {
        match std::fs::remove_dir_all(&proj_dir) {
            Ok(_) => messages.push(msg_log_sys(&format!("Project [{project_id}] deleted"), "success")),
            Err(e) => messages.push(msg_log_sys(&format!("Failed to delete project dir: {e}"), "error")),
        }
    }
    messages
}

// ---------------------------------------------------------------------------
// Discussion helpers
// ---------------------------------------------------------------------------

fn skip_discussion_proceed_s8(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    state.discussion_waiting.remove(agent_id);

    let Some(mut entry) = state.agents.get_mut(agent_id) else {
        return messages;
    };
    let agent = entry.value_mut();

    agent.stage_progress.insert(DISCUSSION_STAGE, "completed".to_string());
    messages.push(msg_stage_update(&agent.id, DISCUSSION_STAGE, "completed"));

    agent.status = AgentStatus::Working;
    agent.current_task = format!("Project {} · S8 hypothesis generation (skipping discussion)", agent.project_id);
    agent.stage_progress.insert(8, "running".to_string());
    messages.push(msg_agent_update(agent));
    messages.push(msg_stage_update(&agent.id, 8, "running"));
    messages.push(msg_log_agent(agent, "Skipping discussion → starting S8 hypothesis generation", "info"));

    let (fs, ts) = (8u32, 8u32);
    match build_agent_cmd(state, agent, &agent.config_path.clone(), &agent.run_dir.clone(), fs, ts, &agent.project_id.clone()) {
        Ok(child) => {
            let pid = child.id();
            agent.process = Some(child);
            agent.is_discussion_s8 = true;
            messages.push(msg_log_agent(agent, &format!("S8 launched (PID={pid})"), "info"));
        }
        Err(e) => {
            agent.status = AgentStatus::Error;
            agent.current_task = format!("S8 launch failed: {e}");
            messages.push(msg_agent_update(agent));
            messages.push(msg_log_agent(agent, &format!("S8 launch failed: {e}"), "error"));
        }
    }

    messages
}

pub fn trigger_discussion(state: &BridgeState, group_project_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let Some(mut group_entry) = state.discussion_groups.get_mut(group_project_id) else {
        return messages;
    };
    let group = group_entry.value_mut();
    group.status = DiscussionStatus::Discussing;

    let disc_dir = state
        .projects_dir()
        .join(&group.project_id)
        .join("discussion");
    let _ = std::fs::create_dir_all(&disc_dir);
    group.discussion_output_dir = disc_dir.to_string_lossy().into_owned();

    let synthesis_dirs = group.synthesis_dirs();
    let agent_ids = group.agent_ids.clone();
    let config_path = group.config_path.clone();
    let topic = group.topic.clone();

    // Update agent statuses
    for aid in &agent_ids {
        if let Some(mut aentry) = state.agents.get_mut(aid) {
            let a = aentry.value_mut();
            a.status = AgentStatus::Discussing;
            a.current_stage = Some(DISCUSSION_STAGE);
            a.current_task = "Multi-agent discussion in progress...".to_string();
            messages.push(msg_agent_update(a));
        }
    }

    let rounds = *state.discussion_rounds.read().unwrap_or_else(|e| e.into_inner());
    let runner_path = PathBuf::from(&state.runs_base_dir)
        .parent()
        .unwrap_or(Path::new("."))
        .join("backend/services/discussion_runner.py");

    use std::process::{Command, Stdio};
    let log_path = disc_dir.join("discussion.log");
    match std::fs::File::create(&log_path).and_then(|log| {
        let mut cmd = Command::new(&state.python_path);
        cmd.arg(&runner_path)
            .arg("--config").arg(&config_path)
            .arg("--output").arg(&disc_dir)
            .arg("--rounds").arg(rounds.to_string())
            .current_dir(&state.agent_package_dir)
            .stdout(log.try_clone()?)
            .stderr(Stdio::from(log))
            .env("PYTHONUNBUFFERED", "1");
        for sd in &synthesis_dirs {
            cmd.arg("--synthesis-dirs").arg(sd);
        }
        if !topic.is_empty() {
            cmd.arg("--topic").arg(&topic);
        }
        cmd.spawn().map_err(Into::into)
    }) {
        Ok(child) => {
            let pid = child.id();
            if let Some(mut ge) = state.discussion_groups.get_mut(group_project_id) {
                ge.value_mut().discussion_process = Some(child);
            }
            messages.push(msg_log_sys(
                &format!("Discussion [{group_project_id}] started: {} agents, {rounds} rounds (PID={pid})", agent_ids.len()),
                "info",
            ));
        }
        Err(e) => {
            if let Some(mut ge) = state.discussion_groups.get_mut(group_project_id) {
                ge.value_mut().status = DiscussionStatus::Done;
            }
            messages.push(msg_log_sys(&format!("Discussion launch failed: {e}"), "error"));
        }
    }

    messages
}

pub fn poll_discussion(state: &BridgeState, group_key: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let exit_code = {
        let Some(mut entry) = state.discussion_groups.get_mut(group_key) else {
            return messages;
        };
        let group = entry.value_mut();
        if group.status != DiscussionStatus::Discussing {
            return messages;
        }
        let Some(ref mut proc) = group.discussion_process else {
            return messages;
        };
        match proc.try_wait() {
            Ok(Some(status)) => {
                group.discussion_process = None;
                Some(status.code().unwrap_or(-1))
            }
            _ => None,
        }
    };

    let Some(code) = exit_code else {
        return messages;
    };

    if code != 0 {
        if let Some(mut entry) = state.discussion_groups.get_mut(group_key) {
            entry.value_mut().status = DiscussionStatus::Done;
        }
        messages.push(msg_log_sys(
            &format!("Discussion [{group_key}] failed (exit={code})"),
            "error",
        ));
        let agent_ids: Vec<String> = state
            .discussion_groups
            .get(group_key)
            .map(|g| g.agent_ids.clone())
            .unwrap_or_default();
        for aid in &agent_ids {
            messages.extend(skip_discussion_proceed_s8(state, aid));
        }
        return messages;
    }

    // Discussion succeeded
    let (consensus_file, agent_ids, disc_output_dir, project_id) = {
        let Some(entry) = state.discussion_groups.get(group_key) else {
            return messages;
        };
        let g = entry.value();
        (
            PathBuf::from(&g.discussion_output_dir).join("consensus_synthesis.md"),
            g.agent_ids.clone(),
            g.discussion_output_dir.clone(),
            g.project_id.clone(),
        )
    };

    if !consensus_file.exists() {
        messages.push(msg_log_sys(
            &format!("Discussion [{group_key}] completed but no consensus produced"),
            "warning",
        ));
        if let Some(mut entry) = state.discussion_groups.get_mut(group_key) {
            entry.value_mut().status = DiscussionStatus::Done;
        }
        for aid in &agent_ids {
            messages.extend(skip_discussion_proceed_s8(state, aid));
        }
        return messages;
    }

    let consensus_text = std::fs::read_to_string(&consensus_file).unwrap_or_default();
    messages.push(msg_log_sys(
        &format!("Discussion [{group_key}] completed, consensus generated, launching hypothesis generation"),
        "success",
    ));

    for aid in &agent_ids {
        if let Some(mut aentry) = state.agents.get_mut(aid) {
            aentry.value_mut().stage_progress.insert(DISCUSSION_STAGE, "completed".to_string());
        }
        messages.push(msg_stage_update(aid, DISCUSSION_STAGE, "completed"));
    }

    let transcript_file = PathBuf::from(&disc_output_dir).join("discussion_transcript.md");
    if transcript_file.exists() {
        let size = transcript_file.metadata().map(|m| format!("{:.1} KB", m.len() as f64 / 1024.0)).unwrap_or_default();
        messages.push(msg_artifact("knowledge", "discussion_transcript.md", "Discussion", &size, &project_id, "", None));
    }

    if let Some(mut entry) = state.discussion_groups.get_mut(group_key) {
        entry.value_mut().status = DiscussionStatus::Done;
    }

    // Enrich each agent's synthesis.md with the consensus, then launch S8
    for aid in &agent_ids {
        state.discussion_waiting.remove(aid);

        let run_dir_str = state
            .agents
            .get(aid)
            .map(|a| a.run_dir.clone())
            .unwrap_or_default();

        if !run_dir_str.is_empty() {
            let s7_dir = PathBuf::from(&run_dir_str).join("stage-07");
            let _ = std::fs::create_dir_all(&s7_dir);
            let synth_file = s7_dir.join("synthesis.md");
            if synth_file.exists() {
                if let Ok(original) = std::fs::read_to_string(&synth_file) {
                    let enriched = format!("{original}\n\n---\n\n# Multi-Agent Discussion Consensus\n\n{consensus_text}");
                    let _ = std::fs::write(&synth_file, enriched);
                }
            } else {
                let _ = std::fs::write(&synth_file, &consensus_text);
            }

            // Save discussion artifacts
            let disc_artifact_dir = PathBuf::from(&run_dir_str).join("discussion");
            let _ = std::fs::create_dir_all(&disc_artifact_dir);
            let _ = std::fs::write(disc_artifact_dir.join("consensus_synthesis.md"), &consensus_text);
            if transcript_file.exists() {
                let _ = std::fs::copy(&transcript_file, disc_artifact_dir.join("discussion_transcript.md"));
            }

            // Launch S8
            messages.extend(launch_s8_for_agent(state, aid, group_key));
        } else {
            // Reviewer agent — reset to idle
            if let Some(mut aentry) = state.agents.get_mut(aid) {
                aentry.value_mut().reset_idle();
                messages.push(msg_agent_update(aentry.value()));
            }
        }
    }

    messages
}

fn launch_s8_for_agent(state: &BridgeState, agent_id: &str, group_key: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let (config_path, run_dir, agent_topic) = {
        let Some(aentry) = state.agents.get(agent_id) else {
            return messages;
        };
        let a = aentry.value();
        (a.config_path.clone(), a.run_dir.clone(), a.topic.clone())
    };

    let group_topic = state
        .discussion_groups
        .get(group_key)
        .map(|g| g.topic.clone())
        .unwrap_or_default();
    let topic = if agent_topic.is_empty() { group_topic } else { agent_topic };

    {
        let Some(mut aentry) = state.agents.get_mut(agent_id) else {
            return messages;
        };
        let agent = aentry.value_mut();
        agent.status = AgentStatus::Working;
        agent.current_task = "S8 hypothesis generation".to_string();
        agent.stage_progress.insert(8, "running".to_string());
        messages.push(msg_agent_update(agent));
        messages.push(msg_stage_update(&agent.id, 8, "running"));
        messages.push(msg_log_agent(agent, "Discussion complete → starting hypothesis generation", "info"));
    }

    let Some(aentry) = state.agents.get(agent_id) else { return messages; };
    let agent = aentry.value();
    match build_agent_cmd(state, agent, &config_path, &run_dir, 8, 8, &topic) {
        Ok(child) => {
            let pid = child.id();
            drop(aentry);
            if let Some(mut ae) = state.agents.get_mut(agent_id) {
                ae.value_mut().process = Some(child);
                ae.value_mut().is_discussion_s8 = true;
                messages.push(msg_log_agent(ae.value(), &format!("S8 launched (PID={pid})"), "info"));
            }
        }
        Err(e) => {
            drop(aentry);
            if let Some(mut ae) = state.agents.get_mut(agent_id) {
                ae.value_mut().status = AgentStatus::Error;
                ae.value_mut().current_task = format!("S8 launch failed: {e}");
                messages.push(msg_agent_update(ae.value()));
                messages.push(msg_log_agent(ae.value(), &format!("S8 launch failed: {e}"), "error"));
            }
        }
    }

    messages
}

fn on_discussion_s8_done(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let project_id = state
        .agents
        .get(agent_id)
        .map(|a| a.project_id.clone())
        .unwrap_or_default();

    if let Some(mut ae) = state.agents.get_mut(agent_id) {
        ae.value_mut().is_discussion_s8 = false;
    }

    // Find group
    let group_key = state
        .discussion_groups
        .iter()
        .find(|entry| entry.value().agent_ids.contains(&agent_id.to_string()))
        .map(|entry| entry.key().clone());

    if let Some(ref key) = group_key {
        if let Some(mut entry) = state.discussion_groups.get_mut(key) {
            entry.value_mut().completed_s8.insert(agent_id.to_string());
        }
        messages.push(msg_log_sys(
            &format!("S8 completed for agent {agent_id}, waiting for peers..."),
            "info",
        ));

        let all_done = state
            .discussion_groups
            .get(key)
            .map(|g| g.all_s8_done())
            .unwrap_or(false);

        if !all_done {
            // Park this agent, wait for peers
            if let Some(mut ae) = state.agents.get_mut(agent_id) {
                ae.value_mut().reset_idle();
                messages.push(msg_agent_update(ae.value()));
            }
            return messages;
        }
    }

    // All S8 done — select best hypothesis and create ONE downstream task
    let (best_run_dir, best_config, merged_project_id) = if let Some(ref key) = group_key {
        let group = state.discussion_groups.get(key);
        if let Some(g) = group {
            let best_id = select_best_hypothesis(state, g.value());
            let run = g.value().run_dirs.get(&best_id).cloned().unwrap_or_else(|| {
                state.agents.get(agent_id).map(|a| a.run_dir.clone()).unwrap_or_default()
            });
            let cfg = g.value().config_path.clone();
            let pid = g.value().project_id.clone();
            (run, cfg, pid)
        } else {
            let a = state.agents.get(agent_id);
            let run = a.as_ref().map(|a| a.run_dir.clone()).unwrap_or_default();
            let cfg = a.as_ref().map(|a| a.config_path.clone()).unwrap_or_default();
            (run, cfg, project_id.clone())
        }
    } else {
        let a = state.agents.get(agent_id);
        let run = a.as_ref().map(|a| a.run_dir.clone()).unwrap_or_default();
        let cfg = a.as_ref().map(|a| a.config_path.clone()).unwrap_or_default();
        (run, cfg, project_id.clone())
    };

    messages.push(msg_log_sys(
        &format!("Project [{merged_project_id}] all S8 complete → best hypothesis selected → entering L2 experiment design"),
        "success",
    ));

    if !merged_project_id.is_empty() {
        let topic = state.agents.get(agent_id).map(|a| a.topic.clone()).unwrap_or_default();
        let follow_task = Task {
            id: format!("task-{}", uid()),
            project_id: merged_project_id.clone(),
            run_dir: best_run_dir,
            config_path: best_config,
            topic,
            source_layer: "idea".to_string(),
            target_layer: "experiment".to_string(),
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: now_ms(),
            assigned_at: 0,
            completed_at: 0,
        };
        if let Some(mut q) = state.queues.get_mut("idea_to_experiment") {
            q.push(follow_task);
        } else {
            warn!("Queue 'idea_to_experiment' not found — follow-up task dropped!");
        }
    }

    if let Some(mut ae) = state.agents.get_mut(agent_id) {
        ae.value_mut().reset_idle();
        messages.push(msg_agent_update(ae.value()));
    }
    messages.push(msg_queue_update(state));
    messages
}

fn select_best_hypothesis(_state: &BridgeState, group: &DiscussionGroup) -> String {
    let mut best_id = group.agent_ids.first().cloned().unwrap_or_default();
    let mut best_score = -1i64;

    for aid in &group.agent_ids {
        let rd = match group.run_dirs.get(aid) {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };
        let mut score = 0i64;
        let hypo = PathBuf::from(rd).join("stage-08/hypotheses.md");
        if let Ok(text) = std::fs::read_to_string(&hypo) {
            score += text.len() as i64;
            score += text.to_lowercase().matches("hypothesis").count() as i64 * 500;
            score += text.matches("## ").count() as i64 * 300;
        }
        if PathBuf::from(rd).join("stage-08/novelty_report.json").exists() {
            score += 2000;
        }
        if score > best_score {
            best_score = score;
            best_id = aid.clone();
        }
    }
    best_id
}

// ---------------------------------------------------------------------------
// Task scheduling (on_agent_done + schedule_idle_agents)
// ---------------------------------------------------------------------------

pub fn on_agent_done(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    state.fail_counts.remove(
        &state.agents.get(agent_id).map(|a| a.project_id.clone()).unwrap_or_default()
    );

    // Complete assigned task
    let assigned_task_id = state.agents.get(agent_id).and_then(|a| a.assigned_task_id.clone());
    if let Some(ref tid) = assigned_task_id {
        for mut q in state.queues.iter_mut() {
            q.value_mut().complete(tid);
        }
    }

    let (layer, project_id, run_dir, config_path, topic) = {
        let Some(a) = state.agents.get(agent_id) else {
            return messages;
        };
        (a.layer.clone(), a.project_id.clone(), a.run_dir.clone(), a.config_path.clone(), a.topic.clone())
    };

    // Discussion mode: idea layer → enter discussion after S7
    let disc_mode = *state.discussion_mode.read().unwrap_or_else(|e| e.into_inner());
    let meta = read_project_meta(&run_dir);
    let is_reproduce = meta.as_ref().and_then(|m| m.get("mode")).and_then(|v| v.as_str()) == Some("reproduce");

    if disc_mode && layer == "idea" && !is_reproduce {
        state.discussion_waiting.insert(agent_id.to_string(), project_id.clone());
        if let Some(mut ae) = state.agents.get_mut(agent_id) {
            ae.value_mut().current_stage = Some(DISCUSSION_STAGE);
            ae.value_mut().stage_progress.insert(DISCUSSION_STAGE, "running".to_string());
            ae.value_mut().status = AgentStatus::WaitingDiscussion;
            ae.value_mut().current_task = "S7 complete, waiting for discussion partner...".to_string();
            messages.push(msg_agent_update(ae.value()));
        }

        let expected_count = state.lab_batches.get(&project_id).map(|v| *v).unwrap_or(0);

        if expected_count >= 2 {
            // Lab mode: wait for all N agents from same project
            let waiting_same: Vec<String> = state
                .discussion_waiting
                .iter()
                .filter(|entry| *entry.value() == project_id)
                .map(|entry| entry.key().clone())
                .collect();

            if waiting_same.len() >= expected_count {
                messages.push(msg_log_sys(
                    &format!("Project [{project_id}] all {expected_count} directions S7 complete → starting discussion"),
                    "info",
                ));
                let group = DiscussionGroup {
                    project_id: project_id.clone(),
                    topic: topic.clone(),
                    config_path: config_path.clone(),
                    agent_ids: waiting_same.clone(),
                    run_dirs: waiting_same
                        .iter()
                        .filter_map(|aid| {
                            state.agents.get(aid).map(|a| (aid.clone(), a.run_dir.clone()))
                        })
                        .collect(),
                    completed_s7: waiting_same.iter().cloned().collect(),
                    completed_s8: HashSet::new(),
                    best_agent_id: String::new(),
                    status: DiscussionStatus::Gathering,
                    discussion_process: None,
                    discussion_output_dir: String::new(),
                    is_cross_project: false,
                };
                for aid in &waiting_same {
                    state.discussion_waiting.remove(aid);
                }
                state.discussion_groups.insert(project_id.clone(), group);
                messages.extend(trigger_discussion(state, &project_id));
            } else {
                messages.push(msg_log_sys(
                    &format!("S7 complete, waiting for other directions ({}/{expected_count})", waiting_same.len()),
                    "info",
                ));
            }
            return messages;
        }

        // Cross-project: find a peer
        let peers: Vec<String> = state
            .discussion_waiting
            .iter()
            .filter(|entry| *entry.key() != agent_id && *entry.value() != project_id)
            .map(|entry| entry.key().clone())
            .collect();

        if let Some(peer_id) = peers.first() {
            messages.push(msg_log_sys(
                &format!("S7 complete, starting cross-project discussion with [{peer_id}]"),
                "info",
            ));
            let peer_run = state.agents.get(peer_id).map(|a| a.run_dir.clone()).unwrap_or_default();
            let peer_project = state.discussion_waiting.get(peer_id).map(|v| v.clone()).unwrap_or_default();

            let disc_name = format!("{project_id}_x_{peer_project}");
            let disc_dir = state.projects_dir().join("_cross_discussions").join(&disc_name);
            let _ = std::fs::create_dir_all(&disc_dir);

            let group = DiscussionGroup {
                project_id: disc_name.clone(),
                topic: format!("{project_id} | {peer_project}"),
                config_path: config_path.clone(),
                agent_ids: vec![agent_id.to_string(), peer_id.clone()],
                run_dirs: [
                    (agent_id.to_string(), run_dir.clone()),
                    (peer_id.clone(), peer_run),
                ].into(),
                completed_s7: [agent_id.to_string(), peer_id.clone()].into(),
                completed_s8: HashSet::new(),
                best_agent_id: String::new(),
                status: DiscussionStatus::Gathering,
                discussion_process: None,
                discussion_output_dir: disc_dir.to_string_lossy().into_owned(),
                is_cross_project: true,
            };
            state.discussion_waiting.remove(agent_id);
            state.discussion_waiting.remove(peer_id.as_str());
            state.discussion_groups.insert(disc_name.clone(), group);
            messages.extend(trigger_discussion(state, &disc_name));
        } else {
            // No peer: wait for human review instead of skipping
            // Scan run_dir for key artifacts to show the user
            let key_files = ["synthesis_report.md", "gap_analysis.json", "knowledge_cards.json", "goal.md", "problem_tree.md"];
            let mut found_artifacts = Vec::new();
            for fname in &key_files {
                for stage_num in 1..=7u32 {
                    let p = PathBuf::from(&run_dir).join(format!("stage-{stage_num:02}")).join(fname);
                    if p.exists() {
                        let size = std::fs::metadata(&p).map(|m| format!("{}B", m.len())).unwrap_or_default();
                        let content = if fname.ends_with(".md") {
                            std::fs::read_to_string(&p).unwrap_or_default().chars().take(500).collect::<String>()
                        } else { String::new() };
                        messages.push(msg_artifact("knowledge", fname, &project_id, &size, &project_id, &content, Some(stage_num)));
                        found_artifacts.push(*fname);
                    }
                }
            }
            let artifacts_hint = if found_artifacts.is_empty() {
                String::new()
            } else {
                format!("\n主要产出: {}", found_artifacts.join(", "))
            };
            messages.push(msg_log_sys(
                &format!("S7 完成，等待人工审阅后继续。请在右侧数据架查看产出，发送反馈后输入「继续」推进到实验层。{artifacts_hint}"),
                "info",
            ));
            if let Some(mut ae) = state.agents.get_mut(agent_id) {
                let a = ae.value_mut();
                a.status = AgentStatus::WaitingHumanReview;
                a.current_task = "想法层完成，等待人工审阅…".to_string();
                messages.push(msg_agent_update(a));
            }
        }
        return messages;
    }

    // Coding layer: check S12 sanity
    if layer == "coding" && !project_id.is_empty() {
        let sanity_msgs = check_s12_sanity_failure(state, &agent_id);
        if !sanity_msgs.is_empty() {
            messages.extend(sanity_msgs);
            messages.push(msg_project_list(list_all_projects(state)));
            return messages;
        }
    }

    // Create follow-up task
    if let Some(output_queue_name) = layer_output_queue(&layer) {
        // L4→L5: only if decision.md says PROCEED
        let should_push = if layer == "execution" && output_queue_name == "execution_to_writing" {
            // Check decision from stage-17 — may be decision.md or decision_record.json
            let dec_md = PathBuf::from(&run_dir).join("stage-17/decision.md");
            let dec_json = PathBuf::from(&run_dir).join("stage-17/decision_record.json");
            let text = if dec_md.exists() {
                std::fs::read_to_string(&dec_md).unwrap_or_default().to_uppercase()
            } else if dec_json.exists() {
                std::fs::read_to_string(&dec_json).unwrap_or_default().to_uppercase()
            } else {
                String::new()
            };
            text.contains("PROCEED")
        } else {
            true
        };

        if should_push {
            // When discussion mode is on, pause at layer boundaries for human review
            if disc_mode && !is_reproduce {
                let layer_cn = match layer.as_str() {
                    "idea" => "想法层",
                    "experiment" => "实验层",
                    "coding" => "编码层",
                    "execution" => "执行层",
                    _ => &layer,
                };
                messages.push(msg_log_sys(
                    &format!(
                        "{layer_cn}完成，等待人工审阅。请查看产出后输入「继续」推进到下一层。"
                    ),
                    "info",
                ));
                // Store pending transition info so approve can resume it
                if let Some(mut ae) = state.agents.get_mut(agent_id) {
                    let a = ae.value_mut();
                    a.status = AgentStatus::WaitingHumanReview;
                    a.current_task = format!("{layer_cn}完成，等待人工审阅…");
                    messages.push(msg_agent_update(a));
                }
                // Save the pending transition so approve_waiting_agents can pick it up
                state.pending_transitions.insert(
                    agent_id.to_string(),
                    PendingTransition {
                        output_queue: output_queue_name.to_string(),
                        project_id: project_id.clone(),
                        run_dir: run_dir.clone(),
                        config_path: config_path.clone(),
                        topic: topic.clone(),
                        source_layer: layer.clone(),
                    },
                );
                messages.push(msg_queue_update(state));
                return messages;
            }

            if let Some((_, target_layer)) = queue_layers(output_queue_name) {
                let follow_task = Task {
                    id: format!("task-{}", uid()),
                    project_id: project_id.clone(),
                    run_dir: run_dir.clone(),
                    config_path: config_path.clone(),
                    topic: topic.clone(),
                    source_layer: layer.clone(),
                    target_layer: target_layer.to_string(),
                    status: TaskStatus::Pending,
                    assigned_to: None,
                    created_at: now_ms(),
                    assigned_at: 0,
                    completed_at: 0,
                };
                if let Some(mut q) = state.queues.get_mut(output_queue_name) {
                    q.push(follow_task);
                } else {
                    warn!("Queue '{output_queue_name}' not found — follow-up task dropped!");
                }
                messages.push(msg_log_sys(
                    &format!("Task complete → project [{project_id}] added to {output_queue_name} queue"),
                    "success",
                ));
            }
        }
    }

    // Release GPU for execution layer
    if layer == "execution" && !project_id.is_empty() {
        let released = state.gpu_allocator.lock().unwrap_or_else(|e| e.into_inner()).release(&project_id);
        if !released.is_empty() {
            messages.push(msg_log_sys(&format!("GPU {released:?} released"), "info"));
        }
    }

    // Reset agent
    if let Some(mut ae) = state.agents.get_mut(agent_id) {
        ae.value_mut().reset_idle();
        messages.push(msg_agent_update(ae.value()));
    }
    messages.push(msg_queue_update(state));
    messages
}

pub fn schedule_idle_agents(state: &BridgeState) -> Vec<Value> {
    let mut messages = Vec::new();
    let disc_mode = *state.discussion_mode.read().unwrap_or_else(|e| e.into_inner());

    let agent_ids: Vec<String> = state.agents.iter().map(|e| e.key().clone()).collect();

    for agent_id in &agent_ids {
        let (status, layer, has_process, assigned) = {
            let Some(a) = state.agents.get(agent_id) else { continue };
            (a.status, a.layer.clone(), a.process.is_some(), a.assigned_task_id.clone())
        };

        if status != AgentStatus::Idle || has_process || assigned.is_some() {
            continue;
        }
        if matches!(status, AgentStatus::WaitingDiscussion | AgentStatus::Discussing | AgentStatus::WaitingHumanReview) {
            continue;
        }

        // L4: check GPU
        if layer == "execution" {
            let can = state.gpu_allocator.lock().unwrap_or_else(|e| e.into_inner()).can_allocate();
            if !can { continue; }
        }

        let candidate_queues: Vec<&str> = if layer == "idea" {
            let mut v = vec!["init_to_idea"];
            if state.auto_loop { v.push("execution_feedback"); }
            v
        } else {
            match layer_input_queue(&layer) {
                Some(q) => vec![q],
                None => continue,
            }
        };

        'queue_loop: for queue_name in &candidate_queues {
            let task = {
                let Some(q) = state.queues.get(*queue_name) else { continue };
                let Some(t) = q.peek_pending() else { continue };
                if t.target_layer != layer { continue }
                let fail_count = state.fail_counts.get(&t.project_id).map(|v| *v).unwrap_or(0);
                if fail_count >= 3 { continue }
                t.clone()
            };

            // Assign task
            {
                let Some(mut q) = state.queues.get_mut(*queue_name) else { continue };
                if q.assign(&task.id, agent_id).is_none() { continue }
            }
            state.fail_counts.remove(&task.project_id);

            // Allocate GPU for execution
            if layer == "execution" {
                let _ = state.gpu_allocator.lock().unwrap_or_else(|e| e.into_inner()).allocate(&task.project_id);
            }

            let Some(mut ae) = state.agents.get_mut(agent_id) else { break 'queue_loop };
            let msgs = launch_agent_for_task(state, ae.value_mut(), &task, disc_mode);
            messages.extend(msgs);
            messages.push(msg_queue_update(state));
            break 'queue_loop;
        }
    }

    messages
}

// ---------------------------------------------------------------------------
// Startup artifact scan
// ---------------------------------------------------------------------------

fn scan_existing_artifacts(state: &BridgeState) -> Vec<Value> {
    let mut messages = Vec::new();
    let projects_dir = state.projects_dir();
    if !projects_dir.exists() {
        return messages;
    }

    let Ok(rd) = std::fs::read_dir(&projects_dir) else {
        return messages;
    };

    for entry in rd.filter_map(|e| e.ok()) {
        let proj_dir = entry.path();
        if !proj_dir.is_dir() {
            continue;
        }
        let project_id = proj_dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut seen = HashSet::new();

        // Collect run dirs
        let angle_dirs: Vec<PathBuf> = match std::fs::read_dir(&proj_dir) {
            Ok(rd2) => rd2
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("run-"))
                .map(|e| e.path())
                .collect(),
            Err(_) => Vec::new(),
        };
        let run_dirs: Vec<PathBuf> = if angle_dirs.is_empty() {
            vec![proj_dir.clone()]
        } else {
            angle_dirs
        };

        for run_dir in run_dirs {
            for s in 1u32..=22 {
                let stage_dir = run_dir.join(format!("stage-{s:02}"));
                if !stage_dir.is_dir() {
                    continue;
                }
                for &expected in stage_outputs(s) {
                    if !is_display_artifact(expected) {
                        continue;
                    }
                    let artifact_path = stage_dir.join(expected.trim_end_matches('/'));
                    let dedup = format!("{project_id}:{s}:{expected}");
                    if seen.contains(&dedup) || !artifact_path.exists() {
                        continue;
                    }
                    seen.insert(dedup);
                    let size = if artifact_path.is_dir() {
                        "dir".to_string()
                    } else {
                        format!("{:.1} KB", artifact_path.metadata().map(|m| m.len()).unwrap_or(0) as f64 / 1024.0)
                    };
                    let content = extract_artifact_summary(&artifact_path, expected);
                    messages.push(msg_artifact(
                        repo_for_stage(s),
                        expected,
                        &project_id,
                        &size,
                        &project_id,
                        &content,
                        Some(s),
                    ));
                }
            }
        }
    }
    messages
}

// ---------------------------------------------------------------------------
// Inbound command handler
// ---------------------------------------------------------------------------

async fn handle_command(state: &Arc<BridgeState>, data: Value) -> Vec<Value> {
    let cmd = data.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let mut messages = Vec::new();

    match cmd {
        "list_agents" => {
            for entry in state.agents.iter() {
                messages.push(msg_agent_update(entry.value()));
            }
            messages.push(msg_queue_update(state));
            messages.push(msg_project_list(list_all_projects(state)));
        }


        "resume_project" => {
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("");
            if !project_id.is_empty() {
                let proj_dir = state.projects_dir().join(project_id);
                if !proj_dir.exists() {
                    messages.push(msg_log_sys(&format!("Project [{project_id}] not found"), "error"));
                    return messages;
                }
                let meta = read_json(&proj_dir.join("project_meta.json"));
                let mut config_path = meta.as_ref().and_then(|m| m.get("config_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let topic = meta.as_ref().and_then(|m| m.get("topic")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mode = meta.as_ref().and_then(|m| m.get("mode")).and_then(|v| v.as_str()).unwrap_or("lab").to_string();
                // Fallback: recover config_path from project_configs/ if meta lost it
                if config_path.is_empty() {
                    let fallback = PathBuf::from(&state.runs_base_dir)
                        .join("project_configs")
                        .join(format!("{project_id}.yaml"));
                    if fallback.exists() {
                        config_path = fallback.canonicalize().unwrap_or(fallback).to_string_lossy().into_owned();
                        warn!("resume_project: recovered config_path from {config_path}");
                    }
                }
                let disc_mode = *state.discussion_mode.read().unwrap_or_else(|e| e.into_inner());
                state.fail_counts.remove(project_id);
                messages.extend(submit_new_project(state, project_id, &config_path, &topic, &mode, disc_mode));
                messages.extend(schedule_idle_agents(state));
                messages.push(msg_project_list(list_all_projects(state)));
            }
        }

        "pause_project" => {
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("");
            if !project_id.is_empty() {
                messages.extend(pause_project(state, project_id));
                messages.extend(schedule_idle_agents(state));
                messages.push(msg_project_list(list_all_projects(state)));
            }
        }

        "restart_project" => {
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("");
            if !project_id.is_empty() {
                let proj_dir = state.projects_dir().join(project_id);
                messages.extend(pause_project(state, project_id));
                // Clear run progress, keep meta
                if proj_dir.exists() {
                    if let Ok(rd) = std::fs::read_dir(&proj_dir) {
                        for entry in rd.filter_map(|e| e.ok()) {
                            if entry.file_name() != "project_meta.json" {
                                let _ = if entry.path().is_dir() {
                                    std::fs::remove_dir_all(entry.path())
                                } else {
                                    std::fs::remove_file(entry.path())
                                };
                            }
                        }
                    }
                }
                messages.push(msg_log_sys(&format!("Project [{project_id}] progress cleared, restarting..."), "info"));
                let meta = read_json(&proj_dir.join("project_meta.json"));
                let mut config_path = meta.as_ref().and_then(|m| m.get("config_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let topic = meta.as_ref().and_then(|m| m.get("topic")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mode = meta.as_ref().and_then(|m| m.get("mode")).and_then(|v| v.as_str()).unwrap_or("lab").to_string();
                // Fallback: recover config_path from project_configs/ if meta lost it
                if config_path.is_empty() {
                    let fallback = PathBuf::from(&state.runs_base_dir)
                        .join("project_configs")
                        .join(format!("{project_id}.yaml"));
                    if fallback.exists() {
                        config_path = fallback.canonicalize().unwrap_or(fallback).to_string_lossy().into_owned();
                        warn!("restart_project: recovered config_path from {config_path}");
                    }
                }
                let disc_mode = *state.discussion_mode.read().unwrap_or_else(|e| e.into_inner());
                messages.extend(submit_new_project(state, project_id, &config_path, &topic, &mode, disc_mode));
                messages.extend(schedule_idle_agents(state));
                messages.push(msg_project_list(list_all_projects(state)));
            }
        }

        "delete_project" => {
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("");
            messages.extend(delete_project(state, project_id));
            messages.push(msg_project_list(list_all_projects(state)));
        }

        "chat_input" => {
            let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let target_layer = data.get("targetLayer").and_then(|v| v.as_str()).unwrap_or("all");
            let intent = classify_chat_intent_keywords(&content);
            if intent == "approve" {
                // Advance all WaitingHumanReview agents
                let has_waiting = state.agents.iter().any(|e| e.value().status == AgentStatus::WaitingHumanReview);
                if has_waiting {
                    messages.extend(approve_waiting_agents(state));
                    messages.push(msg_feedback_ack(
                        &format!("ap-{}", uid()),
                        "✅ 审阅通过，正在推进管线到下一层…",
                        target_layer,
                    ));
                } else {
                    // No waiting agents — treat as feedback instead
                    let fb_id = format!("fb-{}", uid());
                    let (_injected, plan_hint) = inject_feedback(state, &content, target_layer, &fb_id);
                    messages.push(msg_feedback_ack(&fb_id, &plan_hint, target_layer));
                }
            } else if intent == "query" {
                let summary = build_status_summary(state, target_layer);
                messages.push(msg_feedback_ack(&format!("qs-{}", uid()), &summary, target_layer));
            } else {
                // Treat as feedback — record but do NOT advance agents
                let fb_id = format!("fb-{}", uid());
                messages.push(msg_log_sys(
                    &format!(
                        "收到人工反馈: {}{}",
                        &content.chars().take(80).collect::<String>(),
                        if content.chars().count() > 80 { "..." } else { "" }
                    ),
                    "info",
                ));
                let (_injected, plan_hint) = inject_feedback(state, &content, target_layer, &fb_id);
                messages.push(msg_feedback_ack(&fb_id, &plan_hint, target_layer));
            }
        }

        "quick_submit" => {
            let topic = data.get("topic").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mode = data.get("mode").and_then(|v| v.as_str()).unwrap_or("lab").to_string();
            let research_angles: Vec<String> = match data.get("researchAngles") {
                Some(Value::Array(arr)) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
                Some(Value::String(s)) if !s.trim().is_empty() => {
                    s.split(&[',', '\u{FF0C}', '\u{3001}', ';'][..])
                        .map(|a| a.trim().to_string())
                        .filter(|a| !a.is_empty())
                        .collect()
                }
                _ => Vec::new(),
            };
            let reference_papers: Vec<String> = match data.get("referencePapers") {
                Some(v) if v.is_array() => v
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
                Some(v) if v.is_string() => v
                    .as_str()
                    .unwrap()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                _ => Vec::new(),
            };
            let reference_uploads: Vec<Value> = data
                .get("referenceFiles")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let codebases_dir = data.get("codebasesDir").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let datasets_dir = data.get("datasetsDir").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let checkpoints_dir = data.get("checkpointsDir").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut path_overrides: HashMap<String, String> = HashMap::new();
            if !codebases_dir.is_empty() { path_overrides.insert("codebases_dir".to_string(), codebases_dir); }
            if !datasets_dir.is_empty() { path_overrides.insert("datasets_dir".to_string(), datasets_dir); }
            if !checkpoints_dir.is_empty() { path_overrides.insert("checkpoints_dir".to_string(), checkpoints_dir); }

            if topic.is_empty() {
                messages.push(msg_log_sys("请输入研究主题", "error"));
                return messages;
            }

            let base_id = if project_id.is_empty() {
                slugify(&topic, 40)
            } else {
                project_id
            };
            let base_id = {
                let existing = state.projects_dir().join(&base_id);
                if existing.exists() {
                    format!("{base_id}-{}", uid())
                } else {
                    base_id
                }
            };

            messages.extend(quick_submit_project(
                state,
                &topic,
                &base_id,
                &mode,
                &research_angles,
                &reference_papers,
                &reference_uploads,
                &path_overrides,
            ));
            messages.extend(schedule_idle_agents(state));
            messages.push(msg_project_list(list_all_projects(state)));
        }

        "set_discussion_mode" => {
            let enabled = data.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            *state.discussion_mode.write().unwrap_or_else(|e| e.into_inner()) = enabled;
            if let Some(rounds) = data.get("rounds").and_then(|v| v.as_u64()) {
                *state.discussion_rounds.write().unwrap_or_else(|e| e.into_inner()) = rounds as u32;
            }
        }

        "list_projects" => {
            messages.push(msg_project_list(list_all_projects(state)));
        }

        "get_queues" => {
            messages.push(msg_queue_update(state));
        }

        "add_lobster" => {
            let name = data
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("龙虾-{}", uid()));
            let layer = data.get("layer").and_then(|v| v.as_str()).unwrap_or("idea").to_string();
            let agent_id = create_agent(state, &name, &layer);
            if let Some(agent_ref) = state.agents.get(&agent_id) {
                messages.push(msg_agent_update(agent_ref.value()));
                messages.push(msg_log(
                    &agent_ref.id,
                    &agent_ref.name,
                    &agent_ref.layer,
                    None,
                    &format!("龙虾已加入 {} 层", layer),
                    "info",
                ));
            }
        }

        "remove_lobster" => {
            let agent_id = data.get("agentId").and_then(|v| v.as_str()).unwrap_or("");
            if let Some((_, mut agent)) = state.agents.remove(agent_id) {
                messages.extend(stop_agent(&mut agent));
            }
        }

        "submit_project" => {
            let project_id = data
                .get("projectId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("proj-{}", uid()));
            let config_path = data.get("configPath").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let topic = data.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let disc_mode = *state.discussion_mode.read().unwrap_or_else(|e| e.into_inner());
            messages.extend(submit_new_project(state, &project_id, &config_path, &topic, "lab", disc_mode));
            messages.extend(schedule_idle_agents(state));
            messages.push(msg_project_list(list_all_projects(state)));
        }

        "stop_agent" => {
            let agent_id = data.get("agentId").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(mut ae) = state.agents.get_mut(agent_id) {
                messages.extend(stop_agent(ae.value_mut()));
            }
        }

        "get_shared_results" => {
            let results_dir = PathBuf::from(&state.runs_base_dir).join("shared_results");
            let summary = if results_dir.exists() {
                let mut entries = Vec::new();
                if let Ok(rd) = std::fs::read_dir(&results_dir) {
                    for entry in rd.flatten() {
                        if entry.path().extension().map_or(false, |e| e == "json") {
                            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                                entries.push(content);
                            }
                        }
                    }
                }
                format!("{{\"count\":{},\"entries\":[{}]}}", entries.len(), entries.join(","))
            } else {
                "{}".to_string()
            };
            messages.push(msg_system(&summary));
        }

        "start_idea_factory" => {
            let topic = data.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let config_path = data.get("configPath").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let idea_count = data.get("ideaCount").and_then(|v| v.as_i64()).unwrap_or(0);
            *state.idea_factory_topic.write().unwrap_or_else(|e| e.into_inner()) = topic.clone();
            *state.idea_factory_config.write().unwrap_or_else(|e| e.into_inner()) = config_path;
            *state.idea_factory_remaining.write().unwrap_or_else(|e| e.into_inner()) = idea_count;
            state.idea_factory_produced.store(0, Ordering::Relaxed);
            let label = if idea_count == -1 { "无限".to_string() } else { idea_count.to_string() };
            messages.push(msg_log_sys(
                &format!("Idea 工厂已启动: topic={}... count={}", &topic.chars().take(50).collect::<String>(), label),
                "info",
            ));
        }

        "stop_idea_factory" => {
            *state.idea_factory_remaining.write().unwrap_or_else(|e| e.into_inner()) = 0;
            let produced = state.idea_factory_produced.load(Ordering::Relaxed);
            messages.push(msg_log_sys(
                &format!("Idea 工厂已停止 (已产出 {} 个)", produced),
                "info",
            ));
        }

        "query_status" => {
            let target_layer = data.get("targetLayer").and_then(|v| v.as_str()).unwrap_or("all");
            let summary = build_status_summary(state, target_layer);
            messages.push(msg_feedback_ack(&format!("qs-{}", uid()), &summary, target_layer));
        }

        "human_feedback" => {
            let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let target_layer = data.get("targetLayer").and_then(|v| v.as_str()).unwrap_or("all").to_string();
            let message_id = data
                .get("messageId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("fb-{}", uid()));

            messages.push(msg_log_sys(
                &format!(
                    "收到人工反馈: {}{}",
                    &content.chars().take(80).collect::<String>(),
                    if content.chars().count() > 80 { "..." } else { "" }
                ),
                "info",
            ));

            let (_injected, plan_hint) = inject_feedback(state, &content, &target_layer, &message_id);
            messages.push(msg_feedback_ack(&message_id, &plan_hint, &target_layer));
        }

        "get_download_url" => {
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let filename = data
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("latex_package.zip")
                .to_string();
            if !project_id.is_empty() {
                if !is_safe_path_component(&project_id) {
                    messages.push(msg_log_sys("无效的项目 ID", "error"));
                    return messages;
                }
                if !is_safe_path_component(&filename) {
                    messages.push(msg_log_sys("无效的文件名", "error"));
                    return messages;
                }
                // Search for the file in the project's run directories
                let proj_dir = state.projects_dir().join(&project_id);
                let mut found_path: Option<String> = None;
                if let Ok(rd) = std::fs::read_dir(&proj_dir) {
                    // Check sub-run dirs (run-*/) and then stage dirs
                    let mut dirs: Vec<_> = rd.filter_map(|e| e.ok()).collect();
                    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
                    for entry in &dirs {
                        let p = entry.path().join(&filename);
                        if p.exists() {
                            found_path = Some(format!("projects/{}/{}/{}", project_id, entry.file_name().to_string_lossy(), filename));
                            break;
                        }
                        // Search in stage-* subdirectories (highest first)
                        if let Ok(sub_rd) = std::fs::read_dir(entry.path()) {
                            let mut sub_dirs: Vec<_> = sub_rd.filter_map(|e| e.ok()).collect();
                            sub_dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
                            for sub in &sub_dirs {
                                let sp = sub.path().join(&filename);
                                if sp.exists() {
                                    found_path = Some(format!("projects/{}/{}/{}/{}", project_id,
                                        entry.file_name().to_string_lossy(),
                                        sub.file_name().to_string_lossy(), filename));
                                    break;
                                }
                            }
                            if found_path.is_some() { break; }
                        }
                    }
                }
                // Also check project root directly
                if found_path.is_none() {
                    let direct = proj_dir.join(&filename);
                    if direct.exists() {
                        found_path = Some(format!("projects/{}/{}", project_id, filename));
                    }
                }
                let url = found_path
                    .map(|p| format!("/download/{}", p))
                    .unwrap_or_else(|| format!("/download/projects/{}/{}", project_id, filename));
                messages.push(serde_json::json!({
                    "type": "download_url",
                    "payload": {
                        "projectId": project_id,
                        "filename": filename,
                        "url": url,
                    }
                }));
            }
        }

        _ => {
            if !cmd.is_empty() {
                warn!("Unknown command: {cmd}");
            }
        }
    }

    messages
}

/// Shared feedback injection: logs, saves to global feedback store, writes per-agent
/// `human_feedback.jsonl`, and returns (injected_project_ids, plan_hint_message).
fn inject_feedback(
    state: &BridgeState,
    content: &str,
    target_layer: &str,
    message_id: &str,
) -> (Vec<String>, String) {
    save_feedback(state, content, target_layer, message_id);

    let injected_projects: Vec<String> = state
        .agents
        .iter()
        .filter(|e| {
            let a = e.value();
            !a.run_dir.is_empty()
                && matches!(a.status, AgentStatus::Working | AgentStatus::Idle | AgentStatus::WaitingHumanReview | AgentStatus::WaitingDiscussion)
                && (target_layer == "all" || a.layer == target_layer)
                && !a.project_id.is_empty()
        })
        .map(|e| e.value().project_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Write feedback to each matching agent's run_dir/human_feedback.jsonl
    for entry in state.agents.iter() {
        let a = entry.value();
        if a.run_dir.is_empty() {
            continue;
        }
        if !matches!(a.status, AgentStatus::Working | AgentStatus::Idle | AgentStatus::WaitingHumanReview | AgentStatus::WaitingDiscussion) {
            continue;
        }
        if target_layer != "all" && a.layer != target_layer {
            continue;
        }
        let run_dir = PathBuf::from(&a.run_dir);
        if !run_dir.exists() {
            continue;
        }
        let fb_path = run_dir.join("human_feedback.jsonl");
        let entry_json = serde_json::json!({
            "id": message_id,
            "content": content,
            "targetLayer": target_layer,
            "timestamp": now_ms(),
        });
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&fb_path) {
            use std::io::Write;
            let _ = writeln!(f, "{}", serde_json::to_string(&entry_json).unwrap_or_default());
        }
    }

    let plan_hint = if !injected_projects.is_empty() {
        let mut sorted = injected_projects.clone();
        sorted.sort();
        format!(
            "已将反馈注入 {} 个项目的 prompt 上下文中 ({})。当前阶段完成后，下一个阶段的 LLM 将读取并参考你的反馈来调整执行计划。",
            sorted.len(),
            sorted.join(", ")
        )
    } else {
        "已记录反馈。当前无匹配的运行中项目，反馈将在新任务启动时生效。".to_string()
    };

    (injected_projects, plan_hint)
}

fn save_feedback(state: &BridgeState, content: &str, target_layer: &str, message_id: &str) {
    let feedback_dir = PathBuf::from(&state.runs_base_dir).join("feedback");
    let _ = std::fs::create_dir_all(&feedback_dir);
    let entry = serde_json::json!({
        "id": message_id,
        "content": content,
        "targetLayer": target_layer,
        "timestamp": now_ms(),
    });
    let log_path = feedback_dir.join("feedback_log.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        use std::io::Write;
        let _ = writeln!(f, "{}", serde_json::to_string(&entry).unwrap_or_default());
    }
    let _ = write_json(&feedback_dir.join("latest_feedback.json"), &entry);
}

fn build_status_summary(state: &BridgeState, _target_layer: &str) -> String {
    let projects = list_all_projects(state);
    if projects.is_empty() {
        return "当前没有任何项目。".to_string();
    }
    let mut lines = Vec::new();
    for p in projects.iter().take(5) {
        let pid = p["projectId"].as_str().unwrap_or("");
        let status_raw = p["status"].as_str().unwrap_or("");
        let status_cn = match status_raw {
            "running" | "working" => "运行中",
            "idle" => "空闲",
            "done" | "completed" => "已完成",
            "error" | "failed" => "错误",
            "paused" => "已暂停",
            "waiting_human_review" => "等待审阅",
            "waiting_discussion" => "等待讨论",
            "discussing" => "讨论中",
            other => other,
        };
        let stage = p["lastCompletedStage"].as_u64().unwrap_or(0);
        let layer_name = match stage {
            0..=4 => "想法层",
            5..=8 => "实验层",
            9..=14 => "编码层",
            15..=18 => "执行层",
            19..=22 => "写作层",
            _ => "未知层",
        };
        lines.push(format!("项目 {pid}：{status_cn}（阶段 {stage}/22，{layer_name}）"));
    }
    lines.join("\n")
}

/// Advance all agents in WaitingHumanReview state.
///
/// For agents with a stored pending transition, create the follow-up task
/// so the pipeline proceeds to the next layer. For agents waiting at the
/// discussion gate (no pending transition), skip discussion and proceed to S8.
fn approve_waiting_agents(state: &BridgeState) -> Vec<Value> {
    let mut messages = Vec::new();

    let waiting_ids: Vec<String> = state
        .agents
        .iter()
        .filter(|e| e.value().status == AgentStatus::WaitingHumanReview)
        .map(|e| e.key().clone())
        .collect();

    if waiting_ids.is_empty() {
        messages.push(msg_log_sys("当前没有等待审阅的任务。", "info"));
        return messages;
    }

    for agent_id in &waiting_ids {
        if let Some((_k, pt)) = state.pending_transitions.remove(agent_id) {
            // Layer boundary transition: create follow-up task
            if let Some((_, target_layer)) = queue_layers(&pt.output_queue) {
                let follow_task = Task {
                    id: format!("task-{}", uid()),
                    project_id: pt.project_id.clone(),
                    run_dir: pt.run_dir.clone(),
                    config_path: pt.config_path.clone(),
                    topic: pt.topic.clone(),
                    source_layer: pt.source_layer.clone(),
                    target_layer: target_layer.to_string(),
                    status: TaskStatus::Pending,
                    assigned_to: None,
                    created_at: now_ms(),
                    assigned_at: 0,
                    completed_at: 0,
                };
                if let Some(mut q) = state.queues.get_mut(&pt.output_queue) {
                    q.push(follow_task);
                }
                messages.push(msg_log_sys(
                    &format!("人工审阅通过 → 项目 [{}] 推进到 {target_layer} 层", pt.project_id),
                    "success",
                ));
            }
            // Reset agent to idle so it can pick up the new task
            if let Some(mut ae) = state.agents.get_mut(agent_id) {
                ae.value_mut().reset_idle();
                messages.push(msg_agent_update(ae.value()));
            }
        } else {
            // Discussion gate: skip discussion and proceed to S8
            messages.extend(skip_discussion_proceed_s8(state, agent_id));
            messages.push(msg_log_sys("人工审阅通过 → 跳过讨论，推进到实验层", "success"));
        }
    }

    messages.push(msg_queue_update(state));
    messages.extend(schedule_idle_agents(state));
    messages
}

fn is_safe_path_component(s: &str) -> bool {
    !s.is_empty() && !s.contains("..") && !s.contains('/') && !s.contains('\\')
}

fn slugify(text: &str, max_len: usize) -> String {
    let lower = text.to_lowercase();
    let slug: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.chars().take(max_len).collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

// ---------------------------------------------------------------------------
// Poll loop
// ---------------------------------------------------------------------------

pub async fn poll_loop(state: Arc<BridgeState>, interval_secs: f64) {
    let duration = std::time::Duration::from_secs_f64(interval_secs);
    loop {
        tokio::time::sleep(duration).await;
        let mut all_messages: Vec<Value> = Vec::new();

        let agent_ids: Vec<String> = state.agents.iter().map(|e| e.key().clone()).collect();
        let mut done_agents: Vec<String> = Vec::new();
        let mut error_agents: Vec<String> = Vec::new();

        for agent_id in &agent_ids {
            let prev_status = state.agents.get(agent_id).map(|a| a.status);
            let mut msgs = {
                let Some(mut ae) = state.agents.get_mut(agent_id.as_str()) else { continue };
                poll_agent(ae.value_mut())
            };
            all_messages.append(&mut msgs);

            let new_status = state.agents.get(agent_id).map(|a| a.status);
            if prev_status == Some(AgentStatus::Working) && new_status == Some(AgentStatus::Done) {
                let is_s7_only = state.agents.get(agent_id).map(|a| a.is_idea_factory_s7_only).unwrap_or(false);
                let is_factory = state.agents.get(agent_id).map(|a| a.is_idea_factory).unwrap_or(false);
                let is_disc_s8 = state.agents.get(agent_id).map(|a| a.is_discussion_s8).unwrap_or(false);

                if is_disc_s8 {
                    all_messages.extend(on_discussion_s8_done(&state, agent_id));
                } else if is_s7_only {
                    all_messages.extend(_on_idea_factory_s7_done(&state, agent_id));
                } else if is_factory {
                    all_messages.extend(_on_idea_factory_done(&state, agent_id));
                } else {
                    done_agents.push(agent_id.clone());
                }
            }

            if prev_status == Some(AgentStatus::Working) && new_status == Some(AgentStatus::Error) {
                error_agents.push(agent_id.clone());
            }
        }

        for aid in done_agents {
            all_messages.extend(on_agent_done(&state, &aid));
        }

        for aid in &error_agents {
            let project_id = state.agents.get(aid).map(|a| a.project_id.clone()).unwrap_or_default();
            if !project_id.is_empty() {
                let mut count = state.fail_counts.entry(project_id.clone()).or_insert(0);
                *count += 1;
                let n = *count;
                if let Some(mut ae) = state.agents.get_mut(aid) {
                    let task_id = ae.value().assigned_task_id.clone();
                    if let Some(tid) = task_id {
                        for mut q in state.queues.iter_mut() {
                            q.value_mut().fail(&tid);
                        }
                    }
                    ae.value_mut().reset_idle();
                    all_messages.push(msg_agent_update(ae.value()));
                }
                if n >= 3 {
                    all_messages.push(msg_log_sys(
                        &format!("Project [{project_id}] failed {n} consecutive times. Manual intervention required."),
                        "error",
                    ));
                }
            }
        }

        // Poll discussion groups
        let group_keys: Vec<String> = state.discussion_groups.iter().map(|e| e.key().clone()).collect();
        for key in group_keys {
            all_messages.extend(poll_discussion(&state, &key));
        }

        // Schedule idle agents
        all_messages.extend(schedule_idle_agents(&state));

        // Periodic project list broadcast (~10 cycles)
        let counter = state.poll_counter.fetch_add(1, Ordering::Relaxed);
        if counter % 10 == 0 {
            all_messages.push(msg_project_list(list_all_projects(&state)));
        }

        state.broadcast_msgs(all_messages);
    }
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<BridgeState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<BridgeState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.broadcast_tx.subscribe();

    // Send initial state on connect
    {
        let mut init_msgs: Vec<Value> = Vec::new();
        for entry in state.agents.iter() {
            init_msgs.push(msg_agent_update(entry.value()));
        }
        init_msgs.push(msg_queue_update(&state));
        init_msgs.extend(scan_existing_artifacts(&state));
        init_msgs.push(msg_project_list(list_all_projects(&state)));

        for msg in &init_msgs {
            if let Ok(s) = serde_json::to_string(msg) {
                if sender.send(Message::Text(s.into())).await.is_err() {
                    return;
                }
            }
        }
    }

    info!("Client connected to agent bridge");

    // Spawn broadcast forwarder
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle inbound messages
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    let responses = handle_command(&state_clone, data).await;
                    state_clone.broadcast_msgs(responses);
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
    info!("Client disconnected from agent bridge");
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_router(state: Arc<BridgeState>) -> Router {
    Router::new()
        .route("/ws/agents", get(ws_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Intent classification
// ---------------------------------------------------------------------------

/// Classify chat input as "query" or "feedback" using Chinese/English keywords.
///
/// Query indicators: 状态/进度/进展/情况/怎么/如何/多少/哪些/什么/完成/结果/有没有
///                   plus English words: status/progress/how/what/which/done/result
///                   Boosted by trailing ? / ？ and particles 吗/呢.
/// Feedback indicators: 请/应该/建议/改/修/调/优化/停/暂停/重试/加速
///                      plus English: please/should/suggest/change/stop/pause/retry/adjust
///                      Boosted when len > 80.
///
/// Returns "query" or "feedback".
pub fn classify_chat_intent_keywords(text: &str) -> &'static str {
    let lower = text.to_lowercase();

    // Check for approve/proceed intent first — highest priority
    let approve_keywords_cn = ["继续", "通过", "批准", "下一步", "推进", "开始", "确认"];
    let approve_keywords_en = ["continue", "proceed", "approve", "lgtm", "go ahead", "next", "ok"];
    for kw in &approve_keywords_cn {
        if text.contains(kw) {
            return "approve";
        }
    }
    for kw in &approve_keywords_en {
        if lower.contains(kw) {
            return "approve";
        }
    }

    let query_keywords_cn = [
        "状态", "进度", "进展", "情况", "怎么", "如何", "多少", "哪些", "什么", "完成",
        "结果", "有没有",
    ];
    let query_keywords_en = [
        "status", "progress", "how", "what", "which", "done", "result",
    ];
    let feedback_keywords_cn = [
        "请", "应该", "建议", "改", "修", "调", "优化", "停", "暂停", "重试", "加速",
    ];
    let feedback_keywords_en = [
        "please", "should", "suggest", "change", "stop", "pause", "retry", "adjust",
    ];

    let mut query_score: i32 = 0;
    let mut feedback_score: i32 = 0;

    for kw in &query_keywords_cn {
        if text.contains(kw) {
            query_score += 2;
        }
    }
    for kw in &query_keywords_en {
        if lower.contains(kw) {
            query_score += 1;
        }
    }
    for kw in &feedback_keywords_cn {
        if text.contains(kw) {
            feedback_score += 2;
        }
    }
    for kw in &feedback_keywords_en {
        if lower.contains(kw) {
            feedback_score += 1;
        }
    }

    // Boost query score for question suffixes and particles
    if text.ends_with('?') || text.ends_with('？') {
        query_score += 3;
    }
    if text.ends_with("吗") || text.ends_with("呢") {
        query_score += 2;
    }

    // Boost feedback score for long instructive text
    if text.chars().count() > 80 {
        feedback_score += 2;
    }

    // Default to feedback unless there's a clear query signal.
    // Short messages with no keywords should be treated as feedback, not query.
    if query_score > 0 && query_score > feedback_score {
        "query"
    } else {
        "feedback"
    }
}

// ---------------------------------------------------------------------------
// S12 sanity check
// ---------------------------------------------------------------------------

/// Check if S12 sanity_report.json shows "fail" status.
/// If so, read fix_log.json, build a Chinese error detail, persist intervention
/// reason to project_meta.json, pause the project, and return notification messages.
pub fn check_s12_sanity_failure(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let (project_id, run_dir) = {
        let Some(a) = state.agents.get(agent_id) else {
            return messages;
        };
        (a.project_id.clone(), a.run_dir.clone())
    };

    if project_id.is_empty() || run_dir.is_empty() {
        return messages;
    }

    let sanity_path = PathBuf::from(&run_dir).join("stage-12/sanity_report.json");
    if !sanity_path.exists() {
        return messages;
    }

    let Some(report) = read_json(&sanity_path) else {
        return messages;
    };

    if report.get("status").and_then(|v| v.as_str()) != Some("fail") {
        return messages;
    }

    // Read fix_log.json for error details
    let fix_log_path = PathBuf::from(&run_dir).join("stage-12/fix_log.json");
    let error_detail = if let Some(fix_log) = read_json(&fix_log_path) {
        if let Some(arr) = fix_log.as_array() {
            if let Some(last) = arr.last() {
                last.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知错误")
                    .chars()
                    .take(200)
                    .collect::<String>()
            } else {
                "未知错误".to_string()
            }
        } else {
            fix_log
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误")
                .chars()
                .take(200)
                .collect()
        }
    } else {
        "未知错误".to_string()
    };

    // Find experiment dir from stage-11
    let exp_dir = PathBuf::from(&run_dir).join("stage-11/experiment");
    let exp_dir_str = if exp_dir.exists() {
        exp_dir.to_string_lossy().into_owned()
    } else {
        format!("{}/stage-11", run_dir)
    };

    let intervention_reason = format!(
        "S12 代码验收失败，循环修复次数耗尽。实验目录：{}。错误详情：{}",
        exp_dir_str, error_detail
    );

    // Persist intervention reason to project_meta.json
    let meta_path = PathBuf::from(&run_dir).join("project_meta.json");
    if let Some(mut meta) = read_json(&meta_path).or_else(|| Some(serde_json::json!({}))) {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "intervention".to_string(),
                Value::String(intervention_reason.clone()),
            );
            obj.insert("paused_at".to_string(), Value::Number(now_ms().into()));
        }
        let _ = write_json(&meta_path, &meta);
    }

    messages.push(msg_log_sys(
        &format!(
            "S12 SANITY_CHECK 验收失败，项目 [{}] 需要人工干预",
            project_id
        ),
        "error",
    ));
    messages.push(msg_system(&format!(
        "验收完成，代码修复失败：{}\n{}",
        project_id, error_detail
    )));

    messages.extend(pause_project(state, &project_id));
    messages
}

// ---------------------------------------------------------------------------
// Passthrough agent
// ---------------------------------------------------------------------------

/// For passthrough layers: scan stage dirs in layer range, mark existing ones
/// as "completed" with artifacts, mark missing ones as "failed". Set agent
/// status to "done".
pub fn passthrough_agent(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();

    let (layer, run_dir, project_id) = {
        let Some(a) = state.agents.get(agent_id) else {
            return messages;
        };
        (a.layer.clone(), a.run_dir.clone(), a.project_id.clone())
    };

    let (range_start, range_end) = layer_range(&layer);
    let run_dir_path = PathBuf::from(&run_dir);

    {
        let Some(mut ae) = state.agents.get_mut(agent_id) else {
            return messages;
        };
        let agent = ae.value_mut();

        for s in range_start..=range_end {
            let stage_dir = run_dir_path.join(format!("stage-{s:02}"));
            if stage_dir.is_dir() {
                agent.stage_progress.insert(s, "completed".to_string());
                messages.push(msg_stage_update(agent_id, s, "completed"));
                messages.push(msg_log_agent(
                    agent,
                    &format!("{} passthrough completed", stage_name(s)),
                    "success",
                ));

                // Emit display artifacts
                for &expected in stage_outputs(s) {
                    let artifact_path = stage_dir.join(expected.trim_end_matches('/'));
                    if !artifact_path.exists() || !is_display_artifact(expected) {
                        continue;
                    }
                    let key = format!("{s}:{expected}");
                    if agent.known_artifacts.contains(&key) {
                        continue;
                    }
                    agent.known_artifacts.insert(key);
                    let size = if artifact_path.is_dir() {
                        "dir".to_string()
                    } else {
                        format!(
                            "{:.1} KB",
                            artifact_path.metadata().map(|m| m.len()).unwrap_or(0) as f64
                                / 1024.0
                        )
                    };
                    let content = extract_artifact_summary(&artifact_path, expected);
                    messages.push(msg_artifact(
                        repo_for_stage(s),
                        expected,
                        &agent.name,
                        &size,
                        &project_id,
                        &content,
                        Some(s),
                    ));
                }
            } else {
                agent.stage_progress.insert(s, "failed".to_string());
                messages.push(msg_stage_update(agent_id, s, "failed"));
            }
        }

        agent.status = AgentStatus::Done;
        agent.current_task = String::new();
        agent.current_stage = None;
        messages.push(msg_agent_update(agent));
        messages.push(msg_log_agent(
            agent,
            &format!("Passthrough complete for layer {}", layer),
            "info",
        ));
    }

    messages
}

// ---------------------------------------------------------------------------
// Config generation from template
// ---------------------------------------------------------------------------

/// Generate project YAML config from config_template.yaml.
/// Replaces __PROJECT_ID__, __TOPIC__, __REFERENCE_PAPERS__ placeholders,
/// updates path overrides with regex, and saves to project_configs/{project_id}.yaml.
pub fn generate_config_from_template(
    state: &BridgeState,
    project_id: &str,
    topic: &str,
    role_prompt: &str,
    reference_papers: &[String],
    codebases_dir: &str,
    datasets_dir: &str,
    checkpoints_dir: &str,
) -> anyhow::Result<String> {
    use regex::Regex;

    // Find template file
    let template_path = {
        let candidates = [
            PathBuf::from(&state.runs_base_dir).join("config_template.yaml"),
            PathBuf::from(&state.agent_package_dir).join("config_template.yaml"),
            PathBuf::from(&state.runs_base_dir)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("config_template.yaml"),
            PathBuf::from("examples/config_template.yaml"),
        ];
        candidates
            .into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("配置生成失败: config_template.yaml not found"))?
    };

    let template_text = std::fs::read_to_string(&template_path)
        .map_err(|e| anyhow::anyhow!("配置生成失败: {e}"))?;

    // Replace simple placeholders
    // For reference_papers, replace the ENTIRE line (matching Python behavior):
    //   template: "  reference_papers: __REFERENCE_PAPERS__"
    //   output:   "  reference_papers:\n    - \"paper1\"\n    - \"paper2\""
    // or if empty: "  reference_papers: []"
    let papers_replacement = if reference_papers.is_empty() {
        "  reference_papers: []".to_string()
    } else {
        let yaml_list = reference_papers
            .iter()
            .map(|p| format!("    - \"{}\"", p.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join("\n");
        format!("  reference_papers:\n{yaml_list}")
    };

    let mut config_text = template_text
        .replace("__PROJECT_ID__", project_id)
        .replace("__TOPIC__", topic)
        .replace("__ROLE_PROMPT__", role_prompt)
        .replace("  reference_papers: __REFERENCE_PAPERS__", &papers_replacement);

    // Update path overrides with regex
    let path_updates = [
        ("codebases_dir", codebases_dir),
        ("datasets_dir", datasets_dir),
        ("checkpoints_dir", checkpoints_dir),
    ];
    for (key, val) in &path_updates {
        if !val.is_empty() {
            let re = Regex::new(&format!(r"(?m)^(\s*{key}\s*:\s*).*$"))
                .map_err(|e| anyhow::anyhow!("regex error: {e}"))?;
            config_text = re
                .replace_all(&config_text, &format!("${{1}}{val}"))
                .into_owned();
        }
    }

    // Save to project_configs/{project_id}.yaml
    let configs_dir = PathBuf::from(&state.runs_base_dir).join("project_configs");
    std::fs::create_dir_all(&configs_dir)
        .map_err(|e| anyhow::anyhow!("配置生成失败: {e}"))?;
    let out_path = configs_dir.join(format!("{project_id}.yaml"));
    std::fs::write(&out_path, &config_text)
        .map_err(|e| anyhow::anyhow!("配置生成失败: {e}"))?;

    // Return canonical (absolute) path so child processes can find it
    // regardless of their working directory
    let abs_path = out_path.canonicalize().unwrap_or(out_path);
    Ok(abs_path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Reference upload helpers
// ---------------------------------------------------------------------------

/// Sanitize an upload filename: keep only ASCII [\w.\-], ensure .pdf suffix.
/// Non-ASCII characters (e.g., Chinese) are replaced with underscores.
pub fn safe_reference_upload_name(filename: &str) -> String {
    let sanitized: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Collapse runs of underscores/dots into single underscore
    let mut result = String::with_capacity(sanitized.len());
    let mut last_was_sep = false;
    for c in sanitized.chars() {
        if c == '_' {
            if !last_was_sep {
                result.push('_');
            }
            last_was_sep = true;
        } else {
            result.push(c);
            last_was_sep = false;
        }
    }
    let result = result.trim_matches('_').to_string();
    let result = if result.is_empty() { "upload".to_string() } else { result };

    if result.to_lowercase().ends_with(".pdf") {
        result
    } else {
        format!("{}.pdf", result)
    }
}

/// Save base64-encoded PDF uploads to {project_dir}/reference_uploads/ directory.
/// Returns list of saved file paths.
pub fn persist_reference_uploads(project_dir: &Path, reference_uploads: &[Value]) -> Vec<String> {
    use base64::Engine;

    let uploads_dir = project_dir.join("reference_uploads");
    if let Err(e) = std::fs::create_dir_all(&uploads_dir) {
        warn!("Failed to create reference_uploads dir: {e}");
        return Vec::new();
    }

    let mut saved_paths = Vec::new();
    for upload in reference_uploads {
        let filename = upload
            .get("name")
            .or_else(|| upload.get("filename"))
            .and_then(|v| v.as_str())
            .unwrap_or("upload.pdf");
        let data_b64 = upload
            .get("contentBase64")
            .or_else(|| upload.get("data"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if data_b64.is_empty() {
            continue;
        }

        // Strip data URI prefix if present (data:application/pdf;base64,...)
        let raw_b64 = if let Some(idx) = data_b64.find(',') {
            &data_b64[idx + 1..]
        } else {
            data_b64
        };

        match base64::engine::general_purpose::STANDARD.decode(raw_b64) {
            Ok(bytes) => {
                let safe_name = safe_reference_upload_name(filename);
                let dest = uploads_dir.join(&safe_name);
                match std::fs::write(&dest, &bytes) {
                    Ok(_) => {
                        info!("Saved reference upload: {}", dest.display());
                        saved_paths.push(dest.to_string_lossy().into_owned());
                    }
                    Err(e) => {
                        warn!("Failed to write reference upload {safe_name}: {e}");
                    }
                }
            }
            Err(e) => {
                warn!("Failed to decode base64 for {filename}: {e}");
            }
        }
    }
    saved_paths
}

// ---------------------------------------------------------------------------
// Known lab angles and role prompt builder
// ---------------------------------------------------------------------------

/// Maps research angle name → Chinese role prompt.
pub fn known_lab_angles() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert(
        "CV",
        "你是一名计算机视觉研究员，专注于图像识别、目标检测、分割等视觉感知任务。\
你善于分析视觉模型的架构设计，理解卷积神经网络、Transformer等视觉骨干网络的原理，\
并能结合具体任务提出创新性的视觉算法方案。",
    );
    map.insert(
        "VLM",
        "你是一名视觉语言模型研究员，专注于多模态理解与生成任务。\
你熟悉CLIP、BLIP、LLaVA等视觉语言模型，理解视觉与语言的对齐机制，\
并能针对多模态场景提出创新性的跨模态融合方案。",
    );
    map.insert(
        "World Model",
        "你是一名世界模型研究员，专注于环境建模、预测与规划。\
你深入理解基于学习的世界模型（如Dreamer、RSSM），擅长将世界模型与强化学习、\
机器人规划结合，并能提出提升模型泛化与样本效率的创新方法。",
    );
    map.insert(
        "VLA",
        "你是一名视觉-语言-动作（VLA）模型研究员，专注于机器人具身智能。\
你熟悉RT-2、OpenVLA等端到端机器人学习框架，理解如何将视觉感知、语言指令与\
机器人动作控制统一建模，并能提出提升机器人泛化能力的创新方案。",
    );
    map
}

/// Return the known role prompt for the given angle, or generate a generic Chinese prompt.
pub fn build_role_prompt(angle_name: &str, main_topic: &str) -> String {
    let angles = known_lab_angles();
    if let Some(&prompt) = angles.get(angle_name) {
        return prompt.to_string();
    }
    format!(
        "你是一名{}领域的研究员，专注于{}相关的前沿研究。\
你善于分析该领域的最新进展，能够提出创新性的研究方案，并结合实际应用场景给出切实可行的技术路线。",
        angle_name, main_topic
    )
}

// ---------------------------------------------------------------------------
// Quick submit project (full implementation)
// ---------------------------------------------------------------------------

/// Full project submission supporting "reproduce" and "lab" modes.
///
/// - "reproduce" mode: single-agent pipeline.
/// - "lab" mode: multi-angle parallel research with role prompts.
#[allow(clippy::too_many_arguments)]
pub fn quick_submit_project(
    state: &BridgeState,
    topic: &str,
    project_id: &str,
    mode: &str,
    research_angles: &[String],
    reference_papers: &[String],
    reference_uploads: &[Value],
    path_overrides: &HashMap<String, String>,
) -> Vec<Value> {
    let mut messages = Vec::new();

    let proj_dir = state.projects_dir().join(project_id);
    let _ = std::fs::create_dir_all(&proj_dir);

    // Persist reference uploads
    if !reference_uploads.is_empty() {
        let saved = persist_reference_uploads(&proj_dir, reference_uploads);
        if !saved.is_empty() {
            messages.push(msg_log_sys(
                &format!("已保存 {} 个参考文献上传文件", saved.len()),
                "info",
            ));
        }
    }

    let codebases_dir = path_overrides.get("codebases_dir").map(|s| s.as_str()).unwrap_or("");
    let datasets_dir = path_overrides.get("datasets_dir").map(|s| s.as_str()).unwrap_or("");
    let checkpoints_dir = path_overrides.get("checkpoints_dir").map(|s| s.as_str()).unwrap_or("");

    let disc_mode = *state.discussion_mode.read().unwrap_or_else(|e| e.into_inner());

    match mode {
        "reproduce" => {
            // Single-agent reproduce pipeline
            let role_prompt = "";
            let config_path = generate_config_from_template(
                state,
                project_id,
                topic,
                role_prompt,
                reference_papers,
                codebases_dir,
                datasets_dir,
                checkpoints_dir,
            )
            .unwrap_or_else(|e| {
                warn!("{e}");
                String::new()
            });

            messages.push(msg_log_sys(
                &format!("项目 [{project_id}] 复现模式提交中…"),
                "info",
            ));
            messages.extend(submit_new_project(
                state,
                project_id,
                &config_path,
                topic,
                "reproduce",
                false,
            ));
        }

        "lab" | _ => {
            // Lab mode: one sub-agent per research angle
            let effective_angles: Vec<String> = if research_angles.is_empty() {
                vec!["CV".to_string()]
            } else {
                research_angles.to_vec()
            };

            state
                .lab_batches
                .insert(project_id.to_string(), effective_angles.len());

            messages.push(msg_log_sys(
                &format!(
                    "项目 [{project_id}] Lab 模式，{} 个研究方向并行",
                    effective_angles.len()
                ),
                "info",
            ));

            for angle in &effective_angles {
                let sub_id = format!("{}-{}", project_id, slugify(angle, 20));
                let sub_dir = proj_dir.join(format!("run-{}", slugify(angle, 20)));
                let _ = std::fs::create_dir_all(&sub_dir);

                let role_prompt = build_role_prompt(angle, topic);
                let angled_topic = format!("[{angle}] {topic}");

                let config_path = generate_config_from_template(
                    state,
                    &sub_id,
                    &angled_topic,
                    &role_prompt,
                    reference_papers,
                    codebases_dir,
                    datasets_dir,
                    checkpoints_dir,
                )
                .unwrap_or_else(|e| {
                    warn!("{e}");
                    String::new()
                });

                let run_dir = sub_dir.canonicalize().unwrap_or(sub_dir.clone()).to_string_lossy().into_owned();
                save_project_meta(&run_dir, project_id, &config_path, &angled_topic, "lab");

                let task = Task {
                    id: format!("task-{}", uid()),
                    project_id: project_id.to_string(),
                    run_dir,
                    config_path,
                    topic: angled_topic.clone(),
                    source_layer: "init".to_string(),
                    target_layer: "idea".to_string(),
                    status: TaskStatus::Pending,
                    assigned_to: None,
                    created_at: now_ms(),
                    assigned_at: 0,
                    completed_at: 0,
                };

                if let Some(mut q) = state.queues.get_mut("init_to_idea") {
                    q.push(task);
                    messages.push(msg_log_sys(
                        &format!("研究方向 [{angle}] 已加入队列 (sub_id={sub_id})"),
                        "info",
                    ));
                } else {
                    warn!("Queue 'init_to_idea' not found — task for angle [{angle}] dropped!");
                    messages.push(msg_log_sys(
                        &format!("ERROR: 队列 init_to_idea 未找到, 方向 [{angle}] 任务丢失"),
                        "error",
                    ));
                }

                messages.push(msg_log_sys(
                    &format!("研究方向 [{angle}] 已提交 (sub_id={sub_id})"),
                    "info",
                ));
            }

            let _ = disc_mode; // discussion mode handled by on_agent_done
            messages.push(msg_queue_update(state));
        }
    }

    messages.push(msg_project_list(list_all_projects(state)));
    messages
}

// ---------------------------------------------------------------------------
// Idea factory stubs
// ---------------------------------------------------------------------------

/// Called when an idea-factory agent completes all stages.
///
/// Falls through to `on_agent_done` so the agent is properly reset to idle
/// and follow-up tasks are scheduled.
pub fn _on_idea_factory_done(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    info!("Idea factory agent [{agent_id}] completed — routing to on_agent_done");
    on_agent_done(state, agent_id)
}

/// Called when an idea-factory S7-only agent finishes.
///
/// Falls through to `on_agent_done` for proper cleanup.
pub fn _on_idea_factory_s7_done(state: &BridgeState, agent_id: &str) -> Vec<Value> {
    info!("Idea factory S7 agent [{agent_id}] completed — routing to on_agent_done");
    on_agent_done(state, agent_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- classify_chat_intent_keywords ---

    #[test]
    fn test_classify_query_question_mark() {
        assert_eq!(classify_chat_intent_keywords("当前项目进度如何？"), "query");
        assert_eq!(classify_chat_intent_keywords("进展怎么样?"), "query");
    }

    #[test]
    fn test_classify_query_chinese_particles() {
        assert_eq!(classify_chat_intent_keywords("项目完成了吗"), "query");
        assert_eq!(classify_chat_intent_keywords("还有多少任务呢"), "query");
    }

    #[test]
    fn test_classify_query_english_keywords() {
        assert_eq!(classify_chat_intent_keywords("what is the status?"), "query");
        assert_eq!(classify_chat_intent_keywords("how many stages done?"), "query");
    }

    #[test]
    fn test_classify_feedback_chinese() {
        assert_eq!(classify_chat_intent_keywords("请暂停当前项目"), "feedback");
        assert_eq!(classify_chat_intent_keywords("建议优化学习率"), "feedback");
    }

    #[test]
    fn test_classify_feedback_english() {
        assert_eq!(classify_chat_intent_keywords("please stop the current run"), "feedback");
        assert_eq!(classify_chat_intent_keywords("you should adjust the batch size"), "feedback");
    }

    #[test]
    fn test_classify_feedback_long_text() {
        // Long text without question → feedback
        let long = "我认为当前的实验方案需要做出一些改动，具体来说应当调整超参数的设置方式，特别是学习率和批次大小，这样才能使结果更稳定可靠。";
        assert_eq!(classify_chat_intent_keywords(long), "feedback");
    }

    // --- safe_reference_upload_name ---

    #[test]
    fn test_safe_upload_name_already_pdf() {
        let result = safe_reference_upload_name("my-paper_2024.pdf");
        assert!(result.ends_with(".pdf"));
        assert!(!result.contains(' '));
    }

    #[test]
    fn test_safe_upload_name_no_pdf_suffix() {
        let result = safe_reference_upload_name("paper2024");
        assert!(result.ends_with(".pdf"), "got: {result}");
    }

    #[test]
    fn test_safe_upload_name_special_chars() {
        let result = safe_reference_upload_name("my paper (final).pdf");
        assert!(!result.contains(' '));
        assert!(!result.contains('('));
        assert!(!result.contains(')'));
        assert!(result.ends_with(".pdf"), "got: {result}");
    }

    #[test]
    fn test_safe_upload_name_chinese() {
        // Chinese chars should be replaced
        let result = safe_reference_upload_name("论文.pdf");
        assert!(result.ends_with(".pdf"), "got: {result}");
        // No Chinese characters
        assert!(result.chars().all(|c| c.is_ascii()), "got: {result}");
    }

    // --- build_role_prompt ---

    #[test]
    fn test_build_role_prompt_known_angle() {
        let prompt = build_role_prompt("CV", "目标检测");
        assert!(prompt.contains("计算机视觉"), "expected CV prompt, got: {prompt}");
    }

    #[test]
    fn test_build_role_prompt_vlm() {
        let prompt = build_role_prompt("VLM", "多模态理解");
        assert!(prompt.contains("视觉语言模型"), "got: {prompt}");
    }

    #[test]
    fn test_build_role_prompt_world_model() {
        let prompt = build_role_prompt("World Model", "世界建模");
        assert!(prompt.contains("世界模型"), "got: {prompt}");
    }

    #[test]
    fn test_build_role_prompt_unknown_angle() {
        let prompt = build_role_prompt("Quantum", "量子计算");
        assert!(prompt.contains("Quantum"), "got: {prompt}");
        assert!(prompt.contains("量子计算"), "got: {prompt}");
    }

    // --- passthrough_agent logic (unit-level, without full BridgeState) ---

    #[test]
    fn test_layer_range_idea() {
        let (start, end) = layer_range("idea");
        assert_eq!(start, 1);
        assert_eq!(end, 8);
    }

    #[test]
    fn test_layer_range_coding() {
        let (start, end) = layer_range("coding");
        assert_eq!(start, 10);
        assert_eq!(end, 13);
    }

    #[test]
    fn test_known_lab_angles_contains_four() {
        let angles = known_lab_angles();
        assert!(angles.contains_key("CV"));
        assert!(angles.contains_key("VLM"));
        assert!(angles.contains_key("World Model"));
        assert!(angles.contains_key("VLA"));
    }
}
