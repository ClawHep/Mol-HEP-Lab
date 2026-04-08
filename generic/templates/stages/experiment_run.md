{{ agent_role | default(value="") }}

Execute the experiment code.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Execute the experiment:

1. Run all methods as specified in the experiment plan
2. Capture all output and logs
3. Verify outputs are complete and well-formed
4. Report any runtime errors or warnings

{{ output_spec | default(value="") }}
