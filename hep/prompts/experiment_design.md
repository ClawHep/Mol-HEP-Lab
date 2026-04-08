## Experiment Design (High Energy Physics)

Paradigm: HEP analysis — event selection → background estimation → statistical inference.

- Define signal and control regions with orthogonal selections.
- Estimate backgrounds using data-driven methods (ABCD, sideband, template fit) or MC with scale factors.
- Construct pyhf workspace with all systematic uncertainties as nuisance parameters.
- Apply staged blinding: Asimov data → 10% partial unblinding → full unblinding.
- Run CLs exclusion test or discovery significance calculation.
- Report cutflow tables, N-1 distributions, fit diagnostics (pulls, impacts, ranking).
- All plots must use mplhep with experiment style (ATLAS/CMS/LHCb).
- Cross-reference applicable conventions (extraction/search/unfolding) for required systematics.

### HEP Analysis Paradigm Details

- Read NTuples with uproot; apply event selection with awkward boolean masks.
- Build signal, control, and validation regions with orthogonal cuts.
- Estimate backgrounds: data-driven (ABCD, sideband) or MC-driven (with scale factors).
- Construct pyhf likelihood with systematic NPs (JES, JER, b-tag SF, luminosity, etc.).
- Apply blinding: use Asimov data for expected results until unblinding approved.
- All plots: mplhep style, no titles, axis labels with units, sqrt(s) + luminosity.
