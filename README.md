Mol-HEP-Lab: Autonomous Multi-Agent Research Platform for High Energy Physics
===============================================================================

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust 1.80+](https://img.shields.io/badge/Rust-1.80%2B-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tests: 559 passing](https://img.shields.io/badge/Tests-559%20passing-brightgreen)]()

Mol-HEP-Lab is an autonomous multi-agent research platform designed for particle physicists. It orchestrates 17 specialized HEP agents through a 5-phase analysis pipeline with human-in-the-loop review gates. Built entirely in Rust (17 crates) with a React frontend, the system automates the complete workflow from analysis strategy through paper publication while keeping physicists in control.

## Architecture

### Technology Stack
- **Backend:** Pure Rust (17 crates: mol-cli, mol-engine, mol-pipeline, mol-agents, mol-llm, mol-experiment, mol-services, mol-literature, mol-domains, mol-knowledge, mol-config, mol-templates, mol-health, mol-metamol, mol-web, mol-evolution, mol-common)
- **Frontend:** React + TypeScript
- **HEP Tools:** uproot, awkward-array, hist, pyhf, fastjet, mplhep, xgboost
- **Conventions:** Extraction, search (CLs limit-setting), unfolding

### Pipeline Structure
The platform orchestrates a 5-phase HEP analysis pipeline:

**Phase 1: Strategy**
- Topic initialization and problem decomposition
- Entry point for analyst-provided research questions

**Phase 2: Exploration**
- Search strategy formulation
- Literature collection and screening
- Knowledge extraction and synthesis
- Hypothesis generation from literature foundation

**Phase 3: Processing**
- Experiment design and review gate
- Codebase search, code generation, sanity checks
- Resource planning
- Experiment execution with iterative refinement

**Phase 4: Inference**
- Result analysis and research decision
- Knowledge summary of findings

**Phase 5: Documentation**
- Paper outline, draft, peer review, revision
- Quality gate
- Knowledge archival, export/publish, citation verification

### Blinding Protocol
Embedded blinding workflow:
1. **Asimov data** — Hypothesis validation with asimptotic datasets
2. **10% partial unblinding** — Early systematic checks
3. **Full unblinding** — Human-gated decision point

### Review Tiers
Multi-bot review system with A/B/C classification:
- Physics reviewer
- Critical reviewer
- Constructive reviewer
- Arbiter (consensus)

## Agent Roles

Mol-HEP-Lab coordinates 17 specialized agents:

| Agent | Responsibility |
|:------|:---|
| Lead Analyst | Orchestrates analysis strategy and high-level decisions |
| Data Explorer | Examines datasets, identifies distributions and correlations |
| Detector Specialist | Handles detector response, efficiency, resolution |
| Theory Scout | Connects experimental results to theoretical predictions |
| Signal Lead | Designs signal region, optimizes acceptance cuts |
| Background Estimator | Builds background models from data or simulation |
| Systematic Source Evaluator | Identifies and catalogs systematic uncertainties |
| Systematics Fitter | Implements systematic variation infrastructure |
| ML Specialist | Designs and validates machine learning components |
| Cross Checker | Validates analysis steps against physics principles |
| Critical Reviewer | Identifies logical gaps and methodological risks |
| Constructive Reviewer | Suggests improvements and alternative approaches |
| Physics Reviewer | Assesses physics validity and publication readiness |
| Arbiter | Resolves disagreements, final approval authority |
| Plot Validator | Verifies figure quality and specification compliance |
| Rendering Reviewer | Checks publication-standard figure rendering |
| Note Writer | Assembles analysis documentation into publication format |

## Key Features

**Human-in-the-Loop Orchestration**
- Strategic review gates at critical decision points
- Multi-agent discussion mode for consensus building
- Rollback and continuation from any checkpoint
- Real-time WebSocket dashboard for monitoring

**HEP-Native Domain Detection**
- 70+ HEP-specific keywords recognized
- Automatic routing to HEP-specialized analysis pipeline
- Convention system supports measurement and search analyses

**Blinded Analysis Support**
- Asimov data validation before unblinding
- Configurable unblinding thresholds
- Audit trail of all unblinding decisions

**Integrated Tool Chain**
- Statistical modeling: pyhf for likelihood fits
- Plotting: mplhep for publication-standard figures
- Machine learning: xgboost for classification
- Array manipulation: awkward-array for complex physics data

## Quick Start

### Installation

Clone and build:
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
mol serve               # Start server at http://localhost:8765/
```

Run headless:
```bash
mol run --topic "Search for Z' resonances in dilepton channel"
mol run --to-phase exploration
mol report              # View results from latest run
```

## Configuration Reference

Key configuration fields in `config.mol.yaml`:

| Field | Purpose | Example |
|:------|:--------|:--------|
| `project.name` | Analysis identifier | `"bsll-2024"` |
| `research.topic` | Research question | `"B → K* μμ decay"` |
| `research.domains` | Domain specialization | `["high-energy-physics"]` |
| `experiment.sandbox.python_path` | Python interpreter path | `/usr/bin/python3` |
| `llm.primary_model` | Main research model | `gpt-5.4` |
| `llm.coding_model` | Code generation model | `gpt-5.4` |
| `security.hitl_required_phases` | Human review gates | `["exploration", "processing", "documentation"]` |

Set `hitl_required_phases` to enforce human approval at phase boundaries. Common gates are exploration (literature screening), processing (experiment design), and documentation (quality).

## Testing

Run the test suite:
```bash
cargo test
```

The platform includes 559 passing tests validating agent behavior, pipeline state transitions, HEP convention enforcement, and literature retrieval.

## Multi-Agent Discussion Mode

Launch discussion mode to gather consensus from multiple agents:
```bash
mol discuss --topic "Most robust background subtraction method for pp → WW"
```

Agents debate methodologies, resolve contradictions, and synthesize consensus recommendations. Outputs include ranked options and justification for each position.

## Knowledge Base Integration

Analyses accumulate knowledge in a persistent knowledge base (markdown-backed by default):
```bash
mol kb export                 # Export knowledge base
mol kb search "cross section" # Search previous analyses
```

Cross-project knowledge transfer improves subsequent analyses.

## Blinded Analysis Example

For a new physics search with unblinding requirements:
```bash
mol run --topic "Search for stop squark pair production" \
        --blind-strategy asimov-partial-full
```

Pipeline sequence:
1. Validation on Asimov (expected) data
2. 10% partial unblinding for systematic checks
3. HITL gate requires physicist approval before full unblinding
4. Full data analysis proceeds after human sign-off

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

@misc{wu2026clawailab,
  author       = {Wu, Fan and Chen, Cheng and Tan, Zhenshan and Zhang, Taiyu and
                  Gao, Dingcheng and Zhu, Lanyun and Zhu, Qi and Tan, Yi and Ji, Deyi and 
                  Lin, Guosheng and Chen, Tianrun and Ye, Deheng and Liu, Fayao},
  title        = {Claw AI Lab: An Autonomous Multi-Agent Research Team},
  year         = {2026},
  url          = {https://github.com/Claw-AI-Lab/Claw-AI-Lab},
  note         = {GitHub repository}
}
```

## License

MIT — see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome. The platform is designed to be extended with new agents, analysis types, and review workflows. See the codebase organization for integration points.
