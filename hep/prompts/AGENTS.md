<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-12 -->

# HEP-Specific Prompt Templates

## Purpose

This directory contains HEP-specific prompt templates for code generation and experiment design. These templates are injected into agent contexts at runtime to guide code writing, experiment configuration, and result analysis. They complement the phase templates in `templates/phase{N}_claude.md` by providing domain-specific guidance for common HEP tasks.

## Key Files

| File | Description |
|------|-------------|
| code_generation.md | Template for code generation prompts: data I/O (uproot/awkward), selection, fitting, plotting |
| experiment_design.md | Template for experiment design: sample definitions, cut flow design, efficiency calculation |
| result_analysis.md | Template for result analysis: fit diagnostics, significance calculation, systematic breakdown |

## Prompt Template Structure

Each template follows this structure:

```markdown
# [Task Name]

## Context
[Domain-specific context for this task]

## Requirements
- [Requirement 1]
- [Requirement 2]
...

## Tools and Idioms
[Recommended HEP tools and usage patterns]

## Output Format
[Expected output structure]

## Example
[Working code example or pseudocode]
```

## For AI Agents

### Working In This Directory

1. **Templates are injected at runtime:** Agents do not read these files directly. The orchestration system injects them into agent context when spawning agents for specific stages.

2. **Domain-specific guidance:** These templates encode HEP-specific best practices: uproot for data I/O, pyhf for fitting, mplhep for plotting, awkward arrays for manipulation.

3. **Tool references are from methodology/07-tools.md:** Prompts reference HEP tool preferences (uproot, hist, pyhf, fastjet, mplhep, xgboost). Do not recommend tools outside the approved list.

4. **Code examples must be testable:** Every code example in a prompt must use approved tools and be executable with `pixi run py script.py`.

5. **Blinding awareness:** Prompts account for current phase. Phases 1–4a use Asimov or MC data. Phases 4b–4c handle unblinding conditionally.

### Testing Requirements

- All code examples in prompts must be verified to run with uproot, awkward, hist, pyhf, mplhep
- All prompts must reference methodology/07-tools.md for tool choices
- All prompts must validate against appendix-plotting.md for figure standards
- All sample definitions must be creatable from ROOT files or NTuples

### Common Patterns

- **Data I/O:** `uproot.open()` → `awkward.Array`, apply cuts, fill histograms with `hist`
- **Fitting:** `pyhf.Workspace()` with signal + background channels, nuisance parameters, constraints
- **Plotting:** `mplhep.style.use()`, `matplotlib` figures, error bars from fit covariance
- **Systematic sources:** Use `pyhf` `modifierType` entries (histoSys, shapeSys, lumi, stxs, etc.)

## Dependencies

### Internal

- All prompts depend on `../methodology/07-tools.md` (approved tools)
- All prompts depend on `../methodology/appendix-plotting.md` (figure standards)
- All prompts depend on `../conventions/` files (systematic sources, validation checks)
- Injected by orchestration into agent contexts at spawn time

### External

- HEP tools: uproot, awkward-array, hist, boost-histogram, pyhf, fastjet, mplhep, xgboost, scikit-learn
- ROOT data files or NTuples (user-supplied in analysis directory)
- MCP corpus tools for literature queries (search_lep_corpus, get_paper)
