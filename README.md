Mol-HEP-Lab: Autonomous Multi-Agent Research Platform for High Energy Physics
===============================================================================

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust 1.80+](https://img.shields.io/badge/Rust-1.80%2B-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tests: 590 passing](https://img.shields.io/badge/Tests-590%20passing-brightgreen)]()

Mol-HEP-Lab is an autonomous multi-agent research platform designed for particle physicists. It orchestrates specialized HEP agents through an 18-stage, 5-phase analysis pipeline with human-in-the-loop review gates. Built entirely in Rust (17 crates) with a React 19 + TypeScript frontend, the system automates the complete workflow from analysis strategy through paper publication while keeping physicists in control.

## Architecture

### Technology Stack
- **Backend:** Pure Rust workspace (17 crates: mol-cli, mol-engine, mol-pipeline, mol-agents, mol-llm, mol-experiment, mol-services, mol-literature, mol-domains, mol-knowledge, mol-config, mol-templates, mol-health, mol-metamol, mol-web, mol-evolution, mol-common)
- **Frontend:** React 19 + TypeScript + Vite, served by axum static file server with WebSocket live updates
- **LLM Integration:** OpenAI-compatible providers via ACP (Anthropic Claude Protocol) bridge
- **HEP Tools:** uproot, awkward-array, hist, pyhf, fastjet, mplhep, xgboost
- **Conventions:** Extraction, search (CLs limit-setting), unfolding

### Pipeline Structure

The platform orchestrates a 5-phase, 18-stage HEP analysis pipeline:

**Phase 1: Strategy** (Stages 1-2)
1. **Topic Init** — Parse research question, generate SMART objectives, detect hardware
2. **Problem Decompose** — Break topic into sub-question tree, prioritize directions

**Phase 2: Exploration** (Stages 3-6)
3. **Literature Search** — Plan retrieval strategy, query OpenAlex/INSPIRE-HEP
4. **Literature Screen** — Filter candidates by relevance and quality (review gate)
5. **Knowledge Extract** — Build structured knowledge cards from shortlisted papers
6. **Synthesis & Hypotheses** — Cluster themes, identify gaps, generate falsifiable hypotheses

**Phase 3: Execution** (Stages 7-10)
7. **Experiment Design** — Design experiment YAML with baselines and metrics (review gate)
8. **Codebase Search** — Retrieve relevant code repositories and build context
9. **Code Develop** — Generate experiment code, AST-validate, smoke-test, LLM-fix failures
10. **Experiment Cycle** — Execute in sandbox, edit-run-evaluate loop until convergence

**Phase 4: Inference** (Stages 11-13)
11. **Result Analysis** — Analyze metrics, generate visualizations, multi-agent review
12. **Research Decision** — PROCEED / PIVOT / REFINE with decision history
13. **Knowledge Summary** — Distill cross-experiment findings

**Phase 5: Documentation** (Stages 14-18)
14. **Paper Outline** — Generate section structure
15. **Paper Write** — Draft paper with code-aware revision (anti-fabrication guards)
16. **Peer Review** — Physics, statistics, and systematics review (A/B/C classification)
17. **Quality Gate** — Final assessment, figure validation, typesetting check (review gate)
18. **Publish** — Archive manifest, LaTeX compilation, citation verification

### Review Gates & Rework

- **Structured verdict system:** Reviewers produce `{name}_verdict.json` with A/B/C-classified findings
- **Cross-stage rework:** `retry_from_stage` enables targeted re-execution bounded by `MAX_DECISION_PIVOTS`
- **Anti-fabrication guards:** Paper Write and Peer Review stages inject experiment code + run report as read-only ground truth

### Blinding Protocol

Embedded blinding workflow for physics searches:
1. **Asimov data** — Hypothesis validation with asymptotic datasets
2. **10% partial unblinding** — Early systematic checks
3. **Full unblinding** — Human-gated decision point

## Agent Roles

Mol-HEP-Lab coordinates 17 specialized agents across the pipeline:

| Agent | Primary Stages | Responsibility |
|:------|:---------------|:---|
| Lead Analyst | Strategy, Design, Analysis | Orchestrates strategy and high-level decisions |
| Investigator | Search, Screen, Extract | Literature retrieval, screening, and knowledge extraction |
| Theory Scout | Synthesis | Connects results to theoretical predictions, generates hypotheses |
| Data Explorer | (advisor) | Examines datasets, identifies distributions and correlations |
| Detector Specialist | (advisor) | Handles detector response, efficiency, resolution |
| Signal Lead | Codebase, Code Develop | Designs signal region, optimizes acceptance cuts |
| Background Estimator | (advisor) | Builds background models from data or simulation |
| Systematic Source Evaluator | (advisor) | Identifies and catalogs systematic uncertainties |
| Systematics Fitter | Experiment Cycle | Implements systematic variation infrastructure |
| ML Specialist | (advisor) | Designs and validates machine learning components |
| Cross Checker | (advisor) | Validates analysis steps against physics principles |
| Critical Reviewer | (advisor) | Identifies logical gaps and methodological risks |
| Constructive Reviewer | (advisor) | Suggests improvements and alternative approaches |
| Physics Reviewer | Peer Review | Assesses physics validity and publication readiness |
| Arbiter | Decision, Quality Gate | Resolves disagreements, final approval authority |
| Plot Validator | (advisor) | Verifies figure quality and specification compliance |
| Note Writer | Outline, Write, Publish | Assembles documentation into publication format |

Agents operate in two modes: **primary** (drives the stage) and **advisor** (provides domain expertise as injected context).

## Key Features

**Human-in-the-Loop Orchestration**
- Review gates at Literature Screen, Experiment Design, and Quality Gate
- Multi-agent discussion mode for consensus building
- Checkpoint/resume from any stage
- Real-time WebSocket dashboard with per-stage progress

**HEP-Native Domain Detection**
- 70+ HEP-specific keywords recognized
- Automatic routing to HEP-specialized pipeline
- Convention system for measurement, search, and unfolding analyses

**Code-Aware Paper Writing**
- Experiment code and run reports injected as read-only context
- Anti-fabrication rules prevent LLM hallucination of numerical results
- Reviewer cross-verification against ground truth artifacts

**Integrated Tool Chain**
- Statistical modeling: pyhf for likelihood fits
- Plotting: mplhep for publication-standard figures
- Machine learning: xgboost for classification
- Array manipulation: awkward-array for complex physics data

## Quick Start

### Installation

```bash
git clone https://github.com/ClawHep/Mol-HEP-Lab.git
cd Mol-HEP-Lab

# Build Rust backend (requires Rust 1.80+)
cargo build --release

# Build frontend
cd frontend && npm install && npm run build && cd ..

# Install HEP Python dependencies
pip install uproot awkward hist pyhf fastjet mplhep xgboost
```

### Configuration

Generate configuration interactively:
```bash
mol init              # Interactive setup
mol doctor            # Verify all dependencies
```

Or manually configure `config.mol.yaml`:
```yaml
project:
  name: "my-analysis"

research:
  topic: "Search for rare Z boson decays"
  domains: ["high-energy-physics"]

experiment:
  mode: "sandbox"
  sandbox:
    python_path: "/path/to/python3"

llm:
  provider: "openai-compatible"
  primary_model: "gpt-5.4"
  coding_model: "gpt-5.4"
```

### Running Analyses

Start the web interface:
```bash
mol serve               # Start server at http://localhost:5903/
```

Run headless:
```bash
mol run --topic "Search for Z' resonances in dilepton channel"
mol run --to-phase exploration
mol report              # View results from latest run
```

## Configuration Reference

| Field | Purpose | Example |
|:------|:--------|:--------|
| `project.name` | Analysis identifier | `"bsll-2024"` |
| `research.topic` | Research question | `"B -> K* mu mu decay"` |
| `research.domains` | Domain specialization | `["high-energy-physics"]` |
| `experiment.sandbox.python_path` | Python interpreter path | `/usr/bin/python3` |
| `llm.primary_model` | Main research model | `gpt-5.4` |
| `llm.coding_model` | Code generation model | `gpt-5.4` |
| `security.hitl_required_phases` | Human review gates | `["exploration", "execution", "documentation"]` |

## Testing

```bash
cargo test
```

The platform includes 590 passing tests covering agent behavior, pipeline state transitions, HEP convention enforcement, and literature retrieval.

## Citation

If Mol-HEP-Lab contributes to your physics analysis, please cite:

```bibtex
@misc{wang2026molheplab,
  author       = {Wang, Maxen},
  title        = {Mol-HEP-Lab: Autonomous Multi-Agent Research Platform for High-Energy Physics},
  year         = {2026},
  url          = {https://github.com/ClawHep/Mol-HEP-Lab},
  note         = {GitHub repository}
}
```

## License

MIT — see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome. The platform is designed to be extended with new agents, analysis types, and review workflows. See `hep/` for domain knowledge organization and `crates/` for backend architecture.
