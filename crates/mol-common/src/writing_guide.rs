//! Static knowledge base of conference writing tips.
//!
//! Ported from the Python `writing_guide.py` module. Provides
//! [`CONFERENCE_WRITING_TIPS`] (a category -> tips mapping) and
//! [`format_writing_tips`] for rendering a Markdown summary.

use std::collections::HashMap;

/// Returns the full conference writing tips knowledge base.
///
/// Each key is a snake_case category name and each value is a list of tips.
pub fn conference_writing_tips() -> HashMap<&'static str, Vec<&'static str>> {
    let mut tips: HashMap<&str, Vec<&str>> = HashMap::new();

    tips.insert(
        "title",
        vec![
            "Signal novelty \u{2014} title should hint at what is new",
            "Be specific and concrete, under 15 words",
            "No abbreviations unless universally known",
            "Pattern: '[Finding]: [Evidence]' or '[Method]: [What it does]'",
            "Memeability test: would a reader enjoy telling a colleague about this?",
        ],
    );

    tips.insert(
        "abstract",
        vec![
            "5-sentence structure: (1) problem, (2) prior approaches + limitations, \
(3) your approach + novelty, (4) key results with numbers, (5) implication",
            "150-250 words for ML conferences",
            "Include at least 2 specific quantitative results",
        ],
    );

    tips.insert(
        "figure_1",
        vec![
            "Most important figure in the paper \u{2014} many readers look at Figure 1 first",
            "Should convey the key idea or main result at a glance",
            "Invest significant time in this figure",
        ],
    );

    tips.insert(
        "introduction",
        vec![
            "State contributions clearly as bullet points",
            "Many reviewers stop reading carefully after the intro",
            "Include paper organization paragraph at the end",
        ],
    );

    tips.insert(
        "experiments",
        vec![
            "Strong baselines: tune baselines with the same effort as your method",
            "Ablations: remove one component at a time and measure the effect",
            "Reproducibility: include hyperparameters, seeds, hardware specs",
            "Statistical rigor: report variance, run multiple seeds",
        ],
    );

    tips.insert(
        "common_rejections",
        vec![
            "Weak baselines (79% of rejected papers)",
            "Missing ablations",
            "Overclaiming beyond evidence",
            "Poor reproducibility details",
            "Ignoring limitations",
        ],
    );

    tips.insert(
        "rebuttal",
        vec![
            "Start with positives reviewers identified",
            "Quote reviewers directly, then respond",
            "Provide new data/experiments rather than arguing",
            "Do not promise \u{2014} deliver",
        ],
    );

    tips
}

/// Canonical ordering of categories (matches the original Python dict order).
const CATEGORY_ORDER: &[&str] = &[
    "title",
    "abstract",
    "figure_1",
    "introduction",
    "experiments",
    "common_rejections",
    "rebuttal",
];

/// Format a human-friendly title from a snake_case category name.
fn title_case(s: &str) -> String {
    s.replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render conference writing tips as a Markdown string.
///
/// If `categories` is `None`, all categories are included in canonical order.
/// If `Some`, only the listed categories are rendered (unknown names are
/// silently skipped).
pub fn format_writing_tips(categories: Option<&[&str]>) -> String {
    let tips_map = conference_writing_tips();
    let cats: Vec<&str> = match categories {
        Some(c) => c.to_vec(),
        None => CATEGORY_ORDER.to_vec(),
    };

    let mut lines: Vec<String> = vec!["## Conference Writing Best Practices".to_string()];

    for cat in cats {
        if let Some(tips) = tips_map.get(cat) {
            lines.push(String::new());
            lines.push(format!("### {}", title_case(cat)));
            for tip in tips {
                lines.push(format!("- {tip}"));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_all_categories() {
        let tips = format_writing_tips(None);
        assert!(tips.contains("## Conference Writing Best Practices"));
        assert!(tips.contains("### Title"));
        assert!(tips.contains("### Abstract"));
        assert!(tips.contains("### Experiments"));
        assert!(tips.contains("### Common Rejections"));
        assert!(tips.contains("### Rebuttal"));
    }

    #[test]
    fn format_filtered_categories() {
        let tips = format_writing_tips(Some(&["title", "abstract"]));
        assert!(tips.contains("### Title"));
        assert!(tips.contains("### Abstract"));
        assert!(!tips.contains("### Rebuttal"));
    }

    #[test]
    fn unknown_category_is_skipped() {
        let tips = format_writing_tips(Some(&["nonexistent"]));
        assert!(!tips.contains("###"));
    }
}
