//! Security adapter (intrusion detection, anomaly detection).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct SecurityAdapter {
    profile: DomainProfile,
}

impl SecurityAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for SecurityAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Security
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Security / Intrusion Detection)\n\
         Paradigm: comparison — evaluate detection methods on labelled traffic datasets.\n\
         - Use stratified train/test splits (class imbalance is common).\n\
         - Report detection rate, false positive rate, AUROC, and F1.\n\
         - Standard baselines: Random Forest, SVM, Logistic Regression, Isolation Forest.\n\
         - Tune classification threshold for optimal FPR/TPR trade-off."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Security)\n\
         Core libraries: scikit-learn, numpy, pandas, scapy\n\
         1. Balance classes via SMOTE or class_weight='balanced'.\n\
         2. Evaluate with sklearn.metrics: roc_auc_score, f1_score, confusion_matrix.\n\
         3. Report results.json:\n\
            {\"conditions\": {\"method\": {\"auroc\": 0.97, \"f1\": 0.93, \"fpr\": 0.02}}}\n\
         4. Generate synthetic labelled traffic data if no public dataset is available."
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
