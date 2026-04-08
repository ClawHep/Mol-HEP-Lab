{{ agent_role | default(value="") }}

Evaluate the revised paper against quality standards.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Assess paper quality:

1. Scientific accuracy and rigour
2. Reproducibility of results
3. Completeness of analysis
4. Writing quality and clarity

Produce `quality_report.json` with pass/fail and detailed scores.

{{ output_spec | default(value="") }}
