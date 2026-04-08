{{ agent_role | default(value="") }}

Search the codebase for relevant existing code.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Search the project codebase for:

1. **Relevant files** — existing code that can be reused or adapted
2. **Codebase context** — how the existing code is structured
3. **Dependencies** — what libraries and tools are already available

{{ output_spec | default(value="") }}
