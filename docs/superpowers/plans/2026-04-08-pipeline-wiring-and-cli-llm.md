# Pipeline Wiring + CLI LLM Backend + Review Fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `mol run` to the pipeline runner, add CLI-based LLM backend (Claude Code/Codex/OpenCode as pipeline brain), and fix all CRITICAL/HIGH/MEDIUM issues from code review.

**Architecture:** Three streams executed sequentially: (1) Fix security/parity review issues, (2) Add LLM provider abstraction that supports both API and CLI modes, (3) Wire `mol run` → pipeline runner → stages → LLM. The existing `ACPClient` in `mol-llm/src/acp.rs` already shells out to CLI tools — we extend it as the default fallback when no API key is configured.

**Tech Stack:** Rust, axum, tokio, serde_yaml, mol-llm (LLMClient + ACPClient), mol-pipeline (runner + executor + stages_impl)

---

## Stream 1: Fix Review Issues (CRITICAL + HIGH + MEDIUM)

### Task 1: Fix CRITICAL — HTTP header injection in Content-Disposition

**Files:**
- Modify: `crates/mol-cli/src/server.rs:65`

- [ ] **Step 1: Sanitize filename in download handler**

In `server.rs`, replace the unsafe interpolation:
```rust
// BEFORE (line 65):
format!("attachment; filename=\"{filename}\"")

// AFTER:
let safe_name = filename
    .replace('"', "_")
    .replace('\r', "")
    .replace('\n', "");
format!("attachment; filename=\"{safe_name}\"")
```

- [ ] **Step 2: Build and verify**

Run: `cargo build --bin mol`
Expected: exit 0

- [ ] **Step 3: Commit**

```bash
git add crates/mol-cli/src/server.rs
git commit -m "fix: sanitize Content-Disposition filename to prevent header injection"
```

---

### Task 2: Fix HIGH — Bind to localhost, restrict CORS

**Files:**
- Modify: `crates/mol-cli/src/server.rs:108-113`
- Modify: `crates/mol-services/src/lib.rs:32-35`

- [ ] **Step 1: Bind to 127.0.0.1 by default, add --public flag**

In `server.rs`, change the bind address:
```rust
// BEFORE:
let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));

// AFTER:
let bind_addr = if cfg.public { [0, 0, 0, 0] } else { [127, 0, 0, 1] };
let addr = SocketAddr::from((bind_addr, cfg.port));
```

Add `public: bool` to `ServerConfig`. Add `--public` flag to `ServeArgs` in `serve.rs`.

- [ ] **Step 2: Restrict CORS to localhost origins**

In `lib.rs`:
```rust
// BEFORE:
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);

// AFTER:
use tower_http::cors::AllowOrigin;
let cors = CorsLayer::new()
    .allow_origin(AllowOrigin::predicate(|origin, _| {
        origin.as_bytes().starts_with(b"http://localhost")
            || origin.as_bytes().starts_with(b"http://127.0.0.1")
    }))
    .allow_methods(Any)
    .allow_headers(Any);
```

- [ ] **Step 3: Build and verify**

Run: `cargo build --bin mol`

- [ ] **Step 4: Commit**

---

### Task 3: Fix HIGH — Lock poisoning resilience

**Files:**
- Modify: `crates/mol-services/src/agent_bridge.rs` (17 `.unwrap()` callsites on locks)

- [ ] **Step 1: Replace all `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` with poison recovery**

Global search-replace in `agent_bridge.rs`:
```rust
// Pattern: .lock().unwrap()  →  .lock().unwrap_or_else(|e| e.into_inner())
// Pattern: .read().unwrap()  →  .read().unwrap_or_else(|e| e.into_inner())
// Pattern: .write().unwrap() →  .write().unwrap_or_else(|e| e.into_inner())
```

- [ ] **Step 2: Build and test**

Run: `cargo test -p mol-services`

- [ ] **Step 3: Commit**

---

### Task 4: Fix HIGH — Algorithm parity (detect_domain + keyword extraction)

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:422` (detect_domain)
- Modify: `crates/mol-literature/src/novelty.rs:207` (extract_keywords)
- Modify: `crates/mol-literature/src/novelty.rs:376` (count_hypotheses)

- [ ] **Step 1: Add `domains` parameter to `detect_domain`**

```rust
// BEFORE:
pub fn detect_domain(topic: &str) -> (&'static str, &'static str) {

// AFTER:
pub fn detect_domain(topic: &str) -> (&'static str, &'static str) {
    detect_domain_with_hint(topic, &[])
}

pub fn detect_domain_with_hint(topic: &str, domains: &[&str]) -> (&'static str, &'static str) {
    // Check explicit domains first (matching Python behavior)
    if !domains.is_empty() {
        for d in domains {
            let dl = d.to_lowercase();
            // Match against domain_id, display_name, or first 3 keywords
            for &(domain_id, display, ref kws, ..) in &DOMAIN_KEYWORDS {
                if dl == domain_id || dl == display.to_lowercase()
                    || kws.iter().take(3).any(|kw| dl.contains(kw))
                {
                    return (domain_id, display);
                }
            }
        }
    }
    // Fall through to auto-detection from topic text
    // ... existing logic ...
```

- [ ] **Step 2: Fix keyword extraction to match Python regex**

In `novelty.rs`, update `extract_keywords`:
```rust
// Add check: first char must be ASCII alphabetic (matching Python [a-zA-Z])
.filter(|t| t.len() >= 2
    && t.chars().next().map_or(false, |c| c.is_ascii_alphabetic()))
```

- [ ] **Step 3: Fix count_hypotheses regex**

```rust
// BEFORE:
line.starts_with("## H")

// AFTER:
line.starts_with("## H")
    && line.as_bytes().get(4).map_or(false, |b| b.is_ascii_digit())
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mol-pipeline -p mol-literature`

- [ ] **Step 5: Commit**

---

### Task 5: Fix MEDIUM — Seminal papers dedup, data.rs LazyLock, poll_loop safety

**Files:**
- Modify: `crates/mol-common/src/data.rs` (dedup + LazyLock)
- Modify: `crates/mol-cli/src/server.rs` (poll_loop handle)
- Modify: `crates/mol-services/src/agent_bridge.rs` (referencePapers parsing)

- [ ] **Step 1: Add deduplication to load_seminal_papers**

```rust
pub fn load_seminal_papers(topic: &str) -> Vec<SeminalPaper> {
    let lower = topic.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    load_seminal_papers_all()
        .into_iter()
        .filter(|p| p.keywords.iter().any(|kw| lower.contains(&kw.to_lowercase())))
        .filter(|p| seen.insert(p.cite_key.clone()))
        .collect()
}
```

- [ ] **Step 2: Use LazyLock for parsed YAML data**

```rust
use std::sync::LazyLock;

static SEMINAL_PAPERS: LazyLock<Vec<SeminalPaper>> = LazyLock::new(|| {
    let file: SeminalPapersFile =
        serde_yaml::from_str(SEMINAL_PAPERS_YAML).expect("embedded seminal_papers.yaml");
    file.papers
});
```
Apply same pattern to DOCKER_PROFILES and DATASET_REGISTRY.

- [ ] **Step 3: Handle poll_loop JoinHandle in server.rs**

```rust
let poll_handle = tokio::spawn(mol_services::agent_bridge::poll_loop(poll_state, 2.0));

// Later in the server run:
tokio::select! {
    result = axum::serve(listener, app) => result?,
    result = poll_handle => {
        if let Err(e) = result {
            tracing::error!("poll_loop crashed: {e}");
        }
    }
}
```

- [ ] **Step 4: Fix referencePapers parsing (accept string or array)**

In `agent_bridge.rs` quick_submit handler, add string-splitting fallback:
```rust
let reference_papers: Vec<String> = match data.get("referencePapers") {
    Some(v) if v.is_array() => v.as_array().unwrap()
        .iter().filter_map(|x| x.as_str().map(String::from)).collect(),
    Some(v) if v.is_string() => v.as_str().unwrap()
        .split(',').map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()).collect(),
    _ => vec![],
};
```

- [ ] **Step 5: Build and test all**

Run: `cargo test --workspace`

- [ ] **Step 6: Commit**

---

## Stream 2: LLM Provider Abstraction

### Task 6: Create LLM provider trait and CLI backend

**Files:**
- Create: `crates/mol-llm/src/provider.rs`
- Modify: `crates/mol-llm/src/lib.rs`

- [ ] **Step 1: Define the LlmProvider trait**

Create `crates/mol-llm/src/provider.rs`:
```rust
use crate::client::Message;
use crate::response::LLMResponse;
use anyhow::Result;
use async_trait::async_trait;

/// Unified LLM provider trait — abstracts API-based and CLI-based backends.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send chat messages and get a response.
    async fn chat(
        &self,
        messages: &[Message],
        json_mode: bool,
    ) -> Result<LLMResponse>;

    /// Check if the provider is available and configured.
    async fn preflight(&self) -> Result<String>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}
```

- [ ] **Step 2: Implement LlmProvider for LLMClient (API mode)**

In `provider.rs`:
```rust
use crate::client::LLMClient;

#[async_trait]
impl LlmProvider for LLMClient {
    async fn chat(&self, messages: &[Message], json_mode: bool) -> Result<LLMResponse> {
        self.chat(messages, json_mode, None).await
    }
    async fn preflight(&self) -> Result<String> {
        let result = self.preflight().await;
        match result.ok {
            true => Ok(format!("API OK ({})", result.model)),
            false => anyhow::bail!("API preflight failed: {}", result.error),
        }
    }
    fn name(&self) -> &str { "api" }
}
```

- [ ] **Step 3: Implement LlmProvider for ACPClient (CLI mode)**

```rust
use crate::acp::ACPClient;
use tokio::sync::Mutex;

/// Wrapper to make ACPClient work with the trait (needs &mut self → Mutex).
pub struct CliProvider {
    inner: Mutex<ACPClient>,
}

#[async_trait]
impl LlmProvider for CliProvider {
    async fn chat(&self, messages: &[Message], _json_mode: bool) -> Result<LLMResponse> {
        let mut client = self.inner.lock().await;
        client.invoke(messages, None).await
    }
    async fn preflight(&self) -> Result<String> {
        let mut client = self.inner.lock().await;
        let result = client.preflight().await;
        match result.ok {
            true => Ok(format!("CLI OK ({})", result.agent)),
            false => anyhow::bail!("CLI preflight failed: {}", result.error),
        }
    }
    fn name(&self) -> &str { "cli" }
}
```

- [ ] **Step 4: Add factory function with fallback chain**

```rust
use mol_config::MolConfig;

/// Create the best available LLM provider based on config.
///
/// Priority:
/// 1. API key configured → LLMClient (OpenAI/Anthropic)
/// 2. provider = "acp" or CLI tool on PATH → CliProvider
/// 3. Neither → returns None (stages will use fallback templates)
pub fn create_provider(config: &MolConfig) -> Option<Arc<dyn LlmProvider>> {
    let llm = &config.llm;

    // 1. Try API mode
    let api_key = llm.api_key.clone().or_else(|| {
        llm.api_key_env.as_ref()
            .and_then(|env| std::env::var(env).ok())
            .filter(|k| !k.is_empty())
    });
    if let Some(key) = api_key {
        if !key.is_empty() {
            let client = LLMClient::new(LLMClientConfig {
                base_url: llm.base_url.clone(),
                api_key: key,
                primary_model: llm.primary_model.clone(),
                fallback_models: llm.fallback_models.clone(),
                ..Default::default()
            });
            return Some(Arc::new(client));
        }
    }

    // 2. Try CLI mode (ACP or auto-detect)
    if llm.provider == "acp" || which::which("claude").is_ok()
        || which::which("codex").is_ok() || which::which("opencode").is_ok()
    {
        let agent = if llm.provider == "acp" {
            llm.acp.agent.clone()
        } else if which::which("claude").is_ok() {
            "claude".to_string()
        } else if which::which("codex").is_ok() {
            "codex".to_string()
        } else {
            "opencode".to_string()
        };

        let acp_config = ACPConfig {
            agent,
            cwd: llm.acp.cwd.clone(),
            session_name: llm.acp.session_name.clone(),
            timeout_sec: llm.acp.timeout_sec,
            ..Default::default()
        };
        let client = ACPClient::new(acp_config);
        return Some(Arc::new(CliProvider {
            inner: Mutex::new(client),
        }));
    }

    // 3. No provider available
    None
}
```

- [ ] **Step 5: Export from lib.rs**

Add to `crates/mol-llm/src/lib.rs`:
```rust
pub mod provider;
pub use provider::{LlmProvider, CliProvider, create_provider};
```

- [ ] **Step 6: Build and test**

Run: `cargo build --workspace`

- [ ] **Step 7: Commit**

---

### Task 7: Add LlmProvider to StageContext

**Files:**
- Modify: `crates/mol-pipeline/src/executor.rs:98-117` (StageContext)

- [ ] **Step 1: Add optional provider to StageContext**

```rust
use std::sync::Arc;

pub struct StageContext {
    pub run_dir: PathBuf,
    pub run_id: String,
    pub config: MolConfig,
    pub prior_artifacts: HashMap<String, PathBuf>,
    pub auto_approve_gates: bool,
    /// LLM provider — None means stages use fallback templates.
    pub llm: Option<Arc<dyn mol_llm::LlmProvider>>,
}
```

- [ ] **Step 2: Update MolConfig in executor.rs**

The executor's local `MolConfig` struct needs to carry the real config's LLM settings. Add fields:
```rust
pub struct MolConfig {
    pub topic: String,
    pub settings: HashMap<String, String>,
    pub domains: Vec<String>,        // NEW: from config.research.domains
    pub quality_threshold: f64,       // NEW: from config.research.quality_threshold
    pub time_budget_sec: u64,         // NEW: from config.experiment.time_budget_sec
    pub metric_key: String,           // NEW: from config.experiment.metric_key
    pub metric_direction: String,     // NEW: from config.experiment.metric_direction
}
```

- [ ] **Step 3: Build**

Run: `cargo build --workspace`

- [ ] **Step 4: Commit**

---

## Stream 3: Wire Pipeline

### Task 8: Wire `mol run` to `execute_pipeline`

**Files:**
- Modify: `crates/mol-cli/src/commands/run.rs:85-138`

- [ ] **Step 1: Replace stub with real pipeline execution**

```rust
pub async fn execute(args: RunArgs) -> Result<()> {
    let config_path = resolve_config(args.config.as_ref())?;
    tracing::info!("Using config: {}", config_path.display());

    // Load full config
    let config_text = std::fs::read_to_string(&config_path)?;
    let config: mol_config::MolConfig = serde_yaml::from_str(&config_text)?;

    let topic = args.topic.clone().unwrap_or_else(|| {
        config.research.topic.clone()
    });

    // Create LLM provider (API → CLI → None)
    let provider = mol_llm::create_provider(&config);
    match &provider {
        Some(p) => println!("LLM provider: {}", p.name()),
        None => println!("LLM provider: none (fallback templates)"),
    }

    // LLM preflight
    if !args.skip_preflight {
        if let Some(ref p) = provider {
            print!("Preflight check... ");
            match p.preflight().await {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    println!("WARN: {e}");
                    tracing::warn!("LLM preflight failed: {e}");
                }
            }
        }
    }

    let run_id = generate_run_id(&topic);
    let run_dir = args.output.clone()
        .unwrap_or_else(|| PathBuf::from(format!("artifacts/{run_id}")));
    std::fs::create_dir_all(&run_dir)?;

    println!("Mol-HEP-Lab v{} -- Starting pipeline", env!("CARGO_PKG_VERSION"));
    println!("  Run ID:  {run_id}");
    println!("  Topic:   {topic}");
    println!("  Output:  {}", run_dir.display());
    println!();

    // Build pipeline config
    let pipeline_config = mol_pipeline::runner::PipelineConfig {
        from_stage: args.from_stage.as_deref()
            .map(mol_pipeline::stages::Stage::from_name)
            .transpose()?,
        to_stage: args.to_stage.as_deref()
            .map(mol_pipeline::stages::Stage::from_name)
            .transpose()?,
        auto_approve: args.auto_approve,
        skip_noncritical: args.skip_noncritical,
        graceful_degradation: !args.no_graceful_degradation,
        ..Default::default()
    };

    // Execute pipeline
    let summary = mol_pipeline::runner::execute_pipeline(
        &config,
        &pipeline_config,
        &run_dir,
        &run_id,
    ).await?;

    // Print summary
    println!();
    println!("Pipeline complete:");
    println!("  Stages completed: {}", summary.stages_completed);
    println!("  Stages failed:    {}", summary.stages_failed);
    println!("  Stages skipped:   {}", summary.stages_skipped);
    println!("  Status:           {}", summary.final_status);
    println!("  Elapsed:          {:.1}s", summary.total_elapsed_secs);

    if summary.stages_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}
```

- [ ] **Step 2: Fix type mismatches between pipeline's MolConfig and config's MolConfig**

The pipeline runner (`runner.rs:144`) takes `&MolConfig` from `mol_config`. The executor (`executor.rs`) has its own minimal `MolConfig`. We need to bridge these — either:
- Make executor accept `mol_config::MolConfig` directly, OR
- Build the executor's `MolConfig` from the full config in the runner

Choose the simpler: update the runner to pass the full config through to the executor's `StageContext`, converting at the boundary.

- [ ] **Step 3: Build and test**

Run: `cargo build --bin mol && cargo test --workspace`

- [ ] **Step 4: Commit**

---

### Task 9: Wire stage implementations to call LLM

**Files:**
- Modify: `crates/mol-pipeline/src/stages_impl/phase_a.rs` (TopicInit, ProblemDecompose)
- Modify: `crates/mol-pipeline/src/stages_impl/phase_b.rs` (SearchStrategy, LiteratureCollect, LiteratureScreen, KnowledgeExtract)
- Modify: `crates/mol-pipeline/src/stages_impl/phase_c.rs` (Synthesis, HypothesisGen)
- Modify: All other phase files similarly

- [ ] **Step 1: Create helper function for LLM-or-fallback pattern**

In `executor.rs`, add:
```rust
/// Call LLM with prompt, falling back to template output if no provider.
pub async fn llm_generate(
    ctx: &StageContext,
    system_prompt: &str,
    user_prompt: &str,
    json_mode: bool,
) -> String {
    if let Some(ref llm) = ctx.llm {
        let messages = vec![
            mol_llm::Message::system(system_prompt),
            mol_llm::Message::user(user_prompt),
        ];
        match llm.chat(&messages, json_mode).await {
            Ok(resp) => return resp.content,
            Err(e) => {
                tracing::warn!("LLM call failed, using fallback: {e}");
            }
        }
    }
    String::new() // empty = caller uses fallback template
}
```

- [ ] **Step 2: Update TopicInit to use LLM**

In `phase_a.rs`:
```rust
pub async fn execute_topic_init(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    std::fs::create_dir_all(&stage_dir).ok();

    // Load prompt template from embedded data
    let system = "You are a rigorous research planner.";
    let user = format!(
        "Create a SMART research goal in markdown.\n\nTopic: {}\nDomains: {}\n...",
        ctx.config.topic,
        ctx.config.domains.join(", "),
    );

    let content = crate::executor::llm_generate(ctx, system, &user, false).await;

    let goal_text = if content.is_empty() {
        // Fallback template (existing behavior)
        format!("# Research Goal\n\n## Topic\n{}\n\n## SMART Goal\n...", ctx.config.topic)
    } else {
        content
    };

    std::fs::write(stage_dir.join("goal.md"), &goal_text).ok();
    // ... hardware_profile.json (no LLM needed) ...

    StageResult { stage, status: StageStatus::Done, artifacts: vec!["goal.md".into()], .. }
}
```

- [ ] **Step 3: Apply same pattern to all 26 stages**

Each stage follows the same pattern:
1. Build prompt from embedded `PROMPTS_DEFAULT_YAML` template + stage context
2. Call `llm_generate(ctx, system, user, json_mode)`
3. If non-empty → use LLM response
4. If empty → use existing fallback template

For coding stages (CodeGeneration, SanityCheck), add smart routing:
```rust
// Check complexity score and route to OpenCode if above threshold
if complexity_score >= threshold && opencode_available {
    // Use OpenCode bridge (beast mode)
} else {
    // Use standard LLM call
}
```

- [ ] **Step 4: Build and test**

Run: `cargo test --workspace`

- [ ] **Step 5: Commit**

---

### Task 10: Fix remaining stubs (doctor, get_shared_results, preflight)

**Files:**
- Modify: `crates/mol-cli/src/commands/doctor.rs` (add 7 missing checks)
- Modify: `crates/mol-services/src/agent_bridge.rs` (get_shared_results)

- [ ] **Step 1: Add missing doctor checks**

Add to `doctor.rs`:
```rust
// LLM connectivity check
fn check_llm_connectivity(config: &serde_yaml::Value) -> CheckResult { ... }

// API key validation
fn check_api_key(config: &serde_yaml::Value) -> CheckResult { ... }

// CLI tool detection (claude, codex, opencode)
fn check_cli_llm() -> CheckResult {
    if which::which("claude").is_ok() {
        return CheckResult { name: "cli_llm".into(), status: Pass,
            message: "Claude Code CLI available".into() };
    }
    // ... check codex, opencode ...
}

// Sandbox python check
fn check_sandbox_python(config: &serde_yaml::Value) -> CheckResult { ... }

// Matplotlib check
fn check_matplotlib() -> CheckResult { ... }

// Ascend NPU check
fn check_ascend_runtime() -> CheckResult { ... }

// Docker runtime (more detailed)
fn check_docker_runtime() -> CheckResult { ... }
```

- [ ] **Step 2: Fix get_shared_results**

In `agent_bridge.rs`, replace placeholder:
```rust
"get_shared_results" => {
    let results_dir = PathBuf::from(&state.runs_base_dir)
        .join("shared_results");
    let summary = if results_dir.exists() {
        // Scan for result summaries
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&results_dir) {
            for entry in rd.flatten() {
                if entry.path().extension().map_or(false, |e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        entries.push(content);
                    }
                }
            }
        }
        format!("{{\"count\":{},\"entries\":[{}]}}", entries.len(), entries.join(","))
    } else {
        "{}".to_string()
    };
    messages.push(msg_system(&summary));
}
```

- [ ] **Step 3: Build and test**

Run: `cargo test --workspace && cargo build --bin mol`

- [ ] **Step 4: Test doctor with real config**

Run: `./target/debug/mol doctor --config examples/config_template.yaml`
Expected: 10+ checks listed

- [ ] **Step 5: Commit**

---

### Task 11: Integration test — end-to-end pipeline run

**Files:**
- Test manually

- [ ] **Step 1: Test with fallback templates (no LLM)**

```bash
./target/debug/mol init --force --output /tmp/test_run.yaml
./target/debug/mol run --config /tmp/test_run.yaml \
    --topic "Attention mechanisms in protein folding" \
    --skip-preflight --auto-approve \
    --to-stage HYPOTHESIS_GEN
```
Expected: Pipeline runs stages 1-8, produces artifacts in `artifacts/mol-*`, exits 0.

- [ ] **Step 2: Test with CLI LLM (if claude/codex available)**

```bash
# Edit config to use ACP
sed -i '' 's/provider: "openai"/provider: "acp"/' /tmp/test_run.yaml
./target/debug/mol run --config /tmp/test_run.yaml \
    --topic "Attention mechanisms in protein folding" \
    --auto-approve --to-stage PROBLEM_DECOMPOSE
```
Expected: Pipeline calls CLI tool, produces AI-generated content.

- [ ] **Step 3: Full test suite**

Run: `cargo test --workspace`
Expected: All tests pass (537+).

- [ ] **Step 4: Commit — update HANDOFF.md**

Update `docs/superpowers/HANDOFF.md` with Phase 7.5 completion status.

---

## Summary

| Task | Stream | Scope |
|------|--------|-------|
| 1 | Review fixes | CRITICAL: header injection |
| 2 | Review fixes | HIGH: localhost bind + CORS |
| 3 | Review fixes | HIGH: lock poisoning |
| 4 | Review fixes | HIGH: algorithm parity |
| 5 | Review fixes | MEDIUM: dedup + LazyLock + poll_loop + referencePapers |
| 6 | LLM backend | Create LlmProvider trait + CLI backend + factory |
| 7 | LLM backend | Add provider to StageContext |
| 8 | Pipeline wiring | Wire `mol run` → `execute_pipeline` |
| 9 | Pipeline wiring | Wire all 26 stages to call LLM |
| 10 | Pipeline wiring | Fix doctor + get_shared_results |
| 11 | Integration | End-to-end test |
