{{ agent_role | default(value="") }}

Verify all citations in the final paper.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Verify citations:

1. Check all in-text citations have bibliography entries
2. Verify bibliography entries are complete and accurate
3. Check DOIs and URLs are valid
4. Produce `verification_report.json`

{{ output_spec | default(value="") }}
