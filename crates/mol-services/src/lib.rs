//! # mol-services
//!
//! WebSocket service layer for Mol-HEP-Lab / MolAgent.
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`agent_bridge`] | Main agent orchestration WebSocket service |
//! | [`resource_monitor`] | GPU/CPU resource streaming WebSocket service |
//! | [`result_registry`] | Shared experiment result cache (file-backed) |
//! | [`discussion`] | Multi-round multi-agent discussion engine |

pub mod agent_bridge;
pub mod discussion;
pub mod resource_monitor;
pub mod result_registry;

// ---------------------------------------------------------------------------
// Combined server builder
// ---------------------------------------------------------------------------

use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Build a combined Axum router that mounts all service sub-routers.
///
/// - `GET /ws`       — Agent bridge WebSocket endpoint
/// - `GET /res`      — Resource monitor WebSocket endpoint
/// - `GET /download/{project_id}/{filename}` — Artifact file download
pub fn build_server(state: Arc<agent_bridge::BridgeState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let bridge_router = agent_bridge::build_router(state);
    let monitor_router = resource_monitor::build_router();

    Router::new()
        .merge(bridge_router)
        .merge(monitor_router)
        .layer(cors)
}
