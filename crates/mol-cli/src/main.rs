//! mol — Mol-HEP-Lab: Autonomous Multi-Agent Research Platform
//!
//! Entry point for the `mol` binary crate.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod server;

#[derive(Parser)]
#[command(
    name = "mol",
    about = "Mol-HEP-Lab: Autonomous Multi-Agent Research Platform",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the research pipeline
    Run(commands::run::RunArgs),
    /// Validate config file
    Validate(commands::validate::ValidateArgs),
    /// Check environment and configuration health
    Doctor(commands::doctor::DoctorArgs),
    /// Create config.mol.yaml from template
    Init(commands::init::InitArgs),
    /// Check and install optional tools (OpenCode, etc.)
    Setup(commands::setup::SetupArgs),
    /// Generate a human-readable run report
    Report(commands::report::ReportArgs),
    /// Start all services (resource monitor, agent bridge, frontend)
    Serve(commands::serve::ServeArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => commands::run::execute(args).await,
        Commands::Validate(args) => commands::validate::execute(args).await,
        Commands::Doctor(args) => commands::doctor::execute(args).await,
        Commands::Init(args) => commands::init::execute(args).await,
        Commands::Setup(args) => commands::setup::execute(args).await,
        Commands::Report(args) => commands::report::execute(args).await,
        Commands::Serve(args) => commands::serve::execute(args).await,
    }
}
