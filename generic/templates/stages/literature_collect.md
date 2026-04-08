{{ agent_role | default(value="") }}

Collect relevant literature based on the search strategy.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Execute the search strategy and compile candidate papers:

1. For each source, run the defined queries
2. Record: title, authors, year, abstract, URL
3. Output as `candidates.jsonl` (one JSON object per line)

{{ output_spec | default(value="") }}
