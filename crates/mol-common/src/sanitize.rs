//! Input sanitization utilities for untrusted LLM-generated values.
//!
//! Combines two Python modules:
//! - `researchclaw/utils/sanitize.py` — figure ID and filename sanitization
//! - `researchclaw/utils/thinking_tags.py` — stripping reasoning artifacts
//!   from LLM output before they contaminate paper drafts, code, or YAML.

use std::sync::LazyLock;

use regex::Regex;

// ---------------------------------------------------------------------------
// Thinking-tag patterns
// ---------------------------------------------------------------------------

/// XML-style `<think>...</think>` (DeepSeek-R1, QwQ, Gemini 2.5).
static THINK_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?si)<think>.*?</think>").expect("THINK_BLOCK_RE")
});

/// Unclosed `<think>` — everything from the tag to end-of-string.
static THINK_UNCLOSED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?si)<think>.*").expect("THINK_UNCLOSED_RE")
});

/// Stray closing `</think>` tag left after a previous substitution.
static THINK_STRAY_CLOSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)</think>").expect("THINK_STRAY_CLOSE_RE")
});

/// Single-line `[thinking] ...` (Claude Code / ACP format).
/// Handles the common case: one `[thinking]` marker per line.
static BRACKET_THINKING_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)^\[thinking\].*$").expect("BRACKET_THINKING_LINE_RE")
});

/// Unicode insight blocks (Claude Code explanatory style).
static INSIGHT_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)`[*\u{2605}]\s*Insight[^`]*`\s*\n.*?`[\u{2500}-\u{257F}]+`")
        .expect("INSIGHT_BLOCK_RE")
});

/// ASCII insight blocks.
static INSIGHT_ASCII_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)`\*\s*Insight[-]+`\s*\n.*?`[-]+`").expect("INSIGHT_ASCII_RE")
});

/// `[plan] ...` blocks (Claude Code plan mode).
static PLAN_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?si)\[plan\].*?(?=\n\n|\z)").expect("PLAN_BLOCK_RE")
});

/// ACP/acpx metadata lines: `[client]`, `[acpx]`, `[tool]`, `[done]`.
/// Matches lines that start with the tag directly followed by a space, end-of-line,
/// or a non-paren character (avoids matching Markdown links like `[tool](url)`).
static ACPX_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Match lines where the bracket tag is NOT followed by '(' (Markdown link syntax).
    // We do this by requiring either whitespace, end-of-line, or a non-paren after the tag.
    Regex::new(r"(?im)^\[(client|acpx|tool|done)\]([^(\n].*)?$").expect("ACPX_LINE_RE")
});

/// Unicode box-drawing separator lines.
static BOX_SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^`[\u{2500}-\u{257F}]+`\s*$").expect("BOX_SEPARATOR_RE")
});

/// ASCII long-dash separator lines (`---...---` inside backticks).
static DASH_SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^`[-]{20,}`\s*$").expect("DASH_SEPARATOR_RE")
});

/// Three or more consecutive blank lines.
static MULTI_BLANK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\n{3,}").expect("MULTI_BLANK_RE")
});

// ---------------------------------------------------------------------------
// API key / secret patterns
// ---------------------------------------------------------------------------

/// Common API key patterns to redact.
static API_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches common key formats:
    //   sk-...   (OpenAI, Anthropic)
    //   Bearer <token>
    //   api_key / apikey / api-key = <value>
    //   token = <value>
    Regex::new(
        r#"(?xi)
        (
          (?:sk-[A-Za-z0-9\-_]{20,})           # OpenAI / Anthropic style
          | (?:Bearer\s+[A-Za-z0-9\-_.+/]{20,}) # Bearer token
          | (?:(?:api[_\-]?key|token|secret)\s*[=:]\s*["']?[A-Za-z0-9\-_.+/]{16,}["']?)
        )
        "#,
    )
    .expect("API_KEY_RE")
});

// ---------------------------------------------------------------------------
// Public API — thinking-tag stripping
// ---------------------------------------------------------------------------

/// Remove all reasoning artifacts from LLM output.
///
/// Handles:
/// - XML `<think>...</think>` blocks (DeepSeek-R1, QwQ, Gemini 2.5)
/// - `[thinking]` blocks (Claude Code / ACP)
/// - Insight decorators (Claude Code explanatory mode)
/// - `[plan]` blocks (Claude Code plan mode)
/// - `[client]` / `[acpx]` / `[tool]` / `[done]` metadata lines
///
/// Returns cleaned text suitable for paper drafts, code, or YAML/JSON.
pub fn strip_thinking_tags(text: &str) -> String {
    if text.is_empty() {
        return text.to_owned();
    }

    let mut result = text.to_owned();

    // Phase 1: XML <think>...</think>
    if result.to_lowercase().contains("think") {
        result = THINK_BLOCK_RE.replace_all(&result, "").into_owned();
        result = THINK_UNCLOSED_RE.replace_all(&result, "").into_owned();
        result = THINK_STRAY_CLOSE_RE.replace_all(&result, "").into_owned();
    }

    // Phase 2: [thinking] blocks (line-by-line)
    if result.to_lowercase().contains("[thinking]") {
        result = BRACKET_THINKING_LINE_RE.replace_all(&result, "").into_owned();
    }

    // Phase 3: Insight blocks
    result = INSIGHT_BLOCK_RE.replace_all(&result, "").into_owned();
    result = INSIGHT_ASCII_RE.replace_all(&result, "").into_owned();

    // Phase 4: [plan] blocks
    if result.to_lowercase().contains("[plan]") {
        result = PLAN_BLOCK_RE.replace_all(&result, "").into_owned();
    }

    // Phase 5: ACP metadata lines
    result = ACPX_LINE_RE.replace_all(&result, "").into_owned();

    // Phase 6: Leftover separator lines
    result = BOX_SEPARATOR_RE.replace_all(&result, "").into_owned();
    result = DASH_SEPARATOR_RE.replace_all(&result, "").into_owned();

    // Phase 7: Collapse excessive blank lines
    result = MULTI_BLANK_RE.replace_all(&result, "\n\n").into_owned();

    result.trim().to_owned()
}

// ---------------------------------------------------------------------------
// Public API — API key redaction
// ---------------------------------------------------------------------------

/// Redact API keys and secrets in `text`, replacing them with `[REDACTED]`.
pub fn redact_api_keys(text: &str) -> String {
    API_KEY_RE.replace_all(text, "[REDACTED]").into_owned()
}

// ---------------------------------------------------------------------------
// Public API — filename / ID sanitization
// ---------------------------------------------------------------------------

/// Sanitize a figure ID for safe use in file paths and Docker container names.
///
/// Strips path separators, dotdot sequences, and shell metacharacters.
/// Returns `fallback` if the sanitized result is empty.
///
/// # Examples
///
/// ```
/// use mol_common::sanitize::sanitize_figure_id;
/// assert_eq!(sanitize_figure_id("../../etc/evil", "figure"), "etc_evil");
/// assert_eq!(sanitize_figure_id("fig test (v2)", "figure"), "fig_test_v2");
/// assert_eq!(sanitize_figure_id("", "figure"), "figure");
/// ```
pub fn sanitize_figure_id<'a>(raw_id: &str, fallback: &'a str) -> String {
    // Remove dotdot and path separators
    let cleaned = raw_id.replace("..", "").replace('/', "_").replace('\\', "_");
    // Keep only safe chars: alphanumeric, hyphen, underscore, dot
    let cleaned = Regex::new(r"[^a-zA-Z0-9_.\-]")
        .expect("safe-char regex")
        .replace_all(&cleaned, "_")
        .into_owned();
    // Collapse runs of underscores
    let cleaned = Regex::new(r"_+")
        .expect("collapse-underscore regex")
        .replace_all(&cleaned, "_")
        .into_owned();
    let cleaned = cleaned.trim_matches(|c| c == '_' || c == '.').to_owned();
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

/// Sanitize an arbitrary filename for safe use on all platforms.
///
/// - Removes path separators, null bytes, and shell metacharacters.
/// - Collapses consecutive underscores.
/// - Returns `fallback` if the result is empty.
pub fn sanitize_filename<'a>(raw: &str, fallback: &'a str) -> String {
    sanitize_figure_id(raw, fallback)
}

/// Sanitize a project or run ID for use in directory names and database keys.
///
/// Allows alphanumerics, hyphens, and underscores only.
pub fn sanitize_run_id(raw: &str, fallback: &str) -> String {
    let cleaned = Regex::new(r"[^a-zA-Z0-9_\-]")
        .expect("run-id regex")
        .replace_all(raw, "_")
        .into_owned();
    let cleaned = Regex::new(r"_+")
        .expect("collapse-underscore regex")
        .replace_all(&cleaned, "_")
        .into_owned();
    let cleaned = cleaned.trim_matches('_').to_owned();
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

/// Truncate `text` to at most `max_chars` UTF-8 characters, appending `…` if
/// truncated.
pub fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}\u{2026}") // U+2026 HORIZONTAL ELLIPSIS
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- thinking tags ---

    #[test]
    fn strips_xml_think_block() {
        let input = "Before\n<think>inner reasoning</think>\nAfter";
        let out = strip_thinking_tags(input);
        // The think block is removed; surrounding newlines may produce one blank line.
        assert!(!out.contains("<think>"), "think block not removed: {out}");
        assert!(out.contains("Before"), "Before missing: {out}");
        assert!(out.contains("After"), "After missing: {out}");
    }

    #[test]
    fn strips_unclosed_think() {
        let input = "Intro\n<think>reasoning that never ends";
        let out = strip_thinking_tags(input);
        assert_eq!(out, "Intro");
    }

    #[test]
    fn strips_bracket_thinking() {
        let input = "Text\n[thinking] This is internal.\n\nFinal answer.";
        let out = strip_thinking_tags(input);
        assert!(!out.contains("[thinking]"), "got: {out}");
        assert!(out.contains("Final answer."));
    }

    #[test]
    fn strips_acpx_lines() {
        let input = "[client] some metadata\n[done]\nReal content";
        let out = strip_thinking_tags(input);
        assert!(!out.contains("[client]"));
        assert!(!out.contains("[done]"));
        assert!(out.contains("Real content"));
    }

    #[test]
    fn empty_string_passthrough() {
        assert_eq!(strip_thinking_tags(""), "");
    }

    // --- API key redaction ---

    #[test]
    fn redacts_sk_key() {
        let text = "Use key sk-abcdefghijklmnopqrstuvwxyz123 to call the API.";
        let out = redact_api_keys(text);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("sk-abc"));
    }

    // --- filename sanitization ---

    #[test]
    fn sanitize_figure_id_dotdot() {
        assert_eq!(sanitize_figure_id("../../etc/evil", "figure"), "etc_evil");
    }

    #[test]
    fn sanitize_figure_id_spaces() {
        assert_eq!(sanitize_figure_id("fig test (v2)", "figure"), "fig_test_v2");
    }

    #[test]
    fn sanitize_figure_id_empty() {
        assert_eq!(sanitize_figure_id("", "figure"), "figure");
    }

    #[test]
    fn sanitize_run_id_special_chars() {
        assert_eq!(sanitize_run_id("run/1@test", "run"), "run_1_test");
    }

    // --- truncation ---

    #[test]
    fn truncate_short_text() {
        assert_eq!(truncate_text("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_text() {
        let out = truncate_text("hello world", 5);
        assert!(out.starts_with("hell"));
        assert!(out.contains('\u{2026}'));
    }
}
