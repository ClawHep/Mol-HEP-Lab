{# Merged from: synthesis.md + hypothesis_gen.md #}
{{ agent_role | default(value="") }}

Synthesise extracted knowledge and generate testable hypotheses.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

## Part 1: Synthesis

Produce:

1. **Synthesis report** — integrated view of the field
2. **Gap analysis** — what is missing or under-explored
3. **State of the art** — current best approaches and their limitations

## Part 2: Hypothesis Generation

Based on the synthesis and gap analysis, generate:

1. **Hypotheses** — specific, testable predictions
2. **Rationale** — why each hypothesis is plausible
3. **Test criteria** — how to confirm or refute each hypothesis
4. **Priority ranking** — which hypotheses to test first

{{ output_spec | default(value="") }}
