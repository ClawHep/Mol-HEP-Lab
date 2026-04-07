//! Python code validation: syntax, import whitelist, and security checks.
//!
//! Three validation layers:
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
// format_issues_for_llm
// ---------------------------------------------------------------------------

/// Format validation issues as a concise error report for LLM repair prompts.
///
/// Returns `"No issues found."` when the list is empty.
pub fn format_issues_for_llm(issues: &[ValidationIssue]) -> String {
    if issues.is_empty() {
        return "No issues found.".to_string();
    }
    let mut lines = Vec::with_capacity(issues.len());
    for issue in issues {
        let loc = match issue.line {
            Some(l) => format!("line {l}"),
            None => "unknown location".to_string(),
        };
        lines.push(format!(
            "- [{}] ({}) {} @ {}",
            issue.severity.to_string().to_uppercase(),
            issue.category,
            issue.message,
            loc
        ));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// check_code_complexity
// ---------------------------------------------------------------------------

/// Check whether generated experiment code is too simplistic.
///
/// Returns a list of warning strings.  Empty list means no quality concerns.
pub fn check_code_complexity(code: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    // Count non-blank, non-comment, non-import effective lines.
    let effective_lines: Vec<&str> = code
        .lines()
        .filter(|l| {
            let s = l.trim();
            !s.is_empty()
                && !s.starts_with('#')
                && !s.starts_with("import ")
                && !s.starts_with("from ")
        })
        .collect();

    if effective_lines.len() < 10 {
        warnings.push(format!(
            "Code has only {} effective lines (excluding blanks/comments/imports) \
             — likely too simple for a research experiment",
            effective_lines.len()
        ));
    }

    // Check for lack of function definitions when there's substantial code.
    let func_count = code
        .lines()
        .filter(|l| {
            let s = l.trim();
            s.starts_with("def ") || s.starts_with("async def ")
        })
        .count();

    if func_count == 0 && effective_lines.len() > 5 {
        warnings.push(
            "Code has no function definitions — research experiments \
             should be structured with reusable functions"
                .to_string(),
        );
    }

    // Check for hardcoded metric patterns.
    let hardcoded_patterns: &[(&str, &str)] = &[
        (
            r#"print\(['"'].*:\s*0\.\d+['"']\)"#,
            "print statement with hardcoded metric value",
        ),
        (r"metric.*=\s*0\.\d{2,}", "hardcoded metric assignment"),
    ];
    for (pattern, desc) in hardcoded_patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if re.is_match(code) {
                warnings.push(format!("Possible hardcoded metric: {desc}"));
            }
        }
    }

    // Check for fake metrics using random values.
    let fake_patterns: &[(&str, &str)] = &[
        (
            r"(?i)(?:metric|score|accuracy|loss|fid|clip_score)\s*=\s*(?:np\.random|random\.)\w+",
            "random value assigned to metric variable",
        ),
        (
            r"print\(.*(?:np\.random|random\.)\w+.*\)",
            "random value in print output",
        ),
        (
            r"(?i)(?:metric|score|accuracy|loss)\s*=\s*[\d.]+\s*[+\-*/]\s*(?:seed|idx|i)\s*\*\s*[\d.]+",
            "formulaic metric from loop variable",
        ),
        (
            r"sum\(ord\(c\)\s+for\s+c\s+in",
            "deterministic fake metric from string hash",
        ),
    ];
    for (pattern, desc) in fake_patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if re.is_match(code) {
                warnings.push(format!("Possible fake metric: {desc}"));
            }
        }
    }

    // Check for trivially simple computation patterns.
    let trivial_patterns: &[(&str, &str)] = &[
        ("sum(x**2)", "trivial sum-of-squares computation"),
        ("np.sum(x**2)", "trivial sum-of-squares computation"),
        ("0.3 + idx * 0.03", "formulaic/simulated metric generation"),
    ];
    for (pattern, desc) in trivial_patterns {
        if code.contains(pattern) {
            warnings.push(format!("Trivial computation detected: {desc}"));
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// check_main_entry_point
// ---------------------------------------------------------------------------

/// Check that main.py has a runnable entry point.
///
/// Uses regex heuristics as an approximation of the Python AST-based check.
/// Returns a list of critical warning strings.
pub fn check_main_entry_point(code: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    let has_main_func = regex::Regex::new(r"(?m)^def\s+main\s*\(")
        .map(|re| re.is_match(code))
        .unwrap_or(false);
    let has_entry_guard = code.contains("if __name__");
    // Look for a bare `main()` call, NOT on a `def main(` line.
    let has_main_call = code.lines().any(|l| {
        let s = l.trim();
        !s.starts_with("def ") && !s.starts_with("async def ") && s.contains("main()")
    });

    if !has_main_func {
        warnings.push(
            "[main.py] Missing main() function — experiment code will not execute".to_string(),
        );
    }
    if !has_entry_guard && !has_main_call {
        warnings.push(
            "[main.py] Missing entry point: needs 'if __name__ == \"__main__\": main()'".to_string(),
        );
    }

    // Check that main() is not trivially empty (only `pass` in its body).
    if has_main_func {
        // Find main() body — look for lines after `def main(` and check stmt count.
        let func_body_re =
            regex::Regex::new(r"(?ms)^def\s+main\s*\([^)]*\)\s*:\s*\n((?:[ \t]+[^\n]*\n?)*)");
        if let Ok(re) = func_body_re {
            if let Some(cap) = re.captures(code) {
                let body = &cap[1];
                let non_trivial = body
                    .lines()
                    .filter(|l| {
                        let s = l.trim();
                        !s.is_empty() && s != "pass" && !s.starts_with('#')
                    })
                    .count();
                if non_trivial < 3 {
                    warnings.push(
                        "[main.py] main() function is trivially empty — \
                         needs experiment execution logic"
                            .to_string(),
                    );
                }
            }
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// check_class_quality
// ---------------------------------------------------------------------------

/// Analyze class implementations across all experiment files.
///
/// Detects:
/// - Empty or trivial class bodies (< 3 non-blank body lines)
/// - Classes with too few non-dunder methods
/// - nn.Module created inside forward()
/// - Classes that appear to be copy-paste duplicates (same method names, similar line count)
///
/// Since we use regex approximations instead of Python AST, some Python-specific
/// checks (identical AST dump comparisons) are approximated by line-count similarity.
pub fn check_class_quality(files: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut warnings = Vec::new();

    // class_name -> (method_names, body_line_count, has_forward_new_module, filename)
    let mut class_info: Vec<(String, Vec<String>, usize, bool, String)> = Vec::new();

    let class_re = regex::Regex::new(r"(?m)^class\s+(\w+)").expect("class regex");
    let method_re = regex::Regex::new(r"(?m)^    (?:async\s+)?def\s+(\w+)\s*\(").expect("method regex");
    let nn_in_forward_re =
        regex::Regex::new(r"nn\.\s*(?:Linear|Conv2d|Conv1d|LSTM|GRU|Embedding|BatchNorm|LayerNorm|Dropout|MultiheadAttention)\s*\(")
            .expect("nn regex");

    for (fname, code) in files {
        if !fname.ends_with(".py") {
            continue;
        }

        // Extract class blocks: from `class X` to the next non-indented line or EOF.
        let lines: Vec<&str> = code.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if let Some(cap) = class_re.captures(line) {
                let cls_name = cap[1].to_string();
                let class_start = i + 1; // 0-based index of first body line

                // Collect class body until next unindented non-blank line or EOF.
                let mut j = i + 1;
                while j < lines.len() {
                    let body_line = lines[j];
                    if !body_line.is_empty()
                        && !body_line.starts_with(' ')
                        && !body_line.starts_with('\t')
                    {
                        break;
                    }
                    j += 1;
                }
                let class_body = lines[class_start..j].join("\n");

                // Count body lines.
                let body_lines = class_body
                    .lines()
                    .filter(|l| {
                        let s = l.trim();
                        !s.is_empty() && !s.starts_with('#')
                    })
                    .count();

                // Collect method names from body.
                let methods: Vec<String> = method_re
                    .captures_iter(&class_body)
                    .map(|c| c[1].to_string())
                    .collect();

                // Check for nn.Module creation in forward().
                let has_forward_new_module = if class_body.contains("def forward(") {
                    // Find forward body heuristically: lines after `def forward(`
                    let forward_start = class_body
                        .lines()
                        .position(|l| l.trim_start().starts_with("def forward("))
                        .unwrap_or(0);
                    let forward_body: String = class_body
                        .lines()
                        .skip(forward_start + 1)
                        .take_while(|l| l.starts_with("        ") || l.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    nn_in_forward_re.is_match(&forward_body)
                } else {
                    false
                };

                // --- Check 1: Empty or trivial class ---
                if body_lines <= 2 {
                    warnings.push(format!(
                        "[{fname}] Class '{cls_name}' has only {body_lines} body lines \
                         — likely an empty or trivial subclass (class B(A): pass)"
                    ));
                }

                // --- Check 2: Too few non-dunder methods ---
                let non_dunder: Vec<&String> = methods
                    .iter()
                    .filter(|m| !m.starts_with("__"))
                    .collect();
                if body_lines > 5 && non_dunder.len() < 2 {
                    warnings.push(format!(
                        "[{fname}] Class '{cls_name}' has only {} non-dunder method(s) \
                         — algorithm classes should have at least __init__ + one core method \
                         (forward/train_step/predict)",
                        non_dunder.len()
                    ));
                }

                // --- Check 3: nn.Module in forward() ---
                if has_forward_new_module {
                    warnings.push(format!(
                        "[{fname}] Class '{cls_name}' creates nn.Module (nn.Linear etc.) inside \
                         forward() — these modules are unregistered and untrained. \
                         Move to __init__() and register as submodules."
                    ));
                }

                class_info.push((cls_name, methods, body_lines, has_forward_new_module, fname.clone()));
                i = j;
            } else {
                i += 1;
            }
        }
    }

    // --- Check 4: Duplicate class implementations ---
    for i in 0..class_info.len() {
        let (name_a, methods_a, lines_a, _, _) = &class_info[i];
        let non_dunder_a: Vec<&String> = methods_a.iter().filter(|m| !m.starts_with("__")).collect();
        for j in (i + 1)..class_info.len() {
            let (name_b, methods_b, lines_b, _, _) = &class_info[j];
            let non_dunder_b: Vec<&String> = methods_b.iter().filter(|m| !m.starts_with("__")).collect();
            if *lines_a > 5
                && *lines_b > 5
                && non_dunder_a == non_dunder_b
                && (*lines_a as i64 - *lines_b as i64).abs() <= 2
            {
                warnings.push(format!(
                    "Classes '{name_a}' and '{name_b}' have identical method signatures and \
                     similar body sizes ({lines_a} vs {lines_b} lines) \
                     — may be copy-paste variants with no real algorithmic difference"
                ));
            }
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// check_variable_scoping
// ---------------------------------------------------------------------------

/// Detect common variable scoping bugs in experiment code.
///
/// Uses a regex heuristic: finds variables assigned only inside `if` blocks
/// but referenced later in the same function.  Approximates the Python AST
/// UnboundLocalError detection from the original Python validator.
pub fn check_variable_scoping(code: &str, fname: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    // Find variables assigned inside `if` blocks (4-8 space indent followed by assignment).
    // Pattern: lines that are inside an if block (indented >=8 spaces) and contain `var = ...`
    let if_assign_re =
        regex::Regex::new(r"(?m)^(\s{8,})(\w+)\s*=\s*[^=]").expect("if assign regex");
    // Find top-level function assignments (exactly 4 spaces indent).
    let top_assign_re =
        regex::Regex::new(r"(?m)^    (\w+)\s*=\s*[^=]").expect("top assign regex");
    // Find variable loads: word used in non-assignment context.
    let use_re = regex::Regex::new(r"\b(\w+)\b").expect("use regex");

    let lines: Vec<&str> = code.lines().collect();

    // Find if-only assigned vars with their line numbers.
    let mut if_only: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for cap in if_assign_re.captures_iter(code) {
        let var = cap[2].to_string();
        // Find which line this match is on.
        let match_offset = cap.get(0).unwrap().start();
        let line_no = code[..match_offset].chars().filter(|&c| c == '\n').count() + 1;
        if_only.insert(var, line_no as u32);
    }

    // Remove vars that are also assigned at top-level (4 spaces).
    for cap in top_assign_re.captures_iter(code) {
        if_only.remove(&cap[1].to_string());
    }

    // For each if-only var, check if it's used later.
    for (var, def_line) in &if_only {
        for (idx, line) in lines.iter().enumerate() {
            let lineno = (idx + 1) as u32;
            if lineno <= *def_line {
                continue;
            }
            // Skip lines that are themselves assignments to this var.
            let stripped = line.trim();
            if stripped.starts_with(&format!("{var} ="))
                || stripped.starts_with(&format!("{var}="))
            {
                continue;
            }
            if use_re.is_match(line) {
                // Check the var appears as a standalone word (not a substring).
                let word_re = regex::Regex::new(&format!(r"\b{}\b", regex::escape(var)))
                    .expect("word regex");
                if word_re.is_match(line) {
                    warnings.push(format!(
                        "[{fname}:{def_line}] Variable '{var}' is assigned only inside \
                         an if-branch but used at line {lineno} — will cause UnboundLocalError \
                         if the branch is not taken"
                    ));
                    break;
                }
            }
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// auto_fix_unbound_locals
// ---------------------------------------------------------------------------

/// Automatically fix common unbound local patterns by inserting `var = None`
/// before the if-statement that first assigns the variable.
///
/// Returns `(fixed_code, num_fixes)`.
pub fn auto_fix_unbound_locals(code: &str) -> (String, usize) {
    // Find if-only assigned vars (same logic as check_variable_scoping).
    let if_assign_re =
        regex::Regex::new(r"(?m)^(\s{8,})(\w+)\s*=\s*[^=]").expect("if assign regex");
    let top_assign_re =
        regex::Regex::new(r"(?m)^    (\w+)\s*=\s*[^=]").expect("top assign regex");

    let mut if_only: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for cap in if_assign_re.captures_iter(code) {
        let var = cap[2].to_string();
        let match_offset = cap.get(0).unwrap().start();
        let line_no = code[..match_offset].chars().filter(|&c| c == '\n').count() + 1;
        if_only.entry(var).or_insert(line_no as u32);
    }
    for cap in top_assign_re.captures_iter(code) {
        if_only.remove(&cap[1].to_string());
    }

    if if_only.is_empty() {
        return (code.to_string(), 0);
    }

    // Find the enclosing `if` line for each var (the if that starts the block).
    let if_line_re = regex::Regex::new(r"(?m)^( {4,})if\s+").expect("if line regex");
    let mut if_lines: Vec<(u32, String)> = Vec::new(); // (lineno, indent)
    for cap in if_line_re.captures_iter(code) {
        let match_offset = cap.get(0).unwrap().start();
        let line_no = (code[..match_offset].chars().filter(|&c| c == '\n').count() + 1) as u32;
        let indent = cap[1].to_string();
        if_lines.push((line_no, indent));
    }

    let mut lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
    // Collect insertions: (line_index, text_to_insert_before)
    let mut insertions: Vec<(usize, String)> = Vec::new();

    let use_word_re = |var: &str| regex::Regex::new(&format!(r"\b{}\b", regex::escape(var)));

    for (var, def_line) in &if_only {
        // Find if var is used later.
        let used_later = lines
            .iter()
            .enumerate()
            .skip(*def_line as usize)
            .any(|(_, l)| {
                let s = l.trim();
                !s.starts_with(&format!("{var} =")) && !s.starts_with(&format!("{var}="))
                    && use_word_re(var).map(|re| re.is_match(l)).unwrap_or(false)
            });
        if !used_later {
            continue;
        }

        // Find the nearest enclosing if statement before def_line.
        let if_lineno = if_lines
            .iter()
            .filter(|(l, _)| *l < *def_line)
            .map(|(l, _)| *l)
            .max();
        let if_lineno = match if_lineno {
            Some(l) => l as usize,
            None => continue,
        };
        // Determine indent from the if line.
        let if_line_str = &lines[if_lineno - 1];
        let indent: String = if_line_str
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let fix = format!("{indent}{var} = None");
        insertions.push((if_lineno - 1, fix));
    }

    if insertions.is_empty() {
        return (code.to_string(), 0);
    }

    // Deduplicate and sort in reverse order to keep indices stable.
    insertions.sort_by(|a, b| b.0.cmp(&a.0));
    insertions.dedup_by_key(|i| (i.0, i.1.clone()));
    let num_fixes = insertions.len();

    for (idx, fix_line) in &insertions {
        lines.insert(*idx, fix_line.clone());
    }

    (lines.join("\n"), num_fixes)
}

// ---------------------------------------------------------------------------
// check_api_correctness
// ---------------------------------------------------------------------------

/// Detect common API misuse patterns in experiment code.
///
/// Catches:
/// - `np.erf()` (should be `scipy.special.erf`)
/// - `.ptp()` removed in NumPy 2.0
/// - Removed NumPy type aliases (`np.bool`, `np.int`, etc.)
/// - Hardcoded `RandomState` seeds inside loops
/// - `from X import Y` then calling `X.Y()` (NameError)
pub fn check_api_correctness(code: &str, fname: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    for (lineno, line) in code.lines().enumerate() {
        let lineno = lineno + 1;
        let stripped = line.trim();
        if stripped.starts_with('#') {
            continue;
        }

        // np.erf() does not exist.
        if regex::Regex::new(r"\bnp\.erf\b")
            .map(|re| re.is_match(stripped))
            .unwrap_or(false)
        {
            warnings.push(format!(
                "[{fname}:{lineno}] np.erf() does not exist — use \
                 scipy.special.erf() or math.erf() instead"
            ));
        }

        // ndarray.ptp() removed in NumPy 2.0.
        if regex::Regex::new(r"\.ptp\s*\(")
            .map(|re| re.is_match(stripped))
            .unwrap_or(false)
        {
            warnings.push(format!(
                "[{fname}:{lineno}] ndarray.ptp() was removed in NumPy 2.0 — \
                 use np.ptp(arr) or arr.max() - arr.min() instead"
            ));
        }

        // Removed NumPy type aliases.
        for old_alias in &["np.bool", "np.int", "np.float", "np.complex", "np.object", "np.str"] {
            // Use a simple contains-then-word-boundary check:
            // np.bool is followed by anything that isn't a word char or underscore.
            if stripped.contains(old_alias) {
                let pattern = format!(r"(?:^|[^.\w]){}(?:[^_\w\d]|$)", regex::escape(old_alias));
                let matched = regex::Regex::new(&pattern)
                    .map(|re| re.is_match(stripped))
                    .unwrap_or_else(|_| {
                        // Fallback: simple contains with no false-positive guard
                        stripped.contains(old_alias)
                            && !stripped.contains(&format!("{old_alias}_"))
                    });
                if matched {
                    warnings.push(format!(
                        "[{fname}:{lineno}] {old_alias} was removed in NumPy 2.0 — \
                         use {old_alias}_ or Python builtin instead"
                    ));
                }
            }
        }

        // Hardcoded RandomState seed inside non-def lines.
        if regex::Regex::new(r"RandomState\(\s*\d+\s*\)")
            .map(|re| re.is_match(stripped))
            .unwrap_or(false)
            && !stripped.contains("def ")
        {
            warnings.push(format!(
                "[{fname}:{lineno}] Hardcoded RandomState seed inside a loop/function \
                 may produce identical results across calls — pass seed as parameter"
            ));
        }
    }

    // Import-usage mismatch detection.
    // Build from-import map: module -> {imported names}
    let from_re =
        regex::Regex::new(r"(?m)^from\s+([\w.]+)\s+import\s+(.+)$").expect("from import regex");
    let import_re = regex::Regex::new(r"(?m)^import\s+([\w.]+)").expect("import regex");

    let mut import_from_map: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    let mut import_module_set: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for cap in from_re.captures_iter(code) {
        let module = cap[1].to_string();
        let names_str = &cap[2];
        let names: std::collections::HashSet<String> = names_str
            .split(',')
            .map(|n| {
                let n = n.trim();
                // Handle `name as alias` — use the alias.
                if let Some(idx) = n.find(" as ") {
                    n[idx + 4..].trim().to_string()
                } else {
                    n.to_string()
                }
            })
            .collect();
        import_from_map.entry(module).or_default().extend(names);
    }

    for cap in import_re.captures_iter(code) {
        let top = cap[1].split('.').next().unwrap_or("").to_string();
        import_module_set.insert(top);
    }

    for (lineno, line) in code.lines().enumerate() {
        let lineno = lineno + 1;
        let stripped = line.trim();
        if stripped.starts_with('#') {
            continue;
        }
        for (module, names) in &import_from_map {
            let top_mod = module.split('.').next().unwrap_or(module);
            if import_module_set.contains(top_mod) {
                continue;
            }
            for name in names {
                let pattern = format!(r"{}\.{}\s*\(", regex::escape(module), regex::escape(name));
                if regex::Regex::new(&pattern)
                    .map(|re| re.is_match(stripped))
                    .unwrap_or(false)
                {
                    warnings.push(format!(
                        "[{fname}:{lineno}] Import-usage mismatch: '{name}' was imported \
                         via `from {module} import {name}` but called as `{module}.{name}()` \
                         — this will raise NameError. Use `{name}()` directly."
                    ));
                }
            }
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// check_try_except_usage
// ---------------------------------------------------------------------------

/// Detect try/except blocks in experiment code.
///
/// Generated experiment code should let errors crash with full tracebacks.
/// Uses regex heuristics (not AST) to detect `try:` blocks and their handler types.
pub fn check_try_except_usage(code: &str, fname: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    let try_re = regex::Regex::new(r"(?m)^(\s*)try\s*:").expect("try regex");
    let except_re = regex::Regex::new(r"(?m)^\s*except\s*([^:]*):").expect("except regex");

    for cap in try_re.captures_iter(code) {
        let match_start = cap.get(0).unwrap().start();
        let lineno = code[..match_start].chars().filter(|&c| c == '\n').count() + 1;
        let indent = &cap[1];

        // Collect handler types from except clauses after this try:.
        // Scan until we find an unindented line or EOF.
        let after_try = &code[match_start..];
        let mut handler_types: Vec<String> = Vec::new();
        for exc_cap in except_re.captures_iter(after_try) {
            let exc_indent_len = exc_cap
                .get(0)
                .unwrap()
                .as_str()
                .chars()
                .take_while(|c| c.is_whitespace())
                .count();
            if exc_indent_len != indent.len() {
                continue;
            }
            let exc_type = exc_cap[1].trim();
            if exc_type.is_empty() {
                handler_types.push("bare except".to_string());
            } else {
                // Strip `as e` suffix.
                let t = if let Some(idx) = exc_type.find(" as ") {
                    &exc_type[..idx]
                } else {
                    exc_type
                };
                handler_types.push(t.trim().to_string());
            }
            break; // Only need the first handler for a summary.
        }

        warnings.push(format!(
            "[{fname}:{lineno}] try/except block detected \
             (catches: {}). Remove try/except and let errors crash with full tracebacks \
             — the sanity check will diagnose and fix root causes.",
            if handler_types.is_empty() {
                "unknown".to_string()
            } else {
                handler_types.join(", ")
            }
        ));
    }

    warnings
}

// ---------------------------------------------------------------------------
// deep_validate_files
// ---------------------------------------------------------------------------

/// Run all deep quality checks across all experiment files.
///
/// Returns a list of warning strings. Empty = no concerns.
pub fn deep_validate_files(files: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut warnings = Vec::new();
    warnings.extend(check_class_quality(files));
    for (fname, code) in files {
        if !fname.ends_with(".py") {
            continue;
        }
        warnings.extend(check_variable_scoping(code, fname));
        warnings.extend(check_api_correctness(code, fname));
        warnings.extend(check_try_except_usage(code, fname));
    }
    warnings
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

    // --- format_issues_for_llm ---

    #[test]
    fn test_format_issues_for_llm_empty() {
        assert_eq!(format_issues_for_llm(&[]), "No issues found.");
    }

    #[test]
    fn test_format_issues_for_llm_with_issues() {
        let issues = vec![
            ValidationIssue {
                severity: Severity::Error,
                category: Category::Syntax,
                message: "bad syntax".to_string(),
                line: Some(3),
            },
            ValidationIssue {
                severity: Severity::Warning,
                category: Category::Import,
                message: "unknown module".to_string(),
                line: None,
            },
        ];
        let report = format_issues_for_llm(&issues);
        assert!(report.contains("[ERROR]"));
        assert!(report.contains("bad syntax"));
        assert!(report.contains("line 3"));
        assert!(report.contains("[WARNING]"));
        assert!(report.contains("unknown module"));
        assert!(report.contains("unknown location"));
    }

    // --- check_code_complexity ---

    #[test]
    fn test_complexity_too_few_lines() {
        let code = "x = 1\n";
        let warnings = check_code_complexity(code);
        assert!(warnings.iter().any(|w| w.contains("effective lines")));
    }

    #[test]
    fn test_complexity_no_function_defs() {
        let code = "x = 1\ny = 2\nz = 3\na = 4\nb = 5\nc = 6\n";
        let warnings = check_code_complexity(code);
        assert!(warnings.iter().any(|w| w.contains("function definitions")));
    }

    #[test]
    fn test_complexity_hardcoded_metric() {
        let code = "metric = 0.987654\n";
        let warnings = check_code_complexity(code);
        assert!(warnings.iter().any(|w| w.contains("hardcoded metric")));
    }

    #[test]
    fn test_complexity_trivial_computation() {
        let code = "result = sum(x**2)\n";
        let warnings = check_code_complexity(code);
        assert!(warnings.iter().any(|w| w.contains("Trivial computation")));
    }

    // --- check_main_entry_point ---

    #[test]
    fn test_main_entry_missing_main_func() {
        let code = "x = 1\nprint(x)\n";
        let warnings = check_main_entry_point(code);
        assert!(warnings.iter().any(|w| w.contains("Missing main()")));
    }

    #[test]
    fn test_main_entry_missing_guard() {
        let code = "def main():\n    pass\n";
        let warnings = check_main_entry_point(code);
        assert!(warnings.iter().any(|w| w.contains("Missing entry point")));
    }

    #[test]
    fn test_main_entry_ok() {
        let code = "def main():\n    x = 1\n    y = 2\n    z = x + y\n    return z\n\nif __name__ == '__main__':\n    main()\n";
        let warnings = check_main_entry_point(code);
        // Should not flag missing main func or entry guard.
        assert!(!warnings.iter().any(|w| w.contains("Missing main()")));
        assert!(!warnings.iter().any(|w| w.contains("Missing entry point")));
    }

    // --- check_class_quality ---

    #[test]
    fn test_class_quality_empty_class() {
        let mut files = std::collections::HashMap::new();
        files.insert(
            "model.py".to_string(),
            "class Empty:\n    pass\n".to_string(),
        );
        let warnings = check_class_quality(&files);
        assert!(warnings.iter().any(|w| w.contains("body lines")));
    }

    // --- check_variable_scoping ---

    #[test]
    fn test_variable_scoping_detects_unbound() {
        let code = "def train():\n    if flag:\n        result = compute()\n    print(result)\n";
        let warnings = check_variable_scoping(code, "main.py");
        assert!(warnings.iter().any(|w| w.contains("result")));
    }

    #[test]
    fn test_variable_scoping_no_issue_when_top_assigned() {
        let code = "def train():\n    result = None\n    if flag:\n        result = compute()\n    print(result)\n";
        let warnings = check_variable_scoping(code, "main.py");
        assert!(warnings.is_empty());
    }

    // --- auto_fix_unbound_locals ---

    #[test]
    fn test_auto_fix_no_changes_on_clean_code() {
        let code = "def train():\n    x = 1\n    print(x)\n";
        let (fixed, n) = auto_fix_unbound_locals(code);
        assert_eq!(n, 0);
        assert_eq!(fixed, code);
    }

    // --- check_api_correctness ---

    #[test]
    fn test_api_np_erf() {
        let code = "result = np.erf(x)\n";
        let warnings = check_api_correctness(code, "main.py");
        assert!(warnings.iter().any(|w| w.contains("np.erf()")));
    }

    #[test]
    fn test_api_numpy_removed_aliases() {
        let code = "x = np.bool(1)\n";
        let warnings = check_api_correctness(code, "main.py");
        assert!(warnings.iter().any(|w| w.contains("np.bool")));
    }

    #[test]
    fn test_api_ptp_removed() {
        let code = "v = arr.ptp()\n";
        let warnings = check_api_correctness(code, "main.py");
        assert!(warnings.iter().any(|w| w.contains("ptp()")));
    }

    #[test]
    fn test_api_import_mismatch() {
        let code = "from scipy.special import erf\nresult = scipy.special.erf(x)\n";
        let warnings = check_api_correctness(code, "main.py");
        assert!(warnings.iter().any(|w| w.contains("Import-usage mismatch")));
    }

    // --- check_try_except_usage ---

    #[test]
    fn test_try_except_detected() {
        let code = "def run():\n    try:\n        risky()\n    except Exception:\n        pass\n";
        let warnings = check_try_except_usage(code, "main.py");
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("try/except block detected"));
    }

    #[test]
    fn test_try_except_no_false_positive() {
        let code = "def run():\n    x = 1\n    return x\n";
        let warnings = check_try_except_usage(code, "main.py");
        assert!(warnings.is_empty());
    }

    // --- deep_validate_files ---

    #[test]
    fn test_deep_validate_files_aggregates() {
        let mut files = std::collections::HashMap::new();
        files.insert(
            "main.py".to_string(),
            "def run():\n    try:\n        pass\n    except Exception:\n        pass\n".to_string(),
        );
        let warnings = deep_validate_files(&files);
        // Should include at least the try/except warning.
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("try/except")));
    }
}
