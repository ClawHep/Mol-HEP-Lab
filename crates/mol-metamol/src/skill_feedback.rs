//! Skill effectiveness tracking for MetaMol.
//!
//! Records which skills were active during each pipeline stage and correlates
//! with stage success/failure to identify high- and low-value skills over
//! multiple runs.

use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

/// One record of a skill's effectiveness in a single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEffectivenessRecord {
    /// Skill identifier (e.g. `"arc-paper-relevance-screening"`).
    pub skill_id: String,

    /// Pipeline stage number at which the skill was active.
    pub stage: u32,

    /// Human-readable stage name (e.g. `"literature_screen"`).
    pub stage_name: String,

    /// Run identifier (typically a UUID or timestamp-based string).
    pub run_id: String,

    /// Whether the stage was judged to have succeeded.
    pub success: bool,

    /// Optional free-form details (e.g. PRM score, error message).
    pub details: String,

    /// UTC timestamp in ISO-8601 format.
    pub timestamp: String,
}

/// JSONL-backed persistent store for skill effectiveness records.
///
/// Each record is appended as a single JSON line, making the file easy to
/// stream and post-process with standard tools.
pub struct SkillFeedbackStore {
    path: PathBuf,
}

impl SkillFeedbackStore {
    /// Create or open a store at `path`.  Parent directories are created on
    /// the first write, not at construction time.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Default store location: `~/.metamol/skill_feedback.jsonl`.
    pub fn default_store() -> Self {
        let home = dirs_home();
        Self::new(home.join(".metamol").join("skill_feedback.jsonl"))
    }

    /// Record the outcome of a single skill for a completed stage.
    pub async fn record_outcome(
        &mut self,
        skill_id: &str,
        stage: u32,
        stage_name: &str,
        run_id: &str,
        success: bool,
        details: &str,
    ) -> Result<()> {
        let record = SkillEffectivenessRecord {
            skill_id: skill_id.to_owned(),
            stage,
            stage_name: stage_name.to_owned(),
            run_id: run_id.to_owned(),
            success,
            details: details.to_owned(),
            timestamp: Utc::now().to_rfc3339(),
        };
        self.append_record(&record).await
    }

    /// Record outcomes for all active skills in a completed stage at once.
    pub async fn record_stage_skills(
        &mut self,
        stage: u32,
        stage_name: &str,
        run_id: &str,
        success: bool,
        active_skills: &[String],
    ) -> Result<()> {
        if active_skills.is_empty() {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        let records: Vec<SkillEffectivenessRecord> = active_skills
            .iter()
            .map(|skill_id| SkillEffectivenessRecord {
                skill_id: skill_id.clone(),
                stage,
                stage_name: stage_name.to_owned(),
                run_id: run_id.to_owned(),
                success,
                details: String::new(),
                timestamp: now.clone(),
            })
            .collect();

        self.append_records(&records).await?;
        info!(
            count = records.len(),
            stage,
            "Recorded skill effectiveness entries"
        );
        Ok(())
    }

    /// Load all records from the store.  Returns an empty vec if the file does
    /// not exist.
    pub async fn load_all(&self) -> Result<Vec<SkillEffectivenessRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&self.path).await?;
        let mut records = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<SkillEffectivenessRecord>(line) {
                Ok(rec) => records.push(rec),
                Err(e) => warn!(error = %e, "Skipping malformed skill feedback record"),
            }
        }
        Ok(records)
    }

    /// Compute per-skill success rates across all recorded runs.
    ///
    /// Returns a map of `skill_id -> SkillStats`.
    pub async fn compute_skill_stats(&self) -> Result<std::collections::HashMap<String, SkillStats>> {
        let records = self.load_all().await?;
        let mut map: std::collections::HashMap<String, SkillStats> =
            std::collections::HashMap::new();
        for rec in records {
            let entry = map.entry(rec.skill_id).or_default();
            entry.total += 1;
            if rec.success {
                entry.successes += 1;
            }
        }
        for stats in map.values_mut() {
            if stats.total > 0 {
                stats.success_rate = stats.successes as f64 / stats.total as f64;
            }
        }
        Ok(map)
    }

    // --- private helpers ---

    async fn append_record(&self, record: &SkillEffectivenessRecord) -> Result<()> {
        self.append_records(std::slice::from_ref(record)).await
    }

    async fn append_records(&self, records: &[SkillEffectivenessRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        for rec in records {
            let line = serde_json::to_string(rec)?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }
        Ok(())
    }
}

/// Aggregated statistics for a single skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillStats {
    pub total: u64,
    pub successes: u64,
    pub success_rate: f64,
}

/// Platform-agnostic home directory resolution.
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trip_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillFeedbackStore::new(dir.path().join("feedback.jsonl"));
        let records = store.load_all().await.unwrap();
        assert!(records.is_empty());
    }
}
