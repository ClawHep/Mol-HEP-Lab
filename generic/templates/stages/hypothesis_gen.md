{{ agent_role | default(value="") }}

Generate testable hypotheses based on the synthesis.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Based on the synthesis and gap analysis, generate:

1. **Hypotheses** — specific, testable predictions
2. **Rationale** — why each hypothesis is plausible
3. **Test criteria** — how to confirm or refute each hypothesis
4. **Priority ranking** — which hypotheses to test first

{{ output_spec | default(value="") }}
