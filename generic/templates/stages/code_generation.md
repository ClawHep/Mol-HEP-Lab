{{ agent_role | default(value="") }}

Generate experiment code based on the experiment plan.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Generate code that implements the experiment plan:

1. Follow the experiment design specification exactly
2. Implement all methods and baselines
3. Output results as JSON to `results.json`
4. Include clear documentation and parameter reporting
5. Make the code reproducible (fixed seeds, pinned versions)

{{ output_spec | default(value="") }}
