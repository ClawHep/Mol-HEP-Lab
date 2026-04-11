<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Stage Templates

## Purpose
Jinja2 instruction templates for each of the 18 research pipeline stages, aligned with the backend pipeline architecture. Templates define agent roles, expected inputs, task requirements, and output specifications for each stage.

## Key Files (Representative Sample)
| File | Stage | Description |
|------|-------|-------------|
| `topic_init.md` | 1. TOPIC_INIT | Define research goal from topic. Output: goal.md (motivation, questions, requirements, strategy, success criteria, milestones), hardware_profile.json. |
| `problem_decompose.md` | 2. PROBLEM_DECOMPOSE | Break down research problem into subtasks. Output: problem_tree.md, topic_evaluation.json. |
| `literature_search.md` | 3. LITERATURE_SEARCH | Search literature per search plan. Output: search_plan.yaml, sources.json, queries.json, candidates.jsonl. |
| `literature_screen.md` | 4. LITERATURE_SCREEN | Screen papers by inclusion/exclusion criteria. Output: screened_papers.jsonl, exclusion_reasons.json. |
| `knowledge_extract.md` | 5. KNOWLEDGE_EXTRACT | Extract key knowledge from papers. Output: knowledge_cards.json, citation_map.json. |
| `synthesis_hypotheses.md` | 6. SYNTHESIS_HYPOTHESES | Synthesize findings, generate hypotheses. Output: synthesis_report.md, gap_analysis.json, hypotheses.md. |
| `experiment_design.md` | 7. EXPERIMENT_DESIGN | Design controlled experiments. Output: exp_plan.yaml. |
| `codebase_search.md` | 8. CODEBASE_SEARCH | Search for relevant code/tools. Output: codebase_context.md. |
| `code_develop.md` | 9. CODE_DEVELOP | Implement experiment code + sanity check. Output: experiment/, experiment_spec.md, sanity_report.json. |
| `experiment_cycle.md` | 10. EXPERIMENT_CYCLE | Run experiments iteratively with refinement. Output: resource_plan.json, runs/, refinement_log.json, experiment_final/. |
| `result_analysis.md` | 11. RESULT_ANALYSIS | Analyze experiment results statistically. Output: analysis_report.md, experiment_summary.json. |
| `research_decision.md` | 12. RESEARCH_DECISION | Make research decisions based on results. Output: decision_record.json. |
| `knowledge_summary.md` | 13. KNOWLEDGE_SUMMARY | Summarize learned knowledge. Output: knowledge_summary.md. |
| `paper_outline.md` | 14. PAPER_OUTLINE | Create paper outline from research. Output: paper_outline.md. |
| `paper_write.md` | 15. PAPER_WRITE | Write and revise paper draft. Output: paper_draft.md, paper_revised.md, revision_notes.md. |
| `peer_review.md` | 16. PEER_REVIEW | Review paper critically. Output: review_comments.md, critical_review.md. |
| `quality_gate.md` | 17. QUALITY_GATE | Final quality checks (plots, rendering). Output: quality_report.md, plot_validation.md, rendering_review.md. |
| `publish.md` | 18. PUBLISH | Archive and publish findings. Output: archive_manifest.json, paper_final.md, paper.tex. |

All templates currently exist and are aligned with backend pipeline.

## Subdirectories
None.

## For AI Agents

### Working In This Directory

Each template is a Jinja2 file executed with stage-specific context:
- `agent_role` — role description (researcher/reviewer/analyst) from parent `agents.yaml`
- `conventions` — research standards from `conventions/general.md`
- `domain` — domain name from `domain.yaml`
- `topic` — research topic from project
- `previous_outputs` — artifacts from prior stages (if applicable)
- `output_spec` — expected output format

**Template structure example:**
```jinja2
{{ agent_role | default(value="") }}

You are a research scientist...

{{ conventions | default(value="") }}

---user---

Topic: {{ topic }}
Domain: {{ domain | default(value="generic") }}

Produce:
- output1.md
- output2.json
```

To update a template:
1. Edit `{stage_name}.md`.
2. Verify Jinja2 syntax.
3. Ensure output filenames match backend expectations.
4. Test by inspecting rendered output (not done here).

### Testing Requirements

- Syntax check: Verify all `{{` `}}` are balanced and valid Jinja2.
- Output validation: Confirm output filenames match stage definitions in backend `crates/mol-pipeline/src/stages.rs`.
- Context completeness: Ensure all required context variables (topic, domain) are passed at runtime.
- Fallback values: Check that optional variables have sensible defaults.
- Artifact flow: Verify next stage can consume outputs from this stage.

### Common Patterns

**Input specification:**
```jinja2
Previous stage output: {{ previous_outputs.literature_search }}
```

**Conditional output:**
```jinja2
{% if domain == "molecular" %}
Report molecular metrics...
{% endif %}
```

**Output format directive:**
```markdown
Produce `results.json` with structure:
{
  "metric_1": float,
  "metric_2": [...]
}
```

## Dependencies
### Internal
- `../agents.yaml` — agent role assignment per stage
- `../agents/` — agent role descriptions included via `{{ agent_role }}`
- `../conventions/general.md` — research standards included via `{{ conventions }}`
- `../domain.yaml` — domain context

### External
- Jinja2 template engine (backend pipeline executor)
- Backend stage definitions: `crates/mol-pipeline/src/stages.rs`
