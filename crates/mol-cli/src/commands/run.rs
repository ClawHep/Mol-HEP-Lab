//! `mol run` — Execute the research pipeline.
//!
//! Ports `cmd_run` from `backend/agent/researchclaw/cli.py`.

use anyhow::{bail, Result};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Override research topic
    #[arg(short, long)]
    pub topic: Option<String>,

    /// Config file (default: auto-detect config.mol.yaml or config.yaml)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Output directory (default: artifacts/mol-<timestamp>-<hash>)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Start from a specific stage (e.g. TOPIC_INIT)
    #[arg(long)]
    pub from_stage: Option<String>,

    /// Stop after this stage inclusive (e.g. HYPOTHESIS_GEN)
    #[arg(long)]
    pub to_stage: Option<String>,

    /// Auto-approve all gate stages without prompting
    #[arg(long)]
    pub auto_approve: bool,

    /// Skip LLM preflight connectivity check
    #[arg(long)]
    pub skip_preflight: bool,

    /// Resume from last checkpoint in the output directory
    #[arg(long)]
    pub resume: bool,

    /// Skip non-critical stages on failure instead of aborting
    #[arg(long)]
    pub skip_noncritical: bool,

    /// Disable graceful degradation: abort pipeline on quality-gate failure
    #[arg(long)]
    pub no_graceful_degradation: bool,
}

/// Generate a run ID with the format `mol-{timestamp}-{hash}`.
fn generate_run_id(topic: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut h = DefaultHasher::new();
    topic.hash(&mut h);
    let hash = format!("{:016x}", h.finish());
    format!("mol-{ts}-{}", &hash[..6])
}

/// Resolve the config file path, searching well-known locations.
fn resolve_config(explicit: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.clone());
        }
        bail!("config file not found: {}", p.display());
    }
    let candidates = ["config.mol.yaml", "config.yaml", "config.arc.yaml"];
    for name in &candidates {
        let p = PathBuf::from(name);
        if p.exists() {
            return Ok(p);
        }
    }
    bail!(
        "no config file found (searched: {}).\nRun 'mol init' to create one from the template.",
        candidates.join(", ")
    )
}

pub async fn execute(args: RunArgs) -> Result<()> {
    let config_path = resolve_config(args.config.as_ref())?;
    tracing::info!("Using config: {}", config_path.display());

    // Load config YAML for topic / mode
    let config_text = std::fs::read_to_string(&config_path)?;
    let config: serde_yaml::Value = serde_yaml::from_str(&config_text)?;

    let topic = args.topic.clone().unwrap_or_else(|| {
        config["research"]["topic"]
            .as_str()
            .unwrap_or("unknown")
            .to_string()
    });

    let mode = config["project"]["mode"].as_str().unwrap_or("full-auto").to_string();

    // LLM preflight
    if !args.skip_preflight {
        print!("Preflight check... ");
        // Preflight stub — real implementation delegates to mol-llm
        println!("OK (stub)");
    }

    let run_id = generate_run_id(&topic);
    let run_dir = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("artifacts/{run_id}")));
    std::fs::create_dir_all(&run_dir)?;

    println!("Mol-HEP-Lab v{} — Starting pipeline", env!("CARGO_PKG_VERSION"));
    println!("  Run ID:  {run_id}");
    println!("  Topic:   {topic}");
    println!("  Output:  {}", run_dir.display());
    println!("  Mode:    {mode}");

    if let Some(ref from) = args.from_stage {
        println!("  From:    {from}");
    }
    if let Some(ref to) = args.to_stage {
        println!("  To:      {to}");
    }
    if args.resume {
        println!("  Resume:  enabled");
    }
    println!();

    // Pipeline execution stub — delegates to mol-pipeline at runtime
    tracing::info!(run_id, topic, mode, "pipeline ready — executor not yet wired");
    println!("Pipeline stub: run_id={run_id}  (connect mol-pipeline for full execution)");

    Ok(())
}
