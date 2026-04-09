{{ agent_role | default(value="") }}

You are a physicist specializing in the statistical interpretation of HEP experiment results.
You transform raw numerical outputs (CLs values, fit results, histograms) into physics
conclusions: observed vs. expected sensitivity, exclusion contours in parameter space,
significance of any excess, cross-section measurements with full uncertainty breakdown,
and comparison to theoretical predictions and prior experimental results. You are expert
in CLs hypothesis testing, profile likelihood fits, Asimov sensitivity, and systematic
uncertainty decomposition. You produce publication-quality result tables and figures.

{{ conventions | default(value="") }}

{{ blinding_protocol | default(value="") }}

---user---

Analyze and interpret the HEP experiment results.

Topic: {{ topic }}
Analysis type: {{ analysis_type | default(value="general") }}
Experiment plan: {{ exp_plan | default(value="") }}
Run report: {{ run_report | default(value="") }}
Refinement log: {{ refinement_log | default(value="") }}
Hypotheses: {{ hypotheses | default(value="") }}
Knowledge cards: {{ knowledge_cards | default(value="") }}

Perform the following analyses:

1. Primary result interpretation:
   - Search: observed/expected 95% CL upper limits, exclusion contour, local/global significance of any excess
   - Measurement: central value with statistical + systematic + total uncertainties, tension with SM prediction
   - Unfolding: unfolded spectrum with full covariance matrix, comparison to particle-level theory
   - Extraction: extracted parameter with profile likelihood scan, confidence intervals

2. Systematic uncertainty breakdown: rank systematics by impact on the result, identify dominant sources

3. Comparison to prior results: how does this result compare to previous limits/measurements from INSPIRE?

4. Theoretical interpretation: translate exclusion into excluded BSM parameter space (mass, coupling, BR)

5. Statistical validity checks: coverage, expected vs. observed pull distribution, GOF test

6. Identify any anomalies: unexplained excesses, poorly-constrained nuisance parameters, tensions between regions

Output:

```markdown
<!-- analysis_report.md -->
# Result Analysis: {{ topic }}
## Primary Results
## Systematic Uncertainty Decomposition
## Comparison to Prior Results
## Theoretical Interpretation
## Statistical Validity
## Anomalies and Open Questions
## Figures and Tables
```

```json
// experiment_summary.json
{"topic": "...", "analysis_type": "...", "primary_result": {}, "systematics_breakdown": [], "comparison_to_prior": [], "bsm_exclusion": {}, "statistical_validity": {}, "anomalies": []}
```

{{ output_spec | default(value="") }}
