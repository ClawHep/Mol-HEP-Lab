//! GitHub API client — search repos and code, read files, analyse repositories.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Minimal repository metadata returned by the GitHub search API.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoInfo {
    /// `owner/repo` slug.
    pub full_name: String,
    /// Repository description.
    pub description: String,
    /// Star count.
    pub stars: u32,
    /// HTML URL.
    pub html_url: String,
    /// Default branch (usually `main` or `master`).
    pub default_branch: String,
    /// Primary language.
    pub language: Option<String>,
}

/// A code search result entry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeSnippet {
    /// `owner/repo` the snippet comes from.
    pub repo_full_name: String,
    /// Path within the repository.
    pub file_path: String,
    /// Raw file content (populated lazily).
    pub content: String,
    /// HTML URL to the file.
    pub html_url: String,
}

/// Analysis of a single repository.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoAnalysis {
    /// The repository this analysis belongs to.
    pub repo: RepoInfo,
    /// README content (truncated to 3000 chars).
    pub readme: String,
    /// All file paths in the repository tree.
    pub file_tree: Vec<String>,
    /// Key file contents (path → content).
    pub key_files: HashMap<String, String>,
    /// Parsed pip requirements.
    pub requirements: Vec<String>,
}

// ---------------------------------------------------------------------------
// GitHubClient
// ---------------------------------------------------------------------------

/// Lightweight GitHub API client.
///
/// Wraps the GitHub REST API v3.  Requires a personal access token for
/// authenticated requests (recommended to avoid rate limiting).
pub struct GitHubClient {
    client: reqwest::Client,
    token: Option<String>,
    /// Number of HTTP requests made (for monitoring / rate-limit budgeting).
    pub request_count: u32,
}

impl GitHubClient {
    /// Create a new client with an optional personal access token.
    pub fn new(token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            token,
            request_count: 0,
        }
    }

    // -- Internal helper ----------------------------------------------------

    fn request_builder(&self, url: &str) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .get(url)
            .header(USER_AGENT, "mol-agents/0.1")
            .header(ACCEPT, "application/vnd.github.v3+json");

        if let Some(ref tok) = self.token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {tok}"));
        }
        builder
    }

    async fn get_json(&mut self, url: &str) -> Result<Value> {
        debug!(url, "GitHub API GET");
        self.request_count += 1;
        let resp = self.request_builder(url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "GitHub API error {}: {}",
                resp.status(),
                url
            ));
        }
        Ok(resp.json().await?)
    }

    // -- Public API ---------------------------------------------------------

    /// Search GitHub repositories.
    ///
    /// Returns up to `max_results` repositories sorted by star count.
    pub async fn search_repos(
        &mut self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<RepoInfo>> {
        let url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&per_page={}",
            urlencoding(query),
            max_results.min(30),
        );
        let body = self.get_json(&url).await?;
        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let repos = items
            .iter()
            .filter_map(|item| {
                Some(RepoInfo {
                    full_name: item
                        .get("full_name")?
                        .as_str()?
                        .to_owned(),
                    description: item
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                    stars: item
                        .get("stargazers_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    html_url: item
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                    default_branch: item
                        .get("default_branch")
                        .and_then(|v| v.as_str())
                        .unwrap_or("main")
                        .to_owned(),
                    language: item
                        .get("language")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned),
                })
            })
            .collect();

        Ok(repos)
    }

    /// Search GitHub for code matching `query`.
    pub async fn search_code(
        &mut self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<CodeSnippet>> {
        let url = format!(
            "https://api.github.com/search/code?q={}&per_page={}",
            urlencoding(query),
            max_results.min(30),
        );
        let body = self.get_json(&url).await?;
        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let snippets = items
            .iter()
            .filter_map(|item| {
                let file_path = item.get("path")?.as_str()?.to_owned();
                let html_url = item
                    .get("html_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let repo_full_name = item
                    .get("repository")
                    .and_then(|r| r.get("full_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                Some(CodeSnippet {
                    repo_full_name,
                    file_path,
                    html_url,
                    content: String::new(),
                })
            })
            .collect();

        Ok(snippets)
    }

    /// Fetch the README for a repository (returns raw text).
    pub async fn get_readme(&mut self, repo: &str) -> Option<String> {
        let url = format!("https://api.github.com/repos/{repo}/readme");
        let body = self.get_json(&url).await.ok()?;
        let encoded = body.get("content")?.as_str()?;
        // GitHub returns base64-encoded content with newlines.
        let cleaned: String = encoded.chars().filter(|&c| c != '\n').collect();
        let decoded = base64_decode(&cleaned)?;
        String::from_utf8(decoded).ok()
    }

    /// Get the list of file paths in a repository tree.
    pub async fn get_repo_tree(
        &mut self,
        repo: &str,
        branch: &str,
    ) -> Vec<String> {
        let url = format!(
            "https://api.github.com/repos/{repo}/git/trees/{branch}?recursive=1"
        );
        let body = match self.get_json(&url).await {
            Ok(b) => b,
            Err(e) => {
                warn!("get_repo_tree failed for {repo}: {e}");
                return Vec::new();
            }
        };
        body.get("tree")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        if entry.get("type")?.as_str()? == "blob" {
                            entry.get("path")?.as_str().map(str::to_owned)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Fetch the raw content of a file from a repository.
    ///
    /// Returns `None` when the file is absent or exceeds `max_size_kb`.
    pub async fn get_file_content(
        &mut self,
        repo: &str,
        path: &str,
        max_size_kb: u64,
    ) -> Option<String> {
        let url = format!("https://api.github.com/repos/{repo}/contents/{path}");
        let body = self.get_json(&url).await.ok()?;

        let size = body.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        if size > max_size_kb * 1024 {
            return None;
        }

        let encoded = body.get("content")?.as_str()?;
        let cleaned: String = encoded.chars().filter(|&c| c != '\n').collect();
        let decoded = base64_decode(&cleaned)?;
        String::from_utf8(decoded).ok()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal base-64 decoder using stdlib only (no extra deps).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    // Alphabet: A-Z (0-25) a-z (26-51) 0-9 (52-61) + (62) / (63)
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0), // padding
            _ => None,
        }
    }

    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'\n' && b != b'\r').collect();
    if bytes.len() % 4 != 0 {
        return None;
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = val(chunk[2])?;
        let d = val(chunk[3])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push(((b & 0x0F) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((c & 0x03) << 6) | d);
        }
    }
    Some(out)
}

/// Percent-encode a string for use as a URL query parameter.
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => '+'.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}
