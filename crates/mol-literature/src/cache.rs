//! File-based response cache for literature API responses.
//!
//! Keys are SHA-256 hex digests (first 16 chars) of the lowercased, trimmed
//! input string.  Values are stored as JSON files under
//! `~/.cache/mol-literature/<namespace>/`.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A lightweight file-backed cache keyed by a `&str` namespace.
///
/// Each namespace gets its own subdirectory under the base cache dir so that
/// different API backends (arXiv, S2, OpenAlex) don't collide.
#[derive(Debug, Clone)]
pub struct FileCache {
    base_dir: PathBuf,
}

impl FileCache {
    /// Create a cache rooted at `~/.cache/mol-literature`.
    pub fn default_cache() -> Self {
        let base = dirs_or_home().join("mol-literature");
        Self { base_dir: base }
    }

    /// Create a cache rooted at an explicit directory (useful for tests).
    pub fn with_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }

    /// Look up a cached value by namespace + key string.
    ///
    /// Returns `None` when the entry does not exist or cannot be deserialised.
    pub fn get<T: for<'de> Deserialize<'de>>(&self, namespace: &str, key: &str) -> Option<T> {
        let path = self.path_for(namespace, key);
        if !path.exists() {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str::<T>(&text) {
            Ok(v) => {
                debug!("cache HIT  [{namespace}] {key:.32}");
                Some(v)
            }
            Err(e) => {
                warn!("cache corrupt [{namespace}] {key:.32}: {e}");
                None
            }
        }
    }

    /// Store a value under namespace + key.
    ///
    /// Failures are logged but not propagated — a cache write error should
    /// never abort normal operation.
    pub fn set<T: Serialize>(&self, namespace: &str, key: &str, value: &T) {
        let path = self.path_for(namespace, key);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("cache mkdir failed [{namespace}]: {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(value) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    warn!("cache write failed [{namespace}] {key:.32}: {e}");
                } else {
                    debug!("cache WRITE [{namespace}] {key:.32}");
                }
            }
            Err(e) => warn!("cache serialise failed [{namespace}] {key:.32}: {e}"),
        }
    }

    /// Remove a single cached entry.
    pub fn invalidate(&self, namespace: &str, key: &str) -> Result<()> {
        let path = self.path_for(namespace, key);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn path_for(&self, namespace: &str, key: &str) -> PathBuf {
        let hash = cache_key(key);
        self.base_dir.join(namespace).join(format!("{hash}.json"))
    }
}

/// Compute a short (16-char) hex digest of the input used as the filename.
pub fn cache_key(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // We use a simple 64-bit hash for filenames — collision risk is negligible
    // for a local cache.  For a production cache a SHA-256 would be better, but
    // we keep the dep-tree lean.
    let mut h = DefaultHasher::new();
    input.to_lowercase().trim().hash(&mut h);
    format!("{:016x}", h.finish())
}

// ---------------------------------------------------------------------------
// Platform: home dir resolution
// ---------------------------------------------------------------------------

fn dirs_or_home() -> PathBuf {
    // Prefer $XDG_CACHE_HOME, then $HOME/.cache
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache");
    }
    // Fallback: current directory
    PathBuf::from(".cache")
}

// ---------------------------------------------------------------------------
// Utility: wrap an async call with a file-cache layer
// ---------------------------------------------------------------------------

/// Try to load `T` from the cache; if missing, call `fetch`, store, and return.
///
/// Errors from `fetch` are propagated.  Cache misses/write-errors are silent.
pub async fn cached_fetch<T, F, Fut>(
    cache: &FileCache,
    namespace: &str,
    key: &str,
    fetch: F,
) -> Result<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    if let Some(cached) = cache.get::<T>(namespace, key) {
        return Ok(cached);
    }
    let value = fetch().await?;
    cache.set(namespace, key, &value);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_string() {
        let dir = TempDir::new().unwrap();
        let cache = FileCache::with_dir(dir.path());
        cache.set("ns", "hello", &"world".to_owned());
        let got: Option<String> = cache.get("ns", "hello");
        assert_eq!(got, Some("world".to_owned()));
    }

    #[test]
    fn miss_returns_none() {
        let dir = TempDir::new().unwrap();
        let cache = FileCache::with_dir(dir.path());
        let got: Option<String> = cache.get("ns", "missing");
        assert!(got.is_none());
    }
}
