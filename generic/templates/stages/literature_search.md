{# Merged from: search_strategy.md + literature_collect.md #}
{{ agent_role | default(value="") }}

Design a literature search strategy and collect relevant papers.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

## Part 1: Search Strategy

Design a search strategy to find relevant literature:

1. **Search queries** — specific queries for academic databases
2. **Source databases** — where to search (arXiv, Google Scholar, etc.)
3. **Inclusion criteria** — what makes a paper relevant
4. **Exclusion criteria** — what disqualifies a paper

## Part 2: Literature Collection

Execute the search strategy and compile candidate papers:

1. For each source, run the defined queries
2. Record: title, authors, year, abstract, URL
3. Output as `candidates.jsonl` (one JSON object per line)

{{ output_spec | default(value="") }}
