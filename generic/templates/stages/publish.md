{# Merged from: knowledge_archive.md + export_publish.md + citation_verify.md #}
{{ agent_role | default(value="") }}

Archive knowledge, export to publication format, and verify citations.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

## Part 1: Knowledge Archive

Create a comprehensive archive manifest:

1. Link all analysis artifacts (literature, code, results, paper)
2. Classify as public or internal
3. Apply FAIR data principles

Produce `archive_manifest.json`.

## Part 2: Export to Publication Format

Finalize the paper for submission:

1. Ensure notation consistency throughout
2. Format references properly
3. Prepare all figures and tables
4. Convert to submission-ready format (LaTeX if needed)

Produce `paper_final.md` and `paper.tex`.

## Part 3: Citation Verification

Verify all citations:

1. Confirm each reference exists and is correctly cited
2. Check that stated claims match cited sources
3. Identify missing citations
4. Verify bibliography format

Produce `verification_report.json`.

{{ output_spec | default(value="") }}
