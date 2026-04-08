{{ agent_role | default(value="") }}

Refine the experiment based on initial results.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Review initial results and refine:

1. Identify any issues with the initial run
2. Adjust parameters or methodology as needed
3. Re-run refined experiments
4. Document all changes in `refinement_log.json`

{{ output_spec | default(value="") }}
