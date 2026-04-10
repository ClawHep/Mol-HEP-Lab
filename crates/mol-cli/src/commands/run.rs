//! `mol run` — Execute the research pipeline.
//!
//! Ports `cmd_run` from `backend/agent/mol/cli.py`.

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

    /// Stop after this stage inclusive (e.g. SYNTHESIS_HYPOTHESES)
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

    /// Path to user-provided datasets directory.
    /// When set, experiment stages use ONLY data in this directory.
    #[arg(short = 'd', long = "data")]
    pub data: Option<PathBuf>,
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

    // Load full typed config
    let config_text = std::fs::read_to_string(&config_path)?;
    let full_config: mol_config::MolConfig = serde_yaml::from_str(&config_text)?;

    let topic = args.topic.clone().unwrap_or_else(|| {
        full_config.research.topic.clone()
    });

    // Create LLM provider (API → CLI → None)
    let provider = mol_llm::create_provider(&full_config);
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
    let run_dir = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("artifacts/{run_id}")));
    std::fs::create_dir_all(&run_dir)?;

    println!("Mol-HEP-Lab v{} — Starting pipeline", env!("CARGO_PKG_VERSION"));
    println!("  Run ID:  {run_id}");
    println!("  Topic:   {topic}");
    println!("  Output:  {}", run_dir.display());

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

    // Build the executor's MolConfig from the full config
    let knowledge_chain = {
        let roots: Vec<std::path::PathBuf> =
            if let Some(ref chain) = full_config.research.knowledge_chain {
                chain.iter().map(std::path::PathBuf::from).collect()
            } else {
                vec![
                    std::path::PathBuf::from(&full_config.research.knowledge_root),
                    std::path::PathBuf::from("generic"),
                ]
            };
        mol_common::KnowledgeChain::new(roots)
    };

    let executor_config = mol_pipeline::executor::MolConfig {
        topic: topic.clone(),
        settings: {
            let mut s = std::collections::HashMap::new();
            let exp = &full_config.experiment;
            s.insert("sanity_check_max_iterations".into(), exp.sanity_check_max_iterations.to_string());
            s.insert("max_iterations".into(), exp.max_iterations.to_string());
            s.insert("time_budget_sec".into(), exp.time_budget_sec.to_string());
            if !exp.codebases_dir.is_empty() {
                s.insert("codebases_dir".into(), exp.codebases_dir.clone());
            }
            s
        },
        domain: {
            // Prefer knowledge_root as the domain identifier (e.g. "hep", "ml"),
            // because research.domains[] contains research categories ("deep-learning",
            // "physics") which don't map to our domain-specific defaults.
            let kr = &full_config.research.knowledge_root;
            if !kr.is_empty() {
                kr.clone()
            } else {
                full_config
                    .research
                    .domains
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "hep".to_owned())
            }
        },
        analysis_type: full_config.research.analysis_type.clone(),
        knowledge_chain,
        datasets_dir: args.data.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| full_config.experiment.datasets_dir.clone()),
    };

    // Build pipeline config
    let pipeline_config = mol_pipeline::runner::PipelineConfig {
        from_stage: args
            .from_stage
            .as_deref()
            .map(mol_pipeline::stages::Stage::from_name)
            .transpose()?
            .unwrap_or(mol_pipeline::stages::Stage::TopicInit),
        to_stage: args
            .to_stage
            .as_deref()
            .map(mol_pipeline::stages::Stage::from_name)
            .transpose()?,
        auto_approve: args.auto_approve,
        skip_noncritical: args.skip_noncritical,
        graceful_degradation: !args.no_graceful_degradation,
        ..Default::default()
    };

    // Execute pipeline
    let summary = mol_pipeline::runner::execute_pipeline_with_llm(
        &executor_config,
        &pipeline_config,
        &run_dir,
        &run_id,
        provider,
    )
    .await?;

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
