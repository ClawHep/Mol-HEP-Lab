//! Python code validation: syntax, import whitelist, and security checks.
//!
//! Mirrors the Python `validator.py` module. Three validation layers:
//!   1. Syntax check — spawn `python3 -c "compile(...)"` (or AST parse via subprocess).
//!   2. Import whitelist — flag any import not in the allowed list.
//!   3. Security scan — detect dangerous calls and banned modules.

use std::process::Command;

use tracing::warn;

// ---------------------------------------------------------------------------
// ValidationIssue
// ---------------------------------------------------------------------------

/// Severity of a validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
        }
    }
}

/// Category of a validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    Syntax,
    Security,
    Import,
    Style,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Syntax => write!(f, "syntax"),
            Category::Security => write!(f, "security"),
            Category::Import => write!(f, "import"),
            Category::Style => write!(f, "style"),
        }
    }
}

/// A single validation finding.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    /// 1-based line number, if known.
    pub line: Option<u32>,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.line {
            write!(
                f,
                "[{}:{}] line {}: {}",
                self.severity, self.category, line, self.message
            )
        } else {
            write!(f, "[{}:{}] {}", self.severity, self.category, self.message)
        }
    }
}

// ---------------------------------------------------------------------------
// Security constants
// ---------------------------------------------------------------------------

/// Fully-qualified call names that are forbidden in experiment code.
const DANGEROUS_CALLS: &[&str] = &[
    "os.system",
    "os.popen",
    "os.exec",
    "os.execl",
    "os.execle",
    "os.execlp",
    "os.execlpe",
    "os.execv",
    "os.execve",
    "os.execvp",
    "os.execvpe",
    "os.remove",
    "os.unlink",
    "os.rmdir",
    "os.removedirs",
    "subprocess.call",
    "subprocess.run",
    "subprocess.Popen",
    "subprocess.check_call",
    "subprocess.check_output",
    "shutil.rmtree",
];

/// Bare built-in names that should never appear.
const DANGEROUS_BUILTINS: &[&str] = &["eval", "exec", "compile", "__import__"];

/// Module names that must not be imported.
const BANNED_MODULES: &[&str] = &[
    "subprocess",
    "shutil",
    "socket",
    "http",
    "urllib",
    "requests",
    "ftplib",
    "smtplib",
    "ctypes",
    "signal",
];

// ---------------------------------------------------------------------------
// validate_python_syntax
// ---------------------------------------------------------------------------

/// Check Python syntax by spawning `python3 -c "compile(...)"`.
///
/// Returns `Ok(vec![])` on success, or `Ok(vec![issue])` on syntax error.
/// Returns `Err` only if `python3` could not be spawned at all.
pub fn validate_python_syntax(code: &str) -> anyhow::Result<Vec<ValidationIssue>> {
    // Write code to a temp file and use `python3 -m py_compile`.
    let tmp = tempfile::NamedTempFile::with_suffix(".py")?;
    std::fs::write(tmp.path(), code)?;

    let output = Command::new("python3")
        .arg("-m")
        .arg("py_compile")
        .arg(tmp.path())
        .output();

    match output {
        Ok(out) if out.status.success() => Ok(Vec::new()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            // Try to extract a line number from the error message.
            // Python's SyntaxError format: "  File ..., line N"
            let line = extract_line_number(&stderr);

            Ok(vec![ValidationIssue {
                severity: Severity::Error,
                category: Category::Syntax,
                message: stderr.trim().to_string(),
                line,
            }])
        }
        Err(e) => {
            warn!("Could not spawn python3 for syntax check: {e}");
            // Best-effort: do a naive brace/paren check inline.
            Ok(basic_syntax_check(code))
        }
    }
}

fn extract_line_number(stderr: &str) -> Option<u32> {
    let re = regex::Regex::new(r"line (\d+)").ok()?;
    let cap = re.captures(stderr)?;
    cap[1].parse().ok()
}

/// Minimal fallback: check for common bare-syntax issues when Python is not available.
fn basic_syntax_check(code: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Check balanced parentheses / brackets / braces.
    let mut stack: Vec<(char, usize)> = Vec::new();
    for (line_idx, line) in code.lines().enumerate() {
        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => stack.push((ch, line_idx + 1)),
                ')' => {
                    if stack.last().map(|(c, _)| *c) != Some('(') {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            category: Category::Syntax,
                            message: "Unmatched ')'".to_string(),
                            line: Some((line_idx + 1) as u32),
                        });
                    } else {
                        stack.pop();
                    }
                }
                ']' => {
                    if stack.last().map(|(c, _)| *c) != Some('[') {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            category: Category::Syntax,
                            message: "Unmatched ']'".to_string(),
                            line: Some((line_idx + 1) as u32),
                        });
                    } else {
                        stack.pop();
                    }
                }
                '}' => {
                    if stack.last().map(|(c, _)| *c) != Some('{') {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            category: Category::Syntax,
                            message: "Unmatched '}'".to_string(),
                            line: Some((line_idx + 1) as u32),
                        });
                    } else {
                        stack.pop();
                    }
                }
                _ => {}
            }
        }
    }
    for (ch, line_no) in stack {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            category: Category::Syntax,
            message: format!("Unclosed '{ch}'"),
            line: Some(line_no as u32),
        });
    }
    issues
}

// ---------------------------------------------------------------------------
// check_import_whitelist
// ---------------------------------------------------------------------------

/// Warn about any `import X` or `from X import ...` where `X` is not in `allowed`.
///
/// If `allowed` is empty, no warnings are produced (allow-all mode).
pub fn check_import_whitelist(code: &str, allowed: &[String]) -> Vec<ValidationIssue> {
    if allowed.is_empty() {
        return Vec::new();
    }

    let import_re = regex::Regex::new(r"^\s*(?:import|from)\s+([\w.]+)")
        .expect("import regex");

    let mut issues = Vec::new();

    for (line_idx, line) in code.lines().enumerate() {
        if let Some(cap) = import_re.captures(line) {
            let module_name = cap[1].to_string();
            let top_module = module_name
                .split('.')
                .next()
                .unwrap_or(&module_name)
                .to_string();

            if !allowed.contains(&top_module) && !allowed.contains(&module_name) {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    category: Category::Import,
                    message: format!("Import '{module_name}' is not in the allowed list"),
                    line: Some((line_idx + 1) as u32),
                });
            }
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// check_security
// ---------------------------------------------------------------------------

/// Scan code for dangerous patterns (dangerous calls, builtins, banned modules).
///
/// Returns a `Vec<ValidationIssue>` with `Severity::Error` for each finding.
pub fn check_security(code: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for (line_idx, line) in code.lines().enumerate() {
        let line_no = (line_idx + 1) as u32;
        let stripped = line.trim();

        // Skip comment lines.
        if stripped.starts_with('#') {
            continue;
        }

        // --- Dangerous builtins ---
        for &builtin in DANGEROUS_BUILTINS {
            if contains_call(stripped, builtin) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    category: Category::Security,
                    message: format!("Dangerous built-in call: {builtin}()"),
                    line: Some(line_no),
                });
            }
        }

        // --- Dangerous qualified calls ---
        for &call in DANGEROUS_CALLS {
            if contains_call(stripped, call) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    category: Category::Security,
                    message: format!("Dangerous call: {call}()"),
                    line: Some(line_no),
                });
            }
        }

        // --- Banned module imports ---
        let import_re = regex::Regex::new(r"^\s*(?:import|from)\s+([\w.]+)")
            .expect("import regex");
        if let Some(cap) = import_re.captures(line) {
            let module = &cap[1];
            let top = module.split('.').next().unwrap_or(module);
            if BANNED_MODULES.contains(&top) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    category: Category::Security,
                    message: format!("Banned module import: {module}"),
                    line: Some(line_no),
                });
            }
        }
    }

    issues
}

/// Returns `true` if `line` contains a call to `name` (bare or qualified).
fn contains_call(line: &str, name: &str) -> bool {
    // Look for `name(` with optional whitespace before `(`.
    let pattern = format!(r"\b{}\s*\(", regex::escape(name));
    regex::Regex::new(&pattern)
        .map(|re| re.is_match(line))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Convenience: run all checks
// ---------------------------------------------------------------------------

/// Run all three validation layers and return a flat list of issues.
pub fn validate_code(code: &str, allowed_imports: &[String]) -> anyhow::Result<Vec<ValidationIssue>> {
    let mut issues = Vec::new();

    // 1. Syntax
    let syntax = validate_python_syntax(code)?;
    issues.extend(syntax);

    // 2. Import whitelist
    let imports = check_import_whitelist(code, allowed_imports);
    issues.extend(imports);

    // 3. Security
    let security = check_security(code);
    issues.extend(security);

    Ok(issues)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_detects_eval() {
        let code = "result = eval('1+1')";
        let issues = check_security(code);
        assert!(issues.iter().any(|i| i.message.contains("eval")));
    }

    #[test]
    fn test_security_detects_banned_import() {
        let code = "import subprocess";
        let issues = check_security(code);
        assert!(issues.iter().any(|i| i.message.contains("subprocess")));
    }

    #[test]
    fn test_import_whitelist_warns() {
        let code = "import numpy\nimport secret_pkg";
        let allowed = vec!["numpy".to_string()];
        let issues = check_import_whitelist(code, &allowed);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("secret_pkg"));
    }

    #[test]
    fn test_import_whitelist_empty_allows_all() {
        let code = "import anything";
        let issues = check_import_whitelist(code, &[]);
        assert!(issues.is_empty());
    }
}
