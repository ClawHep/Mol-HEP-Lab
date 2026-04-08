//! Domain-aware prompt injection for different research domains.
//!
//! This module provides a [`PromptAdapter`] trait and concrete implementations
//! that produce [`PromptBlocks`] — structured prompt fragments that can be
//! injected into LLM pipeline prompts at various stages (code generation,
//! experiment design, result analysis).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::profile::{DomainProfile, ExperimentParadigm, ResearchDomain};

// ---------------------------------------------------------------------------
// PromptBlocks
// ---------------------------------------------------------------------------

/// Collection of prompt blocks for a specific pipeline stage.
///
/// Empty strings mean "use the default behavior" — the consuming pipeline
/// should skip injection for any empty field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptBlocks {
    pub compute_budget: String,
    pub dataset_guidance: String,
    pub hp_reporting: String,
    pub code_generation_hints: String,
    pub result_analysis_hints: String,
    pub experiment_design_context: String,
    pub statistical_test_guidance: String,
    pub output_format_guidance: String,
}

// ---------------------------------------------------------------------------
// PromptContext
// ---------------------------------------------------------------------------

/// Context for which pipeline stage needs prompt blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptContext {
    CodeGeneration,
    ExperimentDesign,
    ResultAnalysis,
}

// ---------------------------------------------------------------------------
// PromptAdapter trait
// ---------------------------------------------------------------------------

/// Trait for domain-specific prompt adapters.
///
/// Each implementation wraps a [`DomainProfile`] and produces prompt blocks
/// tailored for a specific research domain.
pub trait PromptAdapter: Send + Sync {
    /// Return prompt blocks for the given pipeline stage.
    fn get_prompt_blocks(&self, context: PromptContext) -> PromptBlocks;

    /// Return a short context string for the experiment blueprint.
    fn get_blueprint_context(&self) -> String;

    /// Return domain-specific condition terminology mappings.
    fn get_condition_terminology(&self) -> HashMap<String, String>;
}

// ---------------------------------------------------------------------------
// MLPromptAdapter
// ---------------------------------------------------------------------------

/// Prompt adapter for ML domains.
///
/// Returns empty [`PromptBlocks`] so the pipeline uses its built-in ML
/// defaults (which already target ML workflows).
pub struct MLPromptAdapter {
    profile: DomainProfile,
}

impl MLPromptAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl PromptAdapter for MLPromptAdapter {
    fn get_prompt_blocks(&self, _context: PromptContext) -> PromptBlocks {
        PromptBlocks::default()
    }

    fn get_blueprint_context(&self) -> String {
        format!(
            "Domain: {} ({}). Paradigm: {}.",
            self.profile.display_name,
            self.profile.domain_id,
            self.profile.paradigm,
        )
    }

    fn get_condition_terminology(&self) -> HashMap<String, String> {
        // ML uses the pipeline's built-in terminology.
        HashMap::new()
    }
}

// ---------------------------------------------------------------------------
// GenericPromptAdapter
// ---------------------------------------------------------------------------

/// Prompt adapter that constructs blocks from a [`DomainProfile`].
///
/// Used for non-ML domains where the pipeline defaults need overriding with
/// domain-specific guidance.
pub struct GenericPromptAdapter {
    profile: DomainProfile,
}

impl GenericPromptAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }

    /// Build code-generation hints from the profile's frameworks and paradigm.
    fn build_code_generation_hints(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Library hints from suggested_frameworks.
        if !self.profile.suggested_frameworks.is_empty() {
            let libs = self.profile.suggested_frameworks.join(", ");
            parts.push(format!("Core libraries: {libs}"));
        }

        // Paradigm-specific hints.
        let paradigm_hint = paradigm_code_hints(&self.profile.paradigm);
        if !paradigm_hint.is_empty() {
            parts.push(paradigm_hint);
        }

        // Experiment templates as additional hints.
        if !self.profile.experiment_templates.is_empty() {
            parts.push("Code generation guidance:".into());
            for tmpl in &self.profile.experiment_templates {
                parts.push(format!("- {tmpl}"));
            }
        }

        parts.join("\n")
    }

    /// Build experiment-design context from the profile.
    fn build_experiment_design_context(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        parts.push(format!("Domain: {}", self.profile.display_name));
        parts.push(format!("Paradigm: {}", self.profile.paradigm));

        if !self.profile.benchmarks.is_empty() {
            let baselines = self.profile.benchmarks.join(", ");
            parts.push(format!("Standard baselines: {baselines}"));
        }

        if !self.profile.default_metrics.is_empty() {
            let metrics: Vec<String> = self
                .profile
                .default_metrics
                .iter()
                .map(|m| format!("{} ({})", m.name, m.description))
                .collect();
            parts.push(format!("Key metrics: {}", metrics.join(", ")));
        }

        parts.join("\n")
    }

    /// Build result-analysis hints from the profile.
    fn build_result_analysis_hints(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if !self.profile.default_metrics.is_empty() {
            let metrics: Vec<String> = self
                .profile
                .default_metrics
                .iter()
                .map(|m| {
                    let direction = match m.metric_type {
                        crate::profile::MetricType::HigherBetter => "higher is better",
                        crate::profile::MetricType::LowerBetter => "lower is better",
                        crate::profile::MetricType::Threshold => "threshold-based",
                    };
                    format!("{}: {} ({})", m.name, m.description, direction)
                })
                .collect();
            parts.push(format!("Metrics to analyse:\n{}", metrics.join("\n")));
        }

        // Paradigm-specific analysis hints.
        let paradigm_hint = paradigm_analysis_hints(&self.profile.paradigm);
        if !paradigm_hint.is_empty() {
            parts.push(paradigm_hint);
        }

        parts.join("\n")
    }
}

impl PromptAdapter for GenericPromptAdapter {
    fn get_prompt_blocks(&self, context: PromptContext) -> PromptBlocks {
        match context {
            PromptContext::CodeGeneration => PromptBlocks {
                code_generation_hints: self.build_code_generation_hints(),
                experiment_design_context: self.build_experiment_design_context(),
                ..Default::default()
            },
            PromptContext::ExperimentDesign => PromptBlocks {
                experiment_design_context: self.build_experiment_design_context(),
                ..Default::default()
            },
            PromptContext::ResultAnalysis => PromptBlocks {
                result_analysis_hints: self.build_result_analysis_hints(),
                experiment_design_context: self.build_experiment_design_context(),
                ..Default::default()
            },
        }
    }

    fn get_blueprint_context(&self) -> String {
        self.build_experiment_design_context()
    }

    fn get_condition_terminology(&self) -> HashMap<String, String> {
        let mut terms = HashMap::new();
        // Map generic pipeline concepts to domain-specific terminology.
        match self.profile.domain {
            ResearchDomain::HighEnergyPhysics => {
                terms.insert("condition".into(), "selection / analysis region".into());
                terms.insert("baseline".into(), "cut-based selection".into());
                terms.insert("metric".into(), "significance / upper limit".into());
                terms.insert("method".into(), "analysis strategy".into());
                terms.insert("trial".into(), "pseudo-experiment".into());
            }
            ResearchDomain::Economics => {
                terms.insert("condition".into(), "specification".into());
                terms.insert("baseline".into(), "OLS regression".into());
                terms.insert("metric".into(), "coefficient estimate".into());
            }
            ResearchDomain::Physics => {
                terms.insert("condition".into(), "integrator / method".into());
                terms.insert("baseline".into(), "reference solution".into());
                terms.insert("metric".into(), "error norm".into());
            }
            ResearchDomain::Chemistry => {
                terms.insert("condition".into(), "method / basis set".into());
                terms.insert("baseline".into(), "reference calculation".into());
                terms.insert("metric".into(), "energy error".into());
            }
            ResearchDomain::Biology => {
                terms.insert("condition".into(), "analysis pipeline".into());
                terms.insert("baseline".into(), "control method".into());
                terms.insert("metric".into(), "classification metric".into());
            }
            ResearchDomain::Mathematics => {
                terms.insert("condition".into(), "method / refinement level".into());
                terms.insert("baseline".into(), "exact solution".into());
                terms.insert("metric".into(), "error / convergence order".into());
            }
            _ => {
                terms.insert("condition".into(), "method".into());
                terms.insert("baseline".into(), "reference".into());
                terms.insert("metric".into(), "primary metric".into());
            }
        }
        terms
    }
}

// ---------------------------------------------------------------------------
// Paradigm-specific hint helpers
// ---------------------------------------------------------------------------

/// Return code-generation hints specific to the experiment paradigm.
fn paradigm_code_hints(paradigm: &ExperimentParadigm) -> String {
    match paradigm {
        ExperimentParadigm::Convergence => {
            "Convergence paradigm: implement refinement studies.\n\
             - Run at multiple refinement levels (h, dt, n).\n\
             - Compute error norms at each level.\n\
             - Fit log(error) vs log(h) to extract convergence order.\n\
             - Report results in a convergence table."
                .into()
        }
        ExperimentParadigm::ProgressiveSpec => {
            "Progressive specification paradigm:\n\
             - Start with simplest specification and progressively add complexity.\n\
             - Each step should build on the previous one.\n\
             - Report results for every specification level."
                .into()
        }
        ExperimentParadigm::Simulation => {
            "Simulation paradigm:\n\
             - Run simulation under controlled initial conditions.\n\
             - Track conserved quantities (energy, momentum) for validation.\n\
             - Report trajectories and time-series metrics.\n\
             - Compare against analytical solutions where available."
                .into()
        }
        ExperimentParadigm::HepAnalysis => {
            "HEP analysis paradigm:\n\
             - Read NTuples with uproot; apply event selection with awkward boolean masks.\n\
             - Build signal, control, and validation regions with orthogonal cuts.\n\
             - Estimate backgrounds: data-driven (ABCD, sideband) or MC-driven (with scale factors).\n\
             - Construct pyhf likelihood with systematic NPs (JES, JER, b-tag SF, luminosity, etc.).\n\
             - Apply blinding: use Asimov data for expected results until unblinding approved.\n\
             - All plots: mplhep style, no titles, axis labels with units, sqrt(s) + luminosity."
                .into()
        }
        _ => String::new(),
    }
}

/// Return result-analysis hints specific to the experiment paradigm.
fn paradigm_analysis_hints(paradigm: &ExperimentParadigm) -> String {
    match paradigm {
        ExperimentParadigm::Convergence => {
            "For convergence analysis:\n\
             - Extract convergence order from log-log error plots.\n\
             - Verify order matches theoretical expectations.\n\
             - Flag any order degradation at fine refinement levels."
                .into()
        }
        ExperimentParadigm::ProgressiveSpec => {
            "For progressive specification analysis:\n\
             - Compare coefficient stability across specifications.\n\
             - Flag large changes in estimates when adding controls.\n\
             - Report standard errors and significance levels."
                .into()
        }
        ExperimentParadigm::Simulation => {
            "For simulation analysis:\n\
             - Verify conservation laws are respected.\n\
             - Report energy/momentum drift over the trajectory.\n\
             - Compare trajectories qualitatively and quantitatively."
                .into()
        }
        ExperimentParadigm::HepAnalysis => {
            "For HEP analysis results:\n\
             - Report observed and expected upper limits (CLs method).\n\
             - Show ±1σ and ±2σ expected limit bands (Brazil plot).\n\
             - Report nuisance parameter pulls and constraints.\n\
             - Show pre-fit and post-fit yields per region.\n\
             - Verify background model closure in validation regions.\n\
             - Report signal efficiency × acceptance vs. signal hypothesis."
                .into()
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Factory function
// ---------------------------------------------------------------------------

/// Return the appropriate [`PromptAdapter`] for the given domain profile.
///
/// - ML domains (domain_id starts with `"ml_"` or domain is `MachineLearning`)
///   get [`MLPromptAdapter`] which returns empty blocks.
/// - All other domains get [`GenericPromptAdapter`] which builds blocks from
///   the profile fields.
pub fn get_adapter(profile: &DomainProfile) -> Box<dyn PromptAdapter> {
    if profile.domain_id.starts_with("ml_")
        || profile.domain == ResearchDomain::MachineLearning
    {
        Box::new(MLPromptAdapter::new(profile.clone()))
    } else {
        Box::new(GenericPromptAdapter::new(profile.clone()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::DomainProfile;

    #[test]
    fn ml_adapter_returns_empty_blocks() {
        let profile = DomainProfile::generic();
        let adapter = MLPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.is_empty());
    }

    #[test]
    fn generic_adapter_includes_library_hints() {
        let mut profile = DomainProfile::generic();
        profile.suggested_frameworks = vec!["numpy".into(), "scipy".into()];
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.contains("numpy"));
    }

    #[test]
    fn convergence_paradigm_includes_refinement_hints() {
        let mut profile = DomainProfile::generic();
        profile.paradigm = ExperimentParadigm::Convergence;
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.contains("refinement"));
    }

    #[test]
    fn get_adapter_returns_ml_for_ml_domain() {
        let mut profile = DomainProfile::generic();
        profile.domain_id = "ml_classification".into();
        let adapter = get_adapter(&profile);
        // ML adapter returns empty blocks.
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.is_empty());
    }

    #[test]
    fn generic_adapter_experiment_design_context() {
        let profile = crate::profile::load_profile(ResearchDomain::Physics);
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::ExperimentDesign);
        assert!(blocks.experiment_design_context.contains("Computational Physics"));
        assert!(blocks.experiment_design_context.contains("simulation"));
    }

    #[test]
    fn generic_adapter_result_analysis() {
        let profile = crate::profile::load_profile(ResearchDomain::Economics);
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::ResultAnalysis);
        assert!(!blocks.result_analysis_hints.is_empty());
    }

    #[test]
    fn get_adapter_returns_generic_for_physics() {
        let profile = crate::profile::load_profile(ResearchDomain::Physics);
        let adapter = get_adapter(&profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        // Physics should produce non-empty code hints.
        assert!(!blocks.code_generation_hints.is_empty());
    }

    #[test]
    fn condition_terminology_for_economics() {
        let profile = crate::profile::load_profile(ResearchDomain::Economics);
        let adapter = GenericPromptAdapter::new(profile);
        let terms = adapter.get_condition_terminology();
        assert_eq!(terms.get("condition"), Some(&"specification".to_string()));
    }

    #[test]
    fn hep_adapter_includes_pyhf_hints() {
        let profile = crate::profile::load_profile(ResearchDomain::HighEnergyPhysics);
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.contains("uproot"));
        assert!(blocks.code_generation_hints.contains("pyhf"));
    }

    #[test]
    fn hep_condition_terminology() {
        let profile = crate::profile::load_profile(ResearchDomain::HighEnergyPhysics);
        let adapter = GenericPromptAdapter::new(profile);
        let terms = adapter.get_condition_terminology();
        assert_eq!(terms.get("baseline"), Some(&"cut-based selection".to_string()));
    }

    #[test]
    fn ml_adapter_empty_terminology() {
        let profile = crate::profile::load_profile(ResearchDomain::MachineLearning);
        let adapter = MLPromptAdapter::new(profile);
        assert!(adapter.get_condition_terminology().is_empty());
    }

    #[test]
    fn progressive_spec_paradigm_hints() {
        let mut profile = DomainProfile::generic();
        profile.paradigm = ExperimentParadigm::ProgressiveSpec;
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.contains("Progressive specification"));
    }

    #[test]
    fn simulation_paradigm_hints() {
        let mut profile = DomainProfile::generic();
        profile.paradigm = ExperimentParadigm::Simulation;
        let adapter = GenericPromptAdapter::new(profile);
        let blocks = adapter.get_prompt_blocks(PromptContext::CodeGeneration);
        assert!(blocks.code_generation_hints.contains("Simulation paradigm"));
    }
}
