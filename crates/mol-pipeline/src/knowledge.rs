//! Pipeline-level knowledge helpers built on top of [`mol_common::KnowledgeChain`].
//!
//! This module provides domain-aware loaders that read structured YAML files
//! from the knowledge chain (e.g. `agents.yaml`, `datasets.yaml`).

use crate::stages::Stage;
use mol_common::KnowledgeChain;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AgentMapping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
struct AgentMappingFile {
    #[serde(default)]
    stage_agents: HashMap<String, String>,
    #[serde(default)]
    advisors: HashMap<String, Vec<String>>,
}

/// Data-driven stage→agent mapping loaded from `agents.yaml` in the knowledge chain.
#[derive(Debug, Clone, Default)]
pub struct AgentMapping {
    map: HashMap<String, String>,
    advisor_map: HashMap<String, Vec<String>>,
}

impl AgentMapping {
    /// Load from KnowledgeChain. Reads all layers and merges (general first,
    /// then specific layers override).
    pub fn load(chain: &KnowledgeChain) -> Self {
        let mut map = HashMap::new();
        let mut advisor_map: HashMap<String, Vec<String>> = HashMap::new();
        // Read in reverse order (general → specific) so specific wins
        let all_yaml = chain.read_all("agents.yaml");
        for yaml_content in all_yaml.into_iter().rev() {
            if let Ok(file) = serde_yaml::from_str::<AgentMappingFile>(&yaml_content) {
                for (stage_name, agent_name) in file.stage_agents {
                    map.insert(stage_name, agent_name);
                }
                for (stage_name, advisors) in file.advisors {
                    advisor_map.insert(stage_name, advisors);
                }
            }
        }
        Self { map, advisor_map }
    }

    /// Look up agent for a stage. Returns None if no mapping exists.
    pub fn agent_for(&self, stage: Stage) -> Option<&str> {
        self.map.get(stage.name()).map(|s| s.as_str())
    }

    /// Look up advisory agents for a stage. Returns empty slice if none.
    pub fn advisors_for(&self, stage: Stage) -> Vec<&str> {
        self.advisor_map
            .get(stage.name())
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// DatasetsConfig
// ---------------------------------------------------------------------------

/// A single dataset entry from `datasets.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetEntry {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub description: String,
}

/// Datasets configuration loaded from the knowledge chain.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatasetsConfig {
    #[serde(default)]
    pub datasets: Vec<DatasetEntry>,
}

impl DatasetsConfig {
    /// Load from the first `datasets.yaml` found in the chain.
    pub fn load(chain: &KnowledgeChain) -> Self {
        match chain.read_first("datasets.yaml") {
            Some(yaml) => serde_yaml::from_str(&yaml).unwrap_or_default(),
            None => Self::default(),
        }
    }

    /// Format datasets as a string for template injection.
    pub fn to_template_string(&self) -> String {
        if self.datasets.is_empty() {
            return "(no datasets configured)".to_owned();
        }
        self.datasets
            .iter()
            .map(|d| {
                let desc = if d.description.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", d.description)
                };
                let location = if !d.path.is_empty() {
                    format!("`{}`", d.path)
                } else if !d.url.is_empty() {
                    d.url.clone()
                } else {
                    "(no path)".to_owned()
                };
                format!("- **{}**: {} ({}){}", d.name, location, d.format, desc)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn agent_mapping_loads_from_yaml() {
        let dir = TempDir::new().unwrap();
        let layer = dir.path().join("hep");
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(
            layer.join("agents.yaml"),
            "stage_agents:\n  TOPIC_INIT: lead-analyst\n  CODE_GENERATION: signal-lead\n",
        )
        .unwrap();
        let chain = KnowledgeChain::new(vec![layer]);
        let mapping = AgentMapping::load(&chain);
        assert_eq!(mapping.agent_for(Stage::TopicInit), Some("lead-analyst"));
        assert_eq!(
            mapping.agent_for(Stage::CodeGeneration),
            Some("signal-lead")
        );
        assert_eq!(mapping.agent_for(Stage::Discussion), None);
    }

    #[test]
    fn agent_mapping_merges_layers() {
        let dir = TempDir::new().unwrap();
        let specific = dir.path().join("specific");
        let general = dir.path().join("general");
        std::fs::create_dir_all(&specific).unwrap();
        std::fs::create_dir_all(&general).unwrap();
        std::fs::write(
            general.join("agents.yaml"),
            "stage_agents:\n  TOPIC_INIT: general-analyst\n  CODE_GENERATION: general-coder\n",
        )
        .unwrap();
        std::fs::write(
            specific.join("agents.yaml"),
            "stage_agents:\n  CODE_GENERATION: specific-coder\n",
        )
        .unwrap();
        let chain = KnowledgeChain::new(vec![specific, general]);
        let mapping = AgentMapping::load(&chain);
        assert_eq!(
            mapping.agent_for(Stage::CodeGeneration),
            Some("specific-coder")
        );
        assert_eq!(
            mapping.agent_for(Stage::TopicInit),
            Some("general-analyst")
        );
    }

    #[test]
    fn advisors_load_from_yaml() {
        let dir = TempDir::new().unwrap();
        let layer = dir.path().join("hep");
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(
            layer.join("agents.yaml"),
            "stage_agents:\n  TOPIC_INIT: lead-analyst\nadvisors:\n  CODE_GENERATION: [background-estimator, ml-specialist]\n  SANITY_CHECK: [plot-validator]\n",
        )
        .unwrap();
        let chain = KnowledgeChain::new(vec![layer]);
        let mapping = AgentMapping::load(&chain);
        assert_eq!(mapping.agent_for(Stage::TopicInit), Some("lead-analyst"));
        let advisors = mapping.advisors_for(Stage::CodeGeneration);
        assert_eq!(advisors, vec!["background-estimator", "ml-specialist"]);
        assert_eq!(mapping.advisors_for(Stage::SanityCheck), vec!["plot-validator"]);
        assert!(mapping.advisors_for(Stage::TopicInit).is_empty());
    }

    #[test]
    fn agent_mapping_empty_chain() {
        let chain = KnowledgeChain::new(vec![]);
        let mapping = AgentMapping::load(&chain);
        assert_eq!(mapping.agent_for(Stage::TopicInit), None);
    }

    #[test]
    fn datasets_loads_from_yaml() {
        let dir = TempDir::new().unwrap();
        let layer = dir.path().join("hep");
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(
            layer.join("datasets.yaml"),
            "datasets:\n  - name: signal_mc\n    path: /data/signal.root\n    format: root\n    description: Signal MC\n",
        )
        .unwrap();
        let chain = KnowledgeChain::new(vec![layer]);
        let ds = DatasetsConfig::load(&chain);
        assert_eq!(ds.datasets.len(), 1);
        assert_eq!(ds.datasets[0].name, "signal_mc");
    }

    #[test]
    fn datasets_empty_chain() {
        let chain = KnowledgeChain::new(vec![]);
        let ds = DatasetsConfig::load(&chain);
        assert!(ds.datasets.is_empty());
        assert_eq!(ds.to_template_string(), "(no datasets configured)");
    }
}
