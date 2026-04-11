<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# mol-templates

## Purpose
LaTeX template compilation and rendering for ML conference papers. Provides async LaTeX compilation (pdflatex → bibtex → pdflatex ×2), post-compilation quality checks, structured templates for NeurIPS/ICML/ICLR (2024–2026), and Markdown-to-LaTeX conversion for agent-generated content.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Module organization and re-exports |
| `src/compiler.rs` | LaTeX compilation (pdflatex, bibtex) and quality checks |
| `src/conferences.rs` | Conference templates (NeurIPS, ICML, ICLR) with preambles and sections |
| `src/converter.rs` | Markdown to LaTeX conversion for agent output |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | All modules are flat in this directory |

## For AI Agents

### Working In This Directory
- This is a library crate for LaTeX handling
- Run `cargo test -p mol-templates` to test compilation and conversion
- Run `cargo clippy -p mol-templates` for linting
- Main entry points: `get_template(Conference)` for templates, `compile_latex(path)` for compilation
- Markdown conversion: `markdown_to_latex(markdown_text)` for body sections

### Testing Requirements
- Template tests: verify skeleton generation for each conference/year
- Compilation tests: mock pdflatex/bibtex in PATH (use which crate to find)
- Converter tests: verify Markdown → LaTeX for various syntax (bold, links, citations)
- Quality check tests: verify PDF is valid after compilation
- Error handling: missing pdflatex, bibtex failures, invalid LaTeX

### Common Patterns
- Templates use Tera template engine for variable substitution
- Compilation is async using `tokio::process::Command`
- Multiple compilation passes ensure references resolve
- Quality checks verify PDF size, page count, and structure
- Markdown converter preserves citations via [@key] syntax

## Dependencies
### Internal
- mol-common (types)

### External
- `tokio` (async runtime)
- `anyhow` + `thiserror` (error handling)
- `tracing` (logging)
- `tera` (template engine)
- `regex` (text parsing)
- `serde` (serialization)
