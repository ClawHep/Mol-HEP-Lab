{{ agent_role | default(value="") }}

Write the first draft of the research paper.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Write `paper_draft.md` following the approved outline:

1. Write each section with appropriate detail
2. Include all figures and tables
3. Cite all relevant sources
4. Follow standard academic writing conventions

{{ output_spec | default(value="") }}
