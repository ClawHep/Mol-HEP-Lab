# ResearchClaw Engine - Complete Architecture Report
**Date:** April 6, 2026 | **Total Files:** 168 | **Total Python Code:** ~51,000 LOC

---

## 1. ROOT LEVEL MODULES (5,743 LOC)

### Core Infrastructure Files
- `__init__.py` (3 LOC): Version "0.3.1"
- `__main__.py` (6 LOC): CLI entry point
- `adapters.py` (107 LOC): Adapter protocols & recording stubs
  - Protocols: CronAdapter, MessageAdapter, MemoryAdapter, SessionsAdapter, WebFetchAdapter, BrowserAdapter
  - Recording implementations for testing & deterministic behavior
  - AdapterBundle: aggregates all adapters

- `health.py` (674 LOC): System health checks & doctor reporting
  - Classes: CheckResult, DoctorReport
  - Functions: check_python_version, check_yaml_import, check_llm_connectivity, check_api_key_valid, check_model_available, check_sandbox_python, check_matplotlib, check_experiment_mode, check_acp_agent, check_ascend_runtime, check_docker_runtime, run_doctor, print_doctor_report, write_doctor_report

- `evolution.py` (478 LOC): Lesson storage & evolution tracking
  - Classes: LessonCategory (Enum), LessonEntry, EvolutionStore
  - Tracks learned patterns from failed/successful runs for continuous improvement

- `hardware.py` (272 LOC): Hardware profiling
  - Classes: HardwareProfile
  - Functions: detect_hardware, estimate_batch_size, estimate_parallel_jobs

- `quality.py` (187 LOC): Content quality assessment
  - Classes: TemplateMatch, QualityReport
  - Functions: detect_template_content, compute_template_ratio, assess_quality, check_strict_quality
  - Detects placeholder/template content in LLM-generated text

- `writing_guide.py` (79 LOC): Conference writing best practices
  - Dict: CONFERENCE_WRITING_TIPS (NeurIPS/ICML/ICLR patterns)
  - Function: format_writing_tips() for prompt injection

- `prompts.py` (2,334 LOC): Large prompt library (structure only)
  - Multiple system prompts, user message templates for different stages
  - Dynamic prompt rendering functions

- `report.py` (196 LOC): Report generation & summarization

- `cli.py` (596 LOC): CLI interface (20+ commands)

- `config.py` (812 LOC): Configuration management
  - Classes: RCConfig (primary configuration)
  - YAML/JSON config loading, validation

---

## 2. PIPELINE ORCHESTRATION (21,704 LOC across 40 files)

### Core Structure
- `pipeline/__init__.py`: Package definition
- `pipeline/contracts.py`: Shared data contracts
- `pipeline/stages.py`: Stage definitions & status enums
- `pipeline/executor.py`: Pipeline execution harness
- `pipeline/runner.py`: Top-level pipeline runner

### 2A. CLAW_ENGINE (Agentic Core - 1,503 LOC, 8 files)
**Purpose:** Generic turn-based LLM agent loop with tool calling

Files:
- `claw_engine/__init__.py` (14 LOC)
- `claw_engine/session.py` (91 LOC): StageSession class
  - Timestamped logging, artifact tracking, auto-persist to JSON/text
- `claw_engine/turn_loop.py` (713 LOC): AgentTurnLoop class (CRITICAL)
  - TurnResult dataclass
  - SessionProtocol interface
  - _TraceLog for step-by-step debugging
  - MAX_ITERATIONS = 40
  - LLM call → tool execution loop with verification hooks

Tools subdirectory (4 files, 685 LOC):
- `tools/__init__.py` (10 LOC)
- `tools/definitions.py` (143 LOC): TOOL_SPECS registry
  - Tool specs: bash, read_file, write_file, edit_file, glob_search, grep_search
  - tool_spec() factory function
- `tools/executor.py` (411 LOC): ToolExecutor class
  - Methods: _bash, _read_file, _write_file, _edit_file, _glob_search, _grep_search
  - Path resolution, result truncation, snapshot saving
- `tools/permissions.py` (121 LOC): SandboxPermissionPolicy class

### 2B. CODEGEN (Code Generation Pipeline - 3,198 LOC, 12 files)
**Purpose:** Agentic code generation using strategies pattern

Core files (8 files, 2,397 LOC):
- `codegen/__init__.py` (47 LOC)
- `codegen/session.py` (121 LOC): CodegenSession class
- `codegen/types.py` (124 LOC): CodegenContext, CodegenPhase, CodegenResult, DiscoveredData, HardwareProfile
- `codegen/registry.py` (57 LOC): StrategyRegistry, build_default_registry()
- `codegen/router.py` (55 LOC): CodegenRouter class
- `codegen/runtime.py` (592 LOC): CodegenRuntime class (CRITICAL)
  - _discover_data() pre-reads filesystem context
  - Main execution orchestration
- `codegen/system_prompt.py` (424 LOC): Dynamic system prompt building
- `codegen/turn_loop.py` (1,417 LOC): CodegenTurnLoop class (CRITICAL)
  - LLM iteration with tool dispatch
  - Plan compliance checks
  - Anti-simulation gates
  - Detailed turn tracing

Strategies (4 files, 361 LOC):
- `strategies/__init__.py` (1 LOC)
- `strategies/base.py` (47 LOC): CodegenStrategy protocol
  - Interface: name, can_handle(), generate()
- `strategies/claw_agent.py` (240 LOC): ClawAgentStrategy
  - Uses AgentTurnLoop for code generation
- `strategies/fallback.py` (73 LOC): FallbackStrategy (simple template-based)

### 2C. PIPELINE STAGES (39 LOC + individual stage runtimes)

Standard Stages (7 stages × 3 files each):
1. **sanity_check/** (3 files): Validates setup
   - runtime.py, system_prompt.py, __init__.py

2. **result_analysis/** (3 files): Analyzes experiment results
   - runtime.py, system_prompt.py, __init__.py

3. **codegen/** (12 files, covered above): Code generation

4. **experiment_run/** (3 files): Runs the experiment
   - runtime.py, system_prompt.py, __init__.py

5. **iterative_refine/** (3 files): Iteratively improves results
   - runtime.py, system_prompt.py, __init__.py

Bridges (2 files):
- `opencode_bridge.py`: Integration with OpenCode
- `openhands_bridge.py`: Integration with OpenHands

- `code_agent.py`: Standalone code agent wrapper

---

## 3. AGENTS (6,887 LOC, 24 files)

### Agent Base Infrastructure (200 LOC)
- `agents/base.py`:
  - Classes: _LLMResponseLike (Protocol), _LLMClientLike (Protocol), AgentStepResult, BaseAgent, AgentOrchestrator
  - BaseAgent: abstract base for all agents
  - AgentOrchestrator: coordinates multiple agents

### 3A. BENCHMARK_AGENT (1,200+ LOC, 7 files)
Autonomous benchmark discovery & evaluation

- `orchestrator.py`: BenchmarkAgentConfig, BenchmarkPlan, BenchmarkOrchestrator
- `surveyor.py`: SurveyorAgent — discovers relevant benchmarks
- `selector.py`: SelectorAgent — selects appropriate benchmarks
- `acquirer.py`: AcquirerAgent — downloads & prepares datasets
- `validator.py`: ValidatorAgent — validates benchmark setup
- `__init__.py`

### 3B. CODE_SEARCHER (1,300+ LOC, 6 files)
GitHub code discovery & pattern extraction

- `agent.py`: CodeSearchAgent, CodeSearchResult
- `github_client.py`: GitHubClient, RepoInfo, CodeSnippet, RepoAnalysis
- `cache.py`: SearchCache for caching results
- `query_gen.py`: Query generation from code patterns
- `pattern_extractor.py`: CodePatterns extraction
- `__init__.py`

### 3C. FIGURE_AGENT (3,500+ LOC, 13 files)
Autonomous figure generation for papers

- `orchestrator.py`: FigureAgentConfig, FigurePlan, FigureOrchestrator
- `planner.py`: PlannerAgent — designs figure layouts
- `codegen.py`: CodeGenAgent — generates visualization code
- `critic.py`: CriticAgent — evaluates figure quality
- `decision.py`: FigureDecisionAgent — makes style/content decisions
- `integrator.py`: IntegratorAgent — integrates multiple figures
- `renderer.py`: RendererAgent — renders final figures
- `nano_banana.py`: NanoBananaAgent — lightweight figure generation
- `style_config.py`: Figure styling configuration
- `__init__.py`

---

## 4. LLM INTEGRATION (1,196 LOC, 4 files)

- `llm/__init__.py`: Package definition
- `llm/client.py` (300+ LOC):
  - Classes: LLMResponse, LLMConfig, LLMClient
  - Generic LLM interface (model-agnostic)

- `llm/anthropic_adapter.py` (300+ LOC):
  - Classes: AnthropicAdapter
  - Adapter for Claude/Anthropic API

- `llm/acp_client.py` (300+ LOC):
  - Classes: ACPConfig, ACPClient
  - ACP (Anthropic Custom Protocol) client for Agent Protocol

---

## 5. METACLAW BRIDGE (802 LOC, 7 files)

Integration with metaclaws (higher-level agent framework)

- `config.py`: PRMConfig, LessonToSkillConfig, MetaClawBridgeConfig
- `session.py`: MetaClawSession class
- `prm_gate.py`: ResearchPRMGate class (process reward model gating)
- `lesson_to_skill.py`: Converts learned lessons to reusable skills
- `skill_feedback.py`: SkillEffectivenessRecord, SkillFeedbackStore
- `stage_skill_map.py`: Maps pipeline stages to available skills
- `__init__.py`

---

## 6. LITERATURE MANAGEMENT (3,139 LOC, 9 files)

Academic paper discovery, retrieval, verification

- `models.py`: Author, Paper dataclasses
- `arxiv_client.py`: ArXiv API client for preprints
- `semantic_scholar.py`: Semantic Scholar API client (citation info)
- `openalex_client.py`: OpenAlex API client (bibliographic data)
- `novelty.py`: Novelty detection for discovered papers
- `verify.py`: Citation verification, VerifyStatus (Enum), CitationResult, VerificationReport
- `search.py`: Paper search orchestration
- `cache.py`: Caching layer for API responses
- `__init__.py`

---

## 7. DOMAINS (1,652 LOC, 35 files)

Domain-specific adapters for different research fields

### Core Domain Logic (300+ LOC)
- `detector.py`: DomainProfile, ExperimentParadigm (Enum), MetricType (Enum)
  - Auto-detects research domain from context
- `experiment_schema.py`: UniversalExperimentPlan, Condition, MetricSpec, EvaluationSpec
  - ConditionRole, ExperimentType enums
- `prompt_adapter.py`: PromptAdapter (ABC), MLPromptAdapter, GenericPromptAdapter
  - Abstract base for domain-specific prompting
- `__init__.py`

### Domain Adapters (10 files)
Each domain has a PromptAdapter subclass for specialized prompting:
- `adapters/ml.py`: MLPromptAdapter
- `adapters/physics.py`: PhysicsPromptAdapter
- `adapters/chemistry.py`: ChemistryPromptAdapter
- `adapters/biology.py`: BiologyPromptAdapter
- `adapters/economics.py`: EconomicsPromptAdapter
- `adapters/math.py`: MathPromptAdapter
- `adapters/security.py`: SecurityPromptAdapter
- `adapters/generic.py`: GenericPromptAdapter (fallback)
- `adapters/__init__.py`

---

## 8. EXPERIMENT MANAGEMENT (4,775 LOC, 14 files)

Sandbox execution, validation, metrics collection

### Sandbox Implementations (600+ LOC each)
- `sandbox.py`: ExperimentSandbox (base class), SandboxProtocol, SandboxResult
- `docker_sandbox.py`: DockerSandbox (containerized execution)
- `colab_sandbox.py`: ColabDriveSandbox (Google Colab integration)
- `ssh_sandbox.py`: SshRemoteSandbox (remote SSH execution)

### Experiment Orchestration
- `runner.py`: ExperimentRunner, ExperimentResult, ExperimentHistory
  - _ChatResponse, _ChatClient, _GitManager protocols
- `factory.py`: SandboxFactory (creates appropriate sandbox type)

### Metrics & Validation
- `metrics.py`: MetricType (Enum), ExperimentResults, UniversalMetricParser
  - Parses results from different experiment types
- `validator.py`: ValidationIssue, CodeValidation, _SecurityVisitor (AST-based)
  - Security validation of generated code
- `evaluators/convergence.py`: Convergence evaluation logic

### Infrastructure
- `git_manager.py`: ExperimentGitManager (version control)
- `harness_template.py`: ExperimentHarness (test harness generation)
- `visualize.py`: Result visualization utilities
- `__init__.py`

---

## 9. WEB & SEARCH (1,385 LOC, 7 files)

Web crawling, search, PDF extraction

- `search.py`: SearchResult, WebSearchResponse, WebSearchClient
  - Web search integration
- `agent.py`: WebSearchAgent, WebSearchAgentResult
  - Agentic web search orchestration
- `crawler.py`: WebCrawler, CrawlResult
  - Web page crawling with caching
- `pdf_extractor.py`: PDFContent, PDFExtractor
  - PDF text/metadata extraction
- `scholar.py`: ScholarPaper, GoogleScholarClient
  - Google Scholar integration
- `connectivity.py`: ConnectivityReport
  - Network connectivity checks
- `__init__.py`

---

## 10. KNOWLEDGE BASE (283 LOC, 2 files)

KB entry management with Markdown & Obsidian backends

- `base.py` (283 LOC):
  - Classes: KBEntry
  - Functions: write_kb_entry, write_stage_to_kb, generate_weekly_report
  - _markdown_frontmatter, _obsidian_enhancements helpers
  - KB_CATEGORY_MAP: Maps stages to KB categories
  - Backends: "markdown" (plain) | "obsidian" (with wikilinks)
- `__init__.py`

---

## 11. TEMPLATES & PAPER GENERATION (2,366 LOC, 4 files)

Conference-specific LaTeX templates & paper compilation

- `compiler.py` (600+ LOC):
  - Classes: CompileResult, QualityCheckResult
  - Compilation to PDF with quality checks
  - Multi-stage LaTeX build with error recovery
- `conference.py`:
  - Classes: ConferenceTemplate
  - Conference-specific template loading
- `converter.py` (500+ LOC):
  - Classes: _Section
  - Markdown → LaTeX conversion
- `__init__.py`

### Template Styles (10 files, .sty/.bst)
- NeurIPS 2024, 2025
- ICML 2025, 2026
- ICLR 2025, 2026

---

## 12. UTILITIES (475 LOC, 4 files)

- `codebase_manifest.py`: Manifest of codebase structure
- `sanitize.py`: Text sanitization for safe output
- `thinking_tags.py`: Parsing/handling LLM thinking tags
- `__init__.py`

---

## 13. INFRASTRUCTURE

### Docker Support (9 files in docker/)
- `Dockerfile`: Base image
- `Dockerfile.ascend`: Huawei Ascend NPU support
- `Dockerfile.biology`, `.chemistry`, `.economics`, `.math`, `.physics`, `.generic`: Domain-specific images
- `entrypoint.sh`: Container entrypoint script

### Data Files (5 files in data/)
**YAML registries:**
1. `benchmark_knowledge.yaml` (35 KB): Standard benchmarks, datasets, baselines per domain
   - Tier 1-3 benchmarks (cached, downloadable, too-large)
   - Standard APIs & metrics
2. `dataset_registry.yaml` (4.1 KB): Dataset specifications
3. `seminal_papers.yaml` (19 KB): Canonical papers for different domains
4. `docker_profiles.yaml` (2.9 KB): Docker container profiles
5. `framework_docs/`: MD files for training frameworks (axolotl, llamafactory, peft, transformers, trl)

### Feedback (1 file)
- `feedback/FEEDBACK_ANALYSIS_PROMPT.md`: Prompt for analyzing stage failures

---

## 14. CONFIGURATION & CLI (1,408 LOC)

- `config.py` (812 LOC):
  - RCConfig: Main config class
  - Config validation, model registry
- `cli.py` (596 LOC):
  - 20+ CLI commands
  - Run pipeline, manage experiments, view results
  - Configuration & health check commands

---

## ARCHITECTURAL PATTERNS

### 1. **Strategy Pattern** (CodegenStrategy)
- Protocol-based strategies with can_handle() & generate()
- StrategyRegistry dispatches to appropriate strategy
- Currently: ClawAgentStrategy (agentic) + FallbackStrategy (template)

### 2. **Adapter Pattern** (PromptAdapter, DomainProfile)
- Abstract base with domain-specific implementations
- Auto-detection of domain from context
- Customizes LLM prompts for domain knowledge

### 3. **Turn Loop Architecture**
- Generic AgentTurnLoop used by both claw_engine & codegen
- Handles LLM call → tool execution → retry cycle
- Supports verification hooks for custom gates
- Step-by-step tracing for debugging

### 4. **Sandbox Abstraction** (SandboxProtocol)
- Multiple sandbox implementations: Docker, SSH, Colab
- Unified interface for experiment execution
- Factory pattern for sandbox selection

### 5. **Session State Tracking**
- StageSession: Timestamped logging with auto-persist
- CodegenSession: Code generation specific state
- MetaClawSession: Integration with metaclaws framework

### 6. **Tool Registry Pattern**
- TOOL_SPECS: Centralized tool definitions
- ToolExecutor: Dispatches by name to typed handlers
- Permissions: SandboxPermissionPolicy for access control

### 7. **Plugin/Agent Orchestration**
- AgentOrchestrator: Coordinates multiple specialized agents
- BaseAgent: Extensible base class
- Used by BenchmarkAgent, FigureAgent orchestrators

### 8. **Data Class Heavy**
- Extensive use of dataclasses for immutable contracts
- Type hints throughout (Python 3.9+ with __future__ imports)
- Frozen dataclasses for shared state objects

---

## DEPENDENCIES (Key External Libraries)

**Core:**
- anthropic (Claude API)
- pydantic (validation)
- pyyaml (config)

**LLM/Agent:**
- openai (fallback LLM)
- litellm (multi-model LLM routing)

**Science/ML:**
- numpy, scipy, scikit-learn
- torch, torchvision, transformers
- jax, flax (for deep learning)

**Web/Crawling:**
- requests, httpx
- beautifulsoup4, selenium
- arxiv, crossref (academic APIs)

**Execution:**
- docker (Docker client)
- paramiko (SSH)
- git (version control)

**Data/Config:**
- pandas, polars (data frames)
- json, yaml (serialization)

**Visualization/Output:**
- matplotlib, plotly, seaborn
- PIL (image processing)
- pypdf, reportlab (PDF)

---

## RUST REWRITE SCOPE ESTIMATION

### Modules by Complexity

**CRITICAL (must port):**
1. **claw_engine/** (1,503 LOC)
   - AgentTurnLoop: Core agent loop - moderate complexity
   - ToolExecutor: Tool dispatch - low complexity
   - Turn-by-turn LLM interaction

2. **codegen/** (3,198 LOC)
   - CodegenRuntime: Pre-discovery, context building - moderate
   - CodegenTurnLoop: Extended turn loop with compliance checks - moderate-high
   - System prompt building - moderate (string manipulation)

3. **pipeline/executor.py** + **runner.py** (500+ LOC)
   - Pipeline orchestration - moderate complexity

**HIGH VALUE:**
4. **experiment/** (4,775 LOC)
   - SandboxResult, sandbox protocols - low-moderate
   - Metrics parsing (UniversalMetricParser) - moderate
   - Validator (SecurityVisitor) - moderate

5. **agents/** (6,887 LOC)
   - BenchmarkAgent, FigureAgent, CodeSearchAgent - variable complexity
   - Strategy-based dispatch - low-moderate

**MODERATE VALUE:**
6. **literature/** (3,139 LOC)
   - API clients (ArXiv, SemanticScholar, OpenAlex) - moderate
   - Cache layer - low
   - Novelty detection - moderate

7. **domains/** (1,652 LOC)
   - PromptAdapter implementations - low-moderate
   - Domain detection - moderate

8. **web/** (1,385 LOC)
   - Crawler, PDF extractor, search - moderate
   - API integration - moderate

**LOWER PRIORITY:**
9. **templates/** (2,366 LOC)
   - PDF compilation (external process calls) - moderate
   - LaTeX string generation - low

10. **metaclaw_bridge/** (802 LOC)
    - Skill feedback, PRM gating - moderate

### Estimated Rust Complexity

| Module | LOC | Complexity | Challenges |
|--------|-----|-----------|-----------|
| claw_engine | 1.5K | **MODERATE** | Async I/O, trait bounds, tool dispatch |
| codegen | 3.2K | **MODERATE-HIGH** | Complex state machine, string templating, nested loops |
| pipeline | 21.7K | **MODERATE** | Stage orchestration, file I/O, artifact tracking |
| agents | 6.9K | **MODERATE** | Protocol flexibility, orchestration patterns |
| experiment | 4.8K | **MODERATE** | Process management, result parsing, validation AST |
| literature | 3.1K | **MODERATE** | HTTP clients, JSON parsing, caching |
| domains | 1.7K | **LOW-MODERATE** | Enum dispatch, trait implementations |
| web | 1.4K | **MODERATE** | HTTP, crawling, PDF extraction (C dependency) |
| templates | 2.4K | **LOW-MODERATE** | String manipulation, external process calls |
| Other | 4.2K | **LOW** | Utilities, utilities, config |

**Total:** ~51K LOC Python → ~35-40K LOC Rust (25-30% smaller, more explicit)

### Key Rust Challenges

1. **Async Runtime**: Turn loops, LLM calls, sandbox execution → tokio/async-await
2. **String Templating**: Dynamic system prompts → handlebars/tera or custom
3. **Type Flexibility**: Protocol-based dispatch → trait objects or enums
4. **File System I/O**: Heavy file I/O throughout → tokio::fs
5. **JSON/YAML Config**: pydantic replacement → serde + serde_yaml
6. **External Processes**: Docker, SSH, subprocess calls → tokio::process
7. **PDF Extraction**: Python dependency → pdfium-render or similar
8. **API Clients**: Anthropic, ArXiv, GitHub → reqwest + custom clients
9. **AST Validation**: Python AST visitor pattern → syn crate or custom parser

### Rewrite Priority

**Phase 1 (MVP - 70% effort):** claw_engine + codegen + pipeline (~8K LOC)
**Phase 2 (80% effort):** agents + experiment + domains (~13K LOC)
**Phase 3 (Final 90%+):** literature + web + templates + remaining (~30K LOC)

---

## SUMMARY

ResearchClaw is a **sophisticated multi-agent research automation system** with:

- **168 files, ~51K LOC Python**
- **Generic turn-loop LLM agent engine** (claw_engine)
- **Strategy-based code generation** with tool use (codegen)
- **Domain-aware research pipeline** with 7+ specialized stages
- **Multi-agent orchestration** (benchmark, figure, code-search)
- **Academic integration** (ArXiv, Semantic Scholar, Google Scholar)
- **Flexible sandboxing** (Docker, SSH, Colab)
- **Template-based paper generation** (NeurIPS, ICML, ICLR)
- **Extensible knowledge base** (Markdown + Obsidian)
- **Continuous improvement** via lesson storage & evolution

**Architecture is production-grade:** proper error handling, logging, validation gates, audit trails.

**Rust rewrite complexity: MODERATE-HIGH** (~1-2 person-years for full production port, depending on feature parity)
