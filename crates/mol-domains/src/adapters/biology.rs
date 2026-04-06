//! Biology adapter (single-cell, genomics, protein).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct BiologyAdapter {
    profile: DomainProfile,
}

impl BiologyAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for BiologyAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Biology
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Computational Biology)\n\
         Paradigm: comparison of analysis pipelines or predictive methods.\n\
         - Use stratified train/test splits for class imbalance.\n\
         - For single-cell: apply Leiden clustering and UMAP visualisation.\n\
         - Report AUROC, F1, precision, and recall.\n\
         - Standard baselines: Logistic Regression, Random Forest, SVM."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Biology)\n\
         Core libraries: numpy, scipy, pandas, scikit-learn, biopython, scanpy\n\
         1. Use stratified splits: train_test_split(..., stratify=y).\n\
         2. For single-cell: sc.pp.normalize_total, sc.pp.log1p, sc.tl.leiden.\n\
         3. Report AUROC, F1, precision, recall for each method.\n\
         4. Use paired t-test or Wilcoxon signed-rank test for significance.\n\
         5. Output results.json with per-method metric values."
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
