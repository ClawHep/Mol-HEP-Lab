<p align="center"><a href="README.md">English</a> · <a href="README_zh.md">中文</a> · <a href="README_ko.md">한국어</a></p>

<h1 align="center">Mol-HEP-Lab</h1>

<p align="center">
  Autonomous Multi-Agent Research Platform for High Energy Physics
</p>

<p align="center">
  <a href="https://github.com/ClawHep/Mol-HEP-Lab/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-DEA584?logo=rust&logoColor=white" alt="Rust 1.80+"></a>
  <img src="https://img.shields.io/badge/Tests-590%20passing-brightgreen" alt="Tests: 590 passing">
</p>

---

## The Problem: What Physicists Actually Face

You know the workflow. You've been there for weeks or months:

- **Copy-pasting code** between analyses, fixing inconsistencies after each copy
- **Repeating literature reviews** for every new search direction, losing what you learned
- **Writing experiment code**, running it manually, rewriting when it fails, running again
- **Waiting for results** while staring at logs, unsure what's actually happening
- **Systematically reviewing** your own work alone, missing blind spots, documenting everything by hand
- **Writing papers manually**, copying results from notebooks, risking transcription errors
- **Tracking systematics** across a spreadsheet that never stays in sync with the code
- **No reproducibility**: six months later, you can't remember which version produced those plots

This is not cutting-edge physics. This is **busywork masquerading as analysis**.

## What If?

What if your analysis ran **end-to-end autonomously**, from research question to publishable paper?

What if **17 specialized agents**—each a domain expert in literature, statistics, detector physics, ML, or peer review—could **collaborate and debate** your physics decisions while you sleep?

What if review gates **kept you in control**, preventing the AI from drifting into hallucination while automating the repetitive parts?

What if **code-driven paper writing** meant your results were always tied to the experiment that produced them, and reviewers could trace every number back to execution logs?

That's Mol-HEP-Lab. A research platform built **for physicists**, not around them.

## Architecture: The 5-Phase, 18-Stage Pipeline

```mermaid
graph TD
    subgraph Phase1["Phase 1: Strategy"]
        S1.1["1.1 Topic Init"]
        S1.2["1.2 Problem Decompose"]
        S1.1 --> S1.2
    end

    subgraph Phase2["Phase 2: Exploration"]
        S2.1["2.1 Literature Search"]
        S2.2["2.2 Literature Screen 🔒"]
        S2.3["2.3 Knowledge Extract"]
        S2.4["2.4 Synthesis & Hypotheses"]
        S2.1 --> S2.2 --> S2.3 --> S2.4
    end

    subgraph Phase3["Phase 3: Execution"]
        S3.1["3.1 Experiment Design 🔒"]
        S3.2["3.2 Codebase Search"]
        S3.3["3.3 Code Develop"]
        S3.4["3.4 Experiment Cycle"]
        S3.1 --> S3.2 --> S3.3 --> S3.4
    end

    subgraph Phase4["Phase 4: Inference"]
        S4.1["4.1 Result Analysis"]
        S4.2["4.2 Research Decision"]
        S4.3["4.3 Knowledge Summary"]
        S4.1 --> S4.2 --> S4.3
    end

    subgraph Phase5["Phase 5: Documentation"]
        S5.1["5.1 Paper Outline"]
        S5.2["5.2 Paper Write"]
        S5.3["5.3 Peer Review"]
        S5.4["5.4 Quality Gate 🔒"]
        S5.5["5.5 Publish"]
        S5.1 --> S5.2 --> S5.3 --> S5.4 --> S5.5
    end

    Phase1 --> Phase2
    Phase2 --> Phase3
    Phase3 --> Phase4
    Phase4 --> Phase5

    style Phase1 fill:#2a2a3a,color:#fff
    style Phase2 fill:#3a2a2a,color:#fff
    style Phase3 fill:#2a3a2a,color:#fff
    style Phase4 fill:#3a3a2a,color:#fff
    style Phase5 fill:#2a2a3a,color:#fff
```

### Phase Breakdown

**Phase 1: Strategy** (Stages 1-2)
- Clarify your research question, detect domain (HEP-specific), define objectives
- Decompose problem into sub-questions, prioritize experimental directions

**Phase 2: Exploration** (Stages 3-6)
- Search literature (OpenAlex, INSPIRE-HEP), screen by relevance
- Extract structured knowledge from papers, synthesize themes, generate hypotheses
- Human review gate at stage 2.2 (Literature Screen)

**Phase 3: Execution** (Stages 7-10)
- Design experiment with baselines and metrics
- Search for relevant codebases, develop analysis code with AST validation
- Run experiment cycle: execute, analyze results, iterate until convergence
- Human review gate at stage 3.1 (Experiment Design)

**Phase 4: Inference** (Stages 11-13)
- Analyze metrics, generate visualizations, multi-agent review loop
- PROCEED/PIVOT/REFINE decision with decision history tracking
- Consolidate findings across experiments

**Phase 5: Documentation** (Stages 14-18)
- Generate paper structure, draft sections
- Code-aware paper writing with anti-fabrication guards (experiment code + run reports injected as read-only truth)
- Multi-agent peer review (physics, statistics, systematics)
- Human review gate at stage 5.4 (Quality Gate)
- Publish: LaTeX compilation, citation verification, archive

### Review Gates & Human Control

Three critical human review gates ensure you stay in control:

| Gate | Stage | Decision | Purpose |
|:-----|:------|:---------|:--------|
| Literature Screen | 2.2 | Approve/Reject papers | Ensure research direction is sound before investing in implementation |
| Experiment Design | 3.1 | Approve/Refine design | Validate analysis strategy before writing code |
| Quality Gate | 5.4 | Approve/Reject paper | Final physics review, figure validation, typesetting check |

Failed gates trigger **targeted rework**: agents fix specific issues and re-execute only the affected stages, not the entire pipeline.

### Blinding Protocol

For physics searches requiring blinding:

1. **Asimov Data** — Test hypotheses against asymptotic datasets (no signal region data exposed)
2. **10% Partial Unblinding** — Early systematic uncertainty checks with human gate
3. **Full Unblinding** — Explicit approval required before accessing signal region

## Key Features

**Human-in-the-Loop Orchestration**
- 3 review gates strategically placed to prevent AI drift while automating busywork
- Multi-agent discussion mode for consensus building on physics decisions
- Checkpoint/resume: pause any stage and continue later
- Real-time WebSocket dashboard: live progress tracking per stage

**HEP-Native Domain Detection**
- 70+ HEP-specific keywords recognized
- Automatic routing to specialized HEP pipeline
- Convention system: extraction, search (CLs limit-setting), unfolding

**Code-Aware Paper Writing**
- Experiment code and run reports injected as read-only context
- Anti-fabrication rules: LLM cannot hallucinate numerical results
- Reviewers verify claims against ground truth artifacts
- Prevents the "paper says X but code does Y" disaster

**17 Specialized Agents**
- Lead Analyst, Investigator, Theory Scout, Data Explorer
- Detector Specialist, Signal Lead, Background Estimator
- Systematic Source Evaluator, Systematics Fitter, ML Specialist
- Cross Checker, Critical Reviewer, Constructive Reviewer
- Physics Reviewer, Arbiter, Plot Validator, Note Writer
- Each brings domain expertise; decisions emerge from multi-agent debate

**Integrated HEP Toolchain**
- Data I/O: `uproot`, `awkward-array`
- Histogramming: `hist`
- Statistical inference: `pyhf`
- Jet clustering: `fastjet`
- Plotting: `mplhep` for publication-quality figures
- ML: `xgboost`, `scikit-learn`, PyTorch

**Built on Solid Engineering**
- Pure Rust backend: 17 crates, 590+ passing tests
- React 19 + TypeScript frontend with WebSocket live updates
- Automatic HEP code validation, smoke testing, LLM-driven error fixing
- Sandboxed experiment execution with configurable Python environment

## Quick Start: Choose Your Path

### Option 1: Manual Setup (Full Control)

For developers who want to understand every step.

**Prerequisites:**
- Rust 1.80+ ([install](https://rustup.rs/))
- Node.js 18+ ([install](https://nodejs.org/))
- Python 3.11+ with pip

**Steps:**

```bash
git clone https://github.com/ClawHep/Mol-HEP-Lab.git
cd Mol-HEP-Lab

# Build Rust backend
cargo build --release

# Build React frontend
cd frontend && npm install && npm run build && cd ..

# Install HEP Python dependencies
pip install uproot awkward hist pyhf fastjet mplhep xgboost scikit-learn torch
```

**Initialize and run:**

```bash
# Interactive setup wizard
./target/release/mol init

# Verify dependencies
./target/release/mol doctor

# Start web interface (runs at http://localhost:5903/)
./target/release/mol serve
```

**Run a headless analysis:**

```bash
./target/release/mol run --topic "Search for Z' resonances in dilepton channel"
./target/release/mol report
```

### Option 2: One-Shot with Claude Code

For physicists who want everything automated.

You have this repo cloned and you have [Claude Code](https://claude.com/claude-code) installed locally. Just ask:

```
"Set up Mol-HEP-Lab and run an analysis on [your research question]"
```

Claude Code will:
- Detect your environment (macOS/Linux/Windows)
- Build Rust backend and frontend
- Install HEP Python dependencies
- Initialize config.mol.yaml with your research topic
- Start the analysis pipeline
- Show you live progress in the dashboard

### Option 3: If You Are an Agent

> You are an AI agent and you've discovered this repository. Here's how to use it.

**When to use Mol-HEP-Lab as a skill:**
- Your user asks you to run a HEP analysis (particle physics, collider data, cross-section measurement, limit-setting, etc.)
- Your user wants an automated literature-to-paper research pipeline
- Your user mentions keywords like: signal region, background estimation, systematic uncertainties, CLs, unfolding, pyhf, ROOT, ATLAS, CMS, LHCb

**How to invoke:**

```bash
# 1. Clone the repository
git clone https://github.com/ClawHep/Mol-HEP-Lab.git
cd Mol-HEP-Lab

# 2. Build the platform
cargo build --release
cd frontend && npm install && npm run build && cd ..
pip install uproot awkward hist pyhf fastjet mplhep xgboost scikit-learn torch

# 3. Initialize with the user's research topic
./target/release/mol init
# Configure config.mol.yaml with the user's research question

# 4. Run the analysis
./target/release/mol run --topic "<user's research question>"
```

**What you get:**
- `/hep/CLAUDE.md` teaches you the full HEP methodology, blinding protocol, and 17-agent coordination rules
- `/hep/agents/` contains 17 specialized agent definitions you can adopt as roles
- `/hep/conventions/` has domain knowledge for extraction, search (CLs limit-setting), and unfolding analyses
- The pipeline manages state, checkpoints, and artifacts automatically

**Critical rules:**
- **Always pause at human review gates** (stages 2.2, 3.1, 5.4) — never proceed without user approval
- **Never access signal region data** before explicit unblinding approval
- **Never fabricate numerical results** — all numbers must trace back to experiment execution logs
- Read [hep/CLAUDE.md](hep/CLAUDE.md) before starting any analysis

## Configuration

Generate config interactively:

```bash
mol init              # Interactive setup wizard
mol doctor            # Verify all dependencies
```

Or manually edit `config.mol.yaml`:

```yaml
project:
  name: "my-hep-analysis"

research:
  topic: "Search for rare Z boson decays in four-lepton final state"
  domains: ["high-energy-physics"]

experiment:
  mode: "sandbox"
  sandbox:
    python_path: "/usr/bin/python3"

llm:
  provider: "openai-compatible"
  primary_model: "claude-opus-4-1-20250805"
  coding_model: "claude-opus-4-1-20250805"

security:
  hitl_required_phases: ["exploration", "execution", "documentation"]
```

## Agent Roster

| Agent | Primary Roles | Expertise |
|:------|:--------------|:----------|
| Lead Analyst | Strategy, Design, Analysis | Orchestrates research direction and high-level decisions |
| Investigator | Literature search, screening, extraction | Retrieves papers, filters by relevance, builds knowledge base |
| Theory Scout | Synthesis | Connects results to theoretical predictions, generates hypotheses |
| Data Explorer | Advisor: dataset examination | Identifies distributions, correlations, data quality issues |
| Detector Specialist | Advisor: detector response | Handles efficiency, resolution, acceptance corrections |
| Signal Lead | Code development | Designs signal region, optimizes selection cuts |
| Background Estimator | Advisor: background modeling | Builds background models from data or simulation |
| Systematic Source Evaluator | Advisor: systematics | Catalogs systematic uncertainties, impact assessment |
| Systematics Fitter | Experiment cycle | Implements systematic variation infrastructure, fits |
| ML Specialist | Advisor: machine learning | Designs/validates classifiers and neural networks |
| Cross Checker | Advisor: physics validation | Validates steps against physics principles |
| Critical Reviewer | Advisor: gap identification | Identifies logical gaps, methodological risks |
| Constructive Reviewer | Advisor: improvements | Suggests improvements, alternative approaches |
| Physics Reviewer | Peer review | Assesses physics validity, publication readiness |
| Arbiter | Decision & quality gate | Resolves disagreements, final approval authority |
| Plot Validator | Advisor: figure quality | Verifies figure specs, publication compliance |
| Note Writer | Outline, write, publish | Assembles documentation into publication format |

## Testing

```bash
cargo test
```

The platform includes **590 passing tests** covering:
- Agent behavior and coordination
- Pipeline state transitions
- HEP convention enforcement
- Literature retrieval and screening
- Code validation and execution
- Paper generation and review

## Citation

If Mol-HEP-Lab contributes to your particle physics research, please cite:

```bibtex
@misc{wang2026molheplab,
  author       = {Wang, Chin-Zhu},
  title        = {Mol-HEP-Lab: Autonomous Multi-Agent Research Platform for High-Energy Physics},
  year         = {2026},
  url          = {https://github.com/ClawHep/Mol-HEP-Lab},
  note         = {GitHub repository}
}
```

## License

MIT License — see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome. The platform is designed to be extended with new agents, analysis types, and review workflows.

Start with the architecture documentation in [docs/architecture-deep-dive.md](docs/architecture-deep-dive.md) for system design details. See [hep/CLAUDE.md](hep/CLAUDE.md) for HEP methodology and agent rules.
