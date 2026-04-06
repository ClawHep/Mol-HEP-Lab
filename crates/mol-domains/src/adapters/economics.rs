//! Economics adapter (empirical / causal inference).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct EconomicsAdapter {
    profile: DomainProfile,
}

impl EconomicsAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for EconomicsAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Economics
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Empirical Economics)\n\
         Paradigm: progressive specification — OLS → +controls → +FE → +IV.\n\
         - Generate synthetic panel/cross-section data with a known DGP.\n\
         - Report coefficient, SE, p-value, R², and N for each specification.\n\
         - Use robust or clustered standard errors.\n\
         - Standard baselines: OLS, OLS + controls, Fixed Effects, 2SLS / IV."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Economics)\n\
         Core libraries: statsmodels, linearmodels, pandas, numpy\n\
         1. Generate synthetic data with known treatment effect (DGP).\n\
         2. Implement progressive specifications (OLS → +controls → +FE → +IV).\n\
         3. Use robust/clustered SE: cov_type='HC3' or cluster_entity=True.\n\
         4. Report regression table to results.json:\n\
            {\"regression_table\": {\"spec_1_ols\": {\"coeff\": 0.15, \"se\": 0.03, ...}}}\n\
         5. Bootstrap SE if needed (100-500 replications)."
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
