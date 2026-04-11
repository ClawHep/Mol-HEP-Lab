<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-metamol

## Purpose
Meta-level reasoning and self-improvement loops for Mol-HEP-Lab. Provides MetaMol integration layer including session management, PRM (Process Reward Model) quality gates with majority-vote LLM judging, skill effectiveness tracking, lesson extraction, and static mappings of pipeline stages to applicable skills.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/session.rs` | MetaMol proxy session lifecycle and communication |
| `src/prm_gate.rs` | PRM quality gate with majority-vote LLM judging |
| `src/skill_feedback.rs` | Skill effectiveness tracking and statistics |
| `src/lesson_to_skill.rs` | Convert failure lessons into reusable skill files |
| `src/stage_skill_map.rs` | Static mapping of pipeline stages to skills |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate for meta-learning integration
- Run `cargo test -p mol-metamol` to test skill feedback and lesson extraction
- Run `cargo clippy -p mol-metamol` for linting
- Main entry points: `MetaMolSession::new()` for session mgmt, `ResearchPRMGate::judge()` for quality gates
- Skill mapping: `get_skills_for_stage(stage)` returns applicable skills

### Testing Requirements
- Session tests: MetaMol proxy startup, health checks, connection failure handling
- PRM gate tests: majority-vote logic with multiple LLM judges
- Skill feedback tests: recording effectiveness, calculating statistics
- Lesson extraction tests: converting failures into lesson entries
- Stage skill mapping tests: verify coverage of all pipeline stages
- Skill file writing tests: YAML format and metadata

### Common Patterns
- MetaMol sessions use subprocess bridge for skill loading
- PRM gates use multiple independent LLM calls, majority-vote on pass/fail
- Skill effectiveness tracked via success/failure counts and time series
- Lessons classified by category (Configuration, Methodology, DataHandling, etc.)
- Skill drafts include trigger conditions and remediation text
- Stage-skill mapping is static but extensible

## Dependencies
### Internal
- mol-common (types)
- mol-config (MetaMol configuration)
- mol-llm (LLM client for PRM gates and skill evaluation)

### External
- `serde` + `serde_json` (skill file serialization)
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `reqwest` (subprocess communication)
- `uuid` (unique IDs)
- `chrono` (timestamps)
- `tempfile` (test fixtures)
