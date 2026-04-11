<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-config

## Purpose
Configuration loading, validation, and management for Mol-HEP-Lab. Defines all configuration structures for project settings, LLM providers, experiments, knowledge bases, and pipeline modes. Provides typed config resolution with YAML/JSON support and comprehensive validation.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Config path resolution and public API |
| `src/types.rs` | All configuration struct definitions (MolConfig, LlmConfig, ExperimentConfig, etc.) |
| `src/validate.rs` | Configuration validation rules and error reporting |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate used for configuration throughout the system
- Run `cargo test -p mol-config` to verify validation logic
- Run `cargo clippy -p mol-config` for linting
- Core API is `MolConfig::load(path)` and `resolve_config_path()`
- Add new config sections by extending MolConfig struct and adding validation rules

### Testing Requirements
- Unit tests for config path resolution (with/without explicit path)
- Validation tests for each config section (missing required fields, invalid ranges)
- Round-trip tests: load YAML → validate → serialize back to YAML
- Use `tempfile` to create temporary config files for testing

### Common Patterns
- All config structs derive Serde (Deserialize/Serialize)
- Configuration files use YAML format with fallback to JSON
- Search order: explicit path → config.mol.yaml → config.yaml
- Validation returns `ValidationResult` enum (Pass/Warn/Fail)
- Error messages use `anyhow::Context` for clarity

## Dependencies
### Internal
- mol-common (types and utilities)

### External
- `serde` + `serde_json` + `serde_yaml` (serialization)
- `thiserror` (error types)
- `anyhow` (error handling)
- `tracing` (logging)
