<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-domains

## Purpose
Scientific domain models and ontologies for Mol-HEP-Lab. Provides domain detection via keywords and LLM, per-domain profile loading with experiment paradigms and metrics, domain adapters for code generation hints and Docker image selection, and prompt customization for domain-specific research.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/profile.rs` | DomainProfile, ResearchDomain enums, metric types |
| `src/detector.rs` | Fast keyword detection and LLM-based domain detection |
| `src/adapters/mod.rs` | DomainAdapter trait and per-domain implementations |
| `src/adapters/chain_backed.rs` | Chain-backed adapters for multiple domains |
| `src/experiment_schema.rs` | Per-domain experiment type definitions |
| `src/prompt_adapter.rs` | Domain-specific prompt customization |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/adapters/` | Domain adapter implementations |

## For AI Agents

### Working In This Directory
- This is a library crate for domain detection and adaptation
- Run `cargo test -p mol-domains` to test domain detection and adapter logic
- Run `cargo clippy -p mol-domains` for linting
- Main entry points: `detect_domain(text)` for keyword detection or `detect_domain_with_llm()` for LLM-based
- Adapter factory: `adapter_for(profile)` returns appropriate domain adapter

### Testing Requirements
- Domain detection tests: verify keyword matching (both keyword and LLM methods)
- Profile loading tests: verify static profiles for each domain
- Adapter tests: verify code hints, Docker images, and prompt customization
- Chain adapter tests: handle multiple domains in a single experiment
- LLM detection tests: mock LLM responses for domain classification

### Common Patterns
- DomainAdapter trait defines interface: domain(), default_docker_image(), code_generation_hints()
- Fast detection uses keyword lookup; slower but more accurate detection uses LLM
- Each domain has a static DomainProfile with paradigms and metrics
- Adapters are singleton instances (use lazy_static or once_cell)
- Prompt customization includes domain-specific writing tips and methodology

## Dependencies
### Internal
- mol-common (types)
- mol-config (domain configuration)

### External
- `serde` + `serde_json` + `serde_yaml` (config serialization)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `tempfile` (test fixtures)
