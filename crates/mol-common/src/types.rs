//! Core shared types for Mol-HEP-Lab / MolAgent.
//!
//! These types mirror the TypeScript types in `frontend/src/types.ts` and
//! the Python pipeline stage definitions in `pipeline/stages.py`, re-branded
//! for the Mol-HEP-Lab project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// Type alias
// ---------------------------------------------------------------------------

/// Standard result type for mol-common operations.
pub type MolResult<T> = anyhow::Result<T>;

// ---------------------------------------------------------------------------
// RunId newtype
// ---------------------------------------------------------------------------

/// Opaque identifier for a pipeline run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub String);

impl RunId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RunId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RunId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

// ---------------------------------------------------------------------------
// AgentLayer
// ---------------------------------------------------------------------------

/// The five pyramid layers of the MolAgent research pipeline.
///
/// Each layer groups related pipeline stages and is displayed with a
/// distinctive color in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentLayer {
    /// Layer 1 — Literature survey, knowledge synthesis, hypothesis generation.
    Idea,
    /// Layer 2 — Experiment design.
    Experiment,
    /// Layer 3 — Code retrieval, generation, resource planning.
    Coding,
    /// Layer 4 — Experiment execution, analysis, research decision.
    Execution,
    /// Layer 5 — Paper drafting, peer review, revision.
    Writing,
}

impl AgentLayer {
    /// Hex colour string used in the frontend for this layer.
    pub fn color(&self) -> &'static str {
        match self {
            AgentLayer::Idea => "#f59e0b",
            AgentLayer::Experiment => "#3b82f6",
            AgentLayer::Coding => "#10b981",
            AgentLayer::Execution => "#ef4444",
            AgentLayer::Writing => "#a855f7",
        }
    }

    /// Human-readable display name (English).
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentLayer::Idea => "Layer 1 · Survey & Ideation",
            AgentLayer::Experiment => "Layer 2 · Experiment Design",
            AgentLayer::Coding => "Layer 3 · Code & Resources",
            AgentLayer::Execution => "Layer 4 · Execution & Analysis",
            AgentLayer::Writing => "Layer 5 · Paper Writing",
        }
    }

    /// Pipeline stage IDs belonging to this layer.
    pub fn stage_ids(&self) -> &'static [u32] {
        match self {
            AgentLayer::Idea => &[1, 2, 3, 4, 5, 6, 7, 100, 8],
            AgentLayer::Experiment => &[9],
            AgentLayer::Coding => &[10, 11, 12, 13],
            AgentLayer::Execution => &[14, 15, 16, 17, 18],
            AgentLayer::Writing => &[19, 20, 21, 22],
        }
    }

    /// All layers in order.
    pub fn all() -> &'static [AgentLayer] {
        &[
            AgentLayer::Idea,
            AgentLayer::Experiment,
            AgentLayer::Coding,
            AgentLayer::Execution,
            AgentLayer::Writing,
        ]
    }
}

impl fmt::Display for AgentLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AgentLayer::Idea => "idea",
            AgentLayer::Experiment => "experiment",
            AgentLayer::Coding => "coding",
            AgentLayer::Execution => "execution",
            AgentLayer::Writing => "writing",
        };
        write!(f, "{s}")
    }
}

// ---------------------------------------------------------------------------
// AgentStatus
// ---------------------------------------------------------------------------

/// Runtime status of a MolAgent instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
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

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AgentStatus::Idle => "idle",
            AgentStatus::Working => "working",
            AgentStatus::Error => "error",
            AgentStatus::Done => "done",
            AgentStatus::WaitingDiscussion => "waiting_discussion",
            AgentStatus::Discussing => "discussing",
        };
        write!(f, "{s}")
    }
}

// ---------------------------------------------------------------------------
// StageStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a single pipeline stage execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Waiting,
    Discussing,
    BlockedApproval,
    Approved,
    Rejected,
    Paused,
    Retrying,
    Done,
}

impl fmt::Display for StageStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::Completed => "completed",
            StageStatus::Failed => "failed",
            StageStatus::Skipped => "skipped",
            StageStatus::Waiting => "waiting",
            StageStatus::Discussing => "discussing",
            StageStatus::BlockedApproval => "blocked_approval",
            StageStatus::Approved => "approved",
            StageStatus::Rejected => "rejected",
            StageStatus::Paused => "paused",
            StageStatus::Retrying => "retrying",
            StageStatus::Done => "done",
        };
        write!(f, "{s}")
    }
}

// ---------------------------------------------------------------------------
// ArtifactStatus
// ---------------------------------------------------------------------------

/// Freshness status of a pipeline artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactStatus {
    #[default]
    Fresh,
    Stale,
    Error,
}

impl fmt::Display for ArtifactStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ArtifactStatus::Fresh => "fresh",
            ArtifactStatus::Stale => "stale",
            ArtifactStatus::Error => "error",
        };
        write!(f, "{s}")
    }
}

// ---------------------------------------------------------------------------
// Artifact
// ---------------------------------------------------------------------------

/// A file artifact produced by a pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    /// Unique artifact identifier.
    pub id: String,
    /// Which shared repository this artifact belongs to.
    pub repo_id: String,
    /// Project this artifact belongs to.
    pub project_id: String,
    /// File name (may include a relative sub-path like `cards/foo.md`).
    pub filename: String,
    /// Agent ID that produced this artifact.
    pub produced_by: String,
    /// Unix timestamp (ms) when produced.
    pub timestamp: i64,
    /// Human-readable size string, e.g. `"12 KB"`.
    pub size: String,
    /// Freshness of the artifact.
    pub status: ArtifactStatus,
    /// Raw content, if loaded into memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Pipeline stage number that produced this artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<u32>,
}

// ---------------------------------------------------------------------------
// StageResult
// ---------------------------------------------------------------------------

/// The outcome of running a single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// Pipeline stage number.
    pub stage: u32,
    /// Final status of this stage execution.
    pub status: StageStatus,
    /// Artifacts produced during this stage.
    pub artifacts: Vec<Artifact>,
    /// Error message if `status` is `Failed` or `Error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Human decision string (e.g. `"proceed"`, `"pivot"`, `"retry"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// References to evidence artifacts that informed this result.
    pub evidence_refs: Vec<String>,
}

impl StageResult {
    /// Create a minimal successful stage result.
    pub fn success(stage: u32) -> Self {
        Self {
            stage,
            status: StageStatus::Done,
            artifacts: Vec::new(),
            error: None,
            decision: Some("proceed".to_owned()),
            evidence_refs: Vec::new(),
        }
    }

    /// Create a failed stage result.
    pub fn failure(stage: u32, error: impl Into<String>) -> Self {
        Self {
            stage,
            status: StageStatus::Failed,
            artifacts: Vec::new(),
            error: Some(error.into()),
            decision: Some("retry".to_owned()),
            evidence_refs: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// LogEntry
// ---------------------------------------------------------------------------

/// A single structured log entry emitted by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub layer: AgentLayer,
    pub stage: Option<u32>,
    pub message: String,
    pub level: LogLevel,
    pub timestamp: DateTime<Utc>,
}

/// Severity level of a log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// ResourceStats / GpuInfo
// ---------------------------------------------------------------------------

/// Live GPU statistics for one device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    /// Device index (0-based).
    pub id: u32,
    /// Device name, e.g. `"NVIDIA RTX 4090"`.
    pub name: String,
    /// GPU core utilization 0–100 %.
    pub utilization: f32,
    /// Used VRAM in MiB.
    pub mem_used: u64,
    /// Total VRAM in MiB.
    pub mem_total: u64,
    /// Temperature in °C.
    pub temperature: f32,
}

/// Snapshot of host resource utilisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceStats {
    /// CPU utilization 0–100 %.
    pub cpu_percent: f32,
    /// Used RAM in MiB.
    pub mem_used: u64,
    /// Total RAM in MiB.
    pub mem_total: u64,
    /// Per-GPU statistics (empty on CPU-only hosts).
    pub gpus: Vec<GpuInfo>,
    /// Human-readable label for the accelerator, e.g. `"NVIDIA RTX 4090"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accelerator_label: Option<String>,
    /// Unix timestamp (ms) when this snapshot was taken.
    pub timestamp: i64,
}

// ---------------------------------------------------------------------------
// ChatMessage
// ---------------------------------------------------------------------------

/// A human-feedback chat message exchanged with the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_layer: Option<String>,
    /// Unix timestamp (ms).
    pub timestamp: i64,
}

/// Role of a chat message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    System,
}

// ---------------------------------------------------------------------------
// ProjectInfo
// ---------------------------------------------------------------------------

/// Status of a research project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Running,
    Queued,
    Completed,
    Interrupted,
    #[default]
    New,
}

impl fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ProjectStatus::Running => "running",
            ProjectStatus::Queued => "queued",
            ProjectStatus::Completed => "completed",
            ProjectStatus::Interrupted => "interrupted",
            ProjectStatus::New => "new",
        };
        write!(f, "{s}")
    }
}

/// Metadata about a research project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub project_id: String,
    pub status: ProjectStatus,
    pub last_completed_stage: u32,
    pub last_completed_name: String,
    pub first_stage: u32,
    pub total_stages: u32,
    /// ISO-8601 timestamp string.
    pub timestamp: String,
    /// Research topic / title.
    pub topic: String,
    /// Path to the project config YAML.
    pub config_path: String,
    /// Optional human intervention note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intervention: Option<String>,
}

// ---------------------------------------------------------------------------
// QueueSummary
// ---------------------------------------------------------------------------

/// Summary statistics for a named work queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSummary {
    pub name: String,
    pub total: u32,
    pub pending: u32,
    pub assigned: u32,
    pub completed: u32,
}
