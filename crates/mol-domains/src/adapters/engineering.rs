//! Engineering adapter (systems, distributed computing, hardware).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct EngineeringAdapter {
    profile: DomainProfile,
}

impl EngineeringAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for EngineeringAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Engineering
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Engineering)\n\
         Paradigm: comparison — benchmark across representative workloads.\n\
         - Report mean and p99 latency, throughput, and error rate.\n\
         - Compare against the baseline system or state-of-the-art.\n\
         - Warm up the system before measurement; report cold-start separately."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Engineering)\n\
         Core libraries: numpy, scipy, matplotlib\n\
         1. Use timeit or time.perf_counter_ns for timing.\n\
         2. Run at least 10 trials per configuration; discard the first.\n\
         3. Report mean ± std, p50, p95, p99 latency.\n\
         4. Output results.json:\n\
            {\"conditions\": {\"system\": {\"latency_p99_ms\": 12.4, \"throughput\": 8200}}}"
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
