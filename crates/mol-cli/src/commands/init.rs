//! `mol init` — Create config.mol.yaml from the example template.
//!
//! Ports `cmd_init` from `backend/agent/mol/cli.py`.

use anyhow::Result;
use clap::Args;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Overwrite existing config.mol.yaml
    #[arg(long)]
    pub force: bool,

    /// Output config file path
    #[arg(short, long, default_value = "config.mol.yaml")]
    pub output: PathBuf,
}

struct ProviderConfig {
    provider: &'static str,
    api_key_env: &'static str,
    primary_model: &'static str,
    fallback_models: &'static [&'static str],
    base_url: &'static str,
}

const PROVIDERS: &[ProviderConfig] = &[
    ProviderConfig {
        provider: "openai",
        api_key_env: "OPENAI_API_KEY",
        primary_model: "gpt-4o",
        fallback_models: &["gpt-4.1", "gpt-4o-mini"],
        base_url: "https://api.openai.com/v1",
    },
    ProviderConfig {
        provider: "openrouter",
        api_key_env: "OPENROUTER_API_KEY",
        primary_model: "anthropic/claude-3.5-sonnet",
        fallback_models: &["google/gemini-pro-1.5", "meta-llama/llama-3.1-70b-instruct"],
        base_url: "https://openrouter.ai/api/v1",
    },
    ProviderConfig {
        provider: "deepseek",
        api_key_env: "DEEPSEEK_API_KEY",
        primary_model: "deepseek-chat",
        fallback_models: &["deepseek-reasoner"],
        base_url: "https://api.deepseek.com/v1",
    },
    ProviderConfig {
        provider: "acp",
        api_key_env: "",
        primary_model: "acp-default",
        fallback_models: &[],
        base_url: "",
    },
];

fn is_tty() -> bool {
    use std::io::IsTerminal;
    io::stdin().is_terminal()
}

fn prompt_provider() -> usize {
    println!("Select LLM provider:");
    println!("  1) openai       (requires OPENAI_API_KEY)");
    println!("  2) openrouter   (requires OPENROUTER_API_KEY)");
    println!("  3) deepseek     (requires DEEPSEEK_API_KEY)");
    println!("  4) acp          (local AI agent — no API key needed)");
    print!("Choice [1]: ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        match input.trim() {
            "2" => return 1,
            "3" => return 2,
            "4" => return 3,
            _ => {}
        }
    }
    0
}

fn generate_config(p: &ProviderConfig) -> String {
    let fallbacks = if p.fallback_models.is_empty() {
        "    []".to_string()
    } else {
        p.fallback_models
            .iter()
            .map(|m| format!("    - \"{m}\""))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let api_key_env = if p.api_key_env.is_empty() {
        "".to_string()
    } else {
        format!("  api_key_env: \"{}\"\n", p.api_key_env)
    };

    let base_url = if p.base_url.is_empty() {
        "".to_string()
    } else {
        format!("  base_url: \"{}\"\n", p.base_url)
    };

    format!(
        r#"project:
  name: "__PROJECT_ID__"
  mode: "full-auto"

research:
  topic: "__TOPIC__"
  domains:
    - "deep-learning"
  daily_paper_count: 5
  quality_threshold: 3.0
  reference_papers: []

notifications:
  channel: "console"
  on_stage_start: true
  on_gate_required: true

knowledge_base:
  backend: "markdown"
  root: "docs/kb"

llm:
  provider: "{provider}"
{base_url}{api_key_env}  primary_model: "{primary}"
  coding_model: "{primary}"
  fallback_models:
{fallbacks}

security:
  hitl_required_stages: []

experiment:
  mode: "sandbox"
  time_budget_sec: 2400
  max_iterations: 3
  metric_key: "primary_metric"
  metric_direction: "minimize"
  datasets_dir: ""
  checkpoints_dir: ""
  codebases_dir: ""
  shared_results_dir: "runs/shared_results"
  paper_length: "long"
  sandbox:
    python_path: "__PYTHON_PATH__"
  sanity_check_max_iterations: 100

runtime:
  timezone: "Asia/Shanghai"

prompts:
  custom_file: ""
"#,
        provider = p.provider,
        base_url = base_url,
        api_key_env = api_key_env,
        primary = p.primary_model,
        fallbacks = fallbacks,
    )
}

pub async fn execute(args: InitArgs) -> Result<()> {
    let dest = &args.output;

    if dest.exists() && !args.force {
        eprintln!(
            "{} already exists. Use --force to overwrite.",
            dest.display()
        );
        std::process::exit(1);
    }

    let provider_idx = if is_tty() {
        prompt_provider()
    } else {
        0 // default to openai in non-interactive mode
    };

    let provider = &PROVIDERS[provider_idx];
    let content = generate_config(provider);

    std::fs::write(dest, &content)?;
    println!("Created {} (provider: {})", dest.display(), provider.provider);

    if provider.provider == "acp" {
        println!("\nNext steps:");
        println!("  1. Ensure your ACP agent is installed and on PATH");
        println!("  2. Edit {} to set llm.acp.agent if needed", dest.display());
        println!("  3. Run: mol doctor");
    } else {
        let env_var = if provider.api_key_env.is_empty() {
            "OPENAI_API_KEY"
        } else {
            provider.api_key_env
        };
        println!("\nNext steps:");
        println!("  1. Export your API key: export {env_var}=sk-...");
        println!("  2. Edit {} to customize your settings", dest.display());
        println!("  3. Run: mol doctor");
    }

    Ok(())
}
