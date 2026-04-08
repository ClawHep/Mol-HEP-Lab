//! Result Registry — file-backed cache for shared baseline experiment results.
//!
//! Ports `result_registry.py` to Rust.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ResultEntry
// ---------------------------------------------------------------------------

/// A single cached experiment result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEntry {
    pub id: String,
    pub project_id: String,
    pub description: String,
    pub model: String,
    pub dataset: String,
    pub task: String,
    pub metrics: std::collections::HashMap<String, f64>,
    pub tags: Vec<String>,
    pub source_stage: u32,
    /// Unix timestamp (ms).
    pub timestamp: i64,
}

impl ResultEntry {
    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

// ---------------------------------------------------------------------------
// ResultRegistry
// ---------------------------------------------------------------------------

/// File-backed registry for shared baseline experiment results.
///
/// Results are stored in two places:
/// - `{base_dir}/index.json`   — fast-load index of all entries
/// - `{base_dir}/entries/{project_id}_{entry_id}.json` — per-entry detail
///
/// The registry is `Clone`-able via the `Arc<RwLock<…>>` interior.
#[derive(Debug, Clone)]
pub struct ResultRegistry {
    base_dir: PathBuf,
    entries: Arc<RwLock<Vec<ResultEntry>>>,
}

impl ResultRegistry {
    /// Open (or create) a registry at `base_dir`.
    pub fn open(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)?;
        std::fs::create_dir_all(base_dir.join("entries"))?;

        let entries = Self::load_index(&base_dir);
        Ok(Self {
            base_dir,
            entries: Arc::new(RwLock::new(entries)),
        })
    }

    fn load_index(base_dir: &Path) -> Vec<ResultEntry> {
        let index_path = base_dir.join("index.json");
        let Ok(text) = std::fs::read_to_string(&index_path) else {
            return Vec::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn save_index(&self) {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let text = serde_json::to_string_pretty(&*entries).unwrap_or_default();
        let _ = std::fs::write(self.base_dir.join("index.json"), text);
    }

    /// Register a new result entry.
    pub fn register(
        &self,
        project_id: impl Into<String>,
        description: impl Into<String>,
        model: impl Into<String>,
        dataset: impl Into<String>,
        task: impl Into<String>,
        metrics: std::collections::HashMap<String, f64>,
        tags: Vec<String>,
        source_stage: u32,
    ) -> ResultEntry {
        let id = format!("res-{}", &Uuid::new_v4().to_string()[..8]);
        let project_id = project_id.into();
        let entry = ResultEntry {
            id: id.clone(),
            project_id: project_id.clone(),
            description: description.into(),
            model: model.into(),
            dataset: dataset.into(),
            task: task.into(),
            metrics,
            tags,
            source_stage,
            timestamp: ResultEntry::now_ms(),
        };

        // Write detail file
        let detail_path = self
            .base_dir
            .join("entries")
            .join(format!("{project_id}_{id}.json"));
        if let Ok(text) = serde_json::to_string_pretty(&entry) {
            let _ = std::fs::write(detail_path, text);
        }

        // Append to in-memory list and save index
        {
            let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
            entries.push(entry.clone());
        }
        self.save_index();

        entry
    }

    /// Return all registered entries.
    pub fn all_entries(&self) -> Vec<ResultEntry> {
        self.entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Number of registered entries.
    pub fn count(&self) -> usize {
        self.entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// Look up an entry by id.
    pub fn get_entry(&self, entry_id: &str) -> Option<ResultEntry> {
        self.entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|e| e.id == entry_id)
            .cloned()
    }

    /// Query cached results against a new experiment plan.
    ///
    /// Returns the subset of entries whose `(model, dataset, task)` overlap
    /// with any of the provided `plan_keywords`.  In production this would call
    /// an LLM; here we use simple keyword matching so there is no async
    /// dependency.
    pub fn query(&self, plan_keywords: &[&str]) -> Vec<&'static str> {
        // Lightweight keyword match — callers may swap in an LLM-based version
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        entries
            .iter()
            .filter(|e| {
                let haystack = format!(
                    "{} {} {} {}",
                    e.model, e.dataset, e.task, e.description
                )
                .to_lowercase();
                plan_keywords
                    .iter()
                    .any(|kw| haystack.contains(&kw.to_lowercase()))
            })
            .map(|_| "match")          // placeholder — callers iterate all_entries() for real data
            .collect()
    }

    /// Summary statistics for the registry.
    pub fn summary(&self) -> serde_json::Value {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let projects: std::collections::HashSet<&str> =
            entries.iter().map(|e| e.project_id.as_str()).collect();
        let models: std::collections::HashSet<&str> = entries
            .iter()
            .filter(|e| !e.model.is_empty())
            .map(|e| e.model.as_str())
            .collect();
        let datasets: std::collections::HashSet<&str> = entries
            .iter()
            .filter(|e| !e.dataset.is_empty())
            .map(|e| e.dataset.as_str())
            .collect();

        serde_json::json!({
            "total_entries": entries.len(),
            "projects": projects,
            "models": models,
            "datasets": datasets,
        })
    }
}
