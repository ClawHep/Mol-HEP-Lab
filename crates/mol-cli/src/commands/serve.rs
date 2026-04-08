//! `mol serve` — Start all Mol-HEP-Lab services in one Tokio runtime.
//!
//! Replaces `start.sh` by launching the resource monitor, agent bridge,
//! and static frontend server from a single process.

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Main HTTP port for the frontend and API gateway
    #[arg(long, env = "FRONTEND_PORT", default_value = "5903")]
    pub port: u16,

    /// Directory containing the built frontend assets
    #[arg(long, env = "FRONTEND_DIR", default_value = "frontend/dist")]
    pub frontend_dir: PathBuf,

    /// Agent directory (legacy, unused in Rust-native mode)
    #[arg(long, default_value = ".")]
    pub agent_dir: PathBuf,

    /// Runs directory (backend/runs)
    #[arg(long, default_value = "backend/runs")]
    pub runs_dir: PathBuf,

    /// Bind to 0.0.0.0 instead of 127.0.0.1 (use for remote access)
    #[arg(long, default_value = "false")]
    pub public: bool,
}

pub async fn execute(args: ServeArgs) -> Result<()> {
    use crate::server;

    println!("Mol-HEP-Lab — Starting all services");
    println!();
    println!("  Frontend + API:  http://localhost:{}/", args.port);
    println!("  Resource WS:     ws://localhost:{}/ws/resources", args.port);
    println!("  Agent Bridge WS: ws://localhost:{}/ws/agents", args.port);
    println!("  Frontend dir:    {}", args.frontend_dir.display());
    println!();

    let cfg = server::ServerConfig {
        port: args.port,
        frontend_dir: args.frontend_dir,
        agent_dir: args.agent_dir,
        runs_dir: args.runs_dir,
        public: args.public,
    };

    server::run(cfg).await
}
