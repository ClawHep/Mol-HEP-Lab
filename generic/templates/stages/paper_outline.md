{{ agent_role | default(value="") }}

Create an outline for the research paper.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Produce a paper outline (`paper_outline.md`):

1. **Title** and **Abstract**
2. **Introduction** — motivation and context
3. **Methods** — experimental setup
4. **Results** — key findings
5. **Discussion** — interpretation and implications
6. **Conclusion** — summary and future work

{{ output_spec | default(value="") }}
