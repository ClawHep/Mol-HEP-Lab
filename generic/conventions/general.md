# General Research Conventions

## Reproducibility
- Fix random seeds and document them.
- Pin library versions in requirements.txt or pixi.toml.
- All results must be reproducible from a single command.

## Output Standards
- Primary results in `results.json`.
- Figures saved as PDF and PNG.
- Logs written to stdout/stderr, not files.

## Code Quality
- Clear variable names; avoid single-letter names except loop indices.
- Functions should do one thing and do it well.
- No hardcoded paths — use relative paths or command-line arguments.
