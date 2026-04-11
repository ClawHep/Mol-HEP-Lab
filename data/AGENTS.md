<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# Data

## Purpose

This directory contains static YAML data that drives the pipeline at runtime. These files define prompt templates, benchmark datasets, Docker sandbox profiles, domain-specific knowledge registries, and foundational literature references. They are loaded by `mol-config` at startup and injected into agent prompts and domain detection systems.

## Key Files

| File | Description |
|------|-------------|
| `prompts.default.yaml` | Default prompt templates for all pipeline stages. Organized by stage with template variables `{var_name}` for injection. |
| `dataset_registry.yaml` | Benchmark and dataset catalog with tiers (1=pre-cached, 2=downloadable, 3=too large). APIs for loading datasets in Python. |
| `benchmark_knowledge.yaml` | Domain-indexed registry of standard benchmarks, baselines, and evaluation metrics. Organized by domain (image_classification, NLP, etc.). |
| `docker_profiles.yaml` | Docker image profiles for sandbox execution. Per-domain image URLs, installed packages, GPU flags, memory limits. |
| `seminal_papers.yaml` | Foundational ML/HEP papers indexed by keyword. Used for automatic literature injection into prompts. |

## For AI Agents

### Working With This Directory

1. **Customizing Prompts**: Copy `prompts.default.yaml` to `prompts.custom.yaml`. Point your `config.mol.yaml` to the custom file:
   ```yaml
   prompts:
     custom_file: "data/prompts.custom.yaml"
   ```
   Edit any stage prompt. Use `{var_name}` syntax for template variables.

2. **Adding New Benchmarks**: Edit `benchmark_knowledge.yaml` or `dataset_registry.yaml`. Include tier, size, download API, and evaluation metrics.

3. **Updating Paper Registry**: Add entries to `seminal_papers.yaml` under the `papers` key. Keywords trigger automatic injection into literature prompts.

4. **Docker Profiles**: Edit `docker_profiles.yaml` to add domain-specific Docker images. Profile names must match domain IDs in your config.

5. **Template Variables**: Common variables include:
   - `{topic}` — research topic from config
   - `{time_budget_sec}` — experiment time limit
   - `{metric}` — primary metric key
   - `{exp_plan}` — experiment design from previous stage
   - `{domain}` — detected domain

### Loading at Runtime

The `mol-config` crate reads these files during initialization:

```rust
// In mol-config/src/lib.rs
let prompts = PromptsConfig::load("data/prompts.default.yaml")?;
let datasets = DatasetRegistry::load("data/dataset_registry.yaml")?;
let docker_profiles = DockerProfiles::load("data/docker_profiles.yaml")?;
```

### Validation

Verify YAML syntax before committing:
```bash
# Check YAML validity (requires `yq` or similar)
yq eval '.' data/prompts.default.yaml > /dev/null

# Verify all template variables are defined in the stage context
grep -o '{[^}]*}' data/prompts.default.yaml | sort -u
```

## Common Patterns

### Prompt Template Structure

```yaml
stages:
  stage_name:
    max_tokens: 8192
    system: "You are a role-specific agent..."
    user: |
      Task description.
      
      Constraint blocks: {compute_budget}
      
      Template variable: {topic}
```

### Dataset Entry

```yaml
- name: CIFAR-10
  tier: 1
  domain: image_classification
  size_mb: 170
  samples: 60000
  metrics: [accuracy, top1_accuracy]
  api: "torchvision.datasets.CIFAR10(root='/workspace/data', download=False)"
```

### Docker Profile

```yaml
profiles:
  ml_base:
    image: "molheplab/sandbox-ml:latest"
    packages:
      - torch
      - torchvision
      - numpy
    gpu: true
    memory_limit_mb: 8192
```

### Paper Entry

```yaml
papers:
  - title: "Deep Residual Learning for Image Recognition"
    authors: "He et al."
    year: 2016
    venue: "CVPR"
    cite_key: "he2016deep"
    keywords: ["residual networks", "ResNet", "skip connections"]
```

## Relationships

- **mol-config** reads all YAML files at startup
- **mol-templates** renders prompts using variables from runtime context
- **mol-experiment** selects Docker profiles based on domain
- **mol-literature** injects seminal papers into prompts based on keyword matching
- **mol-domains** uses benchmark_knowledge to classify experiments by domain

## Testing

To verify data consistency:

```bash
# Check YAML syntax
for file in data/*.yaml; do
  yq eval '.' "$file" > /dev/null || echo "Invalid: $file"
done

# Grep for undefined variables
grep -r '{[^}]*}' data/*.yaml | grep -o '{[^}]*}' | sort -u > /tmp/vars.txt
# Compare against known variable list in crates/mol-templates/src/context.rs
```

## Dependencies

### Internal
- `mol-config` — loads these files at startup
- `mol-templates` — renders prompts using Tera
- `mol-literature` — injects paper metadata
- `mol-experiment` — selects Docker profiles

### External
- YAML format (RFC 1123)
- Docker image URLs (must be pullable from runtime environment)

---

See `../AGENTS.md` for root-level context.
See `crates/mol-config/` for parsing logic.
See `crates/mol-templates/` for prompt rendering.
