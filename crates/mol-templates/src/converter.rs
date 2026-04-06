//! Markdown-to-LaTeX converter for Mol-HEP-Lab.
//!
//! Converts CommonMark-flavoured Markdown into LaTeX suitable for
//! conference paper submissions.  Handles:
//!
//! - ATX headings (`#` … `######`) → `\section`, `\subsection`, …
//! - Bold (`**text**`) → `\textbf{}`
//! - Italic (`*text*` / `_text_`) → `\textit{}`
//! - Inline code (`` `code` ``) → `\texttt{}`
//! - Fenced code blocks (` ``` `) → `lstlisting` environment
//! - Unordered lists (`-` / `*`) → `itemize`
//! - Ordered lists (`1.` …) → `enumerate`
//! - GFM-style tables → `tabular`
//! - Citations (`[@key]` or `\cite{key}` pass-through) → `\cite{}`
//! - Figure references (`Figure~\ref{fig:label}` pass-through or `[fig:label]`)
//! - LaTeX special character escaping

use regex::Regex;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert a Markdown string to LaTeX source.
///
/// The output does **not** include a preamble or `\begin{document}`;
/// it is intended to be spliced into the body of a document produced by
/// [`crate::conferences::ConferenceTemplate::render_skeleton`].
pub fn markdown_to_latex(md: &str) -> String {
    let mut out = String::with_capacity(md.len() * 2);
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // --- Fenced code block ---
        if line.trim_start().starts_with("```") {
            let (block, consumed) = consume_code_block(&lines, i);
            out.push_str(&block);
            i += consumed;
            continue;
        }

        // --- GFM table (header | separator | rows) ---
        if is_table_header(line, lines.get(i + 1).copied()) {
            let (table, consumed) = consume_table(&lines, i);
            out.push_str(&table);
            i += consumed;
            continue;
        }

        // --- Unordered list ---
        if is_unordered_item(line) {
            let (list, consumed) = consume_unordered_list(&lines, i);
            out.push_str(&list);
            i += consumed;
            continue;
        }

        // --- Ordered list ---
        if is_ordered_item(line) {
            let (list, consumed) = consume_ordered_list(&lines, i);
            out.push_str(&list);
            i += consumed;
            continue;
        }

        // --- ATX heading ---
        if line.starts_with('#') {
            out.push_str(&convert_heading(line));
            out.push('\n');
            i += 1;
            continue;
        }

        // --- Blank line ---
        if line.trim().is_empty() {
            out.push('\n');
            i += 1;
            continue;
        }

        // --- Regular paragraph line ---
        out.push_str(&convert_inline(line));
        out.push('\n');
        i += 1;
    }

    out
}

// ---------------------------------------------------------------------------
// Heading conversion
// ---------------------------------------------------------------------------

fn convert_heading(line: &str) -> String {
    let level = line.chars().take_while(|&c| c == '#').count();
    let text = line[level..].trim();
    let converted = convert_inline(text);
    match level {
        1 => format!("\\section{{{converted}}}"),
        2 => format!("\\subsection{{{converted}}}"),
        3 => format!("\\subsubsection{{{converted}}}"),
        4 => format!("\\paragraph{{{converted}}}"),
        5 => format!("\\subparagraph{{{converted}}}"),
        _ => format!("\\textbf{{{converted}}}"),
    }
}

// ---------------------------------------------------------------------------
// Inline conversion
// ---------------------------------------------------------------------------

/// Convert inline Markdown markup to LaTeX within a single text span.
pub(crate) fn convert_inline(text: &str) -> String {
    let s = escape_latex_specials(text);

    // Bold (**text** or __text__)
    let bold_re = Regex::new(r"\*\*(.+?)\*\*|__(.+?)__").unwrap();
    let s = bold_re.replace_all(&s, |caps: &regex::Captures| {
        let inner = caps.get(1).or(caps.get(2)).map_or("", |m| m.as_str());
        format!("\\textbf{{{inner}}}")
    });

    // Italic (*text*) — single asterisk, must run after bold
    let italic_star_re = Regex::new(r"\*([^*]+)\*").unwrap();
    let s = italic_star_re.replace_all(&s, |caps: &regex::Captures| {
        format!("\\textit{{{}}}", &caps[1])
    });

    // Italic (_text_) — underscore variant, with boundary via surrounding chars.
    // Pattern: (start-of-string or non-alphanumeric) _ content _ (non-alphanumeric or end).
    // We capture the boundary chars and re-emit them to avoid lookaround.
    let italic_under_re =
        Regex::new(r"(^|[^a-zA-Z0-9\\])_([^_]+)_((?:[^a-zA-Z0-9])|$)").unwrap();
    let s = italic_under_re.replace_all(&s, |caps: &regex::Captures| {
        let before = caps.get(1).map_or("", |m| m.as_str());
        let inner = caps.get(2).map_or("", |m| m.as_str());
        let after = caps.get(3).map_or("", |m| m.as_str());
        format!("{before}\\textit{{{inner}}}{after}")
    });

    // Inline code (`code`)
    let code_re = Regex::new(r"`([^`]+)`").unwrap();
    let s = code_re.replace_all(&s, |caps: &regex::Captures| {
        let inner = &caps[1];
        format!("\\texttt{{{inner}}}")
    });

    // Citations: [@key] or [@key1; @key2]
    let cite_re = Regex::new(r"\[@([^\]]+)\]").unwrap();
    let s = cite_re.replace_all(&s, |caps: &regex::Captures| {
        // Handle multiple keys separated by "; @"
        let raw = &caps[1];
        let keys: String = raw
            .split(';')
            .map(|k| k.trim().trim_start_matches('@').trim())
            .collect::<Vec<_>>()
            .join(", ");
        format!("\\cite{{{keys}}}")
    });

    // Figure references: [fig:label] → Figure~\ref{fig:label}
    let figref_re = Regex::new(r"\[(fig:[^\]]+)\]").unwrap();
    let s = figref_re.replace_all(&s, |caps: &regex::Captures| {
        format!("Figure~\\ref{{{}}}", &caps[1])
    });

    // Table references: [tab:label] → Table~\ref{tab:label}
    let tabref_re = Regex::new(r"\[(tab:[^\]]+)\]").unwrap();
    let s = tabref_re.replace_all(&s, |caps: &regex::Captures| {
        format!("Table~\\ref{{{}}}", &caps[1])
    });

    // Equation references: [eq:label] → Equation~\ref{eq:label}
    let eqref_re = Regex::new(r"\[(eq:[^\]]+)\]").unwrap();
    let s = eqref_re.replace_all(&s, |caps: &regex::Captures| {
        format!("Equation~\\ref{{{}}}", &caps[1])
    });

    s.into_owned()
}

// ---------------------------------------------------------------------------
// LaTeX special character escaping
// ---------------------------------------------------------------------------

/// Escape characters that have special meaning in LaTeX.
///
/// Does **not** escape backslash (`\`) because the input may already contain
/// LaTeX commands (e.g. math mode).  The function is intentionally
/// conservative to avoid double-escaping.
fn escape_latex_specials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        // Skip already-escaped sequences (backslash + next char)
        if c == '\\' {
            out.push(c);
            i += 1;
            if i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        // Skip math spans ($…$) verbatim
        if c == '$' {
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != '$' {
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[i]); // closing $
                i += 1;
            }
            continue;
        }
        match c {
            '%' => out.push_str("\\%"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '^' => out.push_str("\\^{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Fenced code blocks
// ---------------------------------------------------------------------------

fn consume_code_block(lines: &[&str], start: usize) -> (String, usize) {
    let fence_line = lines[start].trim_start();
    let lang = fence_line
        .trim_start_matches('`')
        .trim_start_matches('~')
        .trim()
        .to_owned();

    let lang_opt = if lang.is_empty() {
        String::new()
    } else {
        format!("[language={lang}]")
    };

    let mut block = format!("\\begin{{lstlisting}}{lang_opt}\n");
    let mut i = start + 1;

    while i < lines.len() {
        let l = lines[i];
        if l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~") {
            i += 1; // consume closing fence
            break;
        }
        block.push_str(l);
        block.push('\n');
        i += 1;
    }

    block.push_str("\\end{lstlisting}\n");
    let consumed = i - start;
    (block, consumed)
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

fn is_unordered_item(line: &str) -> bool {
    let t = line.trim_start();
    (t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ "))
        && !t.starts_with("---")
}

fn is_ordered_item(line: &str) -> bool {
    let t = line.trim_start();
    let re = Regex::new(r"^\d+\. ").unwrap();
    re.is_match(t)
}

fn consume_unordered_list(lines: &[&str], start: usize) -> (String, usize) {
    let mut block = "\\begin{itemize}\n".to_owned();
    let mut i = start;
    while i < lines.len() && is_unordered_item(lines[i]) {
        let content = lines[i].trim_start().splitn(2, ' ').nth(1).unwrap_or("");
        block.push_str(&format!("  \\item {}\n", convert_inline(content)));
        i += 1;
    }
    block.push_str("\\end{itemize}\n");
    (block, i - start)
}

fn consume_ordered_list(lines: &[&str], start: usize) -> (String, usize) {
    let mut block = "\\begin{enumerate}\n".to_owned();
    let re = Regex::new(r"^\d+\. ").unwrap();
    let mut i = start;
    while i < lines.len() && is_ordered_item(lines[i]) {
        let trimmed = lines[i].trim_start();
        let content = re.splitn(trimmed, 2).nth(1).unwrap_or(trimmed);
        block.push_str(&format!("  \\item {}\n", convert_inline(content)));
        i += 1;
    }
    block.push_str("\\end{enumerate}\n");
    (block, i - start)
}

// ---------------------------------------------------------------------------
// GFM tables
// ---------------------------------------------------------------------------

fn is_table_header(line: &str, next: Option<&str>) -> bool {
    line.contains('|')
        && next
            .map(|n| n.trim().chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')))
            .unwrap_or(false)
}

fn consume_table(lines: &[&str], start: usize) -> (String, usize) {
    // Parse header
    let header_cols: Vec<String> = split_table_row(lines[start]);
    let col_count = header_cols.len();
    let col_spec = "l ".repeat(col_count).trim().to_owned();

    let mut block = format!("\\begin{{tabular}}{{{col_spec}}}\n\\toprule\n");

    // Header row
    let header_row = header_cols
        .iter()
        .map(|c| convert_inline(c))
        .collect::<Vec<_>>()
        .join(" & ");
    block.push_str(&format!("{header_row} \\\\\n\\midrule\n"));

    // Skip separator row (index start+1)
    let mut i = start + 2;

    // Data rows
    while i < lines.len() && lines[i].contains('|') {
        let cols = split_table_row(lines[i]);
        // Pad or truncate to col_count
        let mut cells: Vec<String> = cols.iter().map(|c| convert_inline(c)).collect();
        cells.resize(col_count, String::new());
        block.push_str(&format!("{} \\\\\n", cells.join(" & ")));
        i += 1;
    }

    block.push_str("\\bottomrule\n\\end{tabular}\n");
    (block, i - start)
}

fn split_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|c| c.trim().to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_levels() {
        assert_eq!(convert_heading("# Hello World"), r"\section{Hello World}");
        assert_eq!(
            convert_heading("## Sub"),
            r"\subsection{Sub}"
        );
        assert_eq!(
            convert_heading("### Sub sub"),
            r"\subsubsection{Sub sub}"
        );
    }

    #[test]
    fn bold_and_italic() {
        let out = convert_inline("This is **bold** and *italic*.");
        assert!(out.contains(r"\textbf{bold}"));
        assert!(out.contains(r"\textit{italic}"));
    }

    #[test]
    fn inline_code() {
        let out = convert_inline("Use `cargo build` here.");
        assert!(out.contains(r"\texttt{cargo build}"));
    }

    #[test]
    fn citation_single() {
        let out = convert_inline("See [@smith2024].");
        assert!(out.contains(r"\cite{smith2024}"));
    }

    #[test]
    fn citation_multi() {
        let out = convert_inline("See [@a; @b].");
        assert!(out.contains(r"\cite{a, b}"));
    }

    #[test]
    fn figure_reference() {
        let out = convert_inline("As shown in [fig:results].");
        assert!(out.contains(r"Figure~\ref{fig:results}"));
    }

    #[test]
    fn special_chars() {
        let out = escape_latex_specials("50% cost & speed");
        assert!(out.contains(r"\%"));
        assert!(out.contains(r"\&"));
    }

    #[test]
    fn full_document_smoke() {
        let md = "# Introduction\n\nThis is a paragraph with **bold**.\n";
        let latex = markdown_to_latex(md);
        assert!(latex.contains(r"\section{Introduction}"));
        assert!(latex.contains(r"\textbf{bold}"));
    }
}
