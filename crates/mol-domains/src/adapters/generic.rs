//! Generic adapter — used as a fallback for any unrecognised domain.

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct GenericAdapter {
    profile: DomainProfile,
}

impl GenericAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for GenericAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Generic
    }

    fn experiment_prompt_overlay(&self) -> String {
        format!(
            "## Experiment Design (Generic Computational Research)\n\
             Paradigm: {paradigm}\n\
             Compare all methods on the same test problems / datasets.\n\
             Report primary metric with error bars (mean ± std over multiple seeds).",
            paradigm = self.profile.paradigm,
        )
    }

    fn code_generation_hints(&self) -> String {
        let libs = self.profile.suggested_frameworks.join(", ");
        format!(
            "## Code Generation Hints\n\
             Core libraries: {libs}\n\
             1. Implement all methods from the experiment plan.\n\
             2. Use the same test problems / data for all methods.\n\
             3. Include error bars / multiple seeds where applicable.\n\
             4. Output all results as JSON to results.json.\n\
             5. Print all parameters: HYPERPARAMETERS: {{...}}"
        )
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
