<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Templates

## Purpose
Stage instruction templates and utilities. Contains Jinja2 templates for each of the 18 research pipeline stages, plus subdirectories for stage-specific patterns. Templates are parameterized with agent roles, conventions, domain info, and previous stage outputs at runtime.

## Key Files
| File | Description |
|------|-------------|
| (None at root level — all templates in `stages/` subdirectory) |  |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `stages/` | 18 stage templates aligned with pipeline: TOPIC_INIT, PROBLEM_DECOMPOSE, LITERATURE_SEARCH, etc. |

## For AI Agents

### Working In This Directory

Templates are rendered at runtime using Jinja2 with context:
- `agent_role` — role description from `agents.yaml` + `agents/{role}.md`
- `conventions` — research standards from `conventions/`
- `domain` — domain name and profile from `domain.yaml`
- `topic` — research topic from user input
- `analysis_type` — analysis type if provided
- `previous_outputs` — artifact filenames from previous stages
- `output_spec` — expected output format and filenames

To add or modify a stage template:
1. Create or edit `stages/{STAGE_NAME}.md`.
2. Use Jinja2 syntax: `{{ variable }}`, `{{ variable | default(value="fallback") }}`.
3. Define all expected outputs clearly.
4. Test rendering with mock context variables.

### Testing Requirements

- Verify template syntax (balanced `{{` `}}`).
- Check that all `{{ variable }}` are available in runtime context.
- Test fallback values for optional fields with `| default()`.
- Ensure output specifications match actual agent artifact naming conventions.
- Verify stage templates are aligned with pipeline architecture (inputs from previous stage, outputs for next stage).

### Common Patterns

**Template structure:**
```jinja2
{{ agent_role | default(value="") }}

You are a {role}. {Task overview}.

{{ conventions | default(value="") }}

---user---

{Task details}

Topic: {{ topic }}
Domain: {{ domain | default(value="generic") }}

Produce: {list outputs}
```

**Variable references:**
```jinja2
{{ topic }}
{{ domain | default(value="generic") }}
{{ previous_outputs.stage_1.goal_md }}
{{ output_spec | default(value="") }}
```

**Output format specification:**
```markdown
Produce:
- `goal.md` — {description}
- `hardware_profile.json` — {description}
```

## Dependencies
### Internal
- `stages/` — individual stage templates
- `../agents/` — agent role definitions included via `{{ agent_role }}`
- `../conventions/` — research standards included via `{{ conventions }}`
- `../prompts/` — reusable prompt snippets

### External
- Jinja2 template engine (used by backend pipeline executor)
