//! Domain adapter trait and per-domain implementations.
//!
//! Each adapter wraps a [`DomainProfile`] and provides prompt overlays,
//! code-generation hints, and other domain-specific metadata that can be
//! injected into LLM pipeline prompts.

mod biology;
mod chemistry;
mod economics;
mod engineering;
mod generic;
mod math;
mod ml;
mod physics;
mod robotics;
mod security;

pub use biology::BiologyAdapter;
pub use chemistry::ChemistryAdapter;
pub use economics::EconomicsAdapter;
pub use engineering::EngineeringAdapter;
pub use generic::GenericAdapter;
pub use math::MathAdapter;
pub use ml::MlAdapter;
pub use physics::PhysicsAdapter;
pub use robotics::RoboticsAdapter;
pub use security::SecurityAdapter;

use crate::profile::{DomainProfile, ResearchDomain};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Domain-specific prompt adaptation interface.
///
/// Implementors wrap a [`DomainProfile`] and return strings that can be
/// injected into LLM prompt templates at various pipeline stages.
pub trait DomainAdapter: Send + Sync {
    /// The domain this adapter handles.
    fn domain(&self) -> ResearchDomain;

    /// Extra context block injected into the *experiment design* prompt.
    ///
    /// Should describe paradigm, terminology, and standard baselines.
    fn experiment_prompt_overlay(&self) -> String;

    /// Guidance block injected into the *code generation* prompt.
    ///
    /// Should include language/library hints, output format, and
    /// any domain-specific coding conventions.
    fn code_generation_hints(&self) -> String;

    /// Default Docker image for sandboxed execution.
    fn default_docker_image(&self) -> &str;

    /// Canonical benchmarks / baselines for this domain.
    fn suggested_benchmarks(&self) -> Vec<String>;

    /// Convenience: return the full [`DomainProfile`] backing this adapter.
    fn profile(&self) -> &DomainProfile;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create the appropriate [`DomainAdapter`] for the given profile.
pub fn adapter_for(profile: DomainProfile) -> Box<dyn DomainAdapter> {
    match profile.domain {
        ResearchDomain::MachineLearning => Box::new(MlAdapter::new(profile)),
        ResearchDomain::Physics => Box::new(PhysicsAdapter::new(profile)),
        ResearchDomain::Chemistry => Box::new(ChemistryAdapter::new(profile)),
        ResearchDomain::Biology => Box::new(BiologyAdapter::new(profile)),
        ResearchDomain::Economics => Box::new(EconomicsAdapter::new(profile)),
        ResearchDomain::Mathematics => Box::new(MathAdapter::new(profile)),
        ResearchDomain::Security => Box::new(SecurityAdapter::new(profile)),
        ResearchDomain::Robotics => Box::new(RoboticsAdapter::new(profile)),
        ResearchDomain::Engineering => Box::new(EngineeringAdapter::new(profile)),
        ResearchDomain::Generic => Box::new(GenericAdapter::new(profile)),
    }
}
