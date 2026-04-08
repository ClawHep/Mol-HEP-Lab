{{ agent_role | default(value="") }}

Facilitate a research discussion.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Facilitate discussion on the research topic:

1. Summarise current state of the research
2. Identify open questions
3. Propose discussion points
4. Record conclusions in `discussion_notes.md`

{{ output_spec | default(value="") }}
