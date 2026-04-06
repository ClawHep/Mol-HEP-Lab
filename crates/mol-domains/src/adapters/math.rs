//! Mathematics adapter (numerical analysis, optimisation).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct MathAdapter {
    profile: DomainProfile,
}

impl MathAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for MathAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Mathematics
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Applied Mathematics)\n\
         Paradigm: convergence study — error vs refinement level.\n\
         - Run at multiple refinement levels (h, dt, n).\n\
         - Compute error norms at each level vs analytical / reference solution.\n\
         - Report in a convergence table: (h, error, empirical order).\n\
         - Fit log(error) vs log(h) to extract convergence order."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Mathematics)\n\
         Core libraries: numpy, scipy, sympy\n\
         1. Define test problems with known analytical solutions.\n\
         2. Run method at refinement levels: [0.1, 0.05, 0.025, 0.0125, ...].\n\
         3. Compute absolute and relative errors at each level.\n\
         4. Fit convergence order: np.polyfit(np.log(hs), np.log(errors), 1)[0].\n\
         5. Output results.json:\n\
            {\"convergence\": {\"method\": [{\"h\": 0.1, \"error\": 0.05, \"order\": 2.0}, ...]}}"
            .into()
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
