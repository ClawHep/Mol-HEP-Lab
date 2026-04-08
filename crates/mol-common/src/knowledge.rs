//! Layered knowledge chain for domain-specific file resolution.
//!
//! A [`KnowledgeChain`] is an ordered list of directory roots searched
//! most-specific first. This enables sub-domain layering (e.g.
//! `hep-cepc` overrides `hep` which falls back to `generic`).

use std::path::PathBuf;

/// Ordered list of knowledge tree roots, searched most-specific first.
///
/// # Example
///
/// ```text
/// chain: ["hep-cepc", "hep", "generic"]
///
/// Looking up "agents/signal-lead.md" checks:
///   hep-cepc/agents/signal-lead.md
///   → hep/agents/signal-lead.md
///   → generic/agents/signal-lead.md
/// ```
#[derive(Debug, Clone)]
pub struct KnowledgeChain {
    roots: Vec<PathBuf>,
}

impl KnowledgeChain {
    /// Create a new chain from an ordered list of directory roots.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// Read the first matching file in the chain.
    pub fn read_first(&self, rel_path: &str) -> Option<String> {
        for root in &self.roots {
            let path = root.join(rel_path);
            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(content);
            }
        }
        None
    }

    /// Read all matching files across all layers (specific → general).
    pub fn read_all(&self, rel_path: &str) -> Vec<String> {
        self.roots
            .iter()
            .filter_map(|root| std::fs::read_to_string(root.join(rel_path)).ok())
            .collect()
    }

    /// Resolve the absolute path of the first matching file.
    pub fn resolve(&self, rel_path: &str) -> Option<PathBuf> {
        for root in &self.roots {
            let path = root.join(rel_path);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Return the first root that contains a `templates/stages/` directory.
    pub fn templates_dir(&self) -> Option<PathBuf> {
        for root in &self.roots {
            let tdir = root.join("templates/stages");
            if tdir.is_dir() {
                return Some(tdir);
            }
        }
        None
    }

    /// Return a reference to the ordered roots.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_chain() -> (TempDir, KnowledgeChain) {
        let dir = TempDir::new().unwrap();
        let specific = dir.path().join("specific");
        let general = dir.path().join("general");
        std::fs::create_dir_all(specific.join("agents")).unwrap();
        std::fs::create_dir_all(general.join("agents")).unwrap();
        std::fs::create_dir_all(general.join("templates/stages")).unwrap();

        std::fs::write(specific.join("agents/agent-a.md"), "specific-a").unwrap();
        std::fs::write(general.join("agents/agent-a.md"), "general-a").unwrap();
        std::fs::write(general.join("agents/agent-b.md"), "general-b").unwrap();

        let chain = KnowledgeChain::new(vec![specific, general]);
        (dir, chain)
    }

    #[test]
    fn read_first_returns_most_specific() {
        let (_dir, chain) = setup_chain();
        assert_eq!(chain.read_first("agents/agent-a.md").unwrap(), "specific-a");
    }

    #[test]
    fn read_first_falls_through_to_general() {
        let (_dir, chain) = setup_chain();
        assert_eq!(chain.read_first("agents/agent-b.md").unwrap(), "general-b");
    }

    #[test]
    fn read_first_returns_none_for_missing() {
        let (_dir, chain) = setup_chain();
        assert!(chain.read_first("agents/nonexistent.md").is_none());
    }

    #[test]
    fn read_all_returns_all_layers() {
        let (_dir, chain) = setup_chain();
        let all = chain.read_all("agents/agent-a.md");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], "specific-a");
        assert_eq!(all[1], "general-a");
    }

    #[test]
    fn resolve_returns_first_path() {
        let (_dir, chain) = setup_chain();
        let path = chain.resolve("agents/agent-a.md").unwrap();
        assert!(path.ends_with("specific/agents/agent-a.md"));
    }

    #[test]
    fn templates_dir_finds_first_with_templates() {
        let (_dir, chain) = setup_chain();
        let tdir = chain.templates_dir().unwrap();
        assert!(tdir.ends_with("general/templates/stages"));
    }

    #[test]
    fn empty_chain_returns_none() {
        let chain = KnowledgeChain::new(vec![]);
        assert!(chain.read_first("anything.md").is_none());
    }
}
