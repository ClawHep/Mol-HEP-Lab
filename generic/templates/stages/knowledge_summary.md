{{ agent_role | default(value="") }}

Summarise the research knowledge gained.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Produce a knowledge summary:

1. **Key findings** — what was discovered
2. **Methods comparison** — which approaches worked best
3. **Limitations** — caveats and scope boundaries
4. **Future work** — open questions and next steps

Output as `knowledge_summary.json`.

{{ output_spec | default(value="") }}
