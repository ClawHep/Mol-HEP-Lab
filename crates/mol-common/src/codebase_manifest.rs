//! # Codebase Manifest Generator
//!
//! Scans a Python codebase, extracts classes, functions, imports, and README
//! excerpts, then produces a compact API manifest suitable for LLM prompt
//! injection.
//!
//! Uses regex-based parsing (no tree-sitter) which is sufficient for manifest
//! generation — full AST fidelity is not required.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Top-level manifest for a Python codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseManifest {
    pub hash: String,
    pub repo_name: String,
    pub repo_path: String,
    pub file_tree: Vec<String>,
    pub readme_excerpt: String,
    pub modules: Vec<ModuleInfo>,
}

/// Parsed information for a single Python module (file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub path: String,
    pub imports: Vec<String>,
    pub classes: Vec<ClassInfo>,
    pub functions: Vec<FunctionInfo>,
    pub is_auxiliary: bool,
}

/// A class extracted from a Python file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: String,
    pub bases: Vec<String>,
    pub docstring: String,
    pub methods: Vec<FunctionInfo>,
}

/// A function (or method) extracted from a Python file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub signature: String,
    pub docstring: String,
}

// ---------------------------------------------------------------------------
// Directory / path constants
// ---------------------------------------------------------------------------

/// Directories to skip entirely when scanning.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "__pycache__",
    "node_modules",
    ".eggs",
    ".egg-info",
    "docs",
    "static",
    "assets",
    "images",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "dist",
    "build",
    ".venv",
    "venv",
];

/// Directory prefixes that mark auxiliary (non-core) code.
const AUX_PREFIXES: &[&str] = &[
    "test", "tests", "eval", "evals", "benchmark", "benchmarks", "examples",
];

// ---------------------------------------------------------------------------
// Cache file name
// ---------------------------------------------------------------------------

const CACHE_FILE: &str = "_manifest.json";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate (or load from cache) the API manifest for a Python codebase at
/// `repo_path`.
///
/// The manifest is cached to `<repo_path>/_manifest.json` and invalidated when
/// file modification times change.
pub fn generate_manifest(repo_path: &Path) -> anyhow::Result<CodebaseManifest> {
    let hash = dir_hash(repo_path);
    let cache_path = repo_path.join(CACHE_FILE);

    // Try loading from cache.
    if cache_path.exists() {
        if let Ok(data) = fs::read_to_string(&cache_path) {
            if let Ok(cached) = serde_json::from_str::<CodebaseManifest>(&data) {
                if cached.hash == hash {
                    return Ok(cached);
                }
            }
        }
    }

    // Collect .py files.
    let mut py_files: Vec<PathBuf> = Vec::new();
    collect_py_files(repo_path, repo_path, &mut py_files);
    py_files.sort();

    let repo_name = repo_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into());

    let file_tree: Vec<String> = py_files
        .iter()
        .filter_map(|p| p.strip_prefix(repo_path).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    // Parse each file.
    let modules: Vec<ModuleInfo> = py_files
        .iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(repo_path).ok()?;
            let rel_str = rel.to_string_lossy().into_owned();
            let content = fs::read_to_string(p).ok()?;
            Some(parse_python_file(&content, &rel_str))
        })
        .collect();

    // Read README.
    let readme_excerpt = read_readme(repo_path);

    let manifest = CodebaseManifest {
        hash,
        repo_name,
        repo_path: repo_path.to_string_lossy().into_owned(),
        file_tree,
        readme_excerpt,
        modules,
    };

    // Write cache (best-effort).
    if let Ok(json) = serde_json::to_string_pretty(&manifest) {
        let _ = fs::write(&cache_path, json);
    }

    Ok(manifest)
}

/// Convert a [`CodebaseManifest`] into a compact prompt string for LLM
/// context injection.
pub fn manifest_to_prompt(manifest: &CodebaseManifest) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Codebase: {}\n\n", manifest.repo_name));

    if !manifest.readme_excerpt.is_empty() {
        out.push_str("## Overview\n");
        out.push_str(&manifest.readme_excerpt);
        out.push_str("\n\n");
    }

    // File tree
    if !manifest.file_tree.is_empty() {
        out.push_str("## File Tree\n```\n");
        for f in &manifest.file_tree {
            out.push_str(f);
            out.push('\n');
        }
        out.push_str("```\n\n");
    }

    // API reference — skip auxiliary modules for brevity.
    let core_modules: Vec<&ModuleInfo> =
        manifest.modules.iter().filter(|m| !m.is_auxiliary).collect();

    if !core_modules.is_empty() {
        out.push_str("## API Reference\n\n");
        for m in &core_modules {
            out.push_str(&format!("### `{}`\n", m.path));

            if !m.imports.is_empty() {
                out.push_str("Imports: ");
                out.push_str(&m.imports.join(", "));
                out.push('\n');
            }

            for cls in &m.classes {
                let bases = if cls.bases.is_empty() {
                    String::new()
                } else {
                    format!("({})", cls.bases.join(", "))
                };
                out.push_str(&format!("- **class {}{}**", cls.name, bases));
                if !cls.docstring.is_empty() {
                    out.push_str(&format!(" — {}", cls.docstring));
                }
                out.push('\n');
                for method in &cls.methods {
                    out.push_str(&format!("  - `{}`", method.signature));
                    if !method.docstring.is_empty() {
                        out.push_str(&format!(" — {}", method.docstring));
                    }
                    out.push('\n');
                }
            }

            for func in &m.functions {
                out.push_str(&format!("- `{}`", func.signature));
                if !func.docstring.is_empty() {
                    out.push_str(&format!(" — {}", func.docstring));
                }
                out.push('\n');
            }
            out.push('\n');
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Recursively collect `.py` files under `dir`, skipping ignored directories.
fn collect_py_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if SKIP_DIRS.iter().any(|s| *s == name_str.as_ref()) {
                continue;
            }
            // Also skip hidden directories (other than those already listed).
            if name_str.starts_with('.') {
                continue;
            }
            collect_py_files(root, &path, out);
        } else if path.extension().map(|e| e == "py").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Quick hash of file modification times for cache invalidation.
fn dir_hash(repo_path: &Path) -> String {
    let mut py_files: Vec<PathBuf> = Vec::new();
    collect_py_files(repo_path, repo_path, &mut py_files);
    py_files.sort();

    let mut hasher = DefaultHasher::new();
    for p in &py_files {
        p.to_string_lossy().hash(&mut hasher);
        if let Ok(meta) = fs::metadata(p) {
            if let Ok(mtime) = meta.modified() {
                mtime.hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

/// Check if a relative path is auxiliary (tests, eval, benchmarks, etc.).
fn is_auxiliary_code(rel_path: &str) -> bool {
    let lower = rel_path.to_lowercase();
    let first_component = lower.split('/').next().unwrap_or("");
    AUX_PREFIXES
        .iter()
        .any(|prefix| first_component == *prefix)
}

/// Parse a single Python file using regex, returning a [`ModuleInfo`].
fn parse_python_file(content: &str, rel_path: &str) -> ModuleInfo {
    let imports = extract_imports(content);
    let classes = extract_classes(content);
    let functions = extract_top_level_functions(content);

    ModuleInfo {
        path: rel_path.to_string(),
        imports,
        classes,
        functions,
        is_auxiliary: is_auxiliary_code(rel_path),
    }
}

/// Extract import statements.
fn extract_imports(content: &str) -> Vec<String> {
    let re = Regex::new(r"(?m)^(?:from\s+(\S+)\s+import\s+.+|import\s+(\S+))").unwrap();
    let mut imports = Vec::new();
    for cap in re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            imports.push(m.as_str().to_string());
        } else if let Some(m) = cap.get(2) {
            // Strip trailing commas from `import a, b` — just take the first
            let module = m.as_str().trim_end_matches(',');
            imports.push(module.to_string());
        }
    }
    imports.sort();
    imports.dedup();
    imports
}

/// Extract class definitions with their bases, docstrings, and methods.
fn extract_classes(content: &str) -> Vec<ClassInfo> {
    let class_re =
        Regex::new(r"(?m)^class\s+(\w+)\s*(?:\(([^)]*)\))?\s*:").unwrap();
    let method_re =
        Regex::new(r"(?m)^[ \t]+def\s+(\w+)\s*\(([^)]*)\)\s*(?:->[^:]+)?\s*:").unwrap();

    let lines: Vec<&str> = content.lines().collect();
    let mut classes = Vec::new();

    for cap in class_re.captures_iter(content) {
        let name = cap[1].to_string();
        let bases: Vec<String> = cap
            .get(2)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // Find the line index of this class.
        let class_start = cap.get(0).unwrap().start();
        let class_line = content[..class_start].matches('\n').count();

        // Determine the class body extent (until next top-level definition or EOF).
        let class_end_line = find_block_end(&lines, class_line);

        let docstring = extract_docstring_from_lines(&lines, class_line);

        // Find methods within the class body.
        let class_body: String = lines[class_line..class_end_line].join("\n");
        let methods: Vec<FunctionInfo> = method_re
            .captures_iter(&class_body)
            .map(|mc| {
                let mname = mc[1].to_string();
                let args = mc[2].to_string();
                let sig = format!("def {}({})", mname, simplify_args(&args));

                // Find this method's line within the class body.
                let m_start = mc.get(0).unwrap().start();
                let m_line = class_body[..m_start].matches('\n').count();
                let body_lines: Vec<&str> = class_body.lines().collect();
                let doc = extract_docstring_from_lines(&body_lines, m_line);

                FunctionInfo {
                    name: mname,
                    signature: sig,
                    docstring: doc,
                }
            })
            .collect();

        classes.push(ClassInfo {
            name,
            bases,
            docstring,
            methods,
        });
    }

    classes
}

/// Extract top-level function definitions (not indented = not methods).
fn extract_top_level_functions(content: &str) -> Vec<FunctionInfo> {
    let re = Regex::new(r"(?m)^def\s+(\w+)\s*\(([^)]*)\)\s*(?:->[^:]+)?\s*:").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut functions = Vec::new();

    for cap in re.captures_iter(content) {
        let name = cap[1].to_string();
        let args = cap[2].to_string();
        let sig = format!("def {}({})", name, simplify_args(&args));

        let fn_start = cap.get(0).unwrap().start();
        let fn_line = content[..fn_start].matches('\n').count();
        let docstring = extract_docstring_from_lines(&lines, fn_line);

        functions.push(FunctionInfo {
            name,
            signature: sig,
            docstring,
        });
    }

    functions
}

/// Simplify function arguments: strip type annotations and defaults for
/// brevity, keeping just parameter names plus a hint.
fn simplify_args(args: &str) -> String {
    // For manifest purposes, keep the raw args but trim excess whitespace.
    args.split(',')
        .map(|a| a.trim())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extract the first-line docstring (max 200 chars) from the line immediately
/// after a `class` or `def` statement.
fn extract_docstring_from_lines(lines: &[&str], def_line: usize) -> String {
    // Look at the next few lines for a triple-quoted string.
    for i in 1..=2 {
        let idx = def_line + i;
        if idx >= lines.len() {
            break;
        }
        let trimmed = lines[idx].trim();

        // Single-line docstring: """...""" or '''...'''
        if (trimmed.starts_with("\"\"\"") && trimmed.ends_with("\"\"\"") && trimmed.len() > 6)
            || (trimmed.starts_with("'''") && trimmed.ends_with("'''") && trimmed.len() > 6)
        {
            let doc = &trimmed[3..trimmed.len() - 3];
            return truncate_str(doc.trim(), 200);
        }

        // Multi-line docstring opening.
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let first_line = &trimmed[3..];
            if !first_line.is_empty() {
                return truncate_str(first_line.trim(), 200);
            }
            // The docstring content is on the next line.
            if idx + 1 < lines.len() {
                return truncate_str(lines[idx + 1].trim(), 200);
            }
        }
    }
    String::new()
}

/// Find the end of a top-level block (class or function) starting at
/// `start_line`. We look for the next line at the same or lower indentation
/// that starts a new definition, or EOF.
fn find_block_end(lines: &[&str], start_line: usize) -> usize {
    if start_line + 1 >= lines.len() {
        return lines.len();
    }
    for i in (start_line + 1)..lines.len() {
        let line = lines[i];
        if line.is_empty() {
            continue;
        }
        // A non-indented, non-empty line that starts a new block.
        let first_char = line.as_bytes().first().copied().unwrap_or(b' ');
        if first_char != b' ' && first_char != b'\t' && first_char != b'#' {
            return i;
        }
    }
    lines.len()
}

/// Read the first README found in the repo root.
fn read_readme(repo_path: &Path) -> String {
    for name in &["README.md", "README.rst", "README.txt", "README"] {
        let path = repo_path.join(name);
        if let Ok(content) = fs::read_to_string(&path) {
            return trim_readme(&content);
        }
    }
    String::new()
}

/// Strip HTML tags, badge images, and boilerplate from a README, keeping at
/// most 1500 characters.
fn trim_readme(readme: &str) -> String {
    let html_tag_re = Regex::new(r"<[^>]+>").unwrap();
    let badge_re = Regex::new(r"\[!\[.*?\]\(.*?\)\]\(.*?\)").unwrap();
    let img_re = Regex::new(r"!\[.*?\]\(.*?\)").unwrap();
    let link_shield_re = Regex::new(r"https?://img\.shields\.io\S*").unwrap();
    let empty_lines_re = Regex::new(r"\n{3,}").unwrap();

    let mut text = readme.to_string();
    text = badge_re.replace_all(&text, "").to_string();
    text = img_re.replace_all(&text, "").to_string();
    text = link_shield_re.replace_all(&text, "").to_string();
    text = html_tag_re.replace_all(&text, "").to_string();

    // Remove common boilerplate sections line-by-line.
    let mut filtered_lines = Vec::new();
    let mut skip_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        let is_heading = trimmed.starts_with('#');
        if is_heading {
            let heading_text = trimmed.trim_start_matches('#').trim().to_lowercase();
            if ["license", "contributing", "changelog", "installation"]
                .iter()
                .any(|kw| heading_text == *kw)
            {
                skip_section = true;
                continue;
            } else {
                skip_section = false;
            }
        }
        if !skip_section {
            filtered_lines.push(line);
        }
    }
    text = filtered_lines.join("\n");

    text = empty_lines_re.replace_all(&text, "\n\n").to_string();
    let text = text.trim();

    truncate_str(text, 1500)
}

/// Truncate a string to at most `max_chars` characters, appending "…" if
/// truncated.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        let mut end = max_chars;
        // Avoid splitting a multi-byte character.
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_auxiliary_detects_test_dirs() {
        assert!(is_auxiliary_code("tests/test_foo.py"));
        assert!(is_auxiliary_code("eval/evaluate.py"));
        assert!(is_auxiliary_code("benchmarks/run.py"));
        assert!(!is_auxiliary_code("src/model.py"));
    }

    #[test]
    fn parse_python_extracts_class() {
        let code = r#"
class MyModel(nn.Module):
    """A simple model."""
    def forward(self, x):
        return x
"#;
        let info = parse_python_file(code, "model.py");
        assert_eq!(info.classes.len(), 1);
        assert_eq!(info.classes[0].name, "MyModel");
        assert!(info.classes[0].docstring.contains("simple model"));
    }

    #[test]
    fn parse_python_extracts_function() {
        let code =
            "def train(model, data, epochs=10):\n    \"\"\"Train the model.\"\"\"\n    pass\n";
        let info = parse_python_file(code, "utils.py");
        assert_eq!(info.functions.len(), 1);
        assert_eq!(info.functions[0].name, "train");
        assert!(info.functions[0].docstring.contains("Train the model"));
    }

    #[test]
    fn parse_python_extracts_imports() {
        let code = "import os\nfrom pathlib import Path\nimport sys\n";
        let info = parse_python_file(code, "foo.py");
        assert!(info.imports.contains(&"os".to_string()));
        assert!(info.imports.contains(&"pathlib".to_string()));
        assert!(info.imports.contains(&"sys".to_string()));
    }

    #[test]
    fn parse_python_extracts_methods() {
        let code = r#"
class Trainer:
    """Handles training."""
    def fit(self, data):
        """Fit the model."""
        pass
    def predict(self, x):
        """Run prediction."""
        pass
"#;
        let info = parse_python_file(code, "trainer.py");
        assert_eq!(info.classes.len(), 1);
        assert_eq!(info.classes[0].methods.len(), 2);
        assert_eq!(info.classes[0].methods[0].name, "fit");
        assert_eq!(info.classes[0].methods[1].name, "predict");
    }

    #[test]
    fn trim_readme_removes_badges() {
        let readme = "# Project\n[![Build](https://img.shields.io/badge)]\nSome content here.";
        let trimmed = trim_readme(readme);
        assert!(!trimmed.contains("img.shields.io"));
        assert!(trimmed.contains("Some content"));
    }

    #[test]
    fn trim_readme_respects_max_length() {
        let long_readme = "x".repeat(3000);
        let trimmed = trim_readme(&long_readme);
        assert!(trimmed.len() <= 1503); // 1500 + "…" (3 bytes in UTF-8)
    }

    #[test]
    fn manifest_to_prompt_includes_repo_name() {
        let manifest = CodebaseManifest {
            hash: "abc".into(),
            repo_name: "my-repo".into(),
            repo_path: "/tmp/my-repo".into(),
            file_tree: vec!["src/main.py".into()],
            readme_excerpt: "A test repo".into(),
            modules: vec![],
        };
        let prompt = manifest_to_prompt(&manifest);
        assert!(prompt.contains("my-repo"));
        assert!(prompt.contains("A test repo"));
    }

    #[test]
    fn manifest_to_prompt_shows_api_reference() {
        let manifest = CodebaseManifest {
            hash: "abc".into(),
            repo_name: "test".into(),
            repo_path: "/tmp/test".into(),
            file_tree: vec!["model.py".into()],
            readme_excerpt: String::new(),
            modules: vec![ModuleInfo {
                path: "model.py".into(),
                imports: vec!["torch".into()],
                classes: vec![ClassInfo {
                    name: "Net".into(),
                    bases: vec!["nn.Module".into()],
                    docstring: "A network.".into(),
                    methods: vec![FunctionInfo {
                        name: "forward".into(),
                        signature: "def forward(self, x)".into(),
                        docstring: "Forward pass.".into(),
                    }],
                }],
                functions: vec![],
                is_auxiliary: false,
            }],
        };
        let prompt = manifest_to_prompt(&manifest);
        assert!(prompt.contains("class Net(nn.Module)"));
        assert!(prompt.contains("forward"));
    }

    #[test]
    fn auxiliary_modules_excluded_from_prompt() {
        let manifest = CodebaseManifest {
            hash: "abc".into(),
            repo_name: "test".into(),
            repo_path: "/tmp/test".into(),
            file_tree: vec![],
            readme_excerpt: String::new(),
            modules: vec![ModuleInfo {
                path: "tests/test_foo.py".into(),
                imports: vec![],
                classes: vec![],
                functions: vec![FunctionInfo {
                    name: "test_something".into(),
                    signature: "def test_something()".into(),
                    docstring: String::new(),
                }],
                is_auxiliary: true,
            }],
        };
        let prompt = manifest_to_prompt(&manifest);
        assert!(!prompt.contains("test_something"));
    }
}
