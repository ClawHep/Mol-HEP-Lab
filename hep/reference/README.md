<p align="center">
  <h1 align="center">MoltHep</h1>
  <p align="center"><strong>Digital Physicist — Autonomous HEP Analysis Agent</strong></p>
  <p align="center">
    <em>A standalone AI system that runs publication-quality particle physics analyses 24/7.<br/>
    You choose the LLM, MoltHep does the physics.</em>
  </p>
</p>

---

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/ClawHep/MoltHep/main/install.sh | bash
```

The installer builds everything, sets up your PATH, and launches the onboarding wizard.

> **Requires:** Rust >= 1.87, Python >= 3.10, Node.js >= 20, Git

## Quick Start

```bash
molthep onboard           # Pick your LLM provider + model (OpenRouter, Anthropic, local, ...)
molthep new               # Create an analysis — describe your physics goal
molthep daemon            # Start the 24/7 daemon (Web UI at http://localhost:42617)
```

MoltHep is a **standalone system**. It uses whatever LLM you configure as the orchestrator brain — no need to run inside Claude Code or any other AI tool. Just `molthep new` and walk away.

## How It Works

MoltHep takes a natural-language physics goal and drives it through a 5-phase pipeline with review gates:

| Phase | What happens |
|-------|-------------|
| 1. Strategy | Multi-bot review of analysis plan |
| 2. Exploration | Data inspection, variable studies |
| 3. Processing | Event selection, background estimation |
| 4. Inference | Expected results, validation on 10% data, then full fit (human gate before unblinding) |
| 5. Documentation | Analysis note, plots, tables |

Each phase produces a gated artifact. Multiple executors (Claude Code, Codex, local LLMs) can run in parallel — best result wins.

## CLI

| Command | Description |
|---------|-------------|
| `molthep new` | Interactive analysis wizard |
| `molthep list` | Show all analyses |
| `molthep status <id>` | Phase-by-phase progress |
| `molthep daemon` | Start 24/7 service |
| `molthep onboard` | Configure LLM providers |
| `molthep doctor` | Health diagnostics |
| `molthep --update` | Pull and rebuild |

## Tests

```bash
PYTHONPATH=src pytest tests/ -v
```

## License

This project incorporates code from multiple open-source projects under their respective licenses.

---

<p align="center">
  <strong>MoltHep</strong> — Because physics research shouldn't sleep.
</p>
