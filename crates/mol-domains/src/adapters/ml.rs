//! Machine Learning adapter.

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct MlAdapter {
    profile: DomainProfile,
}

impl MlAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for MlAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::MachineLearning
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Machine Learning)\n\
         Paradigm: comparison — baseline vs proposed method vs ablations.\n\
         - Use standard train / validation / test splits.\n\
         - Run at least 3 random seeds; report mean ± std.\n\
         - Perform a paired t-test for significance (p < 0.05).\n\
         - Hyperparameters: report all in a HYPERPARAMETERS dictionary."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Machine Learning)\n\
         1. Use PyTorch or scikit-learn; follow the train/eval loop pattern.\n\
         2. Fix random seeds: torch.manual_seed(seed), np.random.seed(seed).\n\
         3. Track loss and primary metric every epoch.\n\
         4. Save results as JSON to results.json:\n\
            {\"conditions\": {\"method\": {\"seed_X\": {\"metric\": value}}}}\n\
         5. Print HYPERPARAMETERS: {...} to stdout."
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
