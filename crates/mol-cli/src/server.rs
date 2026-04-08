//! Server builder — combines all Mol-HEP-Lab services into one axum Router.
//!
//! Architecture (all in one Tokio runtime):
//!
//! - `/ws/resources`  — Resource monitor WebSocket (CPU/GPU stats broadcaster)
//! - `/ws/agents`     — Agent bridge WebSocket (pipeline orchestration)
//! - `/download/*`    — Artifact download handler
//! - `/*`             — Static file server for the built frontend

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::get,
    Router,
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::services::ServeDir;

/// Configuration passed from `mol serve`.
pub struct ServerConfig {
    pub port: u16,
    pub frontend_dir: PathBuf,
    pub agent_dir: PathBuf,
    pub runs_dir: PathBuf,
    /// Bind to 0.0.0.0 instead of 127.0.0.1.
    pub public: bool,
}

// ── Download Handler ────────────────────────────────────────────────────────

#[derive(Clone)]
struct DownloadState {
    runs_dir: Arc<PathBuf>,
}

async fn download_artifact(
    Path(rel_path): Path<String>,
    State(state): State<DownloadState>,
) -> Result<Response, StatusCode> {
    let runs_canonical = state
        .runs_dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Try direct path first
    let full_path = state.runs_dir.join(&rel_path);
    let canonical = full_path.canonicalize().ok().filter(|c| c.starts_with(&runs_canonical));

    // If direct path fails, search in projects/{id}/run-*/stage-*/ for the filename
    let canonical = match canonical {
        Some(c) => c,
        None => {
            // rel_path is typically "{projectId}/{filename}" from the frontend
            let parts: Vec<&str> = rel_path.splitn(2, '/').collect();
            if parts.len() == 2 {
                let (project_id, filename) = (parts[0], parts[1]);
                find_artifact_in_project(&state.runs_dir, project_id, filename)
                    .and_then(|p| p.canonicalize().ok())
                    .filter(|c| c.starts_with(&runs_canonical))
                    .ok_or(StatusCode::NOT_FOUND)?
            } else {
                return Err(StatusCode::NOT_FOUND);
            }
        }
    };

    if !canonical.starts_with(&runs_canonical) {
        return Err(StatusCode::FORBIDDEN);
    }

    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let filename = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");

    // Sanitize filename to prevent HTTP header injection
    let safe_name = filename
        .replace('"', "_")
        .replace('\r', "")
        .replace('\n', "");

    let response = axum::response::Response::builder()
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{safe_name}\""),
        )
        .body(axum::body::Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

/// Search for a file in a project's run directories.
/// Looks in: projects/{id}/run-*/stage-*/{filename} (highest stage first)
/// and projects/{id}/run-*/{filename}
fn find_artifact_in_project(
    runs_dir: &std::path::Path,
    project_id: &str,
    filename: &str,
) -> Option<std::path::PathBuf> {
    let proj_dir = runs_dir.join("projects").join(project_id);
    if !proj_dir.exists() {
        return None;
    }
    // Scan run-* subdirs (newest first)
    let mut run_dirs: Vec<_> = std::fs::read_dir(&proj_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .collect();
    run_dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    for run_entry in &run_dirs {
        // Check run dir itself
        let direct = run_entry.path().join(filename);
        if direct.exists() {
            return Some(direct);
        }
        // Check stage-* dirs (highest number first)
        let mut stage_dirs: Vec<_> = std::fs::read_dir(run_entry.path())
            .ok()
            .map(|rd| rd.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("stage-"))
                .collect())
            .unwrap_or_default();
        stage_dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        for stage_entry in &stage_dirs {
            let p = stage_entry.path().join(filename);
            if p.exists() {
                return Some(p);
            }
        }
    }
    // Also check project dir directly
    let direct = proj_dir.join(filename);
    if direct.exists() {
        return Some(direct);
    }
    None
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub async fn run(cfg: ServerConfig) -> Result<()> {
    // Create the real BridgeState from mol-services
    let bridge_state = mol_services::agent_bridge::BridgeState::new(
        "python3".to_string(),
        cfg.agent_dir.to_string_lossy().into_owned(),
        cfg.runs_dir.to_string_lossy().into_owned(),
        8,  // total_gpus (default, same as start.sh)
        1,  // gpus_per_project
        false, // auto_loop
        false, // discussion_mode
        2,     // discussion_rounds
        vec!["claude-sonnet-4-6".to_string(), "qwen3.5-plus".to_string()],
    );

    // Create default agent pool so submitted tasks have workers immediately
    mol_services::agent_bridge::create_default_agents(&bridge_state);

    // Spawn the poll loop that drives agent state machines
    let poll_state = bridge_state.clone();
    let poll_handle = tokio::spawn(mol_services::agent_bridge::poll_loop(poll_state, 2.0));

    // Build the services router (handles /ws/agents and /ws/resources)
    let services_router = mol_services::build_server(bridge_state);

    // Download handler
    let download_state = DownloadState {
        runs_dir: Arc::new(cfg.runs_dir),
    };
    let download_router = Router::new()
        .route("/download/{*path}", get(download_artifact))
        .with_state(download_state);

    // Static frontend files
    let serve_dir = ServeDir::new(&cfg.frontend_dir).append_index_html_on_directories(true);

    // Combine all routers
    let app = Router::new()
        .merge(services_router)
        .merge(download_router)
        .fallback_service(serve_dir);

    let bind_addr = if cfg.public { [0, 0, 0, 0] } else { [127, 0, 0, 1] };
    let addr = SocketAddr::from((bind_addr, cfg.port));
    tracing::info!("Listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tokio::select! {
        result = axum::serve(listener, app) => result?,
        result = poll_handle => {
            if let Err(e) = result {
                tracing::error!("poll_loop crashed: {e}");
            }
        }
    }

    Ok(())
}
