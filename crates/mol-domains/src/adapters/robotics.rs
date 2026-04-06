//! Robotics & control adapter.

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct RoboticsAdapter {
    profile: DomainProfile,
}

impl RoboticsAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for RoboticsAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Robotics
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Robotics & Control)\n\
         Paradigm: simulation — run each policy for multiple episodes.\n\
         - Report mean cumulative reward ± std, success rate.\n\
         - Compare against classical controllers: PID, LQR, MPC.\n\
         - Use gym/mujoco environments; fix random seeds per episode."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Robotics)\n\
         Core libraries: numpy, scipy, gymnasium, mujoco\n\
         1. Wrap environments in a seed-controlled eval loop.\n\
         2. Run at least 10 evaluation episodes per method.\n\
         3. Report mean ± std of cumulative reward and success rate.\n\
         4. Output results.json:\n\
            {\"conditions\": {\"policy\": {\"mean_reward\": 250.3, \"success_rate\": 0.85}}}"
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
