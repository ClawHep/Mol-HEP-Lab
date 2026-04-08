//! `mol validate` — Validate a config file against the schema.

use anyhow::{bail, Result};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Config file (default: auto-detect config.mol.yaml or config.yaml)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Skip path existence checks
    #[arg(long)]
    pub no_check_paths: bool,
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

pub async fn execute(args: ValidateArgs) -> Result<()> {
    let config_path = resolve_config(args.config.as_ref())?;

    let text = std::fs::read_to_string(&config_path)?;
    let value: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Config validation FAILED:");
            eprintln!("  YAML parse error: {e}");
            std::process::exit(1);
        }
    };

    // Required top-level keys
    let required_keys = ["project", "research", "llm"];
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for key in &required_keys {
        if value.get(key).is_none() {
            errors.push(format!("missing required key: `{key}`"));
        }
    }

    // Warn on missing optional but common keys
    let optional_keys = ["experiment", "knowledge_base", "notifications"];
    for key in &optional_keys {
        if value.get(key).is_none() {
            warnings.push(format!("optional key not set: `{key}`"));
        }
    }

    // Path checks
    if !args.no_check_paths {
        if let Some(kb) = value.get("knowledge_base").and_then(|kb| kb.get("root")) {
            if let Some(root) = kb.as_str() {
                if !root.is_empty() && !PathBuf::from(root).exists() {
                    warnings.push(format!("knowledge_base.root does not exist: `{root}`"));
                }
            }
        }
    }

    if errors.is_empty() {
        println!("Config validation passed: {}", config_path.display());
        for w in &warnings {
            println!("  Warning: {w}");
        }
    } else {
        eprintln!("Config validation FAILED:");
        for e in &errors {
            eprintln!("  Error: {e}");
        }
        for w in &warnings {
            eprintln!("  Warning: {w}");
        }
        std::process::exit(1);
    }

    Ok(())
}
