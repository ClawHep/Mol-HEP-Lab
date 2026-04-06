//! # mol-common
//!
//! Shared types, utilities, and infrastructure for the **Mol-HEP-Lab /
//! MolAgent** research-pipeline system.
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`types`] | Core pipeline types (`AgentLayer`, `Artifact`, `StageResult`, …) |
//! | [`prompts`] | Prompt template engine (Tera / Jinja2-compatible) |
//! | [`quality`] | Content quality assessment and template-content detection |
//! | [`hardware`] | Local hardware detection (GPU / MPS / CPU) |
//! | [`sanitize`] | Text sanitization: thinking-tag stripping, API key redaction, filename cleaning |

pub mod hardware;
pub mod prompts;
pub mod quality;
pub mod sanitize;
pub mod types;

// ---------------------------------------------------------------------------
// Convenience re-exports — the most commonly used items are available
// directly from the crate root.
// ---------------------------------------------------------------------------

// Types
pub use types::{
    AgentLayer, AgentStatus, Artifact, ArtifactStatus, ChatMessage, ChatRole, GpuInfo, LogEntry,
    LogLevel, MolResult, ProjectInfo, ProjectStatus, QueueSummary, ResourceStats, RunId,
    StageResult, StageStatus,
};

// Hardware
pub use hardware::{detect_hardware, HardwareProfile, SystemStats};

// Quality
pub use quality::{assess_quality, check_quality_default, check_strict_quality, QualityReport, QualityScore, TemplateMatch};

// Sanitize
pub use sanitize::{
    redact_api_keys, sanitize_filename, sanitize_figure_id, sanitize_run_id, strip_thinking_tags,
    truncate_text,
};

// Prompts
pub use prompts::PromptEngine;
