<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Generic Agent Definitions

## Purpose
Role definitions for generic (domain-agnostic) research agents. Describes the capabilities, responsibilities, and expected behavior of each agent role used in the research pipeline.

## Key Files
| File | Description |
|------|-------------|
| `researcher.md` | Computational research scientist role. Designs and executes experiments, analyzes results, communicates findings clearly. Follows scientific method: hypotheses → controlled experiments → evidence-based conclusions. |
| `reviewer.md` | Research quality reviewer. Evaluates methodology rigor, checks claims against evidence, identifies gaps and potential issues. Provides constructive feedback. |
| `analyst.md` | Data analyst. Performs statistical analysis, creates visualizations, interprets patterns, validates conclusions with appropriate methods. |

## Subdirectories
None.

## For AI Agents

### Working In This Directory

Each agent file contains a concise role description in markdown format. These are included in stage templates via `{{ agent_role }}` Jinja2 variable substitution.

To create a new agent role:
1. Create `{role_name}.md`.
2. Write 2-3 sentence description of role, capabilities, responsibilities.
3. Add mapping in parent `agents.yaml` to assign this role to stages.
4. Test by verifying stage templates render correctly with the new role.

### Testing Requirements

- Verify agent role descriptions are clear and actionable.
- Check that mapped stages in `agents.yaml` are consistent with agent responsibilities.
- Ensure agent files are valid markdown (no syntax errors).
- Test that agent roles render correctly when substituted into stage templates.

### Common Patterns

**Agent file template:**
```markdown
You are a {role title}. {Brief description of primary responsibility}.
{List key actions or methods}.
{Final guidance on expected output or behavior}.
```

**Role assignment check:**
```yaml
# agents.yaml
TOPIC_INIT: researcher        # OK - researcher plans research
LITERATURE_SCREEN: reviewer   # OK - reviewer filters papers
RESULT_ANALYSIS: analyst      # OK - analyst interprets data
```

## Dependencies
### Internal
- `../agents.yaml` — role-to-stage mappings
- `../templates/` — stage templates that include agent roles

### External
None.
