//! Network connectivity pre-check for external services.
//!
//! Probes a set of well-known endpoints in parallel and returns a
//! [`ConnectivityReport`] that callers can use to decide which services to
//! enable before starting a long literature-collection run.
//!
//! # Usage
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! use mol_web::connectivity::{check_connectivity, DEFAULT_ENDPOINTS};
//!
//! let report = check_connectivity(DEFAULT_ENDPOINTS).await;
//! println!("{}", report.summary());
//! if report.is_up("tavily") {
//!     println!("Tavily is reachable");
//! }
//! # Ok(())
//! # }
//! ```

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tracing::info;

// ---------------------------------------------------------------------------
// Default endpoint list
// ---------------------------------------------------------------------------

/// Well-known endpoints used by the MolHEP research pipeline.
///
/// Each entry is `(name, url, timeout_sec)`.
pub const DEFAULT_ENDPOINTS: &[(&str, &str, u64)] = &[
    (
        "openalex",
        "https://api.openalex.org/works?filter=title.search:test&per_page=1",
        8,
    ),
    (
        "semantic_scholar",
        "https://api.semanticscholar.org/graph/v1/paper/search?query=test&limit=1",
        8,
    ),
    (
        "arxiv",
        "https://export.arxiv.org/api/query?search_query=test&max_results=1",
        8,
    ),
    ("duckduckgo", "https://html.duckduckgo.com/", 3),
    ("google_scholar", "https://scholar.google.com/", 3),
    ("tavily", "https://api.tavily.com/", 5),
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Per-endpoint connectivity status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointStatus {
    /// Endpoint name (e.g. `"openalex"`).
    pub name: String,
    /// Whether the endpoint returned a response (even a 4xx counts as up).
    pub reachable: bool,
    /// Round-trip latency in milliseconds (`None` if unreachable).
    pub latency_ms: Option<f64>,
    /// HTTP status code, if any response was received.
    pub http_status: Option<u16>,
    /// Error description if unreachable.
    pub error: Option<String>,
}

/// Results of a batch network connectivity check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityReport {
    /// Per-endpoint results, keyed by endpoint name.
    pub endpoints: HashMap<String, EndpointStatus>,
    /// Total time taken for the batch probe in seconds.
    pub elapsed_sec: f64,
}

impl ConnectivityReport {
    /// Returns `true` if the named endpoint is reachable.
    pub fn is_up(&self, name: &str) -> bool {
        self.endpoints
            .get(name)
            .map(|s| s.reachable)
            .unwrap_or(false)
    }

    /// Returns the latency for an endpoint in milliseconds, or `None`.
    pub fn latency_ms(&self, name: &str) -> Option<f64> {
        self.endpoints.get(name).and_then(|s| s.latency_ms)
    }

    /// Returns a compact one-line summary of all probed endpoints.
    ///
    /// Example: `"openalex: OK (143ms) | arxiv: UNREACHABLE | tavily: OK (89ms)"`
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self
            .endpoints
            .values()
            .map(|s| {
                if s.reachable {
                    let ms = s
                        .latency_ms
                        .map(|m| format!(" ({m:.0}ms)"))
                        .unwrap_or_default();
                    format!("{}: OK{ms}", s.name)
                } else {
                    format!("{}: UNREACHABLE", s.name)
                }
            })
            .collect();

        // Sort for stable output
        parts.sort();
        parts.join(" | ")
    }

    /// Returns names of all reachable endpoints.
    pub fn reachable_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .endpoints
            .values()
            .filter(|s| s.reachable)
            .map(|s| s.name.as_str())
            .collect();
        names.sort();
        names
    }

    /// Returns names of all unreachable endpoints.
    pub fn unreachable_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .endpoints
            .values()
            .filter(|s| !s.reachable)
            .map(|s| s.name.as_str())
            .collect();
        names.sort();
        names
    }
}

// ---------------------------------------------------------------------------
// Probe logic
// ---------------------------------------------------------------------------

/// Probe all provided `endpoints` concurrently and return a
/// [`ConnectivityReport`].
///
/// `endpoints` is a slice of `(name, url, timeout_sec)` tuples.  Pass
/// [`DEFAULT_ENDPOINTS`] for the standard MolHEP set.
///
/// HTTP 4xx responses are treated as "reachable" (the host is up even if the
/// specific path is forbidden or requires auth).
pub async fn check_connectivity(endpoints: &[(&str, &str, u64)]) -> ConnectivityReport {
    let t0 = Instant::now();

    let mut join_set = JoinSet::new();

    for &(name, url, timeout_sec) in endpoints {
        let name = name.to_owned();
        let url = url.to_owned();
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_sec))
            .user_agent("MolHEP/0.1 connectivity probe")
            .build()
            .expect("failed to build reqwest client");

        join_set.spawn(async move { probe_one(client, name, url).await });
    }

    let mut endpoint_map = HashMap::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(status) => {
                endpoint_map.insert(status.name.clone(), status);
            }
            Err(e) => {
                // JoinError (task panicked) — should not happen in practice
                tracing::warn!("Connectivity probe task panicked: {e}");
            }
        }
    }

    let elapsed_sec = t0.elapsed().as_secs_f64();
    let report = ConnectivityReport {
        endpoints: endpoint_map,
        elapsed_sec,
    };

    info!(
        "Connectivity probe: {} (total {:.1}s)",
        report.summary(),
        elapsed_sec
    );

    report
}

/// Probe a single endpoint.  HTTP 4xx responses count as "reachable".
async fn probe_one(client: Client, name: String, url: String) -> EndpointStatus {
    let t0 = Instant::now();

    match client.get(&url).send().await {
        Ok(resp) => {
            let latency_ms = t0.elapsed().as_secs_f64() * 1_000.0;
            let status_code = resp.status().as_u16();
            // 5xx → treat as unreachable (server error / host down)
            let reachable = status_code < 500;

            EndpointStatus {
                name,
                reachable,
                latency_ms: if reachable { Some(latency_ms) } else { None },
                http_status: Some(status_code),
                error: if reachable {
                    None
                } else {
                    Some(format!("HTTP {status_code}"))
                },
            }
        }
        Err(e) => EndpointStatus {
            name,
            reachable: false,
            latency_ms: None,
            http_status: None,
            error: Some(e.to_string()),
        },
    }
}
