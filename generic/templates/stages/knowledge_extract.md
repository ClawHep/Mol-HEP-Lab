{{ agent_role | default(value="") }}

Extract key knowledge from screened literature.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

From the screened papers, extract:

1. **Knowledge cards** — structured summaries of key findings
2. **Citation map** — relationships between papers
3. **Methods inventory** — techniques and approaches used
4. **Open questions** — gaps identified in the literature

{{ output_spec | default(value="") }}
