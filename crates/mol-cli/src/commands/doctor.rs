//! `mol doctor` — Check environment and configuration health.
//!
//! Ports `cmd_doctor` from `backend/agent/mol/cli.py`.

use anyhow::{bail, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Config file (default: auto-detect config.mol.yaml or config.yaml)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Write JSON report to file
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DoctorReport {
    pub overall: CheckStatus,
    pub checks: Vec<CheckResult>,
}

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

fn check_tool(name: &str) -> CheckResult {
    if which::which(name).is_ok() {
        CheckResult {
            name: name.to_string(),
            status: CheckStatus::Pass,
            message: format!("`{name}` found on PATH"),
        }
    } else {
        CheckResult {
            name: name.to_string(),
            status: CheckStatus::Warn,
            message: format!("`{name}` not found on PATH"),
        }
    }
}

fn check_config(config_path: &PathBuf) -> Vec<CheckResult> {
    let mut results = Vec::new();

    let text = match std::fs::read_to_string(config_path) {
        Ok(t) => t,
        Err(e) => {
            results.push(CheckResult {
                name: "config_readable".to_string(),
                status: CheckStatus::Fail,
                message: format!("cannot read config: {e}"),
            });
            return results;
        }
    };

    results.push(CheckResult {
        name: "config_readable".to_string(),
        status: CheckStatus::Pass,
        message: format!("config file readable: {}", config_path.display()),
    });

    let value: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            results.push(CheckResult {
                name: "config_yaml".to_string(),
                status: CheckStatus::Fail,
                message: format!("YAML parse error: {e}"),
            });
            return results;
        }
    };

    results.push(CheckResult {
        name: "config_yaml".to_string(),
        status: CheckStatus::Pass,
        message: "YAML parses successfully".to_string(),
    });

    // Check LLM provider configured
    let provider = value["llm"]["provider"].as_str().unwrap_or("");
    if provider.is_empty() {
        results.push(CheckResult {
            name: "llm_provider".to_string(),
            status: CheckStatus::Fail,
            message: "llm.provider is not set".to_string(),
        });
    } else {
        results.push(CheckResult {
            name: "llm_provider".to_string(),
            status: CheckStatus::Pass,
            message: format!("llm.provider = \"{provider}\""),
        });
    }

    // Check API key env var
    let api_key_env = value["llm"]["api_key_env"].as_str().unwrap_or("");
    if !api_key_env.is_empty() {
        let var_set = std::env::var(api_key_env).map(|v| !v.is_empty()).unwrap_or(false);
        if var_set {
            results.push(CheckResult {
                name: "api_key_env".to_string(),
                status: CheckStatus::Pass,
                message: format!("env var `{api_key_env}` is set"),
            });
        } else {
            results.push(CheckResult {
                name: "api_key_env".to_string(),
                status: CheckStatus::Warn,
                message: format!("env var `{api_key_env}` is not set"),
            });
        }
    }

    results
}

pub async fn execute(args: DoctorArgs) -> Result<()> {
    let config_path = resolve_config(args.config.as_ref())?;

    let mut checks: Vec<CheckResult> = Vec::new();

    // Config checks
    checks.extend(check_config(&config_path));

    // Tool checks
    checks.push(check_tool("python3"));
    checks.push(check_tool("docker"));
    checks.push(check_tool("pdflatex"));
    checks.push(check_tool("opencode"));
    checks.push(check_tool("npm"));

    // Determine overall status
    let overall = if checks.iter().any(|c| matches!(c.status, CheckStatus::Fail)) {
        CheckStatus::Fail
    } else if checks.iter().any(|c| matches!(c.status, CheckStatus::Warn)) {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };

    // Print report
    println!("Mol-HEP-Lab — Environment Health Report");
    println!();
    for c in &checks {
        let icon = match c.status {
            CheckStatus::Pass => "[OK]",
            CheckStatus::Warn => "[--]",
            CheckStatus::Fail => "[!!]",
        };
        println!("  {icon}  {}: {}", c.name, c.message);
    }
    println!();
    let overall_str = match overall {
        CheckStatus::Pass => "PASS",
        CheckStatus::Warn => "WARN",
        CheckStatus::Fail => "FAIL",
    };
    println!("Overall: {overall_str}");

    // Optionally write JSON report
    if let Some(output_path) = args.output {
        let report = DoctorReport {
            overall,
            checks,
        };
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(&output_path, json)?;
        println!("Report written to {}", output_path.display());
    }

    Ok(())
}
