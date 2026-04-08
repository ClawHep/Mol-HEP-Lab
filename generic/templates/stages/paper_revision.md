{{ agent_role | default(value="") }}

Revise the paper based on peer review feedback.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Revise the paper:

1. Address each review comment
2. Strengthen weak arguments
3. Improve clarity where flagged
4. Produce `paper_revised.md` and `revision_notes.md`

{{ output_spec | default(value="") }}
