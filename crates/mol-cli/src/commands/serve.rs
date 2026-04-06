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

    /// WebSocket port for the resource monitor
    #[arg(long, env = "RESOURCE_MONITOR_PORT", default_value = "8905")]
    pub resource_port: u16,

    /// WebSocket port for the agent bridge
    #[arg(long, env = "AGENT_BRIDGE_PORT", default_value = "8906")]
    pub bridge_port: u16,

    /// Directory containing the built frontend assets
    #[arg(long, env = "FRONTEND_DIR", default_value = "frontend/dist")]
    pub frontend_dir: PathBuf,

    /// Agent directory (backend/agent)
    #[arg(long, default_value = "backend/agent")]
    pub agent_dir: PathBuf,

    /// Runs directory (backend/runs)
    #[arg(long, default_value = "backend/runs")]
    pub runs_dir: PathBuf,
}

pub async fn execute(args: ServeArgs) -> Result<()> {
    use crate::server;

    println!("Mol-HEP-Lab — Starting all services");
    println!();
    println!("  Frontend:        http://localhost:{}/", args.port);
    println!("  Resource WS:     ws://localhost:{}", args.resource_port);
    println!("  Agent Bridge WS: ws://localhost:{}", args.bridge_port);
    println!("  Frontend dir:    {}", args.frontend_dir.display());
    println!();

    let cfg = server::ServerConfig {
        port: args.port,
        resource_port: args.resource_port,
        bridge_port: args.bridge_port,
        frontend_dir: args.frontend_dir,
        agent_dir: args.agent_dir,
        runs_dir: args.runs_dir,
    };

    server::run(cfg).await
}
