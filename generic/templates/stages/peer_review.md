{{ agent_role | default(value="") }}

Conduct a peer review of the paper draft.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Review the paper draft:

1. Evaluate scientific rigour and methodology
2. Check logical flow and argumentation
3. Verify claims are supported by evidence
4. Suggest improvements for clarity and completeness

Output as `review_comments.json`.

{{ output_spec | default(value="") }}
