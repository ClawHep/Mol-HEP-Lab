<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Generic

## Purpose
Domain-agnostic agent templates, conventions, and utilities. Provides baseline agent definitions (Researcher, Reviewer, Analyst), general research conventions, prompt templates for common research tasks, and stage templates for the 18-stage research pipeline. Used as fallback when domain-specific configurations are not available.

## Key Files
| File | Description |
|------|-------------|
| `agents.yaml` | Stage-to-agent mapping. Maps each of 18 pipeline stages (TOPIC_INIT through PUBLISH) to generic agent roles (researcher, reviewer, analyst). Overridable by domain-specific layers. |
| `domain.yaml` | Generic domain profile. Default CPU-only, no GPU. Specifies Docker image, pip packages (numpy, scipy, matplotlib, pandas, sklearn), metrics, and experiment terminology. |
| `contracts.yaml` | Contract definitions (minimal, currently sparse). |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `agents/` | Generic agent role definitions (markdown instructions) |
| `conventions/` | General research standards and best practices |
| `prompts/` | Prompt templates for research tasks |
| `templates/` | Stage-specific instruction templates and stage subdirectory |

## For AI Agents

### Working In This Directory

**Agent role mapping:** Edit `agents.yaml` to change which agent handles each stage. Standard roles: `researcher`, `reviewer`, `analyst`. Add custom roles by creating new `.md` files in `agents/`.

**Domain inheritance:** Stages inherit agent assignments from this directory unless overridden by domain-specific config. Domain-specific `agents.yaml` takes precedence over `generic/agents.yaml`.

**Stage templates:** Each `.md` file in `templates/stages/` is a Jinja2 template executed at runtime with stage-specific context (topic, analysis type, previous outputs, domain info).

### Testing Requirements

- Verify all `stage_agents` keys in `agents.yaml` match actual stage names used in pipeline.
- Ensure referenced agent files (researcher.md, reviewer.md, analyst.md) exist and contain valid prompts.
- Check that domain.yaml pip packages are available in Docker image.
- Test that stage templates render correctly with mock variables.
- Validate YAML syntax with linter.

### Common Patterns

**Stage template structure:**
```markdown
{{ agent_role | default(value="") }}

You are a {role description}.

{{ conventions | default(value="") }}

---user---

{Task description}

Produce outputs: {list}

{{ output_spec | default(value="") }}
```

**Agent YAML mapping:**
```yaml
stage_agents:
  TOPIC_INIT: researcher
  RESULT_ANALYSIS: analyst
  PEER_REVIEW: reviewer
```

**Domain config:**
```yaml
domain: generic
paradigm: comparison
gpu_required: false
pip_packages:
  - numpy
  - scipy
```

## Dependencies
### Internal
- `agents/` — agent role definitions
- `conventions/` — research conventions (included in stage templates)
- `prompts/` — reusable prompt snippets
- `templates/` — stage instruction templates

### External
- Docker image specified in `domain.yaml` (researchmol/sandbox-generic:latest)
- Standard Python packages: numpy, scipy, matplotlib, pandas, scikit-learn
