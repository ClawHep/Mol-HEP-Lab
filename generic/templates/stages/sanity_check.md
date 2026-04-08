{{ agent_role | default(value="") }}

Verify the experiment code before execution.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Review the generated code for:

1. **Correctness** — does the code match the experiment plan?
2. **Completeness** — are all methods and metrics implemented?
3. **Reproducibility** — are random seeds fixed? Dependencies pinned?
4. **Output format** — will results.json have the expected structure?

Produce `sanity_report.json` with pass/fail status and any issues found.

{{ output_spec | default(value="") }}
