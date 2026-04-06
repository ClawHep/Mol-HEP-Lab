//! `mol setup` — Check and install optional tools.
//!
//! Ports `cmd_setup` from `backend/agent/researchclaw/cli.py`.

use anyhow::Result;
use clap::Args;
use std::io::{self, IsTerminal, Write};
use std::process::Command;

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Install missing tools without prompting
    #[arg(long)]
    pub yes: bool,
}

fn is_installed(tool: &str) -> bool {
    which::which(tool).is_ok()
}

fn opencode_version() -> Option<String> {
    let out = Command::new("opencode").arg("--version").output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

fn install_opencode() -> bool {
    println!("  Installing opencode-ai (this may take a minute)...");
    let result = Command::new("npm")
        .args(["i", "-g", "opencode-ai@latest"])
        .status();
    match result {
        Ok(status) if status.success() => {
            println!("  OpenCode installed successfully!");
            true
        }
        Ok(status) => {
            eprintln!("  Installation failed (exit {:?})", status.code());
            false
        }
        Err(e) => {
            eprintln!("  Installation failed: {e}");
            false
        }
    }
}

fn prompt_install(tool: &str, non_interactive: bool) -> bool {
    if non_interactive || !io::stdin().is_terminal() {
        return false;
    }
    print!("  Install {tool} now? [Y/n]: ");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    if io::stdin().read_line(&mut buf).is_ok() {
        let ans = buf.trim().to_lowercase();
        return ans.is_empty() || ans == "y" || ans == "yes";
    }
    false
}

pub async fn execute(args: SetupArgs) -> Result<()> {
    println!("Mol-HEP-Lab — Environment Setup\n");

    // OpenCode
    if is_installed("opencode") {
        let ver = opencode_version().unwrap_or_else(|| "unknown".to_string());
        println!("  [OK] OpenCode is installed (version: {ver})");
    } else {
        println!("  [--] OpenCode not found (beast mode unavailable)");
        if !is_installed("npm") {
            println!("       Node.js/npm is required to install OpenCode.");
            println!("       Install Node.js: https://nodejs.org/");
            println!("       Then run: npm i -g opencode-ai@latest");
        } else if args.yes || prompt_install("opencode-ai", false) {
            if install_opencode() {
                println!("  [OK] OpenCode is now available");
            } else {
                println!("  You can retry later with: mol setup");
            }
        } else {
            println!("  Skipped. Run 'mol setup' to install later.");
        }
    }

    println!();

    // Docker
    if is_installed("docker") {
        println!("  [OK] Docker is available (sandbox execution enabled)");
    } else {
        println!("  [--] Docker not found (experiment sandbox unavailable)");
        println!("       Install: https://docs.docker.com/get-docker/");
    }

    // LaTeX
    if is_installed("pdflatex") {
        println!("  [OK] LaTeX is available (PDF paper compilation enabled)");
    } else {
        println!("  [--] LaTeX not found (paper will be exported as .tex only)");
        println!("       Install: sudo apt install texlive-full  (or equivalent)");
    }

    // Python
    if is_installed("python3") {
        println!("  [OK] Python 3 is available");
    } else {
        println!("  [--] python3 not found");
    }

    println!();
    println!("Run 'mol doctor' for a full environment health check.");

    Ok(())
}
