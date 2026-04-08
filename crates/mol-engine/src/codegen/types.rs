//! Core types for the mol-engine codegen subsystem.
//!
//! Provides phase enumeration, context structures, and result types used by
//! all codegen strategies.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// CodegenPhase
// ---------------------------------------------------------------------------

/// Explicit phase enumeration for the code generation turn loop.
///
/// Analogous to the Python `CodegenPhase` enum. Used for structured logging
/// so operators can `grep` phase transitions in live logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodegenPhase {
    Context,
    LlmSetup,
    Routing,
    Generate,
    Fallback,
    Validate,
    Review,
    Finalize,
    Compliance,
}

impl CodegenPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Context => "CONTEXT",
            Self::LlmSetup => "LLM_SETUP",
            Self::Routing => "ROUTING",
            Self::Generate => "GENERATE",
            Self::Fallback => "FALLBACK",
            Self::Validate => "VALIDATE",
            Self::Review => "REVIEW",
            Self::Finalize => "FINALIZE",
            Self::Compliance => "COMPLIANCE",
        }
    }
}

impl std::fmt::Display for CodegenPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// GeneratedFiles
// ---------------------------------------------------------------------------

/// Mapping of relative file path → source code / content.
pub type GeneratedFiles = HashMap<String, String>;

// ---------------------------------------------------------------------------
// DiscoveredData
// ---------------------------------------------------------------------------

/// Pre-discovered filesystem context, gathered before prompt building.
///
/// Analogous to claw-code's `ProjectContext` fields: real data read from the
/// filesystem and injected into the system prompt so the LLM has ground truth
/// before writing any code.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DiscoveredData {
    /// Parsed `model_index.json` from the checkpoint directory.
    pub checkpoint_model_index: HashMap<String, Value>,
    /// Raw JSON of `model_index.json` (truncated to 3000 chars).
    pub checkpoint_model_index_raw: String,
    /// `_class_name` field from `model_index.json`.
    pub checkpoint_class_name: String,
    /// Top-level files in the checkpoint directory.
    pub checkpoint_files: Vec<String>,
    /// Top-level files in the dataset directory.
    pub dataset_files: Vec<String>,
    /// Sample lines from the first dataset text file.
    pub dataset_sample: String,
    /// Python source files discovered in the codebase directory.
    pub codebase_files: Vec<String>,
    /// Contents of the codebase README (truncated).
    pub codebase_readme: String,
}

// ---------------------------------------------------------------------------
// HardwareProfile
// ---------------------------------------------------------------------------

/// Parsed hardware profile (GPU availability, VRAM tier, etc.).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub has_gpu: bool,
    pub gpu_type: String,
    pub gpu_name: String,
    pub tier: String,
    pub vram_gb: f32,
}

impl HardwareProfile {
    pub fn from_map(data: &HashMap<String, Value>) -> Self {
        Self {
            has_gpu: data.get("has_gpu").and_then(Value::as_bool).unwrap_or(false),
            gpu_type: data.get("gpu_type").and_then(Value::as_str).unwrap_or("cuda").to_owned(),
            gpu_name: data.get("gpu_name").and_then(Value::as_str).unwrap_or("").to_owned(),
            tier: data.get("tier").and_then(Value::as_str).unwrap_or("limited").to_owned(),
            vram_gb: data
                .get("vram_gb")
                .and_then(Value::as_f64)
                .map(|v| v as f32)
                .unwrap_or(0.0),
        }
    }
}

// ---------------------------------------------------------------------------
// CodegenContext
// ---------------------------------------------------------------------------

/// All context needed for code generation, assembled once before the LLM loop.
///
/// Analogous to claw-code's `ProjectContext` which gathers cwd, git status,
/// and instruction files before the `SystemPromptBuilder` consumes them.
#[derive(Debug, Default, Clone)]
pub struct CodegenContext {
    /// Research topic / task description.
    pub topic: String,
    /// Experiment plan (YAML text from prior pipeline stage).
    pub exp_plan: String,
    /// Primary metric key (e.g. `"fid"`, `"accuracy"`).
    pub metric: String,
    /// Metric direction: `"lower"` or `"higher"`.
    pub metric_direction: String,
    /// Time budget for a single experiment run (seconds).
    pub time_budget_sec: u64,
    /// Execution mode: `"train"`, `"evaluate"`, `"reproduce"`, etc.
    pub mode: String,

    /// Optional hardware profile.
    pub hw_profile: Option<HardwareProfile>,
    /// JSON string with codebase candidate information.
    pub codebase_info: String,
    /// Absolute path to the dataset directory.
    pub datasets_dir: String,
    /// Absolute path to the checkpoint directory.
    pub checkpoints_dir: String,
    /// Absolute path to the codebase directory.
    pub codebases_dir: String,

    /// Extra prompt guidance injected into the system prompt.
    pub extra_guidance: String,
    /// Reference paper text (for REPRODUCE projects).
    pub reference_paper_text: String,

    /// The pipeline run directory (parent of all stage dirs).
    pub run_dir: Option<PathBuf>,
    /// The stage directory for this codegen stage.
    pub stage_dir: Option<PathBuf>,

    /// Pre-discovered filesystem data.
    pub discovered: DiscoveredData,
}

// ---------------------------------------------------------------------------
// CodegenResult
// ---------------------------------------------------------------------------

/// Result returned by a [`crate::codegen::strategies::CodegenStrategy`] call.
#[derive(Debug, Default, Clone)]
pub struct CodegenResult {
    /// Generated files (relative path → content).
    pub files: GeneratedFiles,
    /// Name of the strategy that produced this result.
    pub strategy_name: String,
    /// True if the review phase should be skipped for this result.
    pub skip_review: bool,
    /// Wall-clock seconds for the strategy execution.
    pub elapsed_sec: f64,
    /// Non-empty if the strategy encountered a fatal error.
    pub error: String,
    /// Additional strategy-specific metadata.
    pub metadata: HashMap<String, Value>,
}

impl CodegenResult {
    /// True if the result contains at least a `main.py` entry point.
    pub fn has_entrypoint(&self) -> bool {
        self.files.contains_key("main.py")
    }
}
