//! Pattern extractor — identifies reusable code patterns from repository files.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// CodePatterns
// ---------------------------------------------------------------------------

/// Structured patterns extracted from a set of code files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodePatterns {
    /// High-level API usage patterns (e.g. `"trainer.fit(model, datamodule)"`).
    pub api_patterns: Vec<String>,

    /// Typical file/directory structure (`path_fragment → description`).
    pub file_structure: HashMap<String, String>,

    /// Evaluation patterns (e.g. `"test_acc = evaluate(model, test_loader)"`).
    pub evaluation_patterns: Vec<String>,

    /// Library versions inferred from `requirements.txt` / `setup.py`.
    pub library_versions: HashMap<String, String>,
}

impl CodePatterns {
    /// `true` when at least one category of patterns was populated.
    pub fn has_content(&self) -> bool {
        !self.api_patterns.is_empty()
            || !self.file_structure.is_empty()
            || !self.evaluation_patterns.is_empty()
    }

    /// Format as a compact context block for prompt injection.
    pub fn to_prompt_context(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if !self.api_patterns.is_empty() {
            parts.push("### API Patterns".to_owned());
            for p in &self.api_patterns {
                parts.push(format!("- `{p}`"));
            }
        }

        if !self.evaluation_patterns.is_empty() {
            parts.push("### Evaluation Patterns".to_owned());
            for p in &self.evaluation_patterns {
                parts.push(format!("- `{p}`"));
            }
        }

        if !self.library_versions.is_empty() {
            parts.push("### Library Versions".to_owned());
            for (lib, ver) in &self.library_versions {
                parts.push(format!("- {lib} {ver}"));
            }
        }

        if !self.file_structure.is_empty() {
            parts.push("### Typical File Structure".to_owned());
            for (path, desc) in &self.file_structure {
                parts.push(format!("- `{path}` — {desc}"));
            }
        }

        parts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// PatternExtractor
// ---------------------------------------------------------------------------

/// Extracts reusable patterns from a collection of code snippets.
pub struct PatternExtractor;

impl PatternExtractor {
    /// Extract patterns heuristically (no LLM required).
    ///
    /// When an LLM client is available, callers can post-process these with
    /// richer semantic analysis.
    pub fn extract(
        &self,
        code_snippets: &[String],
        _topic: &str,
        _domain_name: &str,
    ) -> CodePatterns {
        debug!(
            snippets = code_snippets.len(),
            "PatternExtractor running heuristic extraction"
        );

        let mut api_patterns: Vec<String> = Vec::new();
        let mut evaluation_patterns: Vec<String> = Vec::new();
        let mut library_versions: HashMap<String, String> = HashMap::new();
        let mut file_structure: HashMap<String, String> = HashMap::new();

        for snippet in code_snippets {
            // API patterns: function definitions and common call patterns.
            for line in snippet.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("def ") || trimmed.starts_with("class ") {
                    let pattern = trimmed.splitn(2, ':').next().unwrap_or("").trim().to_owned();
                    if !pattern.is_empty() && !api_patterns.contains(&pattern) {
                        api_patterns.push(pattern);
                    }
                }

                // Evaluation hints.
                if trimmed.contains("accuracy")
                    || trimmed.contains("evaluate(")
                    || trimmed.contains("test_step")
                    || trimmed.contains("val_loss")
                {
                    let pattern = trimmed.to_owned();
                    if !evaluation_patterns.contains(&pattern) {
                        evaluation_patterns.push(pattern);
                    }
                }

                // Requirements lines.
                if !trimmed.starts_with('#')
                    && (trimmed.contains("==") || trimmed.contains(">="))
                {
                    let parts: Vec<&str> = trimmed
                        .splitn(2, |c| c == '=' || c == '>')
                        .collect();
                    if parts.len() == 2 {
                        let lib = parts[0].trim().trim_end_matches(['>', '<', '=', '!']).to_owned();
                        let ver = parts[1].trim_start_matches(['=', '>']).to_owned();
                        if !lib.is_empty() && !ver.is_empty() {
                            library_versions.entry(lib).or_insert(ver);
                        }
                    }
                }
            }
        }

        // Infer simple file structure from collected snippets.
        file_structure.insert("train.py".to_owned(), "Main training script".to_owned());
        file_structure.insert("eval.py".to_owned(), "Evaluation script".to_owned());
        file_structure.insert(
            "requirements.txt".to_owned(),
            "Python dependencies".to_owned(),
        );

        // Deduplicate and cap.
        api_patterns.dedup();
        api_patterns.truncate(20);
        evaluation_patterns.dedup();
        evaluation_patterns.truncate(10);

        CodePatterns {
            api_patterns,
            file_structure,
            evaluation_patterns,
            library_versions,
        }
    }
}

// ---------------------------------------------------------------------------
// Free function (convenience wrapper)
// ---------------------------------------------------------------------------

/// Extract code patterns — free-function convenience wrapper.
pub fn extract_patterns(
    code_snippets: &[String],
    topic: &str,
    domain_name: &str,
) -> CodePatterns {
    PatternExtractor.extract(code_snippets, topic, domain_name)
}
