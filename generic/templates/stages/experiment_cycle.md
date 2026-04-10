{# Merged from: resource_planning.md + experiment_run.md + iterative_refine.md #}
{{ agent_role | default(value="") }}

Plan resources, execute the experiment, and iteratively refine until convergence.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

## Part 1: Resource Planning

Estimate resource requirements:

1. **Compute** — CPU/GPU hours needed
2. **Memory** — RAM and disk requirements
3. **Time** — estimated wall-clock time
4. **Schedule** — execution order and parallelism opportunities

Produce `resource_plan.json` and `schedule.json`.

## Part 2: Experiment Execution

Execute the experiment:

1. Run all methods as specified in the experiment plan
2. Capture all output and logs
3. Verify outputs are complete and well-formed
4. Report any runtime errors or warnings

## Part 3: Iterative Refinement

Analyze results and refine:

1. Identify underperforming aspects
2. Diagnose root causes
3. Propose and apply targeted fixes
4. Re-run and verify improvement
5. Repeat until convergence

Produce `refinement_log.json` documenting all iterations.

{{ output_spec | default(value="") }}
