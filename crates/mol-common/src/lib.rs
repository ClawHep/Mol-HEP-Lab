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
//! | [`adapters`] | Typed adapter traits and deterministic recording stubs for testing |
//! | [`writing_guide`] | Static knowledge base of conference writing tips |

pub mod adapters;
pub mod codebase_manifest;
pub mod data;
pub mod hardware;
pub mod hep;
pub mod prompts;
pub mod quality;
pub mod sanitize;
pub mod types;
pub mod writing_guide;

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

// Writing Guide
pub use writing_guide::format_writing_tips;

// Adapters
pub use adapters::{
    AdapterBundle, BrowserAdapter, BrowserPage, CronAdapter, FetchResponse, MemoryAdapter,
    MessageAdapter, RecordingBrowserAdapter, RecordingCronAdapter, RecordingMemoryAdapter,
    RecordingMessageAdapter, RecordingSessionsAdapter, RecordingWebFetchAdapter, SessionsAdapter,
    WebFetchAdapter,
};
