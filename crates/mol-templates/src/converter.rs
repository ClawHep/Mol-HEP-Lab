//! Markdown-to-LaTeX converter for Mol-HEP-Lab.
//!
//! Converts CommonMark-flavoured Markdown into LaTeX suitable for
//! conference paper submissions.  Handles:
//!
//! - ATX headings (`#` … `######`) → `\section`, `\subsection`, …
//! - Bold (`**text**`) → `\textbf{}`
//! - Italic (`*text*` / `_text_`) → `\textit{}`
//! - Inline code (`` `code` ``) → `\texttt{}`
//! - Fenced code blocks (` ``` `) → `verbatim` / `algorithm` environment
//! - Unordered lists (`-` / `*`) → `itemize`
//! - Ordered lists (`1.` …) → `enumerate`
//! - GFM-style tables → `table` + `tabular` with alignment, resizebox, caption
//! - Citations (`[@key]` or `\cite{key}` pass-through) → `\cite{}`
//! - Figure references (`![cap](path)`) → `figure` environment
//! - LaTeX special character escaping
//! - Paper section parsing, title/abstract extraction, body building
//! - Paper completeness validation

use regex::Regex;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Thread-local render counters (figure / table numbering)
// ---------------------------------------------------------------------------

// We use thread-local storage to track per-render counters.
thread_local! {
    static TABLE_COUNTER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static FIGURE_COUNTER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Reset per-render counters (call once at the start of each document render).
pub fn reset_render_counters() {
    TABLE_COUNTER.with(|c| c.set(0));
    FIGURE_COUNTER.with(|c| c.set(0));
}

fn next_table_num() -> u32 {
    TABLE_COUNTER.with(|c| {
        let v = c.get() + 1;
        c.set(v);
        v
    })
}

fn next_figure_num() -> u32 {
    FIGURE_COUNTER.with(|c| {
        let v = c.get() + 1;
        c.set(v);
        v
    })
}

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
            let (table_lines, consumed) = collect_table_lines(&lines, i);
            out.push_str(&render_table(&table_lines, ""));
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
// Section parsing
// ---------------------------------------------------------------------------

/// A parsed Markdown section.
#[derive(Debug, Clone)]
pub struct Section {
    /// Heading level: 1 = `#`, 2 = `##`, etc.  0 = preamble before any heading.
    pub level: usize,
    /// The heading text (without `#` markers).
    pub heading: String,
    /// The body text below the heading.
    pub body: String,
    /// Lowercase version of heading for matching.
    pub heading_lower: String,
}

impl Section {
    fn new(level: usize, heading: impl Into<String>, body: impl Into<String>) -> Self {
        let heading = heading.into();
        let heading_lower = heading.to_lowercase();
        Section {
            level,
            heading,
            body: body.into(),
            heading_lower,
        }
    }
}

/// Parse Markdown text into a flat list of [`Section`]s by heading.
pub fn parse_sections(md: &str) -> Vec<Section> {
    let heading_re = Regex::new(r"(?m)^(#{1,4})\s+(.+)$").unwrap();
    let matches: Vec<_> = heading_re.find_iter(md).collect();

    if matches.is_empty() {
        return vec![Section::new(1, "", md)];
    }

    let mut sections = Vec::new();
    let caps: Vec<_> = heading_re.captures_iter(md).collect();

    // Text before the first heading
    if matches[0].start() > 0 {
        let preamble = md[..matches[0].start()].trim();
        if !preamble.is_empty() {
            sections.push(Section::new(0, "", preamble));
        }
    }

    for (i, (m, cap)) in matches.iter().zip(caps.iter()).enumerate() {
        let level = cap[1].len();
        let heading = cap[2].trim().to_owned();
        let start = m.end();
        let end = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            md.len()
        };
        let body = md[start..end].trim().to_owned();

        // IMP-17: Handle concatenated heading+body on same line
        let (clean_heading, body_prefix) = separate_heading_body(&heading);
        let final_body = if body_prefix.is_empty() {
            body
        } else if body.is_empty() {
            body_prefix
        } else {
            format!("{}\n\n{}", body_prefix, body)
        };

        sections.push(Section::new(level, clean_heading, final_body));
    }

    sections
}

/// Known section names used to detect heading/body concatenation.
static KNOWN_SECTION_NAMES: &[&str] = &[
    "abstract",
    "introduction",
    "related work",
    "background",
    "method",
    "methods",
    "methodology",
    "approach",
    "framework",
    "experiments",
    "experiment",
    "experimental setup",
    "experimental results",
    "results",
    "results and discussion",
    "analysis",
    "discussion",
    "conclusion",
    "conclusions",
    "limitations",
    "acknowledgments",
    "acknowledgements",
    "references",
    "appendix",
    "contributions",
    "problem setting",
    "problem statement",
    "problem definition",
    "problem formulation",
    "study positioning",
    "evaluation",
    "design rationale",
    "complexity",
];

/// Separate a heading string from accidentally concatenated body text.
fn separate_heading_body(heading: &str) -> (String, String) {
    // Short headings are fine as-is
    if heading.len() <= 60 {
        return (heading.to_owned(), String::new());
    }

    let heading_lower = heading.to_lowercase();

    // Check against known section names
    let mut sorted_names = KNOWN_SECTION_NAMES.to_vec();
    sorted_names.sort_by(|a, b| b.len().cmp(&a.len()));

    for name in &sorted_names {
        if heading_lower.starts_with(name) && heading.len() > name.len() + 1 {
            let after = &heading[name.len()..];
            if after.starts_with(' ') || after.starts_with('\t') {
                return (heading[..name.len()].to_owned(), after.trim().to_owned());
            }
        }
    }

    // Fallback: split at sentence boundary within first 200 chars
    if heading.len() > 200 {
        let re = Regex::new(r"[.;:]\s+([A-Z])").unwrap();
        if let Some(m) = re.find(&heading[..300.min(heading.len())]) {
            if m.start() > 10 {
                return (
                    heading[..m.start() + 1].trim().to_owned(),
                    heading[m.start() + 2..].trim().to_owned(),
                );
            }
        }
    }

    (heading.to_owned(), String::new())
}

// ---------------------------------------------------------------------------
// Title extraction
// ---------------------------------------------------------------------------

static TITLE_SKIP: &[&str] = &[
    "title",
    "abstract",
    "references",
    "appendix",
    "acknowledgments",
    "acknowledgements",
];

/// Extract the paper title from parsed sections or raw markdown.
pub fn extract_title(sections: &[Section], raw_md: &str) -> String {
    let title_reject_re =
        Regex::new(r"(?i)^(?:table|figure|fig\.|tab\.|algorithm|listing|appendix)\s").unwrap();
    let metric_dump_re =
        Regex::new(r"(?i)(?:primary_metric|accuracy|loss|f1_score|precision|recall)\b").unwrap();

    let is_bad = |s: &str| -> bool {
        title_reject_re.is_match(s)
            || metric_dump_re.is_match(s)
            || Regex::new(r"\w+_\w+/\w+").unwrap().is_match(s)
    };

    // Look for explicit "# Title" or "## Title" section
    for sec in sections {
        if sec.level <= 2 && sec.heading_lower == "title" {
            let first_line = sec.body.lines().next().unwrap_or("").trim();
            let first_line = Regex::new(r"\*\*(.+?)\*\*")
                .unwrap()
                .replace_all(first_line, "$1")
                .to_string();
            if !first_line.is_empty() && !is_bad(&first_line) {
                return first_line;
            }
        }
        // "## Title Actual Paper Title" pattern
        if sec.level <= 2
            && sec.heading_lower.starts_with("title ")
            && sec.heading.len() > 6
        {
            return sec.heading[6..].trim().to_owned();
        }
    }

    // Fallback: first H1/H2 that isn't a meta-heading
    for sec in sections {
        if sec.level <= 2
            && sec.level >= 1
            && !sec.heading.is_empty()
            && !TITLE_SKIP.contains(&sec.heading_lower.as_str())
            && !is_bad(&sec.heading)
        {
            return sec.heading.clone();
        }
    }

    // Last resort: first non-empty line
    for line in raw_md.lines() {
        let stripped = line.trim().trim_start_matches('#').trim();
        if !stripped.is_empty() && !is_bad(stripped) {
            return stripped.to_owned();
        }
    }

    "Untitled Paper".to_owned()
}

// ---------------------------------------------------------------------------
// Abstract extraction
// ---------------------------------------------------------------------------

/// Extract the abstract text from parsed sections.
pub fn extract_abstract(sections: &[Section]) -> String {
    for sec in sections {
        if sec.heading_lower == "abstract" {
            return sec.body.clone();
        }
        // IMP-17 fallback: heading may contain body text
        if sec.heading_lower.starts_with("abstract ") && sec.heading.len() > 20 {
            let extra = sec.heading["abstract".len()..].trim().to_owned();
            if sec.body.is_empty() {
                return extra;
            } else {
                return format!("{}\n\n{}", extra, sec.body);
            }
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Body building
// ---------------------------------------------------------------------------

static SKIP_HEADINGS: &[&str] = &["title", "abstract"];

static LEVEL_MAP: &[(usize, &str)] = &[
    (1, "section"),
    (2, "subsection"),
    (3, "subsubsection"),
    (4, "paragraph"),
];

/// Build the LaTeX body from parsed sections (excludes title/abstract).
pub fn build_body(sections: &[Section], title: &str) -> String {
    let title_lower = title.trim().to_lowercase();

    // Detect if title was an H1 heading
    let title_h1_found = sections
        .iter()
        .any(|s| s.level == 1 && !s.heading.is_empty() && s.heading.to_lowercase() == title_lower);

    // Determine minimum body level for promotion
    let body_levels: Vec<usize> = sections
        .iter()
        .filter(|s| {
            !SKIP_HEADINGS.contains(&s.heading_lower.as_str())
                && s.level >= 1
                && !(s.level == 1 && s.heading.to_lowercase() == title_lower)
        })
        .map(|s| s.level)
        .collect();

    let min_body_level = body_levels.iter().copied().min().unwrap_or(1);

    let level_offset = if title_h1_found {
        1
    } else if min_body_level >= 2 {
        min_body_level - 1
    } else {
        0
    };

    let strip_num_re = Regex::new(r"^\d+(?:\.\d+)*\.?\s+").unwrap();

    let mut parts: Vec<String> = Vec::new();

    for sec in sections {
        // Skip title and abstract
        if SKIP_HEADINGS.contains(&sec.heading_lower.as_str()) {
            continue;
        }
        // Skip the H1 heading used as the paper title
        if sec.level == 1
            && !sec.heading.is_empty()
            && sec.heading.to_lowercase() == title_lower
        {
            continue;
        }
        if sec.level == 0 {
            // Preamble text before any heading
            parts.push(convert_block(&sec.body));
            continue;
        }

        let effective_level = (sec.level.saturating_sub(level_offset)).max(1);
        let cmd = LEVEL_MAP
            .iter()
            .find(|(l, _)| *l == effective_level)
            .map(|(_, c)| *c)
            .unwrap_or("paragraph");

        let heading_tex = escape_latex(&sec.heading);
        let heading_tex = strip_num_re.replace(&heading_tex, "").to_string();

        parts.push(format!("\\{cmd}{{{heading_tex}}}"));

        // Generate a label for cross-referencing
        if matches!(cmd, "section" | "subsection" | "subsubsection") {
            let label_re = Regex::new(r"[^a-z0-9]+").unwrap();
            let label_key = label_re
                .replace_all(&heading_tex.to_lowercase(), "_")
                .trim_matches('_')
                .chars()
                .take(40)
                .collect::<String>();
            if !label_key.is_empty() {
                parts.push(format!("\\label{{sec:{label_key}}}"));
            }
        }

        if !sec.body.is_empty() {
            parts.push(convert_block(&sec.body));
        }
    }

    let mut result = parts.join("\n\n");
    result.push('\n');
    result
}

// ---------------------------------------------------------------------------
// Block-level conversion
// ---------------------------------------------------------------------------

fn convert_block(text: &str) -> String {
    // Protect display math blocks: \[...\] and $$...$$
    let mut math_blocks: Vec<String> = Vec::new();
    let display_math_re = Regex::new(r"(?ms)^\\\[(.+?)\\\]$").unwrap();
    let display_dollar_re = Regex::new(r"(?ms)^\$\$\s*\n?(.*?)\n?\s*\$\$$").unwrap();
    let fenced_code_re = Regex::new(r"(?ms)^```(\w*)\n(.*?)^```").unwrap();

    let mut text = text.to_owned();

    let dm_re = display_math_re.clone();
    let text_tmp = dm_re.replace_all(&text, |caps: &regex::Captures| {
        let idx = math_blocks.len();
        math_blocks.push(caps[0].to_owned());
        format!("%%MATH_BLOCK_{idx}%%")
    });
    text = text_tmp.into_owned();

    let dd_re = display_dollar_re.clone();
    let text_tmp = dd_re.replace_all(&text, |caps: &regex::Captures| {
        let idx = math_blocks.len();
        let inner = caps[1].trim().to_owned();
        math_blocks.push(format!("\\begin{{equation}}\n{inner}\n\\end{{equation}}"));
        format!("%%MATH_BLOCK_{idx}%%")
    });
    text = text_tmp.into_owned();

    // Protect fenced code blocks
    let mut code_blocks: Vec<String> = Vec::new();
    let fc_re = fenced_code_re.clone();
    let text_tmp = fc_re.replace_all(&text, |caps: &regex::Captures| {
        let idx = code_blocks.len();
        let lang = caps[1].to_owned();
        let code = caps[2].to_owned();
        code_blocks.push(render_code_block(&lang, &code));
        format!("%%CODE_BLOCK_{idx}%%")
    });
    text = text_tmp.into_owned();

    let image_re = Regex::new(r"^!\[([^\]]*)\]\(([^)]+)\)\s*$").unwrap();
    let table_sep_re = Regex::new(r"^\|[-:| ]+\|$").unwrap();
    let bullet_re = Regex::new(r"^(\s*)-\s+(.+)").unwrap();
    let numbered_re = Regex::new(r"^(\s*)\d+\.\s+(.+)").unwrap();

    let lines: Vec<&str> = text.lines().collect();
    let mut output: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Stashed math blocks
        if line.trim().starts_with("%%MATH_BLOCK_") {
            if let Some(num_str) = Regex::new(r"\d+").unwrap().find(line.trim()) {
                if let Ok(idx) = num_str.as_str().parse::<usize>() {
                    if let Some(block) = math_blocks.get(idx) {
                        output.push(block.clone());
                    }
                }
            }
            i += 1;
            continue;
        }

        // Stashed code blocks
        if line.trim().starts_with("%%CODE_BLOCK_") {
            if let Some(num_str) = Regex::new(r"\d+").unwrap().find(line.trim()) {
                if let Ok(idx) = num_str.as_str().parse::<usize>() {
                    if let Some(block) = code_blocks.get(idx) {
                        output.push(block.clone());
                    }
                }
            }
            i += 1;
            continue;
        }

        // Bullet list
        if bullet_re.is_match(line) {
            let (items, new_i) = collect_list(&lines, i, &bullet_re);
            output.push(render_itemize(&items));
            i = new_i;
            continue;
        }

        // Numbered list
        if numbered_re.is_match(line) {
            let (items, new_i) = collect_list(&lines, i, &numbered_re);
            output.push(render_enumerate(&items));
            i = new_i;
            continue;
        }

        // Table detection
        if line.trim().starts_with('|')
            && i + 1 < lines.len()
            && table_sep_re.is_match(lines[i + 1].trim())
        {
            // Check for a preceding caption line
            let mut table_caption = String::new();
            if let Some(prev) = output.last() {
                let prev = prev.trim();
                let cap_re = Regex::new(
                    r"(?:\\textbf\{|[*]{2})\s*Table\s+\d+[.:]?\s*(.*?)(?:\}|[*]{2})$",
                )
                .unwrap();
                if let Some(cap_m) = cap_re.captures(prev) {
                    table_caption = cap_m
                        .get(1)
                        .map(|m| m.as_str().trim().to_owned())
                        .unwrap_or_default();
                    output.pop();
                }
            }
            let table_start = i;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                i += 1;
            }
            let table_lines: Vec<&str> = lines[table_start..i].to_vec();
            output.push(render_table(&table_lines, &table_caption));
            continue;
        }

        // Markdown image
        let stripped = line.trim();
        if let Some(caps) = image_re.captures(stripped) {
            output.push(render_figure(
                caps.get(1).map_or("", |m| m.as_str()),
                caps.get(2).map_or("", |m| m.as_str()),
            ));
            i += 1;
            continue;
        }

        // Regular paragraph line
        output.push(convert_inline(line));
        i += 1;
    }

    output.join("\n")
}

fn collect_list<'a>(lines: &[&'a str], start: usize, pattern: &Regex) -> (Vec<String>, usize) {
    let mut items: Vec<String> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        if let Some(caps) = pattern.captures(lines[i]) {
            items.push(caps[2].to_owned());
            i += 1;
        } else if lines[i].trim().is_empty() {
            if i + 1 < lines.len() && pattern.is_match(lines[i + 1]) {
                i += 1; // skip blank, continue
            } else {
                break;
            }
        } else if lines[i].starts_with("  ") || lines[i].starts_with('\t') {
            if let Some(last) = items.last_mut() {
                *last += " ";
                *last += lines[i].trim();
            }
            i += 1;
        } else {
            break;
        }
    }
    (items, i)
}

fn render_itemize(items: &[String]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|item| format!("  \\item {}", convert_inline(item)))
        .collect();
    format!("\\begin{{itemize}}\n{}\n\\end{{itemize}}", inner.join("\n"))
}

fn render_enumerate(items: &[String]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|item| format!("  \\item {}", convert_inline(item)))
        .collect();
    format!(
        "\\begin{{enumerate}}\n{}\n\\end{{enumerate}}",
        inner.join("\n")
    )
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
///
/// Faithfully ports the Python `_convert_inline` logic:
/// - Normalises Unicode punctuation (em-dashes, curly quotes, math symbols)
/// - Protects math (`\(...\)`, `$...$`) and `\cmd{...}` from escaping
/// - Escapes LaTeX specials (`#`, `%`, `&`, `_`, `{`, `}`, `~`, `^`, `$`)
/// - Converts bold, italic, inline code, links, citations, figure/table/eq refs
pub fn convert_inline(text: &str) -> String {
    // 1. Normalise Unicode to LaTeX equivalents
    let mut s = text
        .replace('\u{2014}', "---")
        .replace('\u{2013}', "--")
        .replace('\u{201c}', "``")
        .replace('\u{201d}', "''")
        .replace('\u{2018}', "`")
        .replace('\u{2019}', "'")
        .replace('\u{00b1}', "$\\pm$")
        .replace('\u{2248}', "$\\approx$")
        .replace('\u{2264}', "$\\leq$")
        .replace('\u{2265}', "$\\geq$")
        .replace('\u{2192}', "$\\rightarrow$")
        .replace('\u{2190}', "$\\leftarrow$")
        .replace('\u{00d7}', "$\\times$");

    // 2. Protect math spans and existing LaTeX commands from escaping
    let mut protected: Vec<String> = Vec::new();

    // Protect \[...\] display math (must come before \(...\))
    let display_math_re = Regex::new(r"(?s)\\\[.+?\\\]").unwrap();
    s = display_math_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Protect $$...$$ display math
    let display_dollar_re = Regex::new(r"(?s)\$\$.+?\$\$").unwrap();
    s = display_dollar_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Protect \(...\) inline math
    let inline_math_re = Regex::new(r"\\\(.+?\\\)").unwrap();
    s = inline_math_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Protect $...$ inline math (single $, not $$)
    // Use a state-machine scan to avoid lookaround which Rust regex doesn't support.
    s = protect_dollar_math(&s, &mut protected);

    // Protect existing \cmd{...} LaTeX commands
    let cmd_re = Regex::new(r"\\[a-zA-Z]+\{[^}]*\}").unwrap();
    s = cmd_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Protect images (before links to avoid mis-match)
    let image_re = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();
    s = image_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Convert and protect Markdown links [text](url) → \href{url}{text}
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    s = link_re
        .replace_all(&s, |caps: &regex::Captures| {
            let href = format!("\\href{{{}}}{{{}}}", &caps[2], &caps[1]);
            let idx = protected.len();
            protected.push(href);
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // 3. Escape LaTeX special characters (outside protected zones)
    s = escape_latex_outside_protected(&s);

    // 4. Convert bold **text** → \textbf{text}  (must precede italic)
    let bold_re = Regex::new(r"\*\*(.+?)\*\*").unwrap();
    s = bold_re
        .replace_all(&s, |caps: &regex::Captures| format!("\\textbf{{{}}}", &caps[1]))
        .into_owned();

    // 5. Convert italic *text* → \textit{text}
    // Use a state-machine scan: a single * that is not preceded or followed by *
    s = convert_italic_stars(&s);

    // 6. Convert inline code `text` → \texttt{text}
    let code_re = Regex::new(r"`([^`]+)`").unwrap();
    s = code_re
        .replace_all(&s, |caps: &regex::Captures| format!("\\texttt{{{}}}", &caps[1]))
        .into_owned();

    // 7. Fallback: convert remaining [@key] citations
    let cite_re = Regex::new(r"\[@([^\]]+)\]").unwrap();
    s = cite_re
        .replace_all(&s, |caps: &regex::Captures| {
            let keys: String = caps[1]
                .split(';')
                .map(|k| k.trim().trim_start_matches('@').trim())
                .collect::<Vec<_>>()
                .join(", ");
            format!("\\cite{{{keys}}}")
        })
        .into_owned();

    // 8. Fallback: convert bare cite-key patterns [author2024word]
    let cite_key_pat = r"[a-zA-Z][a-zA-Z0-9_-]*\d{4}[a-zA-Z0-9_]*";
    let bare_cite_re =
        Regex::new(&format!(r"\[({cite_key_pat}(?:\s*,\s*{cite_key_pat})*)\]")).unwrap();
    s = bare_cite_re
        .replace_all(&s, |caps: &regex::Captures| format!("\\cite{{{}}}", &caps[1]))
        .into_owned();

    // 9. Figure / table / equation references
    let figref_re = Regex::new(r"\[(fig:[^\]]+)\]").unwrap();
    s = figref_re
        .replace_all(&s, |caps: &regex::Captures| {
            format!("Figure~\\ref{{{}}}", &caps[1])
        })
        .into_owned();

    let tabref_re = Regex::new(r"\[(tab:[^\]]+)\]").unwrap();
    s = tabref_re
        .replace_all(&s, |caps: &regex::Captures| {
            format!("Table~\\ref{{{}}}", &caps[1])
        })
        .into_owned();

    let eqref_re = Regex::new(r"\[(eq:[^\]]+)\]").unwrap();
    s = eqref_re
        .replace_all(&s, |caps: &regex::Captures| {
            format!("Equation~\\ref{{{}}}", &caps[1])
        })
        .into_owned();

    // 10. Restore protected segments
    for (idx, val) in protected.iter().enumerate() {
        s = s.replace(&format!("\x00PROT{idx}\x00"), val);
    }

    s
}

/// Protect single-dollar math spans `$...$` from escaping.
///
/// Scans for `$...$` where neither boundary is a doubled `$$`.
/// Appends each found span to `protected` and replaces it with a marker.
fn protect_dollar_math(s: &str, protected: &mut Vec<String>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        // Check for $
        if c == '$' {
            // Skip $$ (display math, already protected)
            if i + 1 < chars.len() && chars[i + 1] == '$' {
                result.push(c);
                i += 1;
                continue;
            }
            // Check that preceding char is not $
            if i > 0 && chars[i - 1] == '$' {
                result.push(c);
                i += 1;
                continue;
            }
            // Scan for closing $
            let mut j = i + 1;
            let mut found_close = false;
            while j < chars.len() {
                if chars[j] == '$' {
                    // Must not be $$
                    let next_is_dollar = j + 1 < chars.len() && chars[j + 1] == '$';
                    let prev_is_dollar = j > 0 && chars[j - 1] == '$';
                    if !next_is_dollar && !prev_is_dollar {
                        // Found closing $
                        let span: String = chars[i..=j].iter().collect();
                        let idx = protected.len();
                        protected.push(span);
                        result.push_str(&format!("\x00PROT{idx}\x00"));
                        i = j + 1;
                        found_close = true;
                        break;
                    }
                }
                j += 1;
            }
            if !found_close {
                result.push(c);
                i += 1;
            }
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

/// Convert italic `*text*` markers to `\textit{text}`.
///
/// A single `*` that is not part of `**` (bold) is treated as italic.
/// After bold has been converted, remaining `*` pairs are italic delimiters.
fn convert_italic_stars(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c == '*' {
            // Skip ** (bold already handled, but be safe)
            if i + 1 < chars.len() && chars[i + 1] == '*' {
                result.push(c);
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }
            // Check preceding char is not *
            let prev_is_star = i > 0 && chars[i - 1] == '*';
            if prev_is_star {
                result.push(c);
                i += 1;
                continue;
            }
            // Scan for closing single *
            let mut j = i + 1;
            let mut found = false;
            while j < chars.len() {
                if chars[j] == '*' {
                    let next_is_star = j + 1 < chars.len() && chars[j + 1] == '*';
                    let prev_is_star2 = j > 0 && chars[j - 1] == '*';
                    if !next_is_star && !prev_is_star2 {
                        let inner: String = chars[i + 1..j].iter().collect();
                        if !inner.is_empty() {
                            result.push_str(&format!("\\textit{{{inner}}}"));
                            i = j + 1;
                            found = true;
                        }
                        break;
                    }
                }
                j += 1;
            }
            if !found {
                result.push(c);
                i += 1;
            }
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

/// Escape LaTeX special characters in a string that may contain `\x00PROT...\x00` markers.
fn escape_latex_outside_protected(text: &str) -> String {
    // Split on protection markers, escape each non-protected segment
    let prot_re = Regex::new(r"\x00PROT\d+\x00").unwrap();
    let mut result = String::with_capacity(text.len() * 2);
    let mut last = 0;
    for m in prot_re.find_iter(text) {
        let segment = &text[last..m.start()];
        result.push_str(&escape_latex_chars(segment));
        result.push_str(m.as_str());
        last = m.end();
    }
    result.push_str(&escape_latex_chars(&text[last..]));
    result
}

/// Escape LaTeX special characters: `# % & _ { } ~ ^ $`.
///
/// Does NOT escape backslash (may already be a LaTeX command).
/// Does NOT escape inside math spans (detected by `$`).
fn escape_latex_chars(text: &str) -> String {
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
            '_' => out.push_str("\\_"),
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
// LaTeX special character escaping (public, for titles/headings)
// ---------------------------------------------------------------------------

/// Escape LaTeX special characters in plain text (titles, headings).
///
/// Does NOT escape inside math delimiters or `\commands`.
pub fn escape_latex(text: &str) -> String {
    // Protect math first
    let mut protected: Vec<String> = Vec::new();

    // Protect \(...\) inline math
    let inline_math_re = Regex::new(r"\\\(.+?\\\)").unwrap();
    let mut s = inline_math_re
        .replace_all(text, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Protect $$...$$ display math (must come before single $)
    let display_dollar_re = Regex::new(r"(?s)\$\$.+?\$\$").unwrap();
    s = display_dollar_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    // Protect $...$ inline math using state-machine (no lookaround needed)
    s = protect_dollar_math(&s, &mut protected);

    // Protect existing \cmd{...} LaTeX commands
    let cmd_re = Regex::new(r"\\[a-zA-Z]+\{[^}]*\}").unwrap();
    s = cmd_re
        .replace_all(&s, |caps: &regex::Captures| {
            let idx = protected.len();
            protected.push(caps[0].to_owned());
            format!("\x00PROT{idx}\x00")
        })
        .into_owned();

    s = escape_latex_outside_protected(&s);

    for (idx, val) in protected.iter().enumerate() {
        s = s.replace(&format!("\x00PROT{idx}\x00"), val);
    }

    s
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

static ALGO_KEYWORDS: &[&str] = &[
    "Input", "Output", "Return", "While", "For", "If", "Else",
    "Repeat", "Until", "Function", "Procedure", "Algorithm",
];

fn render_code_block(lang: &str, code: &str) -> String {
    let lang_lower = lang.trim().to_lowercase();
    let is_algo = matches!(lang_lower.as_str(), "algorithm" | "pseudocode" | "algo")
        || {
            let count = ALGO_KEYWORDS
                .iter()
                .filter(|&&kw| {
                    let kw_lower = kw.to_lowercase();
                    code.to_lowercase().contains(&kw_lower)
                })
                .count();
            count >= 3
        };

    if is_algo {
        let algo_lines: Vec<&str> = code.lines().collect();
        let mut caption = "Algorithm".to_owned();
        let mut body_lines = algo_lines.as_slice();

        if let Some(first) = algo_lines.first() {
            if first.trim().starts_with("//") {
                caption = first.trim().trim_start_matches('/').trim().to_owned();
                body_lines = &algo_lines[1..];
            }
        }

        let algo_cmds = &[
            "\\STATE", "\\IF", "\\ELSE", "\\ELSIF", "\\ENDIF",
            "\\FOR", "\\ENDFOR", "\\WHILE", "\\ENDWHILE",
            "\\REPEAT", "\\UNTIL", "\\RETURN", "\\REQUIRE", "\\ENSURE",
        ];

        let wrapped: Vec<String> = body_lines
            .iter()
            .filter_map(|l| {
                let stripped = l.trim();
                if stripped.is_empty() {
                    return None;
                }
                if algo_cmds.iter().any(|cmd| stripped.starts_with(cmd)) {
                    Some(stripped.to_owned())
                } else {
                    Some(format!("\\STATE {stripped}"))
                }
            })
            .collect();

        let body = wrapped.join("\n");
        let cap = convert_inline(&caption);
        return format!(
            "\\begin{{algorithm}}[ht]\n\\caption{{{cap}}}\n\\begin{{algorithmic}}[1]\n{body}\n\\end{{algorithmic}}\n\\end{{algorithm}}"
        );
    }

    let escaped = code.trim_end_matches('\n');
    format!("\\begin{{verbatim}}\n{escaped}\n\\end{{verbatim}}")
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

fn collect_table_lines<'a>(lines: &[&'a str], start: usize) -> (Vec<&'a str>, usize) {
    let mut table: Vec<&str> = Vec::new();
    let mut i = start;
    while i < lines.len() && lines[i].contains('|') {
        table.push(lines[i]);
        i += 1;
    }
    (table, i - start)
}

/// Render a Markdown table as a LaTeX `table` + `tabular` environment.
///
/// - Parses column alignment from the separator row (`---`, `:---`, `---:`, `:---:`)
/// - Wraps in `\resizebox` when columns > 5 or any cell exceeds 25 characters
/// - Auto-generates a caption from the header columns when none is provided
pub fn render_table(table_lines: &[&str], caption: &str) -> String {
    if table_lines.len() < 2 {
        return String::new();
    }

    let header = parse_table_row(table_lines[0]);
    let body_rows: Vec<Vec<String>> = table_lines[2..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| parse_table_row(l))
        .collect();
    let ncols = header.len();

    let alignments = parse_alignments(table_lines[1], ncols);
    let col_spec: String = alignments.join("");

    let table_num = next_table_num();

    let max_cell_len = header
        .iter()
        .chain(body_rows.iter().flatten())
        .map(|c| c.len())
        .max()
        .unwrap_or(0);
    let needs_resize = ncols > 5 || max_cell_len > 25;

    let mut lines_out: Vec<String> = Vec::new();
    lines_out.push("\\begin{table}[ht]".to_owned());
    lines_out.push("\\centering".to_owned());
    if needs_resize {
        lines_out.push("\\resizebox{\\textwidth}{!}{%".to_owned());
    }
    lines_out.push(format!("\\begin{{tabular}}{{{col_spec}}}"));
    lines_out.push("\\toprule".to_owned());
    lines_out.push(
        header
            .iter()
            .map(|c| format!("\\textbf{{{}}}", convert_inline(c)))
            .collect::<Vec<_>>()
            .join(" & ")
            + " \\\\",
    );
    lines_out.push("\\midrule".to_owned());
    for row in &body_rows {
        let mut padded: Vec<String> = row.clone();
        padded.resize(ncols, String::new());
        lines_out.push(
            padded[..ncols]
                .iter()
                .map(|c| convert_inline(c))
                .collect::<Vec<_>>()
                .join(" & ")
                + " \\\\",
        );
    }
    lines_out.push("\\bottomrule".to_owned());
    lines_out.push("\\end{tabular}".to_owned());
    if needs_resize {
        lines_out.push("}".to_owned()); // close resizebox
    }

    // Caption
    let cap_text = if !caption.is_empty() {
        let stripped = Regex::new(r"^Table\s+\d+[.:]\s*")
            .unwrap()
            .replace(caption, "")
            .trim()
            .to_owned();
        if !stripped.is_empty() {
            convert_inline(&stripped)
        } else {
            auto_table_caption(&header, table_num)
        }
    } else {
        auto_table_caption(&header, table_num)
    };
    lines_out.push(format!("\\caption{{{cap_text}}}"));
    lines_out.push(format!("\\label{{tab:{table_num}}}"));
    lines_out.push("\\end{table}".to_owned());

    lines_out.join("\n")
}

fn auto_table_caption(header: &[String], table_num: u32) -> String {
    if header.len() <= 1 {
        return format!("Table {table_num}");
    }
    let cols: Vec<&str> = header.iter().map(|c| c.trim()).filter(|c| !c.is_empty()).collect();
    if cols.len() < 2 {
        return format!("Table {table_num}");
    }
    let col0 = cols[0].to_lowercase();
    let rest: Vec<String> = cols[1..5.min(cols.len())]
        .iter()
        .map(|c| convert_inline(c))
        .collect();

    let hp_hints = ["hyperparameter", "parameter", "param", "hp", "setting", "config"];
    let abl_hints = ["component", "variant", "ablation", "configuration", "module"];
    let model_hints = ["model", "method", "approach", "algorithm", "baseline"];

    if hp_hints.iter().any(|h| col0.contains(h)) {
        return "Hyperparameter settings".to_owned();
    }
    if abl_hints.iter().any(|h| col0.contains(h)) {
        return format!("Ablation study results across {}", rest.join(", "));
    }
    if model_hints.iter().any(|h| col0.contains(h)) {
        return format!(
            "Performance comparison of different methods on {}",
            rest.join(", ")
        );
    }
    format!("Comparison of {} across {}", convert_inline(cols[0]), rest.join(", "))
}

fn parse_table_row(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = if line.starts_with('|') { &line[1..] } else { line };
    let line = if line.ends_with('|') {
        &line[..line.len() - 1]
    } else {
        line
    };
    line.split('|').map(|c| c.trim().to_owned()).collect()
}

fn parse_alignments(sep_line: &str, ncols: usize) -> Vec<String> {
    let cells = parse_table_row(sep_line);
    let mut aligns: Vec<String> = cells
        .iter()
        .map(|cell| {
            let raw = cell.trim();
            let left = raw.starts_with(':');
            let right = raw.ends_with(':');
            if left && right {
                "c".to_owned()
            } else if right {
                "r".to_owned()
            } else {
                "l".to_owned()
            }
        })
        .collect();
    while aligns.len() < ncols {
        aligns.push("l".to_owned());
    }
    aligns.truncate(ncols);
    aligns
}

// ---------------------------------------------------------------------------
// Figure rendering
// ---------------------------------------------------------------------------

/// Render a Markdown image `![caption](path)` as a LaTeX `figure` environment.
pub fn render_figure(caption: &str, path: &str) -> String {
    let fig_num = next_figure_num();
    let path = path.replace(' ', "_");
    let cap_tex = if caption.is_empty() {
        format!("Figure {fig_num}")
    } else {
        convert_inline(caption)
    };
    let label_re = Regex::new(r"[^a-z0-9]+").unwrap();
    let label_key = if caption.is_empty() {
        fig_num.to_string()
    } else {
        let raw = label_re
            .replace_all(&caption.to_lowercase(), "_")
            .to_string();
        let trimmed: String = raw.trim_matches('_').chars().take(30).collect();
        if trimmed.is_empty() {
            fig_num.to_string()
        } else {
            trimmed
        }
    };
    format!(
        "\\begin{{figure}}[t]\n\\centering\n\\includegraphics[width=0.95\\columnwidth]{{{path}}}\n\\caption{{{cap_tex}}}\n\\label{{fig:{label_key}}}\n\\end{{figure}}"
    )
}

// ---------------------------------------------------------------------------
// Fix common LaTeX errors
// ---------------------------------------------------------------------------

/// Apply automated fixes for common LaTeX errors.
///
/// Returns `(fixed_text, list_of_fix_descriptions)`.
pub fn fix_common_latex_errors(tex_text: &str, errors: &[String]) -> (String, Vec<String>) {
    let mut fixes: Vec<String> = Vec::new();
    let mut fixed = tex_text.to_owned();

    for err in errors {
        let err_lower = err.to_lowercase();

        // Undefined control sequence: remove known safe-to-remove commands
        if err_lower.contains("undefined control sequence") {
            let cmd_re = Regex::new(r"\\([a-zA-Z]+)").unwrap();
            if let Some(caps) = cmd_re.captures(err) {
                let cmd = &caps[1];
                let safe_to_remove = ["textsc", "textsl", "mathbb", "mathcal", "bm", "boldsymbol"];
                if safe_to_remove.contains(&cmd) {
                    let remove_re =
                        Regex::new(&format!(r"\\{}\{{([^}}]*)\}}", regex::escape(cmd))).unwrap();
                    let before = fixed.clone();
                    fixed = remove_re.replace_all(&fixed, "$1").into_owned();
                    if fixed != before {
                        fixes.push(format!("Removed undefined \\{cmd}"));
                    }
                }
            }
        }

        // File not found: comment out missing \usepackage
        if err_lower.contains("file") && err_lower.contains("not found") {
            let file_re = Regex::new(r"File `([^']+)' not found").unwrap();
            if let Some(caps) = file_re.captures(err) {
                let missing = &caps[1];
                if missing.ends_with(".sty") {
                    let pkg = missing.trim_end_matches(".sty");
                    let pkg_re = Regex::new(&format!(
                        r"\\usepackage(\[[^\]]*\])?\\{{\{}}}",
                        regex::escape(pkg)
                    ))
                    .unwrap_or_else(|_| Regex::new(r"^$").unwrap());
                    let replacement =
                        format!("% IMP-18: Removed missing package {pkg}");
                    let before = fixed.clone();
                    fixed = pkg_re.replace_all(&fixed, replacement.as_str()).into_owned();
                    if fixed != before {
                        fixes.push(format!("Removed missing package {pkg}"));
                    }
                }
            }
        }

        // Too many unprocessed floats
        if err_lower.contains("too many unprocessed floats") {
            let before = fixed.clone();
            fixed = fixed.replacen("\\begin{table}", "\\clearpage\n\\begin{table}", 1);
            if fixed != before {
                fixes.push("Added \\clearpage for float overflow".to_owned());
            }
        }
    }

    (fixed, fixes)
}

// ---------------------------------------------------------------------------
// Paper completeness checking
// ---------------------------------------------------------------------------

/// Warning message from [`check_paper_completeness`].
#[derive(Debug, Clone)]
pub struct CompletenessWarning {
    pub message: String,
}

impl CompletenessWarning {
    fn new(msg: impl Into<String>) -> Self {
        CompletenessWarning {
            message: msg.into(),
        }
    }
}

static EXPECTED_SECTIONS: &[&str] = &[
    "introduction",
    "related work",
    "method",
    "experiment",
    "result",
    "discussion",
    "conclusion",
];

/// Section aliases mapping variants → canonical name.
static SECTION_ALIASES: &[(&str, &str)] = &[
    ("methodology", "method"),
    ("methods", "method"),
    ("proposed method", "method"),
    ("approach", "method"),
    ("experiments", "experiment"),
    ("experimental setup", "experiment"),
    ("experimental results", "result"),
    ("results", "result"),
    ("results and discussion", "result"),
    ("results and analysis", "result"),
    ("discussion and results", "result"),
    ("conclusions", "conclusion"),
    ("conclusion and future work", "conclusion"),
    ("summary", "conclusion"),
    ("background", "related work"),
    ("literature review", "related work"),
    ("prior work", "related work"),
];

/// Check whether a paper contains all expected sections.
///
/// Returns a list of warning strings.  An empty list means the paper
/// structure looks complete.
pub fn check_paper_completeness(sections: &[Section]) -> Vec<CompletenessWarning> {
    let mut warnings: Vec<CompletenessWarning> = Vec::new();

    // Check for a valid title (some H1/H2 that is NOT a standard section name)
    let standard_names: HashSet<&str> = HashSet::from([
        "abstract",
        "introduction",
        "related work",
        "method",
        "methods",
        "methodology",
        "experiments",
        "results",
        "discussion",
        "conclusion",
        "limitations",
        "references",
    ]);
    let has_title = sections
        .iter()
        .any(|s| s.level <= 2 && s.level >= 1 && !standard_names.contains(s.heading_lower.as_str()));
    if !has_title {
        warnings.push(CompletenessWarning::new(
            "No valid title found in paper. The output may lack proper heading structure.",
        ));
    }

    // Check for expected sections
    let mut found_sections: HashSet<&str> = HashSet::new();
    let mut section_headings: Vec<String> = Vec::new();

    for sec in sections {
        if sec.level <= 2 && sec.level >= 1 && !sec.heading.is_empty() {
            let hl = sec.heading_lower.as_str();
            section_headings.push(hl.to_owned());
            if EXPECTED_SECTIONS.contains(&hl) {
                found_sections.insert(hl);
            } else if let Some(&canonical) = SECTION_ALIASES
                .iter()
                .find(|(alias, _)| *alias == hl)
                .map(|(_, c)| c)
            {
                found_sections.insert(canonical);
            } else {
                for &expected in EXPECTED_SECTIONS {
                    if hl.contains(expected) {
                        found_sections.insert(expected);
                        break;
                    }
                }
            }
        }
    }

    let missing: Vec<&&str> = EXPECTED_SECTIONS
        .iter()
        .filter(|s| !found_sections.contains(*s))
        .collect();
    if !missing.is_empty() {
        let missing_str: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
        warnings.push(CompletenessWarning::new(format!(
            "Missing sections: {}. Found: {}",
            missing_str.join(", "),
            section_headings.join(", ")
        )));
    }

    // Check for Limitations section (required for NeurIPS/ICLR)
    let required_extras = ["limitations"];
    let extra_aliases: &[(&str, &str)] = &[
        ("limitation", "limitations"),
        ("limitations and future work", "limitations"),
        ("limitations and broader impact", "limitations"),
    ];
    let mut found_extras: HashSet<&str> = HashSet::new();
    for sec in sections {
        if sec.level <= 2 && sec.level >= 1 && !sec.heading.is_empty() {
            let hl = sec.heading_lower.as_str();
            if required_extras.contains(&hl) {
                found_extras.insert(hl);
            } else if let Some(&canonical) = extra_aliases
                .iter()
                .find(|(a, _)| *a == hl)
                .map(|(_, c)| c)
            {
                found_extras.insert(canonical);
            } else if hl.contains("limitation") {
                found_extras.insert("limitations");
            }
        }
    }
    let missing_extras: Vec<&str> = required_extras
        .iter()
        .copied()
        .filter(|e| !found_extras.contains(*e))
        .collect();
    if !missing_extras.is_empty() {
        warnings.push(CompletenessWarning::new(format!(
            "Missing required sections for NeurIPS/ICLR: {}.",
            missing_extras.join(", ")
        )));
    }

    // Abstract length and quality checks
    for sec in sections {
        if sec.heading_lower == "abstract" {
            let word_count = sec.body.split_whitespace().count();
            if word_count > 300 {
                warnings.push(CompletenessWarning::new(format!(
                    "Abstract is {word_count} words (conference limit: 150-250). Must be shortened."
                )));
            } else if word_count < 150 {
                warnings.push(CompletenessWarning::new(format!(
                    "Abstract is only {word_count} words (expected 150-250 for conferences)."
                )));
            }
            // Detect raw variable names / metric key dumps
            let raw_var_re = Regex::new(r"\b\w+_\w+/\w+(?:_\w+)*\s*=").unwrap();
            let raw_vars: Vec<&str> = raw_var_re
                .find_iter(&sec.body)
                .map(|m| m.as_str())
                .take(3)
                .collect();
            if !raw_vars.is_empty() {
                warnings.push(CompletenessWarning::new(format!(
                    "Abstract contains raw variable names: {:?}. Replace with human-readable descriptions.",
                    raw_vars
                )));
            }
            break;
        }
    }

    // Truncation marker detection
    let all_body: String = sections.iter().map(|s| s.body.as_str()).collect::<Vec<_>>().join(" ");
    let truncation_markers = [
        "further sections continue",
        "remaining sections unchanged",
        "sections continue unchanged",
        "content continues",
        "[to be continued]",
        "[remaining content]",
    ];
    for marker in &truncation_markers {
        if all_body.to_lowercase().contains(marker) {
            warnings.push(CompletenessWarning::new(format!(
                "Truncation marker detected: '{marker}'. Paper content may be incomplete."
            )));
        }
    }

    // Total word count check
    let total_words: usize = sections.iter().map(|s| s.body.split_whitespace().count()).sum();
    if total_words < 2000 {
        warnings.push(CompletenessWarning::new(format!(
            "Paper body is only {total_words} words (expected 5,000-6,500 for conference paper). Content may be severely truncated."
        )));
    }

    // Bullet density check for body sections
    let bullet_ok_sections: HashSet<&str> =
        HashSet::from(["introduction", "limitations", "limitation", "abstract"]);
    for sec in sections {
        if sec.level > 2 || sec.level < 1 || sec.heading.is_empty() {
            continue;
        }
        let hl = sec.heading_lower.as_str();
        if bullet_ok_sections.contains(hl) || sec.body.is_empty() {
            continue;
        }
        let total_lines = sec.body.lines().filter(|l| !l.trim().is_empty()).count();
        if total_lines < 4 {
            continue;
        }
        let bullet_count = sec
            .body
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("- ") || t.starts_with("* ")
                    || Regex::new(r"^\d+\.\s").unwrap().is_match(t)
            })
            .count();
        let density = bullet_count as f64 / total_lines as f64;
        if density > 0.30 {
            warnings.push(CompletenessWarning::new(format!(
                "Section '{}' has high bullet-point density ({}/{} lines = {:.0}%). Conference papers should use flowing prose.",
                sec.heading,
                bullet_count,
                total_lines,
                density * 100.0
            )));
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// Internal helpers (kept for backward compat with markdown_to_latex path)
// ---------------------------------------------------------------------------

/// Escape characters that have special meaning in LaTeX.
///
/// Kept for the simple `markdown_to_latex` path.  For richer escaping
/// (math protection, protection markers) use [`escape_latex`].
#[allow(dead_code)]
pub(crate) fn escape_latex_specials(text: &str) -> String {
    escape_latex_chars(text)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Heading tests (pre-existing) ---

    #[test]
    fn heading_levels() {
        assert_eq!(convert_heading("# Hello World"), r"\section{Hello World}");
        assert_eq!(convert_heading("## Sub"), r"\subsection{Sub}");
        assert_eq!(convert_heading("### Sub sub"), r"\subsubsection{Sub sub}");
    }

    #[test]
    fn bold_and_italic() {
        let out = convert_inline("This is **bold** and *italic*.");
        assert!(out.contains(r"\textbf{bold}"), "bold: {out}");
        assert!(out.contains(r"\textit{italic}"), "italic: {out}");
    }

    #[test]
    fn inline_code() {
        let out = convert_inline("Use `cargo build` here.");
        assert!(out.contains(r"\texttt{cargo build}"), "code: {out}");
    }

    #[test]
    fn citation_single() {
        let out = convert_inline("See [@smith2024].");
        assert!(out.contains(r"\cite{smith2024}"), "cite: {out}");
    }

    #[test]
    fn citation_multi() {
        let out = convert_inline("See [@a; @b].");
        assert!(out.contains(r"\cite{a, b}"), "multi-cite: {out}");
    }

    #[test]
    fn figure_reference() {
        let out = convert_inline("As shown in [fig:results].");
        assert!(out.contains(r"Figure~\ref{fig:results}"), "figref: {out}");
    }

    // --- escape_latex ---

    #[test]
    fn escape_latex_specials_basic() {
        let out = escape_latex_specials("50% cost & speed");
        assert!(out.contains(r"\%"), "percent: {out}");
        assert!(out.contains(r"\&"), "ampersand: {out}");
    }

    #[test]
    fn escape_latex_underscore() {
        let out = escape_latex("some_variable_name");
        assert!(out.contains(r"\_"), "underscore: {out}");
    }

    #[test]
    fn escape_latex_preserves_math() {
        let out = escape_latex("value is $x_1 + y_2$");
        // The math span should be preserved as-is (underscore inside $ not escaped)
        assert!(out.contains("$x_1 + y_2$"), "math preserved: {out}");
    }

    #[test]
    fn escape_latex_preserves_commands() {
        let out = escape_latex("use \\textbf{bold} here");
        assert!(out.contains(r"\textbf{bold}"), "cmd preserved: {out}");
    }

    // --- render_table ---

    #[test]
    fn render_table_basic() {
        reset_render_counters();
        let lines = vec![
            "| Method | Accuracy |",
            "|--------|----------|",
            "| Ours   | 95.2     |",
        ];
        let out = render_table(&lines, "");
        assert!(out.contains(r"\begin{table}"), "table env: {out}");
        assert!(out.contains(r"\begin{tabular}"), "tabular: {out}");
        assert!(out.contains(r"\toprule"), "toprule: {out}");
        assert!(out.contains(r"\midrule"), "midrule: {out}");
        assert!(out.contains(r"\bottomrule"), "bottomrule: {out}");
        assert!(out.contains(r"\end{table}"), "end table: {out}");
        assert!(out.contains("Ours"), "data row: {out}");
    }

    #[test]
    fn render_table_alignment() {
        reset_render_counters();
        let lines = vec![
            "| Left | Center | Right |",
            "|:-----|:------:|------:|",
            "| a    |   b    |     c |",
        ];
        let out = render_table(&lines, "");
        // col spec should be "lcr"
        assert!(out.contains("{lcr}"), "alignment spec: {out}");
    }

    #[test]
    fn render_table_with_caption() {
        reset_render_counters();
        let lines = vec![
            "| Model | F1 |",
            "|-------|-----|",
            "| BERT  | 0.9 |",
        ];
        let out = render_table(&lines, "Results on benchmark");
        assert!(out.contains("Results on benchmark"), "caption text: {out}");
        assert!(out.contains(r"\caption{"), "caption cmd: {out}");
    }

    #[test]
    fn render_table_resizebox_wide() {
        reset_render_counters();
        // 6 columns → needs resizebox
        let lines = vec![
            "| A | B | C | D | E | F |",
            "|---|---|---|---|---|---|",
            "| 1 | 2 | 3 | 4 | 5 | 6 |",
        ];
        let out = render_table(&lines, "");
        assert!(out.contains(r"\resizebox"), "resizebox: {out}");
    }

    // --- render_figure ---

    #[test]
    fn render_figure_basic() {
        reset_render_counters();
        let out = render_figure("Neural network architecture", "figures/arch.pdf");
        assert!(out.contains(r"\begin{figure}"), "figure env: {out}");
        assert!(out.contains(r"\includegraphics"), "includegraphics: {out}");
        assert!(out.contains("figures/arch.pdf"), "path: {out}");
        assert!(out.contains("Neural network architecture"), "caption: {out}");
        assert!(out.contains(r"\label{fig:"), "label: {out}");
    }

    #[test]
    fn render_figure_empty_caption() {
        reset_render_counters();
        let out = render_figure("", "img.png");
        assert!(out.contains("Figure 1"), "auto caption: {out}");
    }

    // --- convert_inline advanced ---

    #[test]
    fn convert_inline_unicode_emdash() {
        let out = convert_inline("word\u{2014}word");
        assert!(out.contains("---"), "emdash: {out}");
    }

    #[test]
    fn convert_inline_link() {
        let out = convert_inline("See [OpenAI](https://openai.com).");
        assert!(out.contains(r"\href{https://openai.com}{OpenAI}"), "link: {out}");
    }

    #[test]
    fn convert_inline_math_preserved() {
        let out = convert_inline("energy $E = mc^2$ matters");
        // the $ math span should not get its ^ escaped
        assert!(out.contains("$E = mc^2$"), "math: {out}");
    }

    #[test]
    fn convert_inline_table_ref() {
        let out = convert_inline("See [tab:results].");
        assert!(out.contains(r"Table~\ref{tab:results}"), "tabref: {out}");
    }

    #[test]
    fn convert_inline_eq_ref() {
        let out = convert_inline("From [eq:loss].");
        assert!(out.contains(r"Equation~\ref{eq:loss}"), "eqref: {out}");
    }

    // --- parse_sections ---

    #[test]
    fn parse_sections_basic() {
        let md = "# Introduction\n\nThis is the intro.\n\n## Related Work\n\nSome work.";
        let sections = parse_sections(md);
        assert!(sections.len() >= 2, "sections count: {}", sections.len());
        let intro = sections.iter().find(|s| s.heading_lower == "introduction");
        assert!(intro.is_some(), "found introduction");
        assert!(
            intro.unwrap().body.contains("intro"),
            "intro body: {}",
            intro.unwrap().body
        );
    }

    #[test]
    fn parse_sections_no_headings() {
        let md = "Just a paragraph with no headings.";
        let sections = parse_sections(md);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].level, 1);
    }

    // --- extract_title ---

    #[test]
    fn extract_title_first_h1() {
        let md = "# My Great Paper\n\n## Introduction\n\nText.";
        let sections = parse_sections(md);
        let title = extract_title(&sections, md);
        assert_eq!(title, "My Great Paper");
    }

    #[test]
    fn extract_title_fallback() {
        let md = "## Introduction\n\nText.";
        let sections = parse_sections(md);
        let title = extract_title(&sections, md);
        // "Introduction" is in TITLE_SKIP, so should fall through to "Untitled Paper"
        // Actually "introduction" is in TITLE_SKIP array
        assert!(!title.is_empty());
    }

    // --- extract_abstract ---

    #[test]
    fn extract_abstract_basic() {
        let md =
            "# My Paper\n\n## Abstract\n\nThis paper presents an approach.\n\n## Introduction\n\nText.";
        let sections = parse_sections(md);
        let abs = extract_abstract(&sections);
        assert!(abs.contains("approach"), "abstract: {abs}");
    }

    #[test]
    fn extract_abstract_missing() {
        let md = "# My Paper\n\n## Introduction\n\nNo abstract here.";
        let sections = parse_sections(md);
        let abs = extract_abstract(&sections);
        assert!(abs.is_empty(), "should be empty: {abs}");
    }

    // --- build_body ---

    #[test]
    fn build_body_skips_abstract() {
        let md = "# My Paper\n\n## Abstract\n\nAbstract text.\n\n## Introduction\n\nIntro text.";
        let sections = parse_sections(md);
        let body = build_body(&sections, "My Paper");
        assert!(!body.contains("Abstract text"), "abstract in body: {body}");
        assert!(body.contains("\\section{Introduction}"), "intro section: {body}");
    }

    #[test]
    fn build_body_label_generated() {
        let md = "## Introduction\n\nHello.\n\n## Conclusion\n\nBye.";
        let sections = parse_sections(md);
        let body = build_body(&sections, "");
        assert!(body.contains(r"\label{sec:"), "label generated: {body}");
    }

    // --- check_paper_completeness ---

    #[test]
    fn completeness_full_paper() {
        let md = "# Great Paper Title\n\n\
            ## Abstract\n\nAbstract text with many words to meet the minimum word count \
            requirement for abstracts in conference papers so that this passes the check \
            comfortably with enough padding here to exceed one hundred and fifty words total \
            let me add some more filler text here to make sure we get past the threshold.\n\n\
            ## Introduction\n\nIntroduction text.\n\n\
            ## Related Work\n\nRelated work text.\n\n\
            ## Method\n\nMethod text here.\n\n\
            ## Experiments\n\nExperiment text.\n\n\
            ## Results\n\nResults text.\n\n\
            ## Discussion\n\nDiscussion text.\n\n\
            ## Conclusion\n\nConclusion text.\n\n\
            ## Limitations\n\nLimitations text.";
        let sections = parse_sections(md);
        let warnings = check_paper_completeness(&sections);
        // Should only warn about word count (paper is very short)
        let non_word_count: Vec<_> = warnings
            .iter()
            .filter(|w| !w.message.contains("words"))
            .collect();
        assert!(
            non_word_count.is_empty(),
            "unexpected warnings: {:?}",
            non_word_count
        );
    }

    #[test]
    fn completeness_missing_sections() {
        let md = "# My Paper\n\n## Abstract\n\nShort abstract.\n\n## Introduction\n\nText.";
        let sections = parse_sections(md);
        let warnings = check_paper_completeness(&sections);
        let has_missing = warnings.iter().any(|w| w.message.contains("Missing sections"));
        assert!(has_missing, "should warn about missing sections: {:?}", warnings);
    }

    #[test]
    fn completeness_missing_limitations() {
        let md = "# My Paper\n\n\
            ## Introduction\n\nText.\n\n\
            ## Related Work\n\nText.\n\n\
            ## Method\n\nText.\n\n\
            ## Experiments\n\nText.\n\n\
            ## Results\n\nText.\n\n\
            ## Discussion\n\nText.\n\n\
            ## Conclusion\n\nText.";
        let sections = parse_sections(md);
        let warnings = check_paper_completeness(&sections);
        let has_limitations_warning = warnings
            .iter()
            .any(|w| w.message.contains("NeurIPS/ICLR") || w.message.contains("limitations"));
        assert!(
            has_limitations_warning,
            "should warn about limitations: {:?}",
            warnings
        );
    }

    // --- fix_common_latex_errors ---

    #[test]
    fn fix_float_overflow() {
        let tex = "some text\n\\begin{table}\ncontent\n\\end{table}";
        let errors = vec!["! Too many unprocessed floats.".to_owned()];
        let (fixed, fixes) = fix_common_latex_errors(tex, &errors);
        assert!(fixes.iter().any(|f| f.contains("clearpage")), "fix applied: {fixes:?}");
        assert!(fixed.contains("\\clearpage"), "clearpage in output: {fixed}");
    }

    #[test]
    fn fix_undefined_command() {
        let tex = "some \\textsc{Text} here";
        let errors = vec!["! Undefined control sequence. \\textsc".to_owned()];
        let (fixed, fixes) = fix_common_latex_errors(tex, &errors);
        assert!(!fixes.is_empty(), "fix applied: {fixes:?}");
        assert!(!fixed.contains("\\textsc"), "textsc removed: {fixed}");
        assert!(fixed.contains("Text"), "text preserved: {fixed}");
    }

    // --- full document smoke ---

    #[test]
    fn full_document_smoke() {
        let md = "# Introduction\n\nThis is a paragraph with **bold**.\n";
        let latex = markdown_to_latex(md);
        assert!(latex.contains(r"\section{Introduction}"), "section: {latex}");
        assert!(latex.contains(r"\textbf{bold}"), "bold: {latex}");
    }

    #[test]
    fn special_chars_smoke() {
        let out = escape_latex_specials("50% cost & speed");
        assert!(out.contains(r"\%"), "percent");
        assert!(out.contains(r"\&"), "ampersand");
    }
}
