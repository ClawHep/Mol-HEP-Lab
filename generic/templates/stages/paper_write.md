{# Merged from: paper_draft.md + paper_revision.md #}
{{ agent_role | default(value="") }}

Write and revise the research paper.

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}

## Part 1: Paper Draft

Write the complete paper following the outline:

1. Abstract — concise summary of method and results
2. Introduction — motivation and contribution
3. Methods — detailed experimental approach
4. Results — findings with figures and tables
5. Discussion — interpretation and comparison to prior work
6. Conclusion — summary and outlook

## Experiment Context (READ-ONLY — ground truth for all numerical claims)

<experiment_code>
{{ experiment_code | default(value="(not available)") }}
</experiment_code>

<run_report>
{{ run_report | default(value="(not available)") }}
</run_report>

<experiment_summary>
{{ experiment_summary | default(value="(not available)") }}
</experiment_summary>

## Part 2: Revision (if review comments available)

Review comments: {{ review_comments | default(value="") }}

Address all review comments:

1. For each major comment: assess validity, make changes or provide rebuttal
2. For each minor comment: accept improvements, decline inaccuracies
3. Produce `paper_revised.md` and `revision_notes.md`

## CRITICAL: Anti-Fabrication Rules

1. Every numerical result in the paper MUST be traceable to the experiment context above
   or to cited external references.
2. Do NOT invent numbers, figures, or statistical results absent from experiment output.
3. Do NOT reference figure files unless they exist in experiment output.
4. If a reviewer requests a change requiring code modification or re-running experiments,
   mark it as: `[UNRESOLVABLE: requires code modification — <brief description>]`
5. When in doubt, keep the original number from the run report.

{{ output_spec | default(value="") }}
