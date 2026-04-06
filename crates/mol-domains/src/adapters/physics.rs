//! Physics adapter (simulation, PDE, quantum).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct PhysicsAdapter {
    profile: DomainProfile,
}

impl PhysicsAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for PhysicsAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Physics
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Computational Physics)\n\
         Paradigm: simulation / convergence study.\n\
         - Compare integrators / methods at the SAME initial conditions and timestep.\n\
         - For convergence studies: vary dt over [0.1, 0.05, 0.025, 0.0125].\n\
         - Report energy drift as relative error: |E(t) − E(0)| / |E(0)|.\n\
         - Standard baselines: Euler, Leapfrog, Velocity Verlet, RK4."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Physics)\n\
         Core libraries: numpy, scipy, jax\n\
         1. Implement actual physics — conserve energy and momentum as appropriate.\n\
         2. Use appropriate units (reduced units for MD, SI otherwise).\n\
         3. For convergence: run at multiple dt values and fit log(error) vs log(h).\n\
         4. Output results.json with convergence data:\n\
            {\"convergence\": {\"method\": [{\"h\": 0.1, \"error\": 0.05}, ...]}}\n\
         5. Use log-log plots for convergence, linear for time evolution."
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
