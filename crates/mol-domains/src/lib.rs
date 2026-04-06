//! mol-domains — Scientific domain models and ontologies for Mol-HEP-Lab.
//!
//! # Overview
//!
//! This crate provides:
//!
//! * **[`profile`]** — Core types ([`ResearchDomain`], [`ExperimentParadigm`],
//!   [`MetricType`], [`DomainProfile`], [`DomainMetric`]) and the
//!   [`load_profile`] factory function that returns a static [`DomainProfile`]
//!   for any [`ResearchDomain`].
//!
//! * **[`detector`]** — Fast keyword-based domain detection
//!   ([`detector::detect_domain`]) and an LLM-response parser
//!   ([`detector::detect_domain_with_llm`]).
//!
//! * **[`adapters`]** — The [`DomainAdapter`] trait and per-domain
//!   implementations ([`MlAdapter`], [`PhysicsAdapter`], …).
//!
//! # Quick start
//!
//! ```rust
//! use mol_domains::{detect_domain, load_profile, adapters::adapter_for};
//!
//! let domain = detect_domain("Training a PyTorch transformer on ImageNet");
//! let profile = load_profile(domain);
//! let adapter = adapter_for(profile);
//!
//! println!("Detected: {}", adapter.domain());
//! println!("Docker image: {}", adapter.default_docker_image());
//! println!("{}", adapter.code_generation_hints());
//! ```

pub mod adapters;
pub mod detector;
pub mod experiment_schema;
pub mod profile;
pub mod prompt_adapter;

// ---------------------------------------------------------------------------
// Flat re-exports for convenience
// ---------------------------------------------------------------------------

pub use adapters::{
    adapter_for, BiologyAdapter, ChemistryAdapter, DomainAdapter, EconomicsAdapter,
    EngineeringAdapter, GenericAdapter, MathAdapter, MlAdapter, PhysicsAdapter, RoboticsAdapter,
    SecurityAdapter,
};

pub use detector::{detect_domain, detect_domain_with_llm, domain_keywords};

pub use experiment_schema::{
    from_legacy_exp_plan, Condition, ConditionRole, EvaluationSpec, ExperimentType, MetricSpec,
    UniversalExperimentPlan,
};

pub use profile::{
    load_profile, DomainMetric, DomainProfile, ExperimentParadigm, MetricType, ResearchDomain,
};

pub use prompt_adapter::{
    get_adapter as get_prompt_adapter, GenericPromptAdapter, MLPromptAdapter, PromptAdapter,
    PromptBlocks, PromptContext,
};
