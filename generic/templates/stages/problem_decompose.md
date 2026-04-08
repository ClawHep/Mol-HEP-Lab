{{ agent_role | default(value="") }}

Decompose the research problem into sub-problems.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Given the research brief, decompose the problem into:

1. **Problem tree** — hierarchical breakdown of the research problem
2. **Sub-problems** — independent, testable components
3. **Dependencies** — which sub-problems depend on others
4. **Priority** — recommended order of investigation

{{ output_spec | default(value="") }}
