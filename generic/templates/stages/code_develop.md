{# Merged from: code_generation.md + sanity_check.md #}
{{ agent_role | default(value="") }}

Generate experiment code and verify it through sanity checks.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

## Part 1: Code Generation

Generate code that implements the experiment plan:

1. Follow the experiment design specification exactly
2. Implement all methods and baselines
3. Output results as JSON to `results.json`
4. Include clear documentation and parameter reporting
5. Make the code reproducible (fixed seeds, pinned versions)

## Part 2: Sanity Check

Review the generated code for:

1. **Correctness** — does the code match the experiment plan?
2. **Completeness** — are all methods and metrics implemented?
3. **Reproducibility** — are random seeds fixed? Dependencies pinned?
4. **Output format** — will results.json have the expected structure?

If issues are found, fix them and produce the corrected code.
Produce `sanity_report.json` with pass/fail status and any issues found.

{{ output_spec | default(value="") }}
