{{ agent_role | default(value="") }}

Design experiments to test the hypotheses.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}
{{ datasets | default(value="") }}

Design an experiment plan:

1. **Methods** — approaches to implement and compare
2. **Baselines** — reference methods for comparison
3. **Metrics** — how to measure success
4. **Data** — what inputs are needed
5. **Parameters** — configuration and hyperparameters
6. **Success criteria** — thresholds for accepting results

{{ output_spec | default(value="") }}
