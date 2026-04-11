<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Conventions

## Purpose
General research conventions and best practices for reproducibility, output standards, and code quality. Automatically included in stage templates to set baseline expectations for all research agents regardless of domain.

## Key Files
| File | Description |
|------|-------------|
| `general.md` | Reproducibility (fixed random seeds, pinned library versions), Output Standards (results.json, PDF/PNG figures, stdout/stderr logging), Code Quality (clear naming, single responsibility, relative paths) |

## Subdirectories
None.

## For AI Agents

### Working In This Directory

The `general.md` file defines shared standards applied to all research stages. These conventions are included in stage templates via the `{{ conventions }}` Jinja2 variable.

To add domain-specific conventions:
1. Create a new `.md` file in this directory or in domain-specific config.
2. List standards as bullet points or sections.
3. Include via `{{ conventions }}` in stage templates or `conventions` field in domain YAML.

### Testing Requirements

- Verify conventions are clear and enforceable by LLM agents.
- Check for conflicts between general conventions and domain-specific ones.
- Ensure paths and file formats mentioned in conventions are actually used in pipeline.
- Test that conventions are correctly included in rendered stage templates.

### Common Patterns

**Convention structure:**
```markdown
## {Category}
- Standard 1: {Description}
- Standard 2: {Description}
```

**Categories:**
- Reproducibility
- Output Standards
- Code Quality
- Safety/Ethics
- Documentation

**Inclusion in templates:**
```jinja2
{{ conventions | default(value="") }}
```

## Dependencies
### Internal
- `../domain.yaml` — domain-specific conventions may override these
- `../templates/` — stage templates that include these conventions

### External
None.
