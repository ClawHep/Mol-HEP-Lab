//! Embedded YAML data files migrated from `backend/agent/researchclaw/data/`.
//!
//! These are compiled into the binary via `include_str!` so no filesystem
//! access is needed at runtime.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Raw YAML text — available if callers want to parse themselves
// ---------------------------------------------------------------------------

/// Seminal papers database (keyword-indexed foundational citations).
pub const SEMINAL_PAPERS_YAML: &str = include_str!("../../../data/seminal_papers.yaml");

/// Benchmark knowledge base (domain-indexed datasets, baselines, metrics).
pub const BENCHMARK_KNOWLEDGE_YAML: &str = include_str!("../../../data/benchmark_knowledge.yaml");

/// Dataset registry (tiered list of datasets with download/API info).
pub const DATASET_REGISTRY_YAML: &str = include_str!("../../../data/dataset_registry.yaml");

/// Docker sandbox profiles (domain → image + packages mapping).
pub const DOCKER_PROFILES_YAML: &str = include_str!("../../../data/docker_profiles.yaml");

/// Default prompt templates for all pipeline stages.
pub const PROMPTS_DEFAULT_YAML: &str = include_str!("../../../data/prompts.default.yaml");

// ---------------------------------------------------------------------------
// Seminal papers — typed access
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SeminalPaper {
    pub title: String,
    pub authors: String,
    pub year: u32,
    pub venue: String,
    pub cite_key: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeminalPapersFile {
    papers: Vec<SeminalPaper>,
}

static SEMINAL_PAPERS_CACHED: LazyLock<Vec<SeminalPaper>> = LazyLock::new(|| {
    let file: SeminalPapersFile =
        serde_yaml::from_str(SEMINAL_PAPERS_YAML).expect("embedded seminal_papers.yaml is valid");
    file.papers
});

/// Load all seminal papers from the embedded YAML (cached after first call).
pub fn load_seminal_papers_all() -> Vec<SeminalPaper> {
    SEMINAL_PAPERS_CACHED.clone()
}

/// Load seminal papers whose keywords match the given topic (case-insensitive).
/// Deduplicates by `cite_key`.
pub fn load_seminal_papers(topic: &str) -> Vec<SeminalPaper> {
    let lower = topic.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    SEMINAL_PAPERS_CACHED
        .iter()
        .filter(|p| p.keywords.iter().any(|kw| lower.contains(&kw.to_lowercase())))
        .filter(|p| seen.insert(p.cite_key.clone()))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Dataset registry — typed access
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DatasetEntry {
    pub name: String,
    pub tier: u32,
    pub domain: Option<String>,
    pub size_mb: Option<u64>,
    pub classes: Option<u32>,
    pub samples: Option<u64>,
    pub api: Option<String>,
    pub download: Option<String>,
    pub note: Option<String>,
    pub alternatives: Option<Vec<String>>,
}

static DATASET_REGISTRY_CACHED: LazyLock<Vec<DatasetEntry>> = LazyLock::new(|| {
    serde_yaml::from_str(DATASET_REGISTRY_YAML).expect("embedded dataset_registry.yaml is valid")
});

/// Load all dataset entries from the embedded YAML (cached after first call).
pub fn load_dataset_registry() -> Vec<DatasetEntry> {
    DATASET_REGISTRY_CACHED.clone()
}

// ---------------------------------------------------------------------------
// Docker profiles — typed access
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DockerProfile {
    pub image: String,
    pub packages: Vec<String>,
    #[serde(default)]
    pub gpu: bool,
    pub memory_limit_mb: Option<u64>,
    pub network: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DockerProfilesFile {
    profiles: std::collections::HashMap<String, DockerProfile>,
    domain_map: std::collections::HashMap<String, String>,
}

static DOCKER_PROFILES_CACHED: LazyLock<DockerProfilesFile> = LazyLock::new(|| {
    serde_yaml::from_str(DOCKER_PROFILES_YAML).expect("embedded docker_profiles.yaml is valid")
});

/// Load Docker profile for a domain ID (e.g. "ml_vision" → ml_base profile).
pub fn load_docker_profile(domain_id: &str) -> Option<DockerProfile> {
    let file = &*DOCKER_PROFILES_CACHED;
    let profile_name = file.domain_map.get(domain_id)?;
    file.profiles.get(profile_name).cloned()
}

/// Load all Docker profiles (cached after first call).
pub fn load_docker_profiles() -> HashMap<String, DockerProfile> {
    DOCKER_PROFILES_CACHED.profiles.clone()
}

/// Get the domain → profile name mapping (cached after first call).
pub fn docker_domain_map() -> HashMap<String, String> {
    DOCKER_PROFILES_CACHED.domain_map.clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seminal_papers_loads() {
        let all = load_seminal_papers_all();
        assert!(all.len() > 30, "should have 30+ seminal papers");
        assert!(all.iter().any(|p| p.cite_key == "vaswani2017attention"));
    }

    #[test]
    fn seminal_papers_topic_filter() {
        let papers = load_seminal_papers("transformer attention mechanism");
        assert!(!papers.is_empty(), "should match transformer papers");
        assert!(papers.iter().any(|p| p.cite_key == "vaswani2017attention"));
    }

    #[test]
    fn seminal_papers_chinese_keywords() {
        let papers = load_seminal_papers("具身智能 VLA robot");
        assert!(!papers.is_empty(), "should match embodied AI papers");
    }

    #[test]
    fn dataset_registry_loads() {
        let datasets = load_dataset_registry();
        assert!(datasets.len() > 10);
        assert!(datasets.iter().any(|d| d.name == "CIFAR-10"));
        let cifar = datasets.iter().find(|d| d.name == "CIFAR-10").unwrap();
        assert_eq!(cifar.tier, 1);
    }

    #[test]
    fn docker_profiles_loads() {
        let profiles = load_docker_profiles();
        assert!(profiles.contains_key("ml_base"));
        assert!(profiles.contains_key("physics"));
        let ml = &profiles["ml_base"];
        assert!(ml.gpu);
        assert!(ml.packages.contains(&"torch".to_string()));
    }

    #[test]
    fn docker_domain_mapping() {
        let profile = load_docker_profile("ml_vision");
        assert!(profile.is_some());
        assert!(profile.unwrap().gpu);

        let profile = load_docker_profile("physics_pde");
        assert!(profile.is_some());
        assert!(!profile.unwrap().gpu);
    }

    #[test]
    fn prompts_default_not_empty() {
        assert!(PROMPTS_DEFAULT_YAML.len() > 1000);
        assert!(PROMPTS_DEFAULT_YAML.contains("code_generation"));
        assert!(PROMPTS_DEFAULT_YAML.contains("paper_draft"));
    }

    #[test]
    fn benchmark_knowledge_not_empty() {
        assert!(BENCHMARK_KNOWLEDGE_YAML.len() > 5000);
        assert!(BENCHMARK_KNOWLEDGE_YAML.contains("CIFAR-10"));
    }
}
