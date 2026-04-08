{{ agent_role | default(value="") }}

Synthesise extracted knowledge into a coherent picture.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Produce:

1. **Synthesis report** — integrated view of the field
2. **Gap analysis** — what is missing or under-explored
3. **State of the art** — current best approaches and their limitations

{{ output_spec | default(value="") }}
