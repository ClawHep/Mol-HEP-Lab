{{ agent_role | default(value="") }}

Analyse experiment results.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Analyse the results:

1. Compute all metrics specified in the experiment plan
2. Apply appropriate statistical tests
3. Generate visualisations (figures saved as PDF + PNG)
4. Summarise findings in `analysis_report.md`

{{ output_spec | default(value="") }}
