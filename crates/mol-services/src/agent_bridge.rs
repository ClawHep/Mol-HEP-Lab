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
use tokio::sync::{broadcast, RwLock};
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
        19..=26 => Some("writing"),
        _ => None,
    }
}

/// Stage numbers per layer.
pub fn layer_stages(layer: &str) -> &'static [u32] {
    match layer {
        "idea" => &[1, 2, 3, 4, 5, 6, 7, 8],
        "experiment" => &[9],
        "coding" => &[10, 11, 12, 13],
        "execution" => &[14, 15, 16, 17, 18],
        "writing" => &[19, 20, 21, 22, 23, 24, 25, 26],
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
        "writing" => (19, 26),
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
        "idea" => Some("init_to_idea"),
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
                       .replace("waitingdiscussion", "waiting_discussion"),
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
        Arc::new(Self {
            agents: DashMap::new(),
            queues: DashMap::new(),
            broadcast_tx: tx,
            gpu_allocator: std::sync::Mutex::new(GpuAllocator::new(total_gpus, gpus_per_project)),
            discussion_groups: DashMap::new(),
            discussion_waiting: DashMap::new(),
            fail_counts: DashMap::new(),
            lab_batches: DashMap::new(),
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

#[allow(dead_code)]
fn sys_agent_value() -> Value {
    serde_json::json!({
        "id": "system", "name": "System", "layer": "idea",
        "runId": "", "status": "idle", "currentStage": null,
        "currentTask": "", "stageProgress": {}, "projectId": "", "roleTag": ""
    })
}

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
    msg_log("system", "System", "idea", None, message, level)
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

    let mut cmd = Command::new(&state.python_path);
    cmd.arg("-m")
        .arg("researchmol")
        .arg("run")
        .arg("--config").arg(config_path)
        .arg("--output").arg(run_dir)
        .arg("--from-stage").arg(stage_name(from_stage))
        .arg("--to-stage").arg(stage_name(to_stage))
        .arg("--auto-approve")
        .arg("--skip-preflight")
        .current_dir(&state.agent_package_dir)
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
    let run_dir = run_dir_path.to_string_lossy().to_string();
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
            if !a.project_id.is_empty() && a.process.is_some() {
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
        let project_id = entry.file_name().to_string_lossy().to_string();
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

        let status = if last_stage >= 22 {
            "completed"
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
        let released = state.gpu_allocator.lock().unwrap().release(project_id);
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
    group.discussion_output_dir = disc_dir.to_string_lossy().to_string();

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

    let rounds = *state.discussion_rounds.blocking_read();
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

    let aentry = state.agents.get(agent_id).unwrap();
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
    let disc_mode = *state.discussion_mode.blocking_read();
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
                discussion_output_dir: disc_dir.to_string_lossy().to_string(),
                is_cross_project: true,
            };
            state.discussion_waiting.remove(agent_id);
            state.discussion_waiting.remove(peer_id.as_str());
            state.discussion_groups.insert(disc_name.clone(), group);
            messages.extend(trigger_discussion(state, &disc_name));
        } else {
            // No peer: skip discussion
            messages.push(msg_log_sys("S7 complete, no discussion partner available, skipping to S8", "info"));
            messages.extend(skip_discussion_proceed_s8(state, agent_id));
        }
        return messages;
    }

    // Coding layer: check S12 sanity
    if layer == "coding" && !project_id.is_empty() {
        let sanity_path = PathBuf::from(&run_dir).join("stage-12/sanity_report.json");
        if sanity_path.exists() {
            if let Some(report) = read_json(&sanity_path) {
                if report.get("status").and_then(|v| v.as_str()) == Some("fail") {
                    messages.push(msg_log_sys(
                        &format!("S12 SANITY_CHECK loop exhausted for project [{project_id}], manual intervention required"),
                        "error",
                    ));
                    messages.extend(pause_project(state, &project_id));
                    messages.push(msg_project_list(list_all_projects(state)));
                    return messages;
                }
            }
        }
    }

    // Create follow-up task
    if let Some(output_queue_name) = layer_output_queue(&layer) {
        // L4→L5: only if decision.md says PROCEED
        let should_push = if layer == "execution" && output_queue_name == "execution_to_writing" {
            let dec = PathBuf::from(&run_dir).join("stage-17/decision.md");
            dec.exists() && {
                let text = std::fs::read_to_string(&dec).unwrap_or_default().to_uppercase();
                text.contains("PROCEED")
            }
        } else {
            true
        };

        if should_push {
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
        let released = state.gpu_allocator.lock().unwrap().release(&project_id);
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
    let disc_mode = *state.discussion_mode.blocking_read();

    let agent_ids: Vec<String> = state.agents.iter().map(|e| e.key().clone()).collect();

    for agent_id in &agent_ids {
        let (status, layer, has_process, assigned) = {
            let Some(a) = state.agents.get(agent_id) else { continue };
            (a.status, a.layer.clone(), a.process.is_some(), a.assigned_task_id.clone())
        };

        if status != AgentStatus::Idle || has_process || assigned.is_some() {
            continue;
        }
        if matches!(status, AgentStatus::WaitingDiscussion | AgentStatus::Discussing) {
            continue;
        }

        // L4: check GPU
        if layer == "execution" {
            let can = state.gpu_allocator.lock().unwrap().can_allocate();
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
                let _ = state.gpu_allocator.lock().unwrap().allocate(&task.project_id);
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
        let project_id = proj_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
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

        "quick_submit" => {
            let topic = data.get("topic").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let project_id = data.get("projectId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mode = data.get("mode").and_then(|v| v.as_str()).unwrap_or("lab").to_string();

            if topic.is_empty() {
                messages.push(msg_log_sys("Please provide a research topic", "error"));
                return messages;
            }

            let base_id = if project_id.is_empty() {
                slugify(&topic, 40)
            } else {
                project_id
            };

            // Deduplicate
            let base_id = {
                let existing = state.projects_dir().join(&base_id);
                if existing.exists() {
                    format!("{base_id}-{}", uid())
                } else {
                    base_id
                }
            };

            let disc_mode = *state.discussion_mode.read().await;

            // For now: simple single-agent submit (Lab mode with config template requires
            // the config template file; submit_new_project handles checkpoint-aware routing)
            messages.push(msg_log_sys(
                &format!("Project [{base_id}] submitted (mode={mode}, topic={topic})"),
                "info",
            ));
            // In full implementation, generate config from template here.
            // For now we use an empty config_path — the agent binary resolves defaults.
            messages.extend(submit_new_project(state, &base_id, "", &topic, &mode, disc_mode));
            messages.extend(schedule_idle_agents(state));
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
                let config_path = meta.as_ref().and_then(|m| m.get("config_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let topic = meta.as_ref().and_then(|m| m.get("topic")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mode = meta.as_ref().and_then(|m| m.get("mode")).and_then(|v| v.as_str()).unwrap_or("lab").to_string();
                let disc_mode = *state.discussion_mode.read().await;
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
                let config_path = meta.as_ref().and_then(|m| m.get("config_path")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let topic = meta.as_ref().and_then(|m| m.get("topic")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mode = meta.as_ref().and_then(|m| m.get("mode")).and_then(|v| v.as_str()).unwrap_or("lab").to_string();
                let disc_mode = *state.discussion_mode.read().await;
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
            // Keyword-based intent: ends with '?' → query
            let is_query = content.ends_with('?') || content.ends_with('？');
            if is_query {
                let summary = build_status_summary(state, target_layer);
                messages.push(msg_feedback_ack(&format!("qs-{}", uid()), &summary, target_layer));
            } else {
                // Treat as feedback
                save_feedback(state, &content, target_layer, &format!("fb-{}", uid()));
                messages.push(msg_feedback_ack(
                    &format!("fb-{}", uid()),
                    "Feedback recorded and injected into active project contexts.",
                    target_layer,
                ));
            }
        }

        "set_discussion_mode" => {
            let enabled = data.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            *state.discussion_mode.write().await = enabled;
            if let Some(rounds) = data.get("rounds").and_then(|v| v.as_u64()) {
                *state.discussion_rounds.write().await = rounds as u32;
            }
        }

        "list_projects" => {
            messages.push(msg_project_list(list_all_projects(state)));
        }

        "get_queues" => {
            messages.push(msg_queue_update(state));
        }

        _ => {
            if !cmd.is_empty() {
                warn!("Unknown command: {cmd}");
            }
        }
    }

    messages
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
        return "No active projects.".to_string();
    }
    let mut lines = Vec::new();
    for p in projects.iter().take(5) {
        let pid = p["projectId"].as_str().unwrap_or("");
        let status = p["status"].as_str().unwrap_or("");
        let stage = p["lastCompletedStage"].as_u64().unwrap_or(0);
        lines.push(format!("Project {pid}: {status} (stage {stage}/22)"));
    }
    lines.join("\n")
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
                } else if !is_s7_only && !is_factory {
                    done_agents.push(agent_id.clone());
                }
                // Idea factory handlers omitted for brevity — extend here
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
        .route("/ws", get(ws_handler))
        .with_state(state)
}
