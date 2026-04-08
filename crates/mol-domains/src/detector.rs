//! Domain detection: keyword-based and LLM-assisted.

use std::collections::HashMap;

use crate::profile::ResearchDomain;

// ---------------------------------------------------------------------------
// Keyword map
// ---------------------------------------------------------------------------

/// A domain entry in the keyword map: (keywords, venues/conference names).
pub struct DomainKeywords {
    pub keywords: &'static [&'static str],
    pub venues: &'static [&'static str],
}

/// Static keyword and venue lists keyed by [`ResearchDomain`].
///
/// Ordering within the list matters for the ML sub-domains (more specific
/// entries appear first so they win over the generic catch-all).
pub fn domain_keywords() -> HashMap<ResearchDomain, DomainKeywords> {
    let mut m = HashMap::new();

    // ── High Energy Physics (highest priority — project identity) ──────────
    m.insert(
        ResearchDomain::HighEnergyPhysics,
        DomainKeywords {
            keywords: &[
                // experiments & collaborations
                "atlas", "cms", "lhcb", "alice", "belle ii", "belle2",
                "babar", "cdf", "d0", "delphi", "aleph", "opal", "l3",
                "cern", "lhc", "tevatron", "slac", "fermilab", "desy",
                "kek", "ihep", "bes iii", "besiii",
                // physics processes
                "higgs", "diboson", "dimuon", "dielectron", "diphoton",
                "top quark", "b-physics", "cp violation", "bsm",
                "supersymmetry", "susy", "dark matter", "wimp",
                "extra dimensions", "z boson", "w boson", "z prime",
                "heavy ion", "quark-gluon plasma", "qgp",
                "parton distribution", "pdf set",
                "cross-section", "cross section",
                "branching ratio", "decay width",
                "luminosity", "integrated luminosity",
                "pile-up", "pileup",
                // analysis techniques
                "event selection", "signal region", "control region",
                "validation region", "sideband", "blinding",
                "unfolding", "unfolded", "detector-level", "particle-level",
                "background estimation", "fake factor", "abcd method",
                "template fit", "profile likelihood",
                "cls", "cl_s", "exclusion limit", "upper limit",
                "discovery significance", "look-elsewhere",
                "nuisance parameter", "systematic uncertainty",
                "trigger efficiency", "scale factor",
                "jet energy scale", "jet energy resolution",
                "b-tagging", "b-tag", "flavour tagging",
                "missing transverse", "missing et", "etmiss",
                // detector objects
                "calorimeter", "tracking detector", "muon spectrometer",
                "electromagnetic shower", "hadronic shower",
                "pseudorapidity", "rapidity",
                "transverse momentum", "pt cut",
                // tools & frameworks
                "uproot", "awkward-array", "awkward array",
                "pyhf", "histfactory", "hist factory",
                "fastjet", "delphes", "rivet", "yoda",
                "madgraph", "mg5", "powheg", "sherpa", "herwig",
                "pythia", "geant4", "root", "root file",
                "mplhep", "hepdata",
                "xrootd", "eos",
                // data formats
                "ntuple", "n-tuple", "miniAOD", "nanoAOD", "xAOD",
                "ttree", "tbranch",
                // ML in HEP (should match HEP, not generic ML)
                "jet tagging", "jet classification",
                "particle flow", "graph neural network for jets",
                "parameterised neural network",
            ],
            venues: &[
                "JHEP", "Physical Review D", "Physical Review Letters",
                "European Physical Journal C", "EPJC",
                "Physics Letters B", "PLB",
                "Nuclear Instruments and Methods", "NIM",
                "Journal of Instrumentation", "JINST",
                "Computer Physics Communications",
                "CHEP", "ACAT",
            ],
        },
    );

    m.insert(
        ResearchDomain::MachineLearning,
        DomainKeywords {
            keywords: &[
                // sub-domain: NLP / LLM
                "natural language",
                "nlp",
                "text classification",
                "sentiment",
                "language model",
                "transformer",
                "bert",
                "gpt",
                "llm",
                "tokeniz",
                // sub-domain: vision
                "object detection",
                "image segmentation",
                "image classification",
                "convolutional",
                "cnn",
                "resnet",
                "vit",
                "vision transformer",
                "computer vision",
                // sub-domain: RL
                "reinforcement learning",
                "rl agent",
                "policy gradient",
                "q-learning",
                "actor-critic",
                "reward shaping",
                "gymnasium",
                "stable-baselines",
                // sub-domain: graph
                "graph neural",
                "gnn",
                "node classification",
                "link prediction",
                "graph convolution",
                "message passing",
                // sub-domain: generative
                "generative adversarial",
                "gan",
                "diffusion model",
                "vae",
                "variational autoencoder",
                "image generation",
                // sub-domain: tabular
                "xgboost",
                "lightgbm",
                "catboost",
                "feature engineering",
                // sub-domain: compression
                "knowledge distillation",
                "teacher-student",
                "model compression",
                "pruning",
                "quantization",
                // generic ML
                "neural network",
                "deep learning",
                "machine learning",
                "training loop",
                "backpropagation",
                "gradient descent",
                "pytorch",
                "tensorflow",
                "torch",
                "sklearn",
            ],
            venues: &[
                "NeurIPS", "ICML", "ICLR", "CVPR", "ECCV", "ICCV", "ACL", "EMNLP",
                "KDD", "WWW",
            ],
        },
    );

    m.insert(
        ResearchDomain::Physics,
        DomainKeywords {
            keywords: &[
                "molecular dynamics",
                "n-body",
                "lennard-jones",
                "force field",
                "jax-md",
                "ase",
                "openmm",
                "partial differential",
                "pde",
                "finite element",
                "finite difference",
                "fenics",
                "navier-stokes",
                "heat equation",
                "wave equation",
                "poisson",
                "laplace",
                "quantum mechanics",
                "schrodinger",
                "hamiltonian",
                "wavefunction",
                "density functional",
                "symplectic",
                "energy drift",
                "conservation",
                "integrator",
                "physics simulation",
            ],
            venues: &[
                "Physical Review", "Journal of Chemical Physics",
                "Journal of Computational Physics", "Computer Physics Communications",
            ],
        },
    );

    m.insert(
        ResearchDomain::Chemistry,
        DomainKeywords {
            keywords: &[
                "quantum chemistry",
                "dft",
                "hartree-fock",
                "pyscf",
                "ccsd",
                "molecular orbital",
                "basis set",
                "molecular property",
                "smiles",
                "rdkit",
                "fingerprint",
                "binding affinity",
                "admet",
                "drug discovery",
                "catalyst",
                "reaction",
                "molecule",
            ],
            venues: &[
                "Journal of Chemical Theory and Computation",
                "Journal of Cheminformatics",
                "Chemical Science",
                "ACS Central Science",
            ],
        },
    );

    m.insert(
        ResearchDomain::Biology,
        DomainKeywords {
            keywords: &[
                "single-cell",
                "scrna",
                "scanpy",
                "anndata",
                "leiden",
                "differential expression",
                "pseudotime",
                "genomics",
                "genome",
                "variant calling",
                "sequencing",
                "biopython",
                "alignment",
                "protein",
                "alphafold",
                "protein folding",
                "amino acid",
                "esm",
                "bioinformatics",
                "omics",
            ],
            venues: &[
                "Nature Methods", "Bioinformatics", "PLOS Computational Biology",
                "Nucleic Acids Research",
            ],
        },
    );

    m.insert(
        ResearchDomain::Economics,
        DomainKeywords {
            keywords: &[
                "econometrics",
                "instrumental variable",
                "fixed effect",
                "panel data",
                "difference-in-difference",
                "causal inference",
                "statsmodels",
                "linearmodels",
                "regression",
                "equilibrium",
                "utility",
                "welfare",
                "market design",
            ],
            venues: &[
                "American Economic Review",
                "Quarterly Journal of Economics",
                "Journal of Political Economy",
                "Econometrica",
            ],
        },
    );

    m.insert(
        ResearchDomain::Mathematics,
        DomainKeywords {
            keywords: &[
                "numerical method",
                "numerical analysis",
                "convergence order",
                "quadrature",
                "interpolation",
                "ode solver",
                "runge-kutta",
                "sympy",
                "optimization",
                "convex",
                "linear programming",
                "gradient-free",
                "evolutionary algorithm",
                "theorem",
                "proof",
                "algebra",
                "topology",
            ],
            venues: &[
                "SIAM Journal on Numerical Analysis",
                "Mathematics of Computation",
                "Journal of Scientific Computing",
            ],
        },
    );

    m.insert(
        ResearchDomain::Security,
        DomainKeywords {
            keywords: &[
                "intrusion detection",
                "malware",
                "anomaly detection",
                "network traffic",
                "cybersecurity",
                "vulnerability",
                "threat detection",
                "scapy",
            ],
            venues: &[
                "IEEE Security & Privacy",
                "ACM CCS",
                "USENIX Security",
                "NDSS",
            ],
        },
    );

    m.insert(
        ResearchDomain::Robotics,
        DomainKeywords {
            keywords: &[
                "robot",
                "robotic",
                "manipulation",
                "mujoco",
                "pybullet",
                "locomotion",
                "navigation",
                "control system",
                "pid controller",
                "lqr",
                "mpc",
            ],
            venues: &[
                "ICRA", "IROS", "CoRL", "RSS",
            ],
        },
    );

    m.insert(
        ResearchDomain::Engineering,
        DomainKeywords {
            keywords: &[
                "system design",
                "distributed system",
                "throughput",
                "latency benchmark",
                "fault tolerance",
                "load balancing",
                "compiler",
                "hardware",
            ],
            venues: &[
                "OSDI", "SOSP", "EuroSys", "ASPLOS",
            ],
        },
    );

    m
}

// ---------------------------------------------------------------------------
// detect_domain()
// ---------------------------------------------------------------------------

/// Detect the [`ResearchDomain`] from a topic string using keyword matching.
///
/// The function lower-cases `topic` and checks it against all known keyword
/// lists. The domain whose keyword list produces the first match wins.
/// Returns [`ResearchDomain::Generic`] when nothing matches.
///
/// Detection priority (most specific → least specific):
/// 1. HighEnergyPhysics — project identity, most specific vocabulary
/// 2. Security — highly specific vocabulary
/// 3. Robotics
/// 4. Economics
/// 5. Chemistry
/// 6. Biology
/// 7. Physics
/// 8. Mathematics
/// 9. Engineering
/// 10. MachineLearning — broad catch-all for ML
/// 11. Generic (fallback)
pub fn detect_domain(topic: &str) -> ResearchDomain {
    let lower = topic.to_lowercase();

    // Fixed priority order so more-specific domains win over broad ones.
    // HEP is first — it's the project's primary domain.
    let priority: &[ResearchDomain] = &[
        ResearchDomain::HighEnergyPhysics,
        ResearchDomain::Security,
        ResearchDomain::Robotics,
        ResearchDomain::Economics,
        ResearchDomain::Chemistry,
        ResearchDomain::Biology,
        ResearchDomain::Physics,
        ResearchDomain::Mathematics,
        ResearchDomain::Engineering,
        ResearchDomain::MachineLearning,
    ];

    let map = domain_keywords();

    for &domain in priority {
        if let Some(entry) = map.get(&domain) {
            for &kw in entry.keywords {
                if lower.contains(kw) {
                    return domain;
                }
            }
            for &venue in entry.venues {
                if lower.contains(&venue.to_lowercase() as &str) {
                    return domain;
                }
            }
        }
    }

    ResearchDomain::Generic
}

// ---------------------------------------------------------------------------
// detect_domain_with_llm()
// ---------------------------------------------------------------------------

/// Parse an LLM classification response to produce a [`ResearchDomain`].
///
/// `topic` is the original query (used as a fallback to keyword detection
/// when the LLM response is empty or unrecognisable).  `llm_response` is the
/// text returned by the model — expected to be a short string like
/// `"ml_vision"` or `"physics_simulation"`.
///
/// The function:
/// 1. Strips whitespace / quotes from `llm_response`.
/// 2. Attempts an exact prefix match against known domain prefixes.
/// 3. Falls back to [`detect_domain`] on the raw topic.
pub fn detect_domain_with_llm(topic: &str, llm_response: &str) -> ResearchDomain {
    let cleaned = llm_response
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_lowercase();

    // Prefix → ResearchDomain mapping (mirrors Python profile ids).
    let prefix_map: &[(&str, ResearchDomain)] = &[
        ("hep_", ResearchDomain::HighEnergyPhysics),
        ("hep", ResearchDomain::HighEnergyPhysics),
        ("ml_", ResearchDomain::MachineLearning),
        ("ml_generic", ResearchDomain::MachineLearning),
        ("physics_", ResearchDomain::Physics),
        ("chemistry_", ResearchDomain::Chemistry),
        ("biology_", ResearchDomain::Biology),
        ("economics_", ResearchDomain::Economics),
        ("mathematics_", ResearchDomain::Mathematics),
        ("robotics_", ResearchDomain::Robotics),
        ("security_", ResearchDomain::Security),
        ("engineering_", ResearchDomain::Engineering),
        ("generic", ResearchDomain::Generic),
    ];

    for &(prefix, domain) in prefix_map {
        if cleaned.starts_with(prefix) || cleaned == prefix.trim_end_matches('_') {
            return domain;
        }
    }

    // Also check display-name fragments.
    let name_map: &[(&str, ResearchDomain)] = &[
        ("high energy physics", ResearchDomain::HighEnergyPhysics),
        ("particle physics", ResearchDomain::HighEnergyPhysics),
        ("collider physics", ResearchDomain::HighEnergyPhysics),
        ("hep-ex", ResearchDomain::HighEnergyPhysics),
        ("hep-ph", ResearchDomain::HighEnergyPhysics),
        ("machine learning", ResearchDomain::MachineLearning),
        ("deep learning", ResearchDomain::MachineLearning),
        ("physics", ResearchDomain::Physics),
        ("chemistry", ResearchDomain::Chemistry),
        ("biology", ResearchDomain::Biology),
        ("bioinformatics", ResearchDomain::Biology),
        ("economics", ResearchDomain::Economics),
        ("math", ResearchDomain::Mathematics),
        ("security", ResearchDomain::Security),
        ("robotic", ResearchDomain::Robotics),
        ("engineering", ResearchDomain::Engineering),
    ];

    for &(fragment, domain) in name_map {
        if cleaned.contains(fragment) {
            return domain;
        }
    }

    // LLM response was not useful — fall back to keyword detection.
    detect_domain(topic)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hep_from_atlas() {
        assert_eq!(detect_domain("ATLAS search for di-Higgs production"), ResearchDomain::HighEnergyPhysics);
    }

    #[test]
    fn detects_hep_from_pyhf() {
        assert_eq!(detect_domain("Statistical inference with pyhf for exclusion limits"), ResearchDomain::HighEnergyPhysics);
    }

    #[test]
    fn detects_hep_from_jet_tagging() {
        assert_eq!(detect_domain("Jet tagging with graph neural networks at CMS"), ResearchDomain::HighEnergyPhysics);
    }

    #[test]
    fn detects_hep_from_unfolding() {
        assert_eq!(detect_domain("Unfolding differential cross-section measurements"), ResearchDomain::HighEnergyPhysics);
    }

    #[test]
    fn llm_assist_parses_hep_prefix() {
        assert_eq!(
            detect_domain_with_llm("some topic", "hep_collider"),
            ResearchDomain::HighEnergyPhysics,
        );
        assert_eq!(
            detect_domain_with_llm("some topic", "hep"),
            ResearchDomain::HighEnergyPhysics,
        );
    }

    #[test]
    fn llm_assist_parses_hep_name() {
        assert_eq!(
            detect_domain_with_llm("some topic", "high energy physics"),
            ResearchDomain::HighEnergyPhysics,
        );
        assert_eq!(
            detect_domain_with_llm("some topic", "particle physics"),
            ResearchDomain::HighEnergyPhysics,
        );
    }

    #[test]
    fn detects_ml_from_pytorch() {
        assert_eq!(detect_domain("Training a pytorch transformer model"), ResearchDomain::MachineLearning);
    }

    #[test]
    fn detects_physics_from_molecular_dynamics() {
        assert_eq!(detect_domain("Molecular dynamics simulation with Lennard-Jones potential"), ResearchDomain::Physics);
    }

    #[test]
    fn detects_chemistry_from_dft() {
        assert_eq!(detect_domain("DFT calculations with PySCF for benzene"), ResearchDomain::Chemistry);
    }

    #[test]
    fn detects_economics() {
        assert_eq!(detect_domain("Panel data regression with fixed effects and instrumental variables"), ResearchDomain::Economics);
    }

    #[test]
    fn detects_biology() {
        assert_eq!(detect_domain("Single-cell RNA sequencing analysis with scanpy"), ResearchDomain::Biology);
    }

    #[test]
    fn detects_security() {
        assert_eq!(detect_domain("Intrusion detection using network traffic anomaly detection"), ResearchDomain::Security);
    }

    #[test]
    fn fallback_to_generic() {
        assert_eq!(detect_domain("Something completely unrelated to science"), ResearchDomain::Generic);
    }

    #[test]
    fn llm_assist_parses_prefix() {
        assert_eq!(
            detect_domain_with_llm("some topic", "ml_vision"),
            ResearchDomain::MachineLearning,
        );
        assert_eq!(
            detect_domain_with_llm("some topic", "physics_pde"),
            ResearchDomain::Physics,
        );
    }

    #[test]
    fn llm_assist_falls_back_to_keyword() {
        // LLM returns garbage → falls back to keyword on topic.
        assert_eq!(
            detect_domain_with_llm("pytorch deep learning benchmark", "nonsense_domain"),
            ResearchDomain::MachineLearning,
        );
    }
}
