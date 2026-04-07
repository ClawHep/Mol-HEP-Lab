//! Domain adapter trait and data-driven implementation.
//!
//! Each adapter wraps a [`DomainProfile`] and provides prompt overlays,
//! code-generation hints, and other domain-specific metadata that can be
//! injected into LLM pipeline prompts.

use crate::profile::{DomainProfile, ResearchDomain};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Domain-specific prompt adaptation interface.
///
/// Implementors wrap a [`DomainProfile`] and return strings that can be
/// injected into LLM prompt templates at various pipeline stages.
pub trait DomainAdapter: Send + Sync {
    /// The domain this adapter handles.
    fn domain(&self) -> ResearchDomain;

    /// Extra context block injected into the *experiment design* prompt.
    ///
    /// Should describe paradigm, terminology, and standard baselines.
    fn experiment_prompt_overlay(&self) -> String;

    /// Guidance block injected into the *code generation* prompt.
    ///
    /// Should include language/library hints, output format, and
    /// any domain-specific coding conventions.
    fn code_generation_hints(&self) -> String;

    /// Default Docker image for sandboxed execution.
    fn default_docker_image(&self) -> &str;

    /// Canonical benchmarks / baselines for this domain.
    fn suggested_benchmarks(&self) -> Vec<String>;

    /// Convenience: return the full [`DomainProfile`] backing this adapter.
    fn profile(&self) -> &DomainProfile;
}

// ---------------------------------------------------------------------------
// DomainAdapterImpl — single generic implementation
// ---------------------------------------------------------------------------

/// Generic domain adapter — wraps a [`DomainProfile`] and delegates all methods.
///
/// This single struct replaces the 10 per-domain adapter types that previously
/// existed as separate files.  Domain-specific prompt strings are derived from
/// the [`ResearchDomain`] stored in the profile.
pub struct DomainAdapterImpl {
    profile: DomainProfile,
}

impl DomainAdapterImpl {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for DomainAdapterImpl {
    fn domain(&self) -> ResearchDomain {
        self.profile.domain
    }

    fn experiment_prompt_overlay(&self) -> String {
        match self.profile.domain {
            ResearchDomain::MachineLearning => {
                "## Experiment Design (Machine Learning)\n\
                 Paradigm: comparison — baseline vs proposed method vs ablations.\n\
                 - Use standard train / validation / test splits.\n\
                 - Run at least 3 random seeds; report mean ± std.\n\
                 - Perform a paired t-test for significance (p < 0.05).\n\
                 - Hyperparameters: report all in a HYPERPARAMETERS dictionary."
                    .into()
            }
            ResearchDomain::Physics => {
                "## Experiment Design (Computational Physics)\n\
                 Paradigm: simulation / convergence study.\n\
                 - Compare integrators / methods at the SAME initial conditions and timestep.\n\
                 - For convergence studies: vary dt over [0.1, 0.05, 0.025, 0.0125].\n\
                 - Report energy drift as relative error: |E(t) − E(0)| / |E(0)|.\n\
                 - Standard baselines: Euler, Leapfrog, Velocity Verlet, RK4."
                    .into()
            }
            ResearchDomain::Chemistry => {
                "## Experiment Design (Computational Chemistry)\n\
                 Paradigm: comparison of quantum chemistry methods on standard molecule sets.\n\
                 - Reference method: CCSD(T) or high-level experimental data.\n\
                 - Standard test molecules: H2, H2O, CH4, N2, benzene.\n\
                 - Basis sets: STO-3G for quick tests, cc-pVDZ / cc-pVTZ for production.\n\
                 - Error metric: MAE and RMSE in kcal/mol (1 Ha = 627.509 kcal/mol)."
                    .into()
            }
            ResearchDomain::Biology => {
                "## Experiment Design (Computational Biology)\n\
                 Paradigm: comparison of analysis pipelines or predictive methods.\n\
                 - Use stratified train/test splits for class imbalance.\n\
                 - For single-cell: apply Leiden clustering and UMAP visualisation.\n\
                 - Report AUROC, F1, precision, and recall.\n\
                 - Standard baselines: Logistic Regression, Random Forest, SVM."
                    .into()
            }
            ResearchDomain::Economics => {
                "## Experiment Design (Empirical Economics)\n\
                 Paradigm: progressive specification — OLS → +controls → +FE → +IV.\n\
                 - Generate synthetic panel/cross-section data with a known DGP.\n\
                 - Report coefficient, SE, p-value, R², and N for each specification.\n\
                 - Use robust or clustered standard errors.\n\
                 - Standard baselines: OLS, OLS + controls, Fixed Effects, 2SLS / IV."
                    .into()
            }
            ResearchDomain::Mathematics => {
                "## Experiment Design (Applied Mathematics)\n\
                 Paradigm: convergence study — error vs refinement level.\n\
                 - Run at multiple refinement levels (h, dt, n).\n\
                 - Compute error norms at each level vs analytical / reference solution.\n\
                 - Report in a convergence table: (h, error, empirical order).\n\
                 - Fit log(error) vs log(h) to extract convergence order."
                    .into()
            }
            ResearchDomain::Engineering => {
                "## Experiment Design (Engineering)\n\
                 Paradigm: comparison — benchmark across representative workloads.\n\
                 - Report mean and p99 latency, throughput, and error rate.\n\
                 - Compare against the baseline system or state-of-the-art.\n\
                 - Warm up the system before measurement; report cold-start separately."
                    .into()
            }
            ResearchDomain::Security => {
                "## Experiment Design (Security / Intrusion Detection)\n\
                 Paradigm: comparison — evaluate detection methods on labelled traffic datasets.\n\
                 - Use stratified train/test splits (class imbalance is common).\n\
                 - Report detection rate, false positive rate, AUROC, and F1.\n\
                 - Standard baselines: Random Forest, SVM, Logistic Regression, Isolation Forest.\n\
                 - Tune classification threshold for optimal FPR/TPR trade-off."
                    .into()
            }
            ResearchDomain::Robotics => {
                "## Experiment Design (Robotics & Control)\n\
                 Paradigm: simulation — run each policy for multiple episodes.\n\
                 - Report mean cumulative reward ± std, success rate.\n\
                 - Compare against classical controllers: PID, LQR, MPC.\n\
                 - Use gym/mujoco environments; fix random seeds per episode."
                    .into()
            }
            ResearchDomain::Generic => {
                format!(
                    "## Experiment Design (Generic Computational Research)\n\
                     Paradigm: {paradigm}\n\
                     Compare all methods on the same test problems / datasets.\n\
                     Report primary metric with error bars (mean ± std over multiple seeds).",
                    paradigm = self.profile.paradigm,
                )
            }
        }
    }

    fn code_generation_hints(&self) -> String {
        match self.profile.domain {
            ResearchDomain::MachineLearning => {
                "## Code Generation Hints (Machine Learning)\n\
                 1. Use PyTorch or scikit-learn; follow the train/eval loop pattern.\n\
                 2. Fix random seeds: torch.manual_seed(seed), np.random.seed(seed).\n\
                 3. Track loss and primary metric every epoch.\n\
                 4. Save results as JSON to results.json:\n\
                    {\"conditions\": {\"method\": {\"seed_X\": {\"metric\": value}}}}\n\
                 5. Print HYPERPARAMETERS: {...} to stdout."
                    .into()
            }
            ResearchDomain::Physics => {
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
            ResearchDomain::Chemistry => {
                "## Code Generation Hints (Chemistry)\n\
                 Core libraries: pyscf, numpy, scipy, rdkit\n\
                 1. Define molecules: mol = gto.M(atom='H 0 0 0; H 0 0 0.74', basis='sto-3g')\n\
                 2. Run calculations: mf = scf.RHF(mol); mf.kernel()\n\
                 3. Compare multiple methods on the same molecule set.\n\
                 4. Report energies in Hartree, errors in kcal/mol.\n\
                 5. Output results.json with per-molecule, per-method energy data."
                    .into()
            }
            ResearchDomain::Biology => {
                "## Code Generation Hints (Biology)\n\
                 Core libraries: numpy, scipy, pandas, scikit-learn, biopython, scanpy\n\
                 1. Use stratified splits: train_test_split(..., stratify=y).\n\
                 2. For single-cell: sc.pp.normalize_total, sc.pp.log1p, sc.tl.leiden.\n\
                 3. Report AUROC, F1, precision, recall for each method.\n\
                 4. Use paired t-test or Wilcoxon signed-rank test for significance.\n\
                 5. Output results.json with per-method metric values."
                    .into()
            }
            ResearchDomain::Economics => {
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
            ResearchDomain::Mathematics => {
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
            ResearchDomain::Engineering => {
                "## Code Generation Hints (Engineering)\n\
                 Core libraries: numpy, scipy, matplotlib\n\
                 1. Use timeit or time.perf_counter_ns for timing.\n\
                 2. Run at least 10 trials per configuration; discard the first.\n\
                 3. Report mean ± std, p50, p95, p99 latency.\n\
                 4. Output results.json:\n\
                    {\"conditions\": {\"system\": {\"latency_p99_ms\": 12.4, \"throughput\": 8200}}}"
                    .into()
            }
            ResearchDomain::Security => {
                "## Code Generation Hints (Security)\n\
                 Core libraries: scikit-learn, numpy, pandas, scapy\n\
                 1. Balance classes via SMOTE or class_weight='balanced'.\n\
                 2. Evaluate with sklearn.metrics: roc_auc_score, f1_score, confusion_matrix.\n\
                 3. Report results.json:\n\
                    {\"conditions\": {\"method\": {\"auroc\": 0.97, \"f1\": 0.93, \"fpr\": 0.02}}}\n\
                 4. Generate synthetic labelled traffic data if no public dataset is available."
                    .into()
            }
            ResearchDomain::Robotics => {
                "## Code Generation Hints (Robotics)\n\
                 Core libraries: numpy, scipy, gymnasium, mujoco\n\
                 1. Wrap environments in a seed-controlled eval loop.\n\
                 2. Run at least 10 evaluation episodes per method.\n\
                 3. Report mean ± std of cumulative reward and success rate.\n\
                 4. Output results.json:\n\
                    {\"conditions\": {\"policy\": {\"mean_reward\": 250.3, \"success_rate\": 0.85}}}"
                    .into()
            }
            ResearchDomain::Generic => {
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
        }
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

// ---------------------------------------------------------------------------
// Type aliases for backwards compatibility
// ---------------------------------------------------------------------------

/// Type alias preserved for backwards compatibility.
pub type MlAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type PhysicsAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type ChemistryAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type BiologyAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type EconomicsAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type MathAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type SecurityAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type RoboticsAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type EngineeringAdapter = DomainAdapterImpl;
/// Type alias preserved for backwards compatibility.
pub type GenericAdapter = DomainAdapterImpl;

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create the appropriate [`DomainAdapter`] for the given profile.
pub fn adapter_for(profile: DomainProfile) -> Box<dyn DomainAdapter> {
    Box::new(DomainAdapterImpl::new(profile))
}
