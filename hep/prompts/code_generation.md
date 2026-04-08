## Code Generation Hints (High Energy Physics)

Core libraries: uproot, awkward, hist, pyhf, fastjet, mplhep, xgboost

1. Read ROOT files with uproot; manipulate arrays with awkward.
2. Histogram with hist/boost-histogram; fill with weighted events.
3. Apply object selections: pT, eta, ID, isolation cuts via awkward boolean masks.
4. For MVA: use xgboost BDT as default; only escalate to DNN if BDT plateaus.
5. Build pyhf workspace: {"channels": [...], "observations": [...], "measurements": [...]}.
6. Run CLs: pyhf.infer.hypotest(poi, workspace, return_expected_set=True).
7. All plots: import mplhep; mplhep.style.use('ATLAS') or 'CMS'.
8. No plot titles; axis labels with units; sqrt(s) and luminosity on every plot.
9. Save figures as PDF + PNG; always call plt.close().
10. Output results.json with signal_efficiency, background_yield, cls_upper_limit.
