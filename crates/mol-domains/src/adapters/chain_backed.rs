//! Knowledge-chain-backed domain adapter.
//!
//! Reads all domain behaviour from files in the knowledge chain,
//! replacing hardcoded match arms with file-system lookups.

use std::collections::HashMap;

use mol_common::KnowledgeChain;
use serde::Deserialize;

use crate::profile::{
    DomainMetric, DomainProfile, ExperimentParadigm, MetricType, ResearchDomain,
};
use crate::prompt_adapter::{PromptAdapter, PromptBlocks, PromptContext};

use super::DomainAdapter;

// ---------------------------------------------------------------------------
// domain.yaml schema
// ---------------------------------------------------------------------------

/// Deserialized `domain.yaml` from the knowledge chain.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainProfileFile {
    #[serde(default = "default_domain")]
    pub domain: ResearchDomain,
    #[serde(default)]
    pub domain_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub paradigm: ExperimentParadigm,
    #[serde(default)]
    pub gpu_required: bool,
    #[serde(default = "default_docker_image")]
    pub docker_image: String,
    #[serde(default)]
    pub default_metrics: Vec<MetricDef>,
    #[serde(default)]
    pub benchmarks: Vec<String>,
    #[serde(default)]
    pub pip_packages: Vec<String>,
    #[serde(default)]
    pub suggested_frameworks: Vec<String>,
    #[serde(default)]
    pub experiment_templates: Vec<String>,
    #[serde(default)]
    pub condition_terminology: HashMap<String, String>,
}

fn default_domain() -> ResearchDomain {
    ResearchDomain::Generic
}

fn default_docker_image() -> String {
    "researchmol/sandbox-generic:latest".into()
}

/// Metric definition from YAML.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricDef {
    pub name: String,
    #[serde(default)]
    pub metric_type: MetricType,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub unit: String,
}

impl From<MetricDef> for DomainMetric {
    fn from(m: MetricDef) -> Self {
        DomainMetric::new(m.name, m.metric_type, m.description, m.unit)
    }
}

impl DomainProfileFile {
    /// Convert to a full `DomainProfile`.
    pub fn into_profile(self) -> DomainProfile {
        DomainProfile {
            domain: self.domain,
            domain_id: self.domain_id,
            display_name: self.display_name,
            paradigm: self.paradigm,
            default_metrics: self.default_metrics.into_iter().map(Into::into).collect(),
            benchmarks: self.benchmarks,
            docker_image: self.docker_image,
            pip_packages: self.pip_packages,
            suggested_frameworks: self.suggested_frameworks,
            experiment_templates: self.experiment_templates,
            gpu_required: self.gpu_required,
        }
    }
}

// ---------------------------------------------------------------------------
// ChainBackedAdapter
// ---------------------------------------------------------------------------

/// Domain adapter that reads all behaviour from knowledge chain files.
///
/// Replaces `DomainAdapterImpl` and the 10 type aliases. Reads:
/// - `domain.yaml` → profile metadata, terminology
/// - `prompts/experiment_design.md` → experiment design overlay
/// - `prompts/code_generation.md` → code generation hints
/// - `prompts/result_analysis.md` → result analysis hints
pub struct ChainBackedAdapter {
    chain: KnowledgeChain,
    profile: DomainProfile,
    condition_terminology: HashMap<String, String>,
}

impl ChainBackedAdapter {
    /// Load a `ChainBackedAdapter` from the given knowledge chain.
    ///
    /// Reads `domain.yaml` from the first layer that has it. Falls back
    /// to `DomainProfile::generic()` if no `domain.yaml` exists.
    pub fn load(chain: KnowledgeChain, domain: ResearchDomain) -> Self {
        let (profile, terminology) = match chain.read_first("domain.yaml") {
            Some(text) => match serde_yaml::from_str::<DomainProfileFile>(&text) {
                Ok(file) => {
                    let terms = file.condition_terminology.clone();
                    (file.into_profile(), terms)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse domain.yaml; using static profile");
                    (crate::profile::load_profile(domain), HashMap::new())
                }
            },
            None => {
                // No domain.yaml in chain — fall back to static profile
                (crate::profile::load_profile(domain), HashMap::new())
            }
        };

        Self {
            chain,
            profile,
            condition_terminology: terminology,
        }
    }

    /// Access the underlying knowledge chain.
    pub fn chain(&self) -> &KnowledgeChain {
        &self.chain
    }
}

// ---------------------------------------------------------------------------
// DomainAdapter impl
// ---------------------------------------------------------------------------

impl DomainAdapter for ChainBackedAdapter {
    fn domain(&self) -> ResearchDomain {
        self.profile.domain
    }

    fn experiment_prompt_overlay(&self) -> String {
        self.chain
            .read_first("prompts/experiment_design.md")
            .unwrap_or_else(|| {
                // Fallback: build from profile data
                format!(
                    "## Experiment Design ({})\nParadigm: {}",
                    self.profile.display_name, self.profile.paradigm
                )
            })
    }

    fn code_generation_hints(&self) -> String {
        self.chain
            .read_first("prompts/code_generation.md")
            .unwrap_or_else(|| {
                let libs = self.profile.suggested_frameworks.join(", ");
                format!(
                    "## Code Generation Hints\nCore libraries: {libs}\nOutput results as JSON to results.json."
                )
            })
    }

    fn default_docker_image(&self) -> &str {
        &self.profile.docker_image
    }

    fn suggested_benchmarks(&self) -> Vec<String> {
        self.profile.benchmarks.clone()
    }

    fn profile(&self) -> &DomainProfile {
        &self.profile
    }
}

// ---------------------------------------------------------------------------
// PromptAdapter impl
// ---------------------------------------------------------------------------

impl PromptAdapter for ChainBackedAdapter {
    fn get_prompt_blocks(&self, context: PromptContext) -> PromptBlocks {
        match context {
            PromptContext::CodeGeneration => PromptBlocks {
                code_generation_hints: self.code_generation_hints(),
                experiment_design_context: self.experiment_prompt_overlay(),
                ..Default::default()
            },
            PromptContext::ExperimentDesign => PromptBlocks {
                experiment_design_context: self.experiment_prompt_overlay(),
                ..Default::default()
            },
            PromptContext::ResultAnalysis => PromptBlocks {
                result_analysis_hints: self
                    .chain
                    .read_first("prompts/result_analysis.md")
                    .unwrap_or_default(),
                experiment_design_context: self.experiment_prompt_overlay(),
                ..Default::default()
            },
        }
    }

    fn get_blueprint_context(&self) -> String {
        format!(
            "Domain: {} ({}). Paradigm: {}.",
            self.profile.display_name, self.profile.domain_id, self.profile.paradigm,
        )
    }

    fn get_condition_terminology(&self) -> HashMap<String, String> {
        self.condition_terminology.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_chain(dir: &std::path::Path) -> KnowledgeChain {
        KnowledgeChain::new(vec![dir.to_owned()])
    }

    #[test]
    fn loads_domain_yaml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("domain.yaml"),
            r#"
domain: high_energy_physics
domain_id: hep_collider
display_name: "High Energy Physics"
paradigm: hep_analysis
docker_image: "researchmol/sandbox-hep:latest"
benchmarks:
  - "Cut-based selection"
  - "BDT (XGBoost)"
condition_terminology:
  baseline: "cut-based selection"
"#,
        )
        .unwrap();

        let chain = make_chain(dir.path());
        let adapter = ChainBackedAdapter::load(chain, ResearchDomain::HighEnergyPhysics);

        assert_eq!(adapter.domain(), ResearchDomain::HighEnergyPhysics);
        assert_eq!(adapter.default_docker_image(), "researchmol/sandbox-hep:latest");
        assert_eq!(adapter.suggested_benchmarks().len(), 2);
        assert_eq!(
            adapter.get_condition_terminology().get("baseline"),
            Some(&"cut-based selection".to_string())
        );
    }

    #[test]
    fn reads_prompt_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
        std::fs::write(
            dir.path().join("prompts/experiment_design.md"),
            "## HEP experiment design context",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("prompts/code_generation.md"),
            "Use uproot and pyhf",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("prompts/result_analysis.md"),
            "Report CLs limits",
        )
        .unwrap();

        let chain = make_chain(dir.path());
        let adapter = ChainBackedAdapter::load(chain, ResearchDomain::Generic);

        assert!(adapter.experiment_prompt_overlay().contains("HEP experiment"));
        assert!(adapter.code_generation_hints().contains("uproot"));

        let blocks = adapter.get_prompt_blocks(PromptContext::ResultAnalysis);
        assert!(blocks.result_analysis_hints.contains("CLs"));
    }

    #[test]
    fn falls_back_to_static_profile_without_domain_yaml() {
        let dir = tempfile::TempDir::new().unwrap();
        let chain = make_chain(dir.path());
        let adapter = ChainBackedAdapter::load(chain, ResearchDomain::HighEnergyPhysics);

        // Should fall back to hep_profile()
        assert_eq!(adapter.profile().domain_id, "hep_collider");
        assert!(!adapter.profile().pip_packages.is_empty());
    }

    #[test]
    fn fallback_overlay_when_no_prompt_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let chain = make_chain(dir.path());
        let adapter = ChainBackedAdapter::load(chain, ResearchDomain::HighEnergyPhysics);

        // Should produce a fallback overlay string
        let overlay = adapter.experiment_prompt_overlay();
        assert!(overlay.contains("Experiment Design"));
        assert!(overlay.contains("High Energy Physics"));
    }

    #[test]
    fn layered_chain_specific_overrides_general() {
        let general_dir = tempfile::TempDir::new().unwrap();
        let specific_dir = tempfile::TempDir::new().unwrap();

        // General layer
        std::fs::create_dir_all(general_dir.path().join("prompts")).unwrap();
        std::fs::write(
            general_dir.path().join("prompts/code_generation.md"),
            "General hints",
        )
        .unwrap();

        // Specific layer (searched first)
        std::fs::create_dir_all(specific_dir.path().join("prompts")).unwrap();
        std::fs::write(
            specific_dir.path().join("prompts/code_generation.md"),
            "CEPC-specific hints",
        )
        .unwrap();

        let chain = KnowledgeChain::new(vec![
            specific_dir.path().to_owned(),
            general_dir.path().to_owned(),
        ]);
        let adapter = ChainBackedAdapter::load(chain, ResearchDomain::HighEnergyPhysics);
        assert!(adapter.code_generation_hints().contains("CEPC-specific"));
    }

    #[test]
    fn blueprint_context_uses_profile() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("domain.yaml"),
            r#"
domain: high_energy_physics
domain_id: hep_collider
display_name: "High Energy Physics"
paradigm: hep_analysis
"#,
        )
        .unwrap();

        let chain = make_chain(dir.path());
        let adapter = ChainBackedAdapter::load(chain, ResearchDomain::HighEnergyPhysics);
        let ctx = adapter.get_blueprint_context();
        assert!(ctx.contains("High Energy Physics"));
        assert!(ctx.contains("hep_collider"));
    }
}
