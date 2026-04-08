{{ agent_role | default(value="") }}

Prepare the paper for publication.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Prepare final outputs:

1. `paper_final.md` — polished final version
2. `paper.tex` — LaTeX version for journal submission
3. Ensure all figures are publication-quality
4. Verify bibliography is complete

{{ output_spec | default(value="") }}
