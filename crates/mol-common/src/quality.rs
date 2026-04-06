//! Content quality assessment — template detection and metrics.
//!
//! Detects placeholder/template content in LLM-generated text and provides
//! quality metrics for pipeline outputs.
//!
//! Ported from Python `mol/quality.py`.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Template patterns
// ---------------------------------------------------------------------------

/// A compiled pattern with its human-readable description.
struct TemplatePattern {
    re: Regex,
    desc: &'static str,
}

static TEMPLATE_PATTERNS: LazyLock<Vec<TemplatePattern>> = LazyLock::new(|| {
    let raw: &[(&str, &str)] = &[
        (
            r"(?i)template\s+(abstract|introduction|method|methodology|conclusion|discussion|results|related\s+work)",
            "Template section header",
        ),
        (r"(?i)\[INSERT\s+.*?\]", "Insert placeholder"),
        (r"(?i)\[TODO\s*:?\s*.*?\]", "TODO placeholder"),
        (r"(?i)\[PLACEHOLDER\s*:?\s*.*?\]", "Explicit placeholder"),
        (r"(?i)lorem\s+ipsum", "Lorem ipsum filler"),
        (
            r"(?i)this\s+section\s+will\s+(describe|discuss|present|outline|explain)",
            "Future-tense placeholder",
        ),
        (
            r"(?i)we\s+will\s+(describe|discuss|present|outline|explain)\s+in\s+this\s+section",
            "Future-tense placeholder",
        ),
        (
            r"(?i)add\s+(your|the)\s+(content|text|description)\s+here",
            "Add content placeholder",
        ),
        (r"(?i)replace\s+this\s+(text|content|section)", "Replace placeholder"),
        (r"(?i)^#+\s*section\s+\d+\s*$", "Generic section header"),
        (
            r"(?i)your\s+(abstract|introduction|method|results)\s+goes?\s+here",
            "Content placeholder",
        ),
        (r"(?i)sample\s+(abstract|introduction|text|content)", "Sample content marker"),
    ];

    raw.iter()
        .map(|(pattern, desc)| TemplatePattern {
            re: Regex::new(pattern).expect("template pattern should compile"),
            desc,
        })
        .collect()
});

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single template/placeholder detection within a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateMatch {
    /// Description of the pattern that matched.
    pub pattern_desc: String,
    /// 1-based line number in the source text.
    pub line_number: usize,
    /// Up to 100 characters of the matched text.
    pub excerpt: String,
}

/// Quality assessment for a text document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub total_lines: usize,
    pub total_chars: usize,
    pub template_matches: Vec<TemplateMatch>,
    /// Fraction 0.0–1.0 of the text estimated to be template/placeholder
    /// content. 0.0 = fully original, 1.0 = fully template.
    pub template_ratio: f64,
}

impl QualityReport {
    /// Returns `true` if any template patterns were detected.
    pub fn has_template_content(&self) -> bool {
        !self.template_matches.is_empty()
    }

    /// Number of template matches found.
    pub fn match_count(&self) -> usize {
        self.template_matches.len()
    }
}

/// A scored quality assessment (convenience wrapper).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    pub report: QualityReport,
    /// `true` if the text passes the strict quality gate.
    pub passed: bool,
    /// Human-readable verdict message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Detection logic
// ---------------------------------------------------------------------------

/// Scan `text` for template/placeholder patterns.
///
/// Returns one [`TemplateMatch`] per regex hit (multiple hits per line are
/// possible).
pub fn detect_template_content(text: &str) -> Vec<TemplateMatch> {
    let mut matches = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        for pat in TEMPLATE_PATTERNS.iter() {
            for m in pat.re.find_iter(stripped) {
                let excerpt: String = m.as_str().chars().take(100).collect();
                matches.push(TemplateMatch {
                    pattern_desc: pat.desc.to_owned(),
                    line_number: line_idx + 1,
                    excerpt,
                });
            }
        }
    }
    matches
}

/// Estimate what fraction of `text` is template/placeholder content.
///
/// Heuristic: count characters in lines that match at least one template
/// pattern versus total non-empty character count.
///
/// Returns 0.0 (fully original) to 1.0 (fully template).
pub fn compute_template_ratio(text: &str) -> f64 {
    if text.trim().is_empty() {
        return 0.0;
    }

    let total_chars: usize = text
        .lines()
        .map(|l| l.trim().len())
        .filter(|&n| n > 0)
        .sum();

    if total_chars == 0 {
        return 0.0;
    }

    let template_chars: usize = text
        .lines()
        .map(|l| l.trim())
        .filter(|s| !s.is_empty())
        .filter(|s| TEMPLATE_PATTERNS.iter().any(|p| p.re.is_match(s)))
        .map(|s| s.len())
        .sum();

    (template_chars as f64 / total_chars as f64).min(1.0)
}

/// Run a full quality assessment of `text`.
pub fn assess_quality(text: &str) -> QualityReport {
    let lines: Vec<&str> = text.lines().collect();
    let matches = detect_template_content(text);
    let ratio = compute_template_ratio(text);

    tracing::debug!(
        total_lines = lines.len(),
        total_chars = text.len(),
        match_count = matches.len(),
        template_ratio = format_args!("{ratio:.4}"),
        "quality assessed"
    );

    QualityReport {
        total_lines: lines.len(),
        total_chars: text.len(),
        template_matches: matches,
        template_ratio: ratio,
    }
}

/// Check whether `text` passes a strict quality gate.
///
/// Fails if `template_ratio > threshold` (default 0.05).
///
/// Returns `(passed, message)`.
pub fn check_strict_quality(text: &str, threshold: f64) -> (bool, String) {
    let report = assess_quality(text);

    if report.template_ratio > threshold {
        let details: String = report
            .template_matches
            .iter()
            .take(5)
            .map(|m| format!("L{}: {}", m.line_number, m.excerpt))
            .collect::<Vec<_>>()
            .join("; ");
        let msg = format!(
            "Template content detected: ratio={:.2}%, {} matches. Examples: {}",
            report.template_ratio * 100.0,
            report.match_count(),
            details,
        );
        return (false, msg);
    }

    (
        true,
        format!(
            "Quality check passed: template_ratio={:.2}%",
            report.template_ratio * 100.0
        ),
    )
}

/// Convenience: run [`check_strict_quality`] with the default threshold (5 %).
pub fn check_quality_default(text: &str) -> QualityScore {
    let (passed, message) = check_strict_quality(text, 0.05);
    let report = assess_quality(text);
    QualityScore {
        report,
        passed,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_passes() {
        let text = "This is a well-written abstract about neural network architectures.";
        let (passed, _) = check_strict_quality(text, 0.05);
        assert!(passed);
    }

    #[test]
    fn lorem_ipsum_detected() {
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.";
        let matches = detect_template_content(text);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pattern_desc, "Lorem ipsum filler");
    }

    #[test]
    fn todo_placeholder_detected() {
        let text = "Introduction\n[TODO: add content here]\nConclusion";
        let matches = detect_template_content(text);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].line_number, 2);
    }

    #[test]
    fn high_template_ratio_fails() {
        let text = "Lorem ipsum\n[INSERT description]\n[TODO: fill in]\n[PLACEHOLDER: results]";
        let (passed, msg) = check_strict_quality(text, 0.05);
        assert!(!passed);
        assert!(msg.contains("Template content detected"));
    }

    #[test]
    fn ratio_zero_for_empty_text() {
        assert_eq!(compute_template_ratio(""), 0.0);
        assert_eq!(compute_template_ratio("   \n\n  "), 0.0);
    }
}
