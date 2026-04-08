{{ agent_role | default(value="") }}

Archive the research knowledge for future reference.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

Create an archive:

1. Compile all artifacts, code, and data references
2. Document the full research trail
3. Tag key learnings for future retrieval
4. Produce `archive_manifest.json`

{{ output_spec | default(value="") }}
