{{ agent_role | default(value="") }}

Plan computational resources for experiment execution.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Estimate resource requirements:

1. **Compute** — CPU/GPU hours needed
2. **Memory** — RAM and disk requirements
3. **Time** — estimated wall-clock time
4. **Schedule** — execution order and parallelism opportunities

Produce `resource_plan.json` and `schedule.json`.

{{ output_spec | default(value="") }}
