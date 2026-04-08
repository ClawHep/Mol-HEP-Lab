//! Code search agent sub-system.
//!
//! Orchestrates GitHub search, repository analysis, pattern extraction, and
//! result caching so that code generation stages have real reference material.

pub mod cache;
pub mod github_client;
pub mod pattern_extractor;
pub mod query_gen;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use cache::SearchCache;
use github_client::{CodeSnippet, GitHubClient, RepoAnalysis, RepoInfo};
use pattern_extractor::{CodePatterns, extract_patterns};
use query_gen::generate_search_queries;

// ---------------------------------------------------------------------------
// CodeSearchResult
// ---------------------------------------------------------------------------

/// Complete result from a code search operation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeSearchResult {
    /// Extracted code patterns ready for prompt injection.
    pub patterns: CodePatterns,
    /// Top-ranked repositories discovered.
    pub repos_found: Vec<RepoInfo>,
    /// Raw code search snippets.
    pub snippets_found: Vec<CodeSnippet>,
    /// Detailed analyses of the top repositories.
    pub repo_analyses: Vec<RepoAnalysis>,
    /// GitHub search queries that were executed.
    pub queries_used: Vec<String>,
    /// `true` when these results came from the local cache.
    pub from_cache: bool,
    /// Total GitHub API requests made.
    pub github_requests: u32,
}

impl CodeSearchResult {
    /// Format as a compact context block for injection into code generation prompts.
    pub fn to_prompt_context(&self) -> String {
        if !self.patterns.has_content() {
            return String::new();
        }
        self.patterns.to_prompt_context()
    }

    /// Serialise for caching.
    pub fn to_cache_value(&self) -> Value {
        serde_json::json!({
            "api_patterns": self.patterns.api_patterns,
            "file_structure": self.patterns.file_structure,
            "evaluation_patterns": self.patterns.evaluation_patterns,
            "library_versions": self.patterns.library_versions,
            "repos": self.repos_found.iter().take(5).map(|r| serde_json::json!({
                "full_name": r.full_name,
                "description": r.description,
                "stars": r.stars,
                "html_url": r.html_url,
            })).collect::<Vec<_>>(),
            "queries": self.queries_used,
        })
    }

    /// Deserialise from a cached value.
    pub fn from_cache_value(data: Value) -> Self {
        let patterns = CodePatterns {
            api_patterns: data
                .get("api_patterns")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            file_structure: data
                .get("file_structure")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            evaluation_patterns: data
                .get("evaluation_patterns")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            library_versions: data
                .get("library_versions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        };
        let repos: Vec<RepoInfo> = data
            .get("repos")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        Some(RepoInfo {
                            full_name: r.get("full_name")?.as_str()?.to_owned(),
                            description: r
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_owned(),
                            stars: r
                                .get("stars")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32,
                            html_url: r
                                .get("html_url")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_owned(),
                            ..Default::default()
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let queries: Vec<String> = data
            .get("queries")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        Self {
            patterns,
            repos_found: repos,
            queries_used: queries,
            from_cache: true,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// CodeSearchAgent
// ---------------------------------------------------------------------------

/// Orchestrates code search for reference material before code generation.
///
/// Flow:
/// 1. Check cache
/// 2. Generate search queries
/// 3. Search GitHub repos + code
/// 4. Read key files from top repos
/// 5. Extract patterns
/// 6. Cache and return results
pub struct CodeSearchAgent {
    github: GitHubClient,
    cache: SearchCache,
    max_repos_to_analyze: usize,
    max_code_searches: usize,
}

impl CodeSearchAgent {
    /// Create a new agent.
    pub fn new(
        github_token: Option<String>,
        cache: Option<SearchCache>,
        max_repos_to_analyze: usize,
        max_code_searches: usize,
    ) -> Self {
        Self {
            github: GitHubClient::new(github_token),
            cache: cache.unwrap_or_default(),
            max_repos_to_analyze,
            max_code_searches,
        }
    }

    /// Execute a complete code search for the given topic and domain.
    ///
    /// `domain_id` is a unique identifier for the domain (e.g. `"hep"`).
    /// `domain_name` is a human-readable display name (e.g. `"High-Energy Physics"`).
    pub async fn search(
        &mut self,
        topic: &str,
        domain_id: &str,
        domain_name: &str,
        core_libraries: &[String],
        specific_needs: &[String],
    ) -> Result<CodeSearchResult> {
        info!(topic, domain_id, "CodeSearchAgent starting");

        // 1. Check cache.
        if let Some(cached) = self.cache.get(domain_id, topic) {
            info!(topic, "Using cached code search results");
            return Ok(CodeSearchResult::from_cache_value(cached));
        }

        // 2. Generate search queries.
        let queries = generate_search_queries(
            topic,
            domain_name,
            core_libraries,
            specific_needs,
            self.max_code_searches + 3,
        );
        debug!(count = queries.len(), "Generated search queries");

        let mut result = CodeSearchResult {
            queries_used: queries.clone(),
            ..Default::default()
        };

        // 3. Search GitHub repos.
        if let Some(first_query) = queries.first() {
            match self.github.search_repos(first_query, 10).await {
                Ok(repos) => {
                    let mut filtered: Vec<RepoInfo> =
                        repos.into_iter().filter(|r| r.stars >= 10).collect();
                    filtered.truncate(self.max_repos_to_analyze * 2);
                    result.repos_found = filtered;
                }
                Err(e) => warn!("Repo search failed: {e}"),
            }
        }

        // 4. Search GitHub code.
        let mut code_snippets: Vec<String> = Vec::new();
        for query in queries.iter().skip(1).take(self.max_code_searches) {
            match self.github.search_code(query, 5).await {
                Ok(mut snippets) => result.snippets_found.append(&mut snippets),
                Err(e) => warn!("Code search failed for '{}': {e}", query),
            }
        }

        // 5. Analyse top repositories.
        let repos_to_analyze: Vec<RepoInfo> =
            result.repos_found[..result.repos_found.len().min(self.max_repos_to_analyze)]
                .to_vec();

        for repo in &repos_to_analyze {
            match self.analyze_repo(repo).await {
                Some(analysis) => {
                    for content in analysis.key_files.values() {
                        if !content.is_empty() {
                            code_snippets.push(content.clone());
                        }
                    }
                    result.repo_analyses.push(analysis);
                }
                None => warn!("Failed to analyze repo {}", repo.full_name),
            }
        }

        // Fetch code snippet content.
        for snippet in result.snippets_found.iter_mut().take(5) {
            match self
                .github
                .get_file_content(&snippet.repo_full_name, &snippet.file_path, 50)
                .await
            {
                Some(content) => {
                    code_snippets.push(content.clone());
                    snippet.content = content;
                }
                None => {}
            }
        }

        // 6. Extract patterns.
        if !code_snippets.is_empty() {
            result.patterns = extract_patterns(&code_snippets, topic, domain_name);
        }

        result.github_requests = self.github.request_count;

        // 7. Cache results.
        if result.patterns.has_content() {
            self.cache.put(domain_id, topic, result.to_cache_value());
        }

        info!(
            repos = result.repos_found.len(),
            snippets = result.snippets_found.len(),
            api_patterns = result.patterns.api_patterns.len(),
            github_requests = result.github_requests,
            "CodeSearchAgent complete"
        );

        Ok(result)
    }

    /// Analyse a single repository by reading README, file tree, and key files.
    async fn analyze_repo(&mut self, repo: &RepoInfo) -> Option<RepoAnalysis> {
        let mut analysis = RepoAnalysis {
            repo: repo.clone(),
            ..Default::default()
        };

        // README
        if let Some(readme) = self.github.get_readme(&repo.full_name).await {
            analysis.readme = readme.chars().take(3000).collect();
        }

        // File tree
        analysis.file_tree = self
            .github
            .get_repo_tree(&repo.full_name, &repo.default_branch)
            .await;

        // Key files
        let key_patterns = [
            "main.py",
            "run.py",
            "train.py",
            "experiment.py",
            "requirements.txt",
            "setup.py",
            "pyproject.toml",
        ];

        for pattern in &key_patterns {
            let matches: Vec<&String> = analysis
                .file_tree
                .iter()
                .filter(|f| f.ends_with(pattern))
                .collect();

            if let Some(first) = matches.first() {
                if let Some(content) = self
                    .github
                    .get_file_content(&repo.full_name, first, 50)
                    .await
                {
                    analysis.key_files.insert((*first).clone(), content);
                }
            }
        }

        // Parse requirements
        if let Some(req_content) = analysis.key_files.get("requirements.txt") {
            analysis.requirements = req_content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .map(|l| {
                    l.trim()
                        .split(|c| c == '=' || c == '>')
                        .next()
                        .unwrap_or(l)
                        .to_owned()
                })
                .collect();
        }

        Some(analysis)
    }
}
