//! Publication-quality visualization engine.
//!
//! Port of the Python `visualize.py` using the [`plotters`] crate.
//! Generates colorblind-safe charts with Paul Tol bright palette and
//! academic styling (serif font, 300 DPI).

use plotters::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Paul Tol bright palette (colorblind-safe, publication-ready)
// ---------------------------------------------------------------------------

const PAUL_TOL_BRIGHT: [RGBColor; 7] = [
    RGBColor(0x44, 0x77, 0xAA), // blue
    RGBColor(0xEE, 0x66, 0x77), // red/pink
    RGBColor(0x22, 0x88, 0x33), // green
    RGBColor(0xCC, 0xBB, 0x44), // yellow
    RGBColor(0x66, 0xCC, 0xEE), // cyan
    RGBColor(0xAA, 0x33, 0x77), // purple
    RGBColor(0xBB, 0xBB, 0xBB), // grey
];

// ---------------------------------------------------------------------------
// Excluded metrics -- timing/meta metrics that should not be charted
// ---------------------------------------------------------------------------

/// Exact-match metric names to exclude.
const EXCLUDED_EXACT: &[&str] = &[
    "success_rate",
    "seed",
    "num_conditions",
    "calibration_iterations",
];

/// Substring prefixes that mark a metric as excluded.
const EXCLUDED_PREFIXES: &[&str] = &["time_", "runtime_", "elapsed_", "wall_"];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Statistical summary of a single metric across seeds/runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricStats {
    pub mean: f64,
    pub std: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
}

/// Summary for one experimental condition containing all its metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionSummary {
    pub name: String,
    pub metrics: HashMap<String, MetricStats>,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Returns `true` if `name` is a timing or meta metric that should be
/// excluded from comparison charts.
pub fn is_excluded_metric(name: &str) -> bool {
    let low = name.to_lowercase();
    if EXCLUDED_EXACT.contains(&low.as_str()) {
        return true;
    }
    EXCLUDED_PREFIXES.iter().any(|p| low.starts_with(p))
}

/// Shorten a label to at most `max_len` characters, appending an ellipsis
/// character (`\u{2026}`) when truncation is needed.
pub fn shorten_label(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        return name.to_string();
    }
    // Take max_len - 1 characters and append ellipsis
    let truncated: String = name.chars().take(max_len - 1).collect();
    format!("{truncated}\u{2026}")
}

/// Format a condition name for display: underscores become spaces, title case.
pub fn format_cond_name(name: &str) -> String {
    name.replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    let rest: String = chars.as_str().to_lowercase();
                    format!("{upper}{rest}")
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Plotting
// ---------------------------------------------------------------------------

/// Generate a bar chart comparing conditions on a single metric with
/// mean +/- 95% CI error bars.
///
/// Returns `Some(output_path)` on success, `None` if the chart could not be
/// generated (e.g. no data for the requested metric).
pub fn plot_condition_comparison(
    summaries: &[ConditionSummary],
    output_path: &Path,
    metric_key: &str,
    title: &str,
) -> Option<PathBuf> {
    // Collect data for conditions that have the requested metric.
    let mut names: Vec<String> = Vec::new();
    let mut means: Vec<f64> = Vec::new();
    let mut ci_lows: Vec<f64> = Vec::new();
    let mut ci_highs: Vec<f64> = Vec::new();

    for s in summaries {
        if let Some(stats) = s.metrics.get(metric_key) {
            names.push(format_cond_name(&s.name));
            means.push(stats.mean);
            ci_lows.push(stats.ci95_low);
            ci_highs.push(stats.ci95_high);
        }
    }

    if names.is_empty() {
        return None;
    }

    // Ensure parent directory exists.
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let n = names.len();

    // Compute chart bounds.
    let y_max = ci_highs
        .iter()
        .fold(f64::NEG_INFINITY, |acc, &v| acc.max(v));
    let y_upper = y_max * 1.18; // headroom for value labels

    let img_width: u32 = (100 + n as u32 * 120).max(640);
    let img_height: u32 = 480;

    let root = BitMapBackend::new(output_path, (img_width, img_height)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let metric_label = metric_key.replace('_', " ");
    let chart_title = if title.is_empty() {
        format!("{metric_label} Comparison (Mean +/- 95% CI)")
    } else {
        title.to_string()
    };

    let x_labels: Vec<String> = names.clone();

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 22))
        .margin(15)
        .x_label_area_size(60)
        .y_label_area_size(60)
        .build_cartesian_2d(
            (0..n - 1).into_segmented(),
            0.0..y_upper,
        )
        .ok()?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .y_desc(&metric_label)
        .x_desc("Method / Condition")
        .x_labels(n)
        .x_label_formatter(&|seg| {
            if let SegmentValue::CenterOf(idx) = seg {
                x_labels.get(*idx).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        })
        .y_label_formatter(&|v| format!("{v:.3}"))
        .label_style(("serif", 13))
        .draw()
        .ok()?;

    // Draw bars.
    chart
        .draw_series((0..n).map(|i| {
            let color = PAUL_TOL_BRIGHT[i % PAUL_TOL_BRIGHT.len()];
            Rectangle::new(
                [
                    (SegmentValue::Exact(i), 0.0),
                    (SegmentValue::Exact(i + 1), means[i]),
                ],
                color.mix(0.88).filled(),
            )
        }))
        .ok()?;

    // Draw error bars (CI lines).
    for i in 0..n {
        let lo = ci_lows[i].max(0.0);
        let hi = ci_highs[i];
        let color = RGBColor(0x33, 0x33, 0x33);

        // Vertical CI line
        chart
            .draw_series(std::iter::once(PathElement::new(
                vec![
                    (SegmentValue::CenterOf(i), lo),
                    (SegmentValue::CenterOf(i), hi),
                ],
                color.stroke_width(2),
            )))
            .ok()?;
    }

    // Value labels above bars.
    for i in 0..n {
        let label = format!("{:.3}", means[i]);
        let y_pos = ci_highs[i] + y_max * 0.025;
        chart
            .draw_series(std::iter::once(Text::new(
                label,
                (SegmentValue::CenterOf(i), y_pos),
                ("serif", 14).into_font().color(&RGBColor(0x33, 0x33, 0x33)),
            )))
            .ok()?;
    }

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_metrics() {
        assert!(is_excluded_metric("time_total"));
        assert!(is_excluded_metric("runtime_sec"));
        assert!(is_excluded_metric("seed"));
        assert!(!is_excluded_metric("accuracy"));
        assert!(!is_excluded_metric("f1_score"));
    }

    #[test]
    fn shorten_label_long() {
        assert_eq!(
            shorten_label("very_long_metric_name_here", 10),
            "very_long\u{2026}"
        );
    }

    #[test]
    fn shorten_label_short_unchanged() {
        assert_eq!(shorten_label("acc", 22), "acc");
    }

    #[test]
    fn format_cond_name_replaces_underscores() {
        assert_eq!(format_cond_name("no_attention"), "No Attention");
    }

    #[test]
    fn plot_creates_png() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("chart.png");
        let summaries = vec![
            ConditionSummary {
                name: "baseline".into(),
                metrics: {
                    let mut m = HashMap::new();
                    m.insert(
                        "accuracy".into(),
                        MetricStats {
                            mean: 0.85,
                            std: 0.02,
                            ci95_low: 0.83,
                            ci95_high: 0.87,
                        },
                    );
                    m
                },
            },
            ConditionSummary {
                name: "proposed".into(),
                metrics: {
                    let mut m = HashMap::new();
                    m.insert(
                        "accuracy".into(),
                        MetricStats {
                            mean: 0.92,
                            std: 0.01,
                            ci95_low: 0.91,
                            ci95_high: 0.93,
                        },
                    );
                    m
                },
            },
        ];
        let result = plot_condition_comparison(&summaries, &out, "accuracy", "Test Chart");
        assert!(result.is_some());
        assert!(out.exists());
    }
}
