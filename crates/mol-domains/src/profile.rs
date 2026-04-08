//! Domain profile types: enums, structs, and static profile loading.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// High-level research domain classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchDomain {
    HighEnergyPhysics,
    MachineLearning,
    Physics,
    Chemistry,
    Biology,
    Economics,
    Mathematics,
    Engineering,
    Security,
    Robotics,
    Generic,
}

impl ResearchDomain {
    /// Return a human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::HighEnergyPhysics => "High Energy Physics",
            Self::MachineLearning => "Machine Learning",
            Self::Physics => "Computational Physics",
            Self::Chemistry => "Computational Chemistry",
            Self::Biology => "Computational Biology",
            Self::Economics => "Empirical Economics",
            Self::Mathematics => "Applied Mathematics",
            Self::Engineering => "Engineering",
            Self::Security => "Security / Intrusion Detection",
            Self::Robotics => "Robotics & Control",
            Self::Generic => "Generic Computational Research",
        }
    }

    /// Return the canonical domain_id prefix used in the Python profiles.
    pub fn domain_prefix(&self) -> &'static str {
        match self {
            Self::HighEnergyPhysics => "hep",
            Self::MachineLearning => "ml",
            Self::Physics => "physics",
            Self::Chemistry => "chemistry",
            Self::Biology => "biology",
            Self::Economics => "economics",
            Self::Mathematics => "mathematics",
            Self::Engineering => "engineering",
            Self::Security => "security",
            Self::Robotics => "robotics",
            Self::Generic => "generic",
        }
    }
}

impl std::fmt::Display for ResearchDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

/// High-level experiment structure/paradigm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentParadigm {
    /// Method A vs method B (ML, security) — the default paradigm.
    Comparison,
    /// Error vs refinement level (math, physics PDE).
    Convergence,
    /// OLS → +FE → +IV stepwise (economics).
    ProgressiveSpec,
    /// Run → observe → analyze (physics simulation, robotics).
    Simulation,
    /// Systematic removal of components.
    AblationStudy,
    /// HEP analysis: event selection → background estimation → statistical inference.
    HepAnalysis,
}

impl ExperimentParadigm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Comparison => "comparison",
            Self::Convergence => "convergence",
            Self::ProgressiveSpec => "progressive_spec",
            Self::Simulation => "simulation",
            Self::AblationStudy => "ablation_study",
            Self::HepAnalysis => "hep_analysis",
        }
    }
}

impl std::fmt::Display for ExperimentParadigm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for ExperimentParadigm {
    fn default() -> Self {
        Self::Comparison
    }
}

/// How to interpret a metric value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Higher values are better (accuracy, F1, R²).
    HigherBetter,
    /// Lower values are better (loss, error, latency).
    LowerBetter,
    /// A value must be at or below/above a fixed threshold.
    Threshold,
}

impl Default for MetricType {
    fn default() -> Self {
        Self::LowerBetter
    }
}

// ---------------------------------------------------------------------------
// DomainMetric
// ---------------------------------------------------------------------------

/// A single evaluation metric associated with a domain profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainMetric {
    pub name: String,
    pub metric_type: MetricType,
    pub description: String,
    pub unit: String,
}

impl DomainMetric {
    pub fn new(
        name: impl Into<String>,
        metric_type: MetricType,
        description: impl Into<String>,
        unit: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            metric_type,
            description: description.into(),
            unit: unit.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// DomainProfile
// ---------------------------------------------------------------------------

/// Complete description of a research domain's experiment conventions.
///
/// This mirrors the Python `DomainProfile` dataclass loaded from YAML profiles,
/// but is expressed as a static Rust struct rather than requiring YAML at
/// runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainProfile {
    /// Primary domain classification.
    pub domain: ResearchDomain,
    /// Fine-grained identifier matching the Python profile (e.g. `"ml_vision"`).
    pub domain_id: String,
    /// Human-readable name shown in UIs.
    pub display_name: String,
    /// Experiment structure paradigm.
    pub paradigm: ExperimentParadigm,
    /// Key metrics tracked in this domain.
    pub default_metrics: Vec<DomainMetric>,
    /// Well-known benchmarks / baselines in this domain.
    pub benchmarks: Vec<String>,
    /// Default Docker image (researchmol/sandbox-*).
    pub docker_image: String,
    /// Python packages typically required.
    pub pip_packages: Vec<String>,
    /// Framework / library suggestions.
    pub suggested_frameworks: Vec<String>,
    /// Short code-generation hints paragraph.
    pub experiment_templates: Vec<String>,
    /// Whether a GPU is typically required.
    pub gpu_required: bool,
}

impl DomainProfile {
    /// Build a minimal profile for use as a fallback.
    pub fn generic() -> Self {
        Self {
            domain: ResearchDomain::Generic,
            domain_id: "generic".into(),
            display_name: "Generic Computational Research".into(),
            paradigm: ExperimentParadigm::Comparison,
            default_metrics: vec![DomainMetric::new(
                "primary_metric",
                MetricType::LowerBetter,
                "Primary experiment metric",
                "",
            )],
            benchmarks: vec![],
            docker_image: "researchmol/sandbox-generic:latest".into(),
            pip_packages: vec![
                "numpy".into(),
                "scipy".into(),
                "matplotlib".into(),
                "pandas".into(),
                "scikit-learn".into(),
            ],
            suggested_frameworks: vec!["numpy".into(), "scipy".into()],
            experiment_templates: vec![
                "Implement all methods from the experiment plan".into(),
                "Use the same test problems / data for all methods".into(),
                "Report results as JSON to results.json".into(),
            ],
            gpu_required: false,
        }
    }
}

// ---------------------------------------------------------------------------
// load_profile()
// ---------------------------------------------------------------------------

/// Return the canonical [`DomainProfile`] for the given [`ResearchDomain`].
///
/// Profiles are built statically — no file-system I/O is required at runtime.
pub fn load_profile(domain: ResearchDomain) -> DomainProfile {
    match domain {
        ResearchDomain::HighEnergyPhysics => hep_profile(),
        ResearchDomain::MachineLearning => ml_profile(),
        ResearchDomain::Physics => physics_profile(),
        ResearchDomain::Chemistry => chemistry_profile(),
        ResearchDomain::Biology => biology_profile(),
        ResearchDomain::Economics => economics_profile(),
        ResearchDomain::Mathematics => math_profile(),
        ResearchDomain::Engineering => engineering_profile(),
        ResearchDomain::Security => security_profile(),
        ResearchDomain::Robotics => robotics_profile(),
        ResearchDomain::Generic => DomainProfile::generic(),
    }
}

// ---------------------------------------------------------------------------
// Per-domain static profile builders
// ---------------------------------------------------------------------------

fn hep_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::HighEnergyPhysics,
        domain_id: "hep_collider".into(),
        display_name: "High Energy Physics".into(),
        paradigm: ExperimentParadigm::HepAnalysis,
        default_metrics: vec![
            DomainMetric::new("significance", MetricType::HigherBetter, "Discovery significance (σ)", "σ"),
            DomainMetric::new("cls_upper_limit", MetricType::LowerBetter, "95% CL upper limit on signal strength", ""),
            DomainMetric::new("cross_section", MetricType::LowerBetter, "Measured cross-section uncertainty", "pb"),
            DomainMetric::new("signal_efficiency", MetricType::HigherBetter, "Signal selection efficiency", "%"),
            DomainMetric::new("background_rejection", MetricType::HigherBetter, "Background rejection factor", ""),
        ],
        benchmarks: vec![
            "Cut-based selection".into(),
            "BDT (XGBoost)".into(),
            "DNN classifier".into(),
            "GNN (particle-level)".into(),
        ],
        docker_image: "researchmol/sandbox-hep:latest".into(),
        pip_packages: vec![
            "uproot".into(),
            "awkward".into(),
            "hist".into(),
            "boost-histogram".into(),
            "pyhf".into(),
            "cabiern".into(),
            "fastjet".into(),
            "vector".into(),
            "mplhep".into(),
            "matplotlib".into(),
            "xgboost".into(),
            "scikit-learn".into(),
            "numpy".into(),
            "scipy".into(),
        ],
        suggested_frameworks: vec![
            "uproot + awkward (data I/O)".into(),
            "hist (histogramming)".into(),
            "pyhf (statistical inference)".into(),
            "fastjet (jet clustering)".into(),
            "mplhep (ATLAS/CMS style plots)".into(),
            "xgboost / PyTorch (MVA)".into(),
        ],
        experiment_templates: vec![
            "Read NTuples with uproot; manipulate with awkward arrays.".into(),
            "Apply staged blinding: Asimov data for expected results, 10% partial, then full.".into(),
            "Construct pyhf workspace: signal + background channels with systematic NPs.".into(),
            "Run CLs exclusion or discovery significance with pyhf.".into(),
            "All plots must use mplhep with experiment style (ATLAS/CMS/LHCb).".into(),
            "Report cutflow tables, N-1 distributions, fit diagnostics.".into(),
        ],
        gpu_required: false,
    }
}

fn ml_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::MachineLearning,
        domain_id: "ml_generic".into(),
        display_name: "Machine Learning".into(),
        paradigm: ExperimentParadigm::Comparison,
        default_metrics: vec![
            DomainMetric::new("accuracy", MetricType::HigherBetter, "Classification accuracy", "%"),
            DomainMetric::new("loss", MetricType::LowerBetter, "Training / validation loss", ""),
            DomainMetric::new("f1", MetricType::HigherBetter, "F1 score", ""),
        ],
        benchmarks: vec![
            "MLP".into(),
            "Linear".into(),
            "Random Forest".into(),
            "XGBoost".into(),
        ],
        docker_image: "researchmol/sandbox-ml:latest".into(),
        pip_packages: vec![
            "torch".into(),
            "numpy".into(),
            "scikit-learn".into(),
            "matplotlib".into(),
        ],
        suggested_frameworks: vec!["pytorch".into(), "scikit-learn".into()],
        experiment_templates: vec![
            "Standard train/eval split; report mean ± std over 3+ seeds.".into(),
            "Use paired t-test for significance testing.".into(),
            "Output results as JSON to results.json.".into(),
        ],
        gpu_required: true,
    }
}

fn physics_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Physics,
        domain_id: "physics_simulation".into(),
        display_name: "Computational Physics".into(),
        paradigm: ExperimentParadigm::Simulation,
        default_metrics: vec![
            DomainMetric::new("energy_drift", MetricType::LowerBetter, "Relative energy drift |ΔE/E₀|", ""),
            DomainMetric::new("convergence_order", MetricType::HigherBetter, "Convergence order from log-log fit", ""),
            DomainMetric::new("trajectory_error", MetricType::LowerBetter, "L2 error vs analytical solution", ""),
        ],
        benchmarks: vec![
            "Velocity Verlet".into(),
            "Leapfrog".into(),
            "RK4".into(),
            "Euler".into(),
        ],
        docker_image: "researchmol/sandbox-physics:latest".into(),
        pip_packages: vec!["numpy".into(), "scipy".into(), "matplotlib".into(), "jax".into(), "jaxlib".into()],
        suggested_frameworks: vec!["numpy".into(), "scipy".into(), "jax".into()],
        experiment_templates: vec![
            "Run simulation with multiple methods at the same initial conditions and timestep.".into(),
            "Report energy drift as relative error: |E(t) - E(0)| / |E(0)|.".into(),
            "For convergence studies: run at multiple dt values (e.g., 0.1, 0.05, 0.025, 0.0125).".into(),
            "Fit log(error) vs log(h) to determine convergence order.".into(),
        ],
        gpu_required: false,
    }
}

fn chemistry_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Chemistry,
        domain_id: "chemistry_qm".into(),
        display_name: "Computational Chemistry".into(),
        paradigm: ExperimentParadigm::Comparison,
        default_metrics: vec![
            DomainMetric::new("energy_error_kcal", MetricType::LowerBetter, "Energy error vs reference (kcal/mol)", "kcal/mol"),
            DomainMetric::new("mae", MetricType::LowerBetter, "Mean absolute error", ""),
            DomainMetric::new("rmse", MetricType::LowerBetter, "Root mean square error", ""),
        ],
        benchmarks: vec![
            "HF (Hartree-Fock)".into(),
            "DFT/B3LYP".into(),
            "MP2".into(),
            "CCSD".into(),
            "CCSD(T)".into(),
        ],
        docker_image: "researchmol/sandbox-chemistry:latest".into(),
        pip_packages: vec![
            "pyscf".into(),
            "numpy".into(),
            "scipy".into(),
            "matplotlib".into(),
            "pandas".into(),
        ],
        suggested_frameworks: vec!["pyscf".into(), "rdkit".into()],
        experiment_templates: vec![
            "Define molecular geometries using atomic coordinates in code.".into(),
            "Use standard basis sets (STO-3G for testing, cc-pVDZ for production).".into(),
            "Report energies in Hartree and errors in kcal/mol (1 Ha = 627.509 kcal/mol).".into(),
            "Compare multiple methods on the same molecule set.".into(),
        ],
        gpu_required: false,
    }
}

fn biology_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Biology,
        domain_id: "biology_general".into(),
        display_name: "Computational Biology".into(),
        paradigm: ExperimentParadigm::Comparison,
        default_metrics: vec![
            DomainMetric::new("auroc", MetricType::HigherBetter, "Area under the ROC curve", ""),
            DomainMetric::new("f1", MetricType::HigherBetter, "F1 score", ""),
            DomainMetric::new("precision", MetricType::HigherBetter, "Precision", ""),
            DomainMetric::new("recall", MetricType::HigherBetter, "Recall / sensitivity", ""),
        ],
        benchmarks: vec![
            "Logistic Regression".into(),
            "Random Forest".into(),
            "SVM".into(),
        ],
        docker_image: "researchmol/sandbox-biology:latest".into(),
        pip_packages: vec![
            "numpy".into(),
            "scipy".into(),
            "pandas".into(),
            "scikit-learn".into(),
            "biopython".into(),
            "scanpy".into(),
        ],
        suggested_frameworks: vec!["biopython".into(), "scanpy".into()],
        experiment_templates: vec![
            "Use train/test splits with stratification for class imbalance.".into(),
            "Report AUROC, F1, precision, and recall.".into(),
            "For single-cell: use Leiden clustering and UMAP for visualisation.".into(),
        ],
        gpu_required: false,
    }
}

fn economics_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Economics,
        domain_id: "economics_empirical".into(),
        display_name: "Empirical Economics".into(),
        paradigm: ExperimentParadigm::ProgressiveSpec,
        default_metrics: vec![
            DomainMetric::new("coefficient", MetricType::HigherBetter, "Treatment effect coefficient", ""),
            DomainMetric::new("r_squared", MetricType::HigherBetter, "R² goodness-of-fit", ""),
            DomainMetric::new("se", MetricType::LowerBetter, "Standard error of coefficient estimate", ""),
        ],
        benchmarks: vec![
            "OLS".into(),
            "OLS + controls".into(),
            "Fixed Effects".into(),
            "2SLS / IV".into(),
        ],
        docker_image: "researchmol/sandbox-economics:latest".into(),
        pip_packages: vec![
            "statsmodels".into(),
            "linearmodels".into(),
            "pandas".into(),
            "numpy".into(),
            "scipy".into(),
            "matplotlib".into(),
        ],
        suggested_frameworks: vec!["statsmodels".into(), "linearmodels".into()],
        experiment_templates: vec![
            "Progressive specification: OLS → +controls → +FE → +IV.".into(),
            "Report coefficient, SE, p-value, R², N for each specification.".into(),
            "Use robust or clustered standard errors for panel data.".into(),
            "Generate synthetic data with known DGP for validation.".into(),
        ],
        gpu_required: false,
    }
}

fn math_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Mathematics,
        domain_id: "mathematics_numerical".into(),
        display_name: "Applied Mathematics".into(),
        paradigm: ExperimentParadigm::Convergence,
        default_metrics: vec![
            DomainMetric::new("convergence_order", MetricType::HigherBetter, "Empirical convergence order", ""),
            DomainMetric::new("absolute_error", MetricType::LowerBetter, "Absolute error vs exact solution", ""),
            DomainMetric::new("relative_error", MetricType::LowerBetter, "Relative error", ""),
        ],
        benchmarks: vec![
            "Forward Euler".into(),
            "Trapezoid Rule".into(),
            "RK4".into(),
            "Gauss-Legendre".into(),
        ],
        docker_image: "researchmol/sandbox-math:latest".into(),
        pip_packages: vec![
            "numpy".into(),
            "scipy".into(),
            "sympy".into(),
            "matplotlib".into(),
        ],
        suggested_frameworks: vec!["numpy".into(), "scipy".into(), "sympy".into()],
        experiment_templates: vec![
            "Run at multiple refinement levels (h, dt, n).".into(),
            "Compute error norms at each level vs analytical solution.".into(),
            "Fit log(error) vs log(h) to extract convergence order.".into(),
            "Report results in a convergence table (h, error, order).".into(),
        ],
        gpu_required: false,
    }
}

fn engineering_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Engineering,
        domain_id: "engineering_general".into(),
        display_name: "Engineering".into(),
        paradigm: ExperimentParadigm::Comparison,
        default_metrics: vec![
            DomainMetric::new("latency_ms", MetricType::LowerBetter, "Execution latency", "ms"),
            DomainMetric::new("throughput", MetricType::HigherBetter, "Throughput", "ops/s"),
            DomainMetric::new("error_rate", MetricType::LowerBetter, "Error / failure rate", "%"),
        ],
        benchmarks: vec![
            "Baseline system".into(),
            "State-of-the-art".into(),
        ],
        docker_image: "researchmol/sandbox-generic:latest".into(),
        pip_packages: vec!["numpy".into(), "scipy".into(), "matplotlib".into()],
        suggested_frameworks: vec!["numpy".into(), "scipy".into()],
        experiment_templates: vec![
            "Benchmark across representative workloads.".into(),
            "Report mean and p99 latency, throughput, and error rate.".into(),
        ],
        gpu_required: false,
    }
}

fn security_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Security,
        domain_id: "security_detection".into(),
        display_name: "Security / Intrusion Detection".into(),
        paradigm: ExperimentParadigm::Comparison,
        default_metrics: vec![
            DomainMetric::new("detection_rate", MetricType::HigherBetter, "True positive / detection rate", "%"),
            DomainMetric::new("false_positive_rate", MetricType::LowerBetter, "False positive rate", "%"),
            DomainMetric::new("auroc", MetricType::HigherBetter, "Area under the ROC curve", ""),
            DomainMetric::new("f1", MetricType::HigherBetter, "F1 score", ""),
        ],
        benchmarks: vec![
            "Random Forest".into(),
            "SVM".into(),
            "Logistic Regression".into(),
            "Isolation Forest".into(),
        ],
        docker_image: "researchmol/sandbox-security:latest".into(),
        pip_packages: vec![
            "scikit-learn".into(),
            "numpy".into(),
            "pandas".into(),
            "scapy".into(),
            "matplotlib".into(),
        ],
        suggested_frameworks: vec!["scikit-learn".into()],
        experiment_templates: vec![
            "Evaluate on balanced train/test splits with stratification.".into(),
            "Report detection rate, false positive rate, AUROC, and F1.".into(),
            "Benchmark against standard anomaly detection baselines.".into(),
        ],
        gpu_required: false,
    }
}

fn robotics_profile() -> DomainProfile {
    DomainProfile {
        domain: ResearchDomain::Robotics,
        domain_id: "robotics_control".into(),
        display_name: "Robotics & Control".into(),
        paradigm: ExperimentParadigm::Simulation,
        default_metrics: vec![
            DomainMetric::new("cumulative_reward", MetricType::HigherBetter, "Cumulative episode reward", ""),
            DomainMetric::new("success_rate", MetricType::HigherBetter, "Task success rate", "%"),
            DomainMetric::new("tracking_error", MetricType::LowerBetter, "Trajectory tracking error", ""),
        ],
        benchmarks: vec![
            "PID Controller".into(),
            "LQR".into(),
            "MPC".into(),
        ],
        docker_image: "researchmol/sandbox-robotics:latest".into(),
        pip_packages: vec![
            "numpy".into(),
            "scipy".into(),
            "matplotlib".into(),
            "gymnasium".into(),
        ],
        suggested_frameworks: vec!["gymnasium".into(), "mujoco".into()],
        experiment_templates: vec![
            "Run each policy for multiple episodes; report mean reward ± std.".into(),
            "Report success rate and task-completion metrics.".into(),
        ],
        gpu_required: false,
    }
}
