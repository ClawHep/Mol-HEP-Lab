<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Examples

## Purpose

This directory contains configuration templates and example files for starting new research projects. Use these templates to bootstrap a project with the correct structure and placeholder values.

## Key Files

| File | Description |
|------|-------------|
| `config_template.yaml` | Template configuration file. Copy and customize for your project. Includes placeholders for project name, research topic, LLM provider, experiment budgets, and security settings. |

## For AI Agents

### Creating a New Project

1. **Copy the template**:
   ```bash
   cp examples/config_template.yaml my_project.yaml
   ```

2. **Replace placeholders**:
   - `__PROJECT_ID__` → unique identifier (e.g., `jet-classification-2026`)
   - `__TOPIC__` → research topic (e.g., "Jet tagging in high energy physics")
   - `__REFERENCE_PAPERS__` → YAML list of reference paper cite_keys from `data/seminal_papers.yaml`
   - `__API_KEY__` → LLM provider API key (or set via environment variable)

3. **Adjust settings**:
   - `research.domains` — list of relevant domain IDs (e.g., `["deep-learning", "particle-physics"]`)
   - `experiment.time_budget_sec` — total execution time in seconds
   - `experiment.max_iterations` — max rework/refinement loops
   - `llm.primary_model` — model to use for reasoning tasks
   - `llm.coding_model` — model to use for code generation
   - `security.hitl_required_stages` — stages requiring human review

4. **Start the project**:
   ```bash
   mol init --config my_project.yaml
   ```

### Template Structure

The template defines:

```yaml
project:
  name: project identifier
  mode: "full-auto" or "interactive"

research:
  topic: research question
  domains: [domain1, domain2]
  daily_paper_count: number
  quality_threshold: float
  reference_papers: [cite_key1, cite_key2]

notifications:
  channel: "console" | "email" | "slack" | "webhook"
  on_stage_start: true/false
  on_gate_required: true/false

knowledge_base:
  backend: "markdown" | "sqlite"
  root: "docs/kb"

openmol_bridge:
  use_message: enable Claude Messages API
  use_memory: enable persistent memory
  use_web_fetch: enable web search

llm:
  provider: "openai-compatible" | "acp" | "none"
  base_url: API endpoint
  api_key: secret (use env var)
  api_key_env: environment variable name
  primary_model: model name
  coding_model: code generation model
  image_model: vision model (if needed)
  fallback_models: [model1, model2]

security:
  hitl_required_stages: [stage_name, ...]

experiment:
  mode: "sandbox" | "local" | "docker" | "ssh" | "colab"
  time_budget_sec: seconds
  max_iterations: number
  metric_key: primary metric name
  metric_direction: "minimize" | "maximize"
  datasets_dir: path
  checkpoints_dir: path
  codebases_dir: path
```

### Common Customizations

**For HEP projects:**
```yaml
research:
  topic: "Higgs boson pair production cross-section measurement"
  domains:
    - "particle-physics"
    - "deep-learning"
  reference_papers:
    - "he2016deep"  # ResNet
    - "vaswani2017attention"  # Transformer (if applicable)
```

**For ML research:**
```yaml
research:
  topic: "Efficient vision transformer training"
  domains:
    - "deep-learning"
    - "computer-vision"
  quality_threshold: 4.0  # Higher threshold for ML venues
```

**With human review gates:**
```yaml
security:
  hitl_required_stages:
    - "literature_screening"
    - "experiment_design"
    - "quality_gate"
```

## For Developers

### Adding New Template Examples

1. Create a new file: `examples/config_DOMAIN.yaml` (e.g., `config_chemistry.yaml`)
2. Add domain-specific defaults (pre-configured packages, datasets, models)
3. Document the example in this AGENTS.md file

### Verifying Templates

Before committing, validate YAML syntax:
```bash
yq eval '.' examples/config_template.yaml > /dev/null
yq eval '.' examples/config_*.yaml > /dev/null
```

Check for undefined placeholders:
```bash
grep -n '__[A-Z_]*__' examples/*.yaml
```

All placeholders should be documented in this file.

## Dependencies

### Internal
- `mol-config` — parses these YAML files at runtime
- `mol-cli` — uses these templates in `mol init` command

### External
- YAML format (RFC 1123)

---

See `../AGENTS.md` for root-level context.
See `../config.mol.yaml` for the actual runtime configuration used by the default pipeline.
See `crates/mol-config/` for YAML parsing logic.
