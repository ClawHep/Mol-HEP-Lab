//! Mol-HEP-Lab: Self-evolution system for adaptive pipeline learning.
//!
//! Records lessons from each pipeline run (failures, slow stages, quality
//! issues) and injects them as prompt overlays into future runs. Inspired by
//! Sibyl's time-weighted evolution mechanism.
//!
//! # Architecture
//!
//! * [`LessonCategory`] — issue classification enum.
//! * [`LessonSeverity`] — lesson severity levels.
//! * [`LessonEntry`] — single lesson record with metadata.
//! * [`EvolutionStore`] — JSON-backed persistent store with query and overlay generation.
//! * [`EvolutionSummary`] — aggregate statistics over the lesson store.
//!
//! # Example
//!
//! ```no_run
//! use mol_evolution::{EvolutionStore, LessonEntry, LessonCategory, LessonSeverity};
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut store = EvolutionStore::new(PathBuf::from("evolution"));
//! store.load().await?;
//! let overlay = store.get_evolution_overlay(12);
//! println!("{overlay}");
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Issue classification for extracted lessons.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonCategory {
    /// Agent or LLM configuration problems.
    Configuration,
    /// Research methodology weaknesses.
    Methodology,
    /// Data loading, parsing, or preprocessing issues.
    DataHandling,
    /// Model architecture or selection problems.
    ModelSelection,
    /// Experimental design flaws.
    ExperimentDesign,
    /// Paper writing quality issues.
    Writing,
    /// Code generation bugs or syntax errors.
    CodeGeneration,
    /// Compute / memory / scheduling resource problems.
    ResourceManagement,
}

impl LessonCategory {
    /// Human-readable label for display in overlays.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Methodology => "methodology",
            Self::DataHandling => "data_handling",
            Self::ModelSelection => "model_selection",
            Self::ExperimentDesign => "experiment_design",
            Self::Writing => "writing",
            Self::CodeGeneration => "code_generation",
            Self::ResourceManagement => "resource_management",
        }
    }
}

impl std::fmt::Display for LessonCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Severity level for a lesson entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonSeverity {
    /// Informational note — low urgency.
    Info,
    /// Warning — should be addressed to avoid degraded quality.
    Warning,
    /// Critical failure — must be fixed before the next run.
    Critical,
}

impl LessonSeverity {
    /// Numeric score used in weighted ranking (higher = more important).
    pub fn weight(&self) -> f64 {
        match self {
            Self::Info => 1.0,
            Self::Warning => 1.5,
            Self::Critical => 2.5,
        }
    }

    /// Icon used in prompt overlays.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Info => "ℹ️",
            Self::Warning => "⚠️",
            Self::Critical => "❌",
        }
    }
}

impl std::fmt::Display for LessonSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// LessonEntry
// ---------------------------------------------------------------------------

/// A single lesson extracted from a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonEntry {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Issue classification.
    pub category: LessonCategory,
    /// Severity level.
    pub severity: LessonSeverity,
    /// Short human-readable title.
    pub title: String,
    /// Full description of the lesson.
    pub description: String,
    /// Pipeline stage number where the lesson originated.
    pub stage: u32,
    /// Run identifier (empty string if not from a specific run).
    pub run_id: String,
    /// UTC timestamp when the lesson was recorded (ISO 8601).
    pub timestamp: DateTime<Utc>,
    /// Free-form classification tags.
    pub tags: Vec<String>,
    /// How many times this lesson has been applied to a prompt overlay.
    pub applied_count: u32,
}

impl LessonEntry {
    /// Create a new lesson with a generated UUID and current timestamp.
    pub fn new(
        category: LessonCategory,
        severity: LessonSeverity,
        title: impl Into<String>,
        description: impl Into<String>,
        stage: u32,
        run_id: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid_v4(),
            category,
            severity,
            title: title.into(),
            description: description.into(),
            stage,
            run_id: run_id.into(),
            timestamp: Utc::now(),
            tags: Vec::new(),
            applied_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// EvolutionSummary
// ---------------------------------------------------------------------------

/// Aggregate statistics over all lessons in the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionSummary {
    /// Total number of lessons.
    pub total_lessons: usize,
    /// Lessons broken down by category.
    pub by_category: HashMap<String, usize>,
    /// Lessons broken down by severity.
    pub by_severity: HashMap<String, usize>,
    /// Lessons broken down by stage number.
    pub by_stage: HashMap<u32, usize>,
    /// Total number of times lessons have been applied to overlays.
    pub total_applied: u64,
    /// Number of unique run IDs recorded.
    pub unique_runs: usize,
}

// ---------------------------------------------------------------------------
// EvolutionStore
// ---------------------------------------------------------------------------

/// Persistent store for pipeline evolution lessons.
///
/// Backed by a single JSON file (`lessons.json`) inside `store_path`.
/// Lessons are held in memory after [`load`](Self::load) is called and
/// flushed to disk with [`save`](Self::save).
pub struct EvolutionStore {
    store_path: PathBuf,
    lessons: Vec<LessonEntry>,
}

impl EvolutionStore {
    // Half-life for exponential decay weighting (days).
    const HALF_LIFE_DAYS: f64 = 30.0;
    // Lessons older than this are excluded from overlays.
    const MAX_AGE_DAYS: f64 = 90.0;

    /// Create a new store backed by `store_path`.
    ///
    /// The directory is created lazily on the first [`save`](Self::save).
    /// Call [`load`](Self::load) to populate the in-memory lesson list from
    /// an existing file.
    pub fn new(store_path: PathBuf) -> Self {
        Self {
            store_path,
            lessons: Vec::new(),
        }
    }

    /// Path to the backing JSON file.
    fn lessons_file(&self) -> PathBuf {
        self.store_path.join("lessons.json")
    }

    /// Load lessons from disk into memory.
    ///
    /// If the file does not exist the store starts empty without error.
    pub async fn load(&mut self) -> anyhow::Result<()> {
        let path = self.lessons_file();
        if !path.exists() {
            info!("mol-evolution: no lesson file at {path:?}, starting fresh");
            return Ok(());
        }
        let raw = tokio::fs::read_to_string(&path).await.map_err(|e| {
            anyhow::anyhow!("mol-evolution: failed to read {path:?}: {e}")
        })?;
        let lessons: Vec<LessonEntry> = serde_json::from_str(&raw).map_err(|e| {
            anyhow::anyhow!("mol-evolution: failed to parse lessons JSON: {e}")
        })?;
        info!(
            "mol-evolution: loaded {} lessons from {path:?}",
            lessons.len()
        );
        self.lessons = lessons;
        Ok(())
    }

    /// Persist all in-memory lessons to disk.
    pub async fn save(&self) -> anyhow::Result<()> {
        let path = self.lessons_file();
        // Ensure directory exists.
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                anyhow::anyhow!("mol-evolution: failed to create dir {parent:?}: {e}")
            })?;
        }
        let json = serde_json::to_string_pretty(&self.lessons).map_err(|e| {
            anyhow::anyhow!("mol-evolution: serialization error: {e}")
        })?;
        tokio::fs::write(&path, json.as_bytes()).await.map_err(|e| {
            anyhow::anyhow!("mol-evolution: failed to write {path:?}: {e}")
        })?;
        info!(
            "mol-evolution: saved {} lessons to {path:?}",
            self.lessons.len()
        );
        Ok(())
    }

    /// Add a lesson to the in-memory store.
    ///
    /// Call [`save`](Self::save) to persist.
    pub fn add_lesson(&mut self, entry: LessonEntry) {
        info!(
            "mol-evolution: add lesson [{}] stage={} id={}",
            entry.severity, entry.stage, entry.id
        );
        self.lessons.push(entry);
    }

    /// Return all lessons relevant to `stage`.
    ///
    /// Returns references to lessons whose `stage` field matches.
    pub fn get_lessons_for_stage(&self, stage: u32) -> Vec<&LessonEntry> {
        self.lessons.iter().filter(|l| l.stage == stage).collect()
    }

    /// Return all lessons of a given `category`.
    pub fn get_lessons_by_category(&self, category: LessonCategory) -> Vec<&LessonEntry> {
        self.lessons
            .iter()
            .filter(|l| l.category == category)
            .collect()
    }

    /// Increment `applied_count` for the lesson with `lesson_id`.
    ///
    /// Logs a warning if the ID is not found.
    pub fn mark_applied(&mut self, lesson_id: &str) {
        if let Some(lesson) = self.lessons.iter_mut().find(|l| l.id == lesson_id) {
            lesson.applied_count += 1;
        } else {
            warn!(
                "mol-evolution: mark_applied called for unknown lesson id={lesson_id}"
            );
        }
    }

    /// Generate a prompt overlay string with lessons relevant to `stage`.
    ///
    /// Lessons are ranked by a time-decay weight (30-day half-life) boosted by
    /// severity. The top 5 lessons (or fewer if unavailable) are included.
    /// Returns an empty string when no relevant lessons exist within the 90-day
    /// age window.
    pub fn get_evolution_overlay(&self, stage: u32) -> String {
        let scored = self.rank_lessons_for_stage(stage, 5);
        if scored.is_empty() {
            return String::new();
        }

        let mut parts: Vec<String> = Vec::new();
        parts.push("## Lessons from Prior Runs (Mol-HEP-Lab Evolution)".to_string());
        for (i, lesson) in scored.iter().enumerate() {
            parts.push(format!(
                "{}. {} [{}] {}: {}",
                i + 1,
                lesson.severity.icon(),
                lesson.category,
                lesson.title,
                lesson.description,
            ));
        }
        parts.push(String::new());
        parts.push(
            "Apply these lessons to avoid repeating past mistakes in this stage.".to_string(),
        );
        parts.join("\n")
    }

    /// Analyze a run's stage results and extract [`LessonEntry`] records.
    ///
    /// `stages` is a slice of `(stage_number, status_string, had_error)` tuples
    /// produced by the pipeline after a run completes. The method heuristically
    /// classifies failures into [`LessonCategory`] variants and returns the new
    /// lessons **without** adding them to the store — call
    /// [`add_lesson`](Self::add_lesson) for each entry if you want them persisted.
    pub fn extract_lessons_from_run(
        &mut self,
        run_id: &str,
        stages: &[(u32, String, bool)],
    ) -> Vec<LessonEntry> {
        let now = Utc::now();
        let mut lessons: Vec<LessonEntry> = Vec::new();

        for (stage_num, status, had_error) in stages {
            let status_lower = status.to_lowercase();

            if *had_error
                || status_lower.contains("failed")
                || status_lower.contains("error")
            {
                let (category, severity, title, description) =
                    classify_stage_failure(*stage_num, &status_lower);
                let mut entry =
                    LessonEntry::new(category, severity, title, description, *stage_num, run_id);
                entry.timestamp = now;
                lessons.push(entry);
            }

            if status_lower.contains("blocked") {
                let mut entry = LessonEntry::new(
                    LessonCategory::Configuration,
                    LessonSeverity::Warning,
                    format!("Stage {stage_num} blocked awaiting approval"),
                    format!(
                        "Stage {stage_num} was blocked during run '{run_id}'. \
                         Review approval gate configuration to reduce manual interruptions."
                    ),
                    *stage_num,
                    run_id,
                );
                entry.timestamp = now;
                entry.tags.push("blocked".to_string());
                lessons.push(entry);
            }

            if status_lower.contains("timeout") {
                let mut entry = LessonEntry::new(
                    LessonCategory::ResourceManagement,
                    LessonSeverity::Warning,
                    format!("Stage {stage_num} timed out"),
                    format!(
                        "Stage {stage_num} exceeded its time budget in run '{run_id}'. \
                         Consider increasing the timeout or splitting the stage."
                    ),
                    *stage_num,
                    run_id,
                );
                entry.timestamp = now;
                entry.tags.push("timeout".to_string());
                lessons.push(entry);
            }
        }

        info!(
            "mol-evolution: extracted {} lessons from run '{run_id}'",
            lessons.len()
        );
        lessons
    }

    /// Compute aggregate statistics over the in-memory lesson store.
    pub fn summary(&self) -> EvolutionSummary {
        let mut by_category: HashMap<String, usize> = HashMap::new();
        let mut by_severity: HashMap<String, usize> = HashMap::new();
        let mut by_stage: HashMap<u32, usize> = HashMap::new();
        let mut total_applied: u64 = 0;
        let mut run_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for lesson in &self.lessons {
            *by_category
                .entry(lesson.category.label().to_string())
                .or_default() += 1;
            *by_severity
                .entry(lesson.severity.to_string())
                .or_default() += 1;
            *by_stage.entry(lesson.stage).or_default() += 1;
            total_applied += u64::from(lesson.applied_count);
            if !lesson.run_id.is_empty() {
                run_ids.insert(&lesson.run_id);
            }
        }

        EvolutionSummary {
            total_lessons: self.lessons.len(),
            by_category,
            by_severity,
            by_stage,
            total_applied,
            unique_runs: run_ids.len(),
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Return the top `limit` lessons for `stage`, ranked by time-decay weight.
    ///
    /// Direct stage matches and higher severities receive score boosts.
    fn rank_lessons_for_stage(&self, stage: u32, limit: usize) -> Vec<&LessonEntry> {
        let now = Utc::now();
        let mut scored: Vec<(f64, &LessonEntry)> = self
            .lessons
            .iter()
            .filter_map(|lesson| {
                let age_days =
                    (now - lesson.timestamp).num_seconds() as f64 / 86_400.0;
                if age_days > Self::MAX_AGE_DAYS {
                    return None;
                }
                let decay = (-age_days * std::f64::consts::LN_2 / Self::HALF_LIFE_DAYS).exp();
                let stage_boost = if lesson.stage == stage { 2.0_f64 } else { 1.0 };
                let severity_boost = lesson.severity.weight();
                Some((decay * stage_boost * severity_boost, lesson))
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(limit).map(|(_, l)| l).collect()
    }
}

// ---------------------------------------------------------------------------
// Heuristic stage failure classifier
// ---------------------------------------------------------------------------

/// Stage name lookup table matching the Python `_STAGE_NAMES` dict.
fn stage_name(stage_num: u32) -> &'static str {
    match stage_num {
        1 => "topic_init",
        2 => "problem_decompose",
        3 => "literature_search",
        4 => "literature_screen",
        5 => "knowledge_extract",
        6 => "synthesis_hypotheses",
        7 => "experiment_design",
        8 => "codebase_search",
        9 => "code_develop",
        10 => "experiment_cycle",
        11 => "result_analysis",
        12 => "research_decision",
        13 => "knowledge_summary",
        14 => "paper_outline",
        15 => "paper_write",
        16 => "peer_review",
        17 => "quality_gate",
        18 => "publish",
        _ => "unknown_stage",
    }
}

/// Map a stage number and status string to a (category, severity, title, description) tuple.
fn classify_stage_failure(
    stage_num: u32,
    status_lower: &str,
) -> (LessonCategory, LessonSeverity, String, String) {
    let name = stage_name(stage_num);

    // Category heuristics based on stage semantics and status keywords.
    let category = match stage_num {
        // Literature and search stages
        3..=5 => LessonCategory::Methodology,
        // Synthesis and hypotheses
        6 => LessonCategory::DataHandling,
        // Experiment stages — inspect status for more specific classification
        7 | 9 | 10 => {
            if status_lower.contains("syntax")
                || status_lower.contains("import")
                || status_lower.contains("compile")
            {
                LessonCategory::CodeGeneration
            } else if status_lower.contains("memory")
                || status_lower.contains("oom")
                || status_lower.contains("timeout")
                || status_lower.contains("resource")
            {
                LessonCategory::ResourceManagement
            } else {
                LessonCategory::ExperimentDesign
            }
        }
        // Codebase search
        8 => LessonCategory::CodeGeneration,
        // Analysis and decision stages
        11 | 12 | 13 => LessonCategory::Methodology,
        // Writing stages
        14..=18 => LessonCategory::Writing,
        // Topic stages
        1 | 2 => LessonCategory::DataHandling,
        // Default: configuration
        _ => LessonCategory::Configuration,
    };

    let severity = if status_lower.contains("critical")
        || status_lower.contains("fatal")
        || status_lower.contains("panic")
    {
        LessonSeverity::Critical
    } else if status_lower.contains("error") || status_lower.contains("failed") {
        LessonSeverity::Warning
    } else {
        LessonSeverity::Info
    };

    let title = format!("Stage '{name}' failure");
    let description = format!(
        "Stage '{name}' (#{stage_num}) encountered a failure: {status_lower}. \
         Review inputs, dependencies, and resource allocation for this stage."
    );

    (category, severity, title, description)
}

// ---------------------------------------------------------------------------
// UUID generation
// ---------------------------------------------------------------------------

/// Generate a pseudo-random UUID v4 string using `std` primitives.
///
/// Uses system time nanoseconds and process ID as entropy sources, mixed
/// with a simple xorshift scramble. Sufficient for unique lesson IDs within
/// a pipeline run; not cryptographically strong.
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let pid = std::process::id();

    let mut bytes = [0u8; 16];
    let ts_bytes = nanos.to_le_bytes();
    let pid_bytes = pid.to_le_bytes();
    for (i, b) in ts_bytes.iter().enumerate() {
        bytes[i] = *b;
    }
    for (i, b) in pid_bytes.iter().enumerate() {
        bytes[4 + i] ^= *b;
    }

    // Xorshift scramble over the two halves.
    let mut state: u64 = u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0u8; 8]));
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    let high = state.to_le_bytes();

    let mut state2: u64 = u64::from_le_bytes(bytes[8..].try_into().unwrap_or([0u8; 8]));
    state2 ^= state2 << 13;
    state2 ^= state2 >> 7;
    state2 ^= state2 << 17;
    state2 ^= state; // cross-mix
    let low = state2.to_le_bytes();

    let mut all = [0u8; 16];
    all[..8].copy_from_slice(&high);
    all[8..].copy_from_slice(&low);

    // Set RFC 4122 version (4) and variant bits.
    all[6] = (all[6] & 0x0f) | 0x40;
    all[8] = (all[8] & 0x3f) | 0x80;

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes(all[0..4].try_into().unwrap()),
        u16::from_be_bytes(all[4..6].try_into().unwrap()),
        u16::from_be_bytes(all[6..8].try_into().unwrap()),
        u16::from_be_bytes(all[8..10].try_into().unwrap()),
        {
            let b = &all[10..16];
            (u64::from(b[0]) << 40)
                | (u64::from(b[1]) << 32)
                | (u64::from(b[2]) << 24)
                | (u64::from(b[3]) << 16)
                | (u64::from(b[4]) << 8)
                | u64::from(b[5])
        }
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lesson(stage: u32) -> LessonEntry {
        LessonEntry::new(
            LessonCategory::ExperimentDesign,
            LessonSeverity::Warning,
            "Test lesson",
            "Sandbox timed out during training loop.",
            stage,
            "run-001",
        )
    }

    #[test]
    fn lesson_entry_roundtrip() {
        let original = sample_lesson(12);
        let json = serde_json::to_string(&original).unwrap();
        let restored: LessonEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(original.id, restored.id);
        assert_eq!(original.stage, restored.stage);
        assert_eq!(original.title, restored.title);
        assert!(matches!(restored.category, LessonCategory::ExperimentDesign));
        assert!(matches!(restored.severity, LessonSeverity::Warning));
    }

    #[test]
    fn add_and_query_by_stage() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        store.add_lesson(sample_lesson(12));
        store.add_lesson(sample_lesson(12));
        store.add_lesson(sample_lesson(5));

        assert_eq!(store.get_lessons_for_stage(12).len(), 2);
        assert_eq!(store.get_lessons_for_stage(5).len(), 1);
        assert_eq!(store.get_lessons_for_stage(99).len(), 0);
    }

    #[test]
    fn add_and_query_by_category() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        store.add_lesson(sample_lesson(12));
        let mut writing_lesson = sample_lesson(17);
        writing_lesson.category = LessonCategory::Writing;
        store.add_lesson(writing_lesson);

        let experiment = store.get_lessons_by_category(LessonCategory::ExperimentDesign);
        assert_eq!(experiment.len(), 1);
        let writing = store.get_lessons_by_category(LessonCategory::Writing);
        assert_eq!(writing.len(), 1);
        let config = store.get_lessons_by_category(LessonCategory::Configuration);
        assert_eq!(config.len(), 0);
    }

    #[test]
    fn mark_applied_increments_count() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        let lesson = sample_lesson(10);
        let id = lesson.id.clone();
        store.add_lesson(lesson);

        store.mark_applied(&id);
        store.mark_applied(&id);

        let entry = store.lessons.iter().find(|l| l.id == id).unwrap();
        assert_eq!(entry.applied_count, 2);
    }

    #[test]
    fn mark_applied_unknown_id_does_not_panic() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        // Should not panic.
        store.mark_applied("nonexistent-id");
    }

    #[test]
    fn get_evolution_overlay_empty_when_no_lessons() {
        let store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        assert!(store.get_evolution_overlay(12).is_empty());
    }

    #[test]
    fn get_evolution_overlay_contains_lesson() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        store.add_lesson(sample_lesson(12));
        let overlay = store.get_evolution_overlay(12);
        assert!(!overlay.is_empty());
        assert!(overlay.contains("Prior Runs"));
        assert!(overlay.contains("Sandbox timed out"));
    }

    #[test]
    fn extract_lessons_from_run_failed_stage() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        let stages = vec![
            (10u32, "failed: syntax error in generated code".to_string(), true),
            (12u32, "completed".to_string(), false),
            (17u32, "blocked".to_string(), false),
        ];
        let lessons = store.extract_lessons_from_run("run-abc", &stages);
        // Stage 10 failure + stage 17 blocked = at least 2 lessons.
        assert!(lessons.len() >= 2);
        let has_code_gen = lessons
            .iter()
            .any(|l| matches!(l.category, LessonCategory::CodeGeneration));
        assert!(has_code_gen, "expected CodeGeneration lesson for syntax error");
        let has_blocked = lessons
            .iter()
            .any(|l| l.tags.contains(&"blocked".to_string()));
        assert!(has_blocked, "expected blocked lesson for stage 17");
    }

    #[test]
    fn summary_counts_correctly() {
        let mut store = EvolutionStore::new(PathBuf::from("/tmp/mol-evolution-test"));
        store.add_lesson(sample_lesson(12));
        store.add_lesson(sample_lesson(12));
        let mut critical = sample_lesson(17);
        critical.category = LessonCategory::Writing;
        critical.severity = LessonSeverity::Critical;
        critical.run_id = "run-002".to_string();
        store.add_lesson(critical);

        let sum = store.summary();
        assert_eq!(sum.total_lessons, 3);
        assert_eq!(*sum.by_stage.get(&12).unwrap_or(&0), 2);
        assert_eq!(*sum.by_severity.get("warning").unwrap_or(&0), 2);
        assert_eq!(*sum.by_severity.get("critical").unwrap_or(&0), 1);
        assert_eq!(sum.unique_runs, 2);
    }

    #[test]
    fn uuid_v4_looks_valid() {
        let id = uuid_v4();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        // Version nibble must be '4'.
        assert_eq!(&parts[2][..1], "4");
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let mut store = EvolutionStore::new(path.clone());
        store.add_lesson(sample_lesson(12));
        store.add_lesson(sample_lesson(5));
        store.save().await.unwrap();

        let mut loaded = EvolutionStore::new(path);
        loaded.load().await.unwrap();
        assert_eq!(loaded.lessons.len(), 2);
        assert_eq!(loaded.get_lessons_for_stage(12).len(), 1);
        assert_eq!(loaded.get_lessons_for_stage(5).len(), 1);
    }
}
