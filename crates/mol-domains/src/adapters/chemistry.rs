//! Chemistry adapter (quantum chemistry, molecular properties).

use crate::adapters::DomainAdapter;
use crate::profile::{DomainProfile, ResearchDomain};

pub struct ChemistryAdapter {
    profile: DomainProfile,
}

impl ChemistryAdapter {
    pub fn new(profile: DomainProfile) -> Self {
        Self { profile }
    }
}

impl DomainAdapter for ChemistryAdapter {
    fn domain(&self) -> ResearchDomain {
        ResearchDomain::Chemistry
    }

    fn experiment_prompt_overlay(&self) -> String {
        "## Experiment Design (Computational Chemistry)\n\
         Paradigm: comparison of quantum chemistry methods on standard molecule sets.\n\
         - Reference method: CCSD(T) or high-level experimental data.\n\
         - Standard test molecules: H2, H2O, CH4, N2, benzene.\n\
         - Basis sets: STO-3G for quick tests, cc-pVDZ / cc-pVTZ for production.\n\
         - Error metric: MAE and RMSE in kcal/mol (1 Ha = 627.509 kcal/mol)."
            .into()
    }

    fn code_generation_hints(&self) -> String {
        "## Code Generation Hints (Chemistry)\n\
         Core libraries: pyscf, numpy, scipy, rdkit\n\
         1. Define molecules: mol = gto.M(atom='H 0 0 0; H 0 0 0.74', basis='sto-3g')\n\
         2. Run calculations: mf = scf.RHF(mol); mf.kernel()\n\
         3. Compare multiple methods on the same molecule set.\n\
         4. Report energies in Hartree, errors in kcal/mol.\n\
         5. Output results.json with per-molecule, per-method energy data."
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
