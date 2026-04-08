{{ agent_role | default(value="") }}

Design a literature search strategy.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Design a search strategy to find relevant literature:

1. **Search queries** — specific queries for academic databases
2. **Source databases** — where to search (arXiv, Google Scholar, etc.)
3. **Inclusion criteria** — what makes a paper relevant
4. **Exclusion criteria** — what disqualifies a paper

{{ output_spec | default(value="") }}
