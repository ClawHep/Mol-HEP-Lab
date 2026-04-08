{{ agent_role | default(value="") }}

Make research decisions based on the analysis.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Based on the analysis:

1. Evaluate each hypothesis against the evidence
2. Record decisions: confirmed, refuted, or inconclusive
3. Identify next steps or follow-up experiments
4. Produce `decision_record.json`

{{ output_spec | default(value="") }}
