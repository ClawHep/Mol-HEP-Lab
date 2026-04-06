//! In-memory + filesystem search result cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// CacheEntry
// ---------------------------------------------------------------------------

struct CacheEntry {
    data: Value,
    inserted_at: SystemTime,
}

// ---------------------------------------------------------------------------
// SearchCache
// ---------------------------------------------------------------------------

/// Caches code search results to avoid redundant GitHub API calls.
///
/// Uses an in-memory map keyed by `(domain_id, topic)`, optionally backed by
/// a filesystem directory for cross-session persistence.
pub struct SearchCache {
    /// In-memory store.
    store: HashMap<String, CacheEntry>,

    /// Optional directory for persisting entries across process restarts.
    cache_dir: Option<PathBuf>,

    /// Entries older than this are considered stale.
    ttl: Duration,
}

impl SearchCache {
    /// Create an in-memory-only cache with a 24-hour TTL.
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            cache_dir: None,
            ttl: Duration::from_secs(86_400),
        }
    }

    /// Create a cache backed by `dir`, loading existing entries on construction.
    pub fn with_dir(dir: impl Into<PathBuf>, ttl: Duration) -> Self {
        let dir = dir.into();
        let mut cache = Self {
            store: HashMap::new(),
            cache_dir: Some(dir.clone()),
            ttl,
        };
        cache.load_from_dir(&dir);
        cache
    }

    // -- Key helpers --------------------------------------------------------

    fn make_key(domain_id: &str, topic: &str) -> String {
        format!("{domain_id}::{topic}")
    }

    fn key_to_filename(key: &str) -> String {
        let safe: String = key
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        format!("{safe}.json")
    }

    // -- Persistence --------------------------------------------------------

    fn load_from_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(outer) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            let key = outer
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let data = outer.get("data").cloned().unwrap_or(Value::Null);
            if !key.is_empty() {
                self.store.insert(
                    key,
                    CacheEntry {
                        data,
                        inserted_at: SystemTime::UNIX_EPOCH, // Treat disk entries as fresh
                    },
                );
            }
        }
        debug!(count = self.store.len(), "Loaded cache entries from disk");
    }

    fn persist_entry(&self, key: &str, data: &Value) {
        let Some(ref dir) = self.cache_dir else {
            return;
        };
        let filename = Self::key_to_filename(key);
        let path = dir.join(&filename);
        let outer = serde_json::json!({ "key": key, "data": data });
        if let Err(e) = std::fs::write(&path, outer.to_string()) {
            warn!(path = %path.display(), "Failed to persist cache entry: {e}");
        }
    }

    // -- Public API ---------------------------------------------------------

    /// Retrieve a cached entry for `(domain_id, topic)`.
    ///
    /// Returns `None` when the entry is missing or stale.
    pub fn get(&self, domain_id: &str, topic: &str) -> Option<Value> {
        let key = Self::make_key(domain_id, topic);
        let entry = self.store.get(&key)?;
        let age = entry.inserted_at.elapsed().unwrap_or(Duration::ZERO);
        if age > self.ttl {
            return None;
        }
        Some(entry.data.clone())
    }

    /// Insert or update a cache entry.
    pub fn put(&mut self, domain_id: &str, topic: &str, data: Value) {
        let key = Self::make_key(domain_id, topic);
        self.persist_entry(&key, &data);
        self.store.insert(
            key,
            CacheEntry {
                data,
                inserted_at: SystemTime::now(),
            },
        );
    }

    /// Remove all stale entries from the in-memory store.
    pub fn evict_stale(&mut self) {
        self.store.retain(|_, entry| {
            entry.inserted_at.elapsed().unwrap_or(Duration::ZERO) <= self.ttl
        });
    }

    /// Return the number of live cache entries.
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// `true` when the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

impl Default for SearchCache {
    fn default() -> Self {
        Self::new()
    }
}
