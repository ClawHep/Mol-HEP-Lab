{{ agent_role | default(value="") }}

Screen collected literature for relevance and quality.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Screen the collected papers against inclusion/exclusion criteria:

1. Evaluate each candidate against the criteria
2. Produce `screened_papers.jsonl` with accepted papers
3. Produce `exclusion_reasons.json` documenting why rejected papers were excluded

{{ output_spec | default(value="") }}
