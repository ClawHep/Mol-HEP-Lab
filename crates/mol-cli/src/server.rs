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
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::Response,
    routing::get,
    Router,
};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, services::ServeDir};

/// Configuration passed from `mol serve`.
#[allow(dead_code)]
pub struct ServerConfig {
    pub port: u16,
    pub resource_port: u16,
    pub bridge_port: u16,
    pub frontend_dir: PathBuf,
    pub agent_dir: PathBuf,
    pub runs_dir: PathBuf,
}

/// Shared application state threaded through axum handlers.
#[derive(Clone)]
struct AppState {
    /// Broadcast channel for resource stats JSON blobs.
    resource_tx: broadcast::Sender<String>,
    /// Broadcast channel for agent bridge messages.
    bridge_tx: broadcast::Sender<String>,
    /// Root directory for downloadable artifacts.
    runs_dir: Arc<PathBuf>,
}

// ── Resource Monitor WebSocket ──────────────────────────────────────────────

async fn ws_resources(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_resource_socket(socket, state.resource_tx.subscribe()))
}

async fn handle_resource_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<String>,
) {
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
}

// ── Agent Bridge WebSocket ──────────────────────────────────────────────────

async fn ws_agents(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_agent_socket(socket, state.bridge_tx.subscribe()))
}

async fn handle_agent_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<String>,
) {
    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(_text))) => {
                        // Commands from the browser are handled here.
                        // Full implementation delegates to mol-agents.
                        tracing::debug!("agent bridge received client command");
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

// ── Download Handler ────────────────────────────────────────────────────────

async fn download_artifact(
    Path(rel_path): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let full_path = state.runs_dir.join(&rel_path);

    // Prevent path traversal
    let canonical = full_path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let runs_canonical = state
        .runs_dir
        .canonicalize()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

    let response = axum::response::Response::builder()
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

// ── Resource stats ticker (stub) ────────────────────────────────────────────

async fn resource_ticker(tx: broadcast::Sender<String>) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        let stats = serde_json::json!({
            "type": "resource_stats",
            "payload": {
                "cpuPercent": 0.0,
                "memUsed": 0,
                "memTotal": 0,
                "gpus": [],
                "timestamp": chrono::Utc::now().timestamp_millis()
            }
        });
        let _ = tx.send(stats.to_string());
    }
}

// ── Router assembly ─────────────────────────────────────────────────────────

fn build_router(state: AppState, frontend_dir: PathBuf) -> Router {
    let serve_dir = ServeDir::new(&frontend_dir).append_index_html_on_directories(true);

    Router::new()
        .route("/ws/resources", get(ws_resources))
        .route("/ws/agents", get(ws_agents))
        .route("/download/{*path}", get(download_artifact))
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub async fn run(cfg: ServerConfig) -> Result<()> {
    let (resource_tx, _) = broadcast::channel::<String>(256);
    let (bridge_tx, _) = broadcast::channel::<String>(256);

    let state = AppState {
        resource_tx: resource_tx.clone(),
        bridge_tx: bridge_tx.clone(),
        runs_dir: Arc::new(cfg.runs_dir),
    };

    // Spawn the resource stats ticker
    tokio::spawn(resource_ticker(resource_tx));

    let app = build_router(state, cfg.frontend_dir);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("Listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
