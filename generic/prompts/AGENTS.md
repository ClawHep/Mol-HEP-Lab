<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Prompts

## Purpose
Reusable prompt templates for common research tasks. Can be included in stage templates or composed with agent roles and conventions to form complete prompts for specific stages.

## Key Files
| File | Description |
|------|-------------|
| `experiment_design.md` | Generic experiment design guidance. Defines paradigm (comparison), specifies hypothesis formulation, test problem setup, baseline selection, confounding variable control, multi-trial runs with seed variation, and reproducibility documentation. |
| `code_generation.md` | Code generation standards. Emphasizes clarity, modularity, error handling, logging, and adherence to domain conventions. |
| `result_analysis.md` | Result analysis methodology. Covers statistical methods, visualization, interpretation, and evidence-based conclusions. |

## Subdirectories
None.

## For AI Agents

### Working In This Directory

Each prompt template is a reusable snippet that can be:
1. Included directly in stage templates via `{{ include('prompts/experiment_design.md') }}`.
2. Referenced in domain YAML as prompt customizations.
3. Extended by domain-specific prompts.

To add a new prompt:
1. Create `{task_name}.md`.
2. Write clear, actionable instructions for the task.
3. Use bullet points and sections for scannability.
4. Reference in stage templates or domain config.

### Testing Requirements

- Verify prompt instructions are specific and measurable (not vague).
- Check that all tools/libraries mentioned in prompts are available in domain environment.
- Test that prompts produce expected artifact types (JSON, markdown, etc.).
- Ensure prompts are domain-agnostic or clearly marked for specific domains.

### Common Patterns

**Prompt structure:**
```markdown
## {Task Name}

Paradigm: {comparison|validation|optimization|etc}

- {Key principle 1}
- {Key principle 2}
- {Specific method or format requirement}
```

**Metrics specification in prompts:**
```markdown
- Define clear hypotheses and success criteria before running experiments.
- Report results as JSON to {filename}.
- Include mean, standard deviation, and sample count.
```

## Dependencies
### Internal
- `../domain.yaml` — domain-specific customizations may extend these
- `../templates/` — stage templates that include these prompts

### External
None.
