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
// plot_metric_heatmap
// ---------------------------------------------------------------------------

/// Generate a heatmap showing normalized metric values across conditions.
///
/// Each column (metric) is normalized to \[0, 1\] via min-max scaling.
/// Returns `Some(output_path)` on success.
pub fn plot_metric_heatmap(
    summaries: &[ConditionSummary],
    output_path: &Path,
    title: &str,
    max_metrics: usize,
) -> Option<PathBuf> {
    if summaries.is_empty() {
        return None;
    }

    // Collect all metric names present in any condition, excluding timing metrics.
    let mut all_metrics: Vec<String> = summaries
        .iter()
        .flat_map(|s| s.metrics.keys().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .filter(|m| !is_excluded_metric(m))
        .collect();
    all_metrics.sort();
    all_metrics.truncate(max_metrics);

    if all_metrics.is_empty() {
        return None;
    }

    let n_cond = summaries.len();
    let n_met = all_metrics.len();

    // Build raw matrix: rows = conditions, cols = metrics.
    let mut matrix = vec![vec![f64::NAN; n_met]; n_cond];
    for (i, s) in summaries.iter().enumerate() {
        for (j, mname) in all_metrics.iter().enumerate() {
            if let Some(stats) = s.metrics.get(mname) {
                matrix[i][j] = stats.mean;
            }
        }
    }

    // Normalize each column to [0, 1].
    for j in 0..n_met {
        let vals: Vec<f64> = matrix.iter().map(|row| row[j]).filter(|v| v.is_finite()).collect();
        if vals.is_empty() {
            continue;
        }
        let mn = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = mx - mn;
        for row in &mut matrix {
            if row[j].is_finite() && range > 1e-15 {
                row[j] = (row[j] - mn) / range;
            } else if row[j].is_finite() {
                row[j] = 0.5;
            }
        }
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let img_w = (120 + n_met as u32 * 80).max(640);
    let img_h = (80 + n_cond as u32 * 60).max(400);

    let root = BitMapBackend::new(output_path, (img_w, img_h)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let chart_title = if title.is_empty() {
        "Performance Heatmap (Normalized)".to_string()
    } else {
        title.to_string()
    };

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 20))
        .margin(15)
        .x_label_area_size(80)
        .y_label_area_size(120)
        .build_cartesian_2d(0..n_met, 0..n_cond)
        .ok()?;

    chart
        .configure_mesh()
        .disable_mesh()
        .x_labels(n_met)
        .y_labels(n_cond)
        .x_label_formatter(&|idx| {
            all_metrics.get(*idx).map(|s| shorten_label(s, 12)).unwrap_or_default()
        })
        .y_label_formatter(&|idx| {
            summaries.get(*idx).map(|s| format_cond_name(&s.name)).unwrap_or_default()
        })
        .label_style(("serif", 12))
        .draw()
        .ok()?;

    // Draw cells.
    for i in 0..n_cond {
        for j in 0..n_met {
            let val = matrix[i][j];
            if !val.is_finite() {
                continue;
            }
            // Interpolate from blue (0) to red (1).
            let r = (val * 220.0) as u8;
            let b = ((1.0 - val) * 220.0) as u8;
            let g = 60;
            let color = RGBColor(r, g, b);
            chart
                .draw_series(std::iter::once(Rectangle::new(
                    [(j, i), (j + 1, i + 1)],
                    color.filled(),
                )))
                .ok()?;

            // Value label.
            let label = format!("{:.2}", val);
            chart
                .draw_series(std::iter::once(Text::new(
                    label,
                    (j, i),
                    ("serif", 11).into_font().color(&WHITE),
                )))
                .ok()?;
        }
    }

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// plot_ablation_deltas
// ---------------------------------------------------------------------------

/// Horizontal bar chart showing percentage delta from baseline per condition.
///
/// Bars extend left (worse) or right (better) from zero.
pub fn plot_ablation_deltas(
    summaries: &[ConditionSummary],
    output_path: &Path,
    metric_key: &str,
    baseline_name: &str,
    title: &str,
    higher_is_better: bool,
) -> Option<PathBuf> {
    if summaries.is_empty() {
        return None;
    }

    // Find baseline.
    let baseline = if baseline_name.is_empty() {
        summaries
            .iter()
            .find(|s| {
                let low = s.name.to_lowercase();
                low.contains("baseline") || low.contains("control") || low.contains("vanilla")
            })
            .or_else(|| summaries.first())
    } else {
        summaries.iter().find(|s| s.name == baseline_name)
    };

    let baseline_val = baseline?.metrics.get(metric_key)?.mean;
    if baseline_val.abs() < 1e-15 {
        return None;
    }

    let mut names: Vec<String> = Vec::new();
    let mut deltas: Vec<f64> = Vec::new();

    for s in summaries {
        if let Some(stats) = s.metrics.get(metric_key) {
            if baseline.map(|b| b.name == s.name).unwrap_or(false) {
                continue;
            }
            let pct = (stats.mean - baseline_val) / baseline_val.abs() * 100.0;
            let delta = if higher_is_better { pct } else { -pct };
            names.push(format_cond_name(&s.name));
            deltas.push(delta);
        }
    }

    if names.is_empty() {
        return None;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let n = names.len();
    let abs_max = deltas.iter().map(|d| d.abs()).fold(0.0_f64, f64::max) * 1.3;

    let img_w: u32 = 720;
    let img_h: u32 = (80 + n as u32 * 50).max(360);

    let root = BitMapBackend::new(output_path, (img_w, img_h)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let chart_title = if title.is_empty() {
        format!("{} Ablation Deltas (%)", metric_key.replace('_', " "))
    } else {
        title.to_string()
    };

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 20))
        .margin(15)
        .x_label_area_size(40)
        .y_label_area_size(120)
        .build_cartesian_2d(-abs_max..abs_max, 0..n)
        .ok()?;

    chart
        .configure_mesh()
        .disable_y_mesh()
        .x_desc("Delta (%)")
        .y_labels(n)
        .y_label_formatter(&|idx| names.get(*idx).cloned().unwrap_or_default())
        .label_style(("serif", 12))
        .draw()
        .ok()?;

    for i in 0..n {
        let color = if deltas[i] >= 0.0 {
            PAUL_TOL_BRIGHT[2] // green
        } else {
            PAUL_TOL_BRIGHT[1] // red
        };
        chart
            .draw_series(std::iter::once(Rectangle::new(
                [(0.0, i), (deltas[i], i + 1)],
                color.mix(0.85).filled(),
            )))
            .ok()?;
    }

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// plot_metric_trajectory
// ---------------------------------------------------------------------------

/// A refinement iteration entry for trajectory plotting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationRun {
    pub iteration: usize,
    pub metric_value: f64,
}

/// Line chart plotting metric values across refinement iterations.
pub fn plot_metric_trajectory(
    runs: &[IterationRun],
    metric_key: &str,
    output_path: &Path,
    title: &str,
) -> Option<PathBuf> {
    if runs.is_empty() {
        return None;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let x_max = runs.iter().map(|r| r.iteration).max().unwrap_or(1);
    let y_min = runs.iter().map(|r| r.metric_value).fold(f64::INFINITY, f64::min);
    let y_max = runs.iter().map(|r| r.metric_value).fold(f64::NEG_INFINITY, f64::max);
    let y_range = (y_max - y_min).max(1e-6);

    let root = BitMapBackend::new(output_path, (720, 480)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let chart_title = if title.is_empty() {
        format!("{} Trajectory", metric_key.replace('_', " "))
    } else {
        title.to_string()
    };

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 22))
        .margin(15)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..x_max + 1, (y_min - y_range * 0.1)..(y_max + y_range * 0.1))
        .ok()?;

    chart
        .configure_mesh()
        .x_desc("Iteration")
        .y_desc(&metric_key.replace('_', " "))
        .label_style(("serif", 13))
        .draw()
        .ok()?;

    let points: Vec<(usize, f64)> = runs.iter().map(|r| (r.iteration, r.metric_value)).collect();

    chart
        .draw_series(LineSeries::new(points.clone(), PAUL_TOL_BRIGHT[0].stroke_width(2)))
        .ok()?;

    chart
        .draw_series(points.iter().map(|&(x, y)| {
            Circle::new((x, y), 4, PAUL_TOL_BRIGHT[0].filled())
        }))
        .ok()?;

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// plot_experiment_comparison
// ---------------------------------------------------------------------------

/// Grouped bar chart comparing mean/min/max ranges across metrics.
pub fn plot_experiment_comparison(
    summaries: &[ConditionSummary],
    output_path: &Path,
    title: &str,
) -> Option<PathBuf> {
    if summaries.is_empty() {
        return None;
    }

    // Collect common metrics, filter excluded, limit to 12.
    let mut all_metrics: Vec<String> = summaries
        .iter()
        .flat_map(|s| s.metrics.keys().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .filter(|m| !is_excluded_metric(m))
        .collect();
    all_metrics.sort();
    all_metrics.truncate(12);

    if all_metrics.is_empty() {
        return None;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let n_met = all_metrics.len();
    let n_cond = summaries.len();

    let y_max: f64 = summaries
        .iter()
        .flat_map(|s| all_metrics.iter().filter_map(move |m| s.metrics.get(m).map(|st| st.ci95_high)))
        .fold(0.0_f64, f64::max)
        * 1.2;

    let img_w = (120 + (n_met * n_cond) as u32 * 40).max(720);
    let root = BitMapBackend::new(output_path, (img_w, 480)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let chart_title = if title.is_empty() {
        "Experiment Results Comparison".to_string()
    } else {
        title.to_string()
    };

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 20))
        .margin(15)
        .x_label_area_size(80)
        .y_label_area_size(60)
        .build_cartesian_2d(
            (0..n_met * n_cond).into_segmented(),
            0.0..y_max,
        )
        .ok()?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .x_labels(n_met)
        .x_label_formatter(&|seg| {
            if let SegmentValue::CenterOf(idx) = seg {
                let met_idx = idx / n_cond;
                all_metrics.get(met_idx).map(|s| shorten_label(s, 10)).unwrap_or_default()
            } else {
                String::new()
            }
        })
        .label_style(("serif", 12))
        .draw()
        .ok()?;

    for (ci, s) in summaries.iter().enumerate() {
        let color = PAUL_TOL_BRIGHT[ci % PAUL_TOL_BRIGHT.len()];
        for (mi, mname) in all_metrics.iter().enumerate() {
            if let Some(stats) = s.metrics.get(mname) {
                let idx = mi * n_cond + ci;
                chart
                    .draw_series(std::iter::once(Rectangle::new(
                        [
                            (SegmentValue::Exact(idx), 0.0),
                            (SegmentValue::Exact(idx + 1), stats.mean),
                        ],
                        color.mix(0.85).filled(),
                    )))
                    .ok()?;
            }
        }
    }

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// plot_pipeline_timeline
// ---------------------------------------------------------------------------

/// A stage timing entry for timeline plotting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageTimingEntry {
    pub stage_name: String,
    pub elapsed_secs: f64,
    pub status: String, // "done" | "failed" | "skipped"
}

/// Horizontal bar chart showing execution time per pipeline stage,
/// color-coded by status.
pub fn plot_pipeline_timeline(
    stages: &[StageTimingEntry],
    output_path: &Path,
    title: &str,
) -> Option<PathBuf> {
    if stages.is_empty() {
        return None;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let n = stages.len();
    let x_max = stages.iter().map(|s| s.elapsed_secs).fold(0.0_f64, f64::max) * 1.15;

    let img_h = (60 + n as u32 * 32).max(400);
    let root = BitMapBackend::new(output_path, (720, img_h)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let chart_title = if title.is_empty() {
        "Pipeline Execution Timeline".to_string()
    } else {
        title.to_string()
    };

    let names: Vec<String> = stages.iter().map(|s| s.stage_name.clone()).collect();

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 20))
        .margin(15)
        .x_label_area_size(40)
        .y_label_area_size(140)
        .build_cartesian_2d(0.0..x_max, 0..n)
        .ok()?;

    chart
        .configure_mesh()
        .disable_y_mesh()
        .x_desc("Seconds")
        .y_labels(n)
        .y_label_formatter(&|idx| names.get(*idx).cloned().unwrap_or_default())
        .label_style(("serif", 11))
        .draw()
        .ok()?;

    for (i, stage) in stages.iter().enumerate() {
        let color = match stage.status.as_str() {
            "done" => PAUL_TOL_BRIGHT[2],   // green
            "failed" => PAUL_TOL_BRIGHT[1],  // red
            _ => PAUL_TOL_BRIGHT[6],         // grey
        };
        chart
            .draw_series(std::iter::once(Rectangle::new(
                [(0.0, i), (stage.elapsed_secs, i + 1)],
                color.mix(0.85).filled(),
            )))
            .ok()?;
    }

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// plot_iteration_scores
// ---------------------------------------------------------------------------

/// Line chart of quality scores across iterations with optional pass threshold.
pub fn plot_iteration_scores(
    scores: &[f64],
    output_path: &Path,
    threshold: f64,
    title: &str,
) -> Option<PathBuf> {
    if scores.is_empty() {
        return None;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let n = scores.len();
    let y_min = scores.iter().cloned().fold(f64::INFINITY, f64::min).min(0.0);
    let y_max = scores
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max)
        .max(threshold)
        * 1.15;

    let root = BitMapBackend::new(output_path, (720, 480)).into_drawing_area();
    root.fill(&WHITE).ok()?;

    let chart_title = if title.is_empty() {
        "Quality Score by Iteration".to_string()
    } else {
        title.to_string()
    };

    let mut chart = ChartBuilder::on(&root)
        .caption(&chart_title, ("serif", 22))
        .margin(15)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..n, y_min..y_max)
        .ok()?;

    chart
        .configure_mesh()
        .x_desc("Iteration")
        .y_desc("Quality Score")
        .label_style(("serif", 13))
        .draw()
        .ok()?;

    // Threshold line.
    chart
        .draw_series(LineSeries::new(
            vec![(0, threshold), (n - 1, threshold)],
            RED.stroke_width(1),
        ))
        .ok()?
        .label(format!("threshold = {threshold:.1}"))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED.stroke_width(1)));

    // Score line.
    let points: Vec<(usize, f64)> = scores.iter().enumerate().map(|(i, &v)| (i, v)).collect();
    chart
        .draw_series(LineSeries::new(
            points.clone(),
            PAUL_TOL_BRIGHT[0].stroke_width(2),
        ))
        .ok()?;

    // Points.
    chart
        .draw_series(points.iter().map(|&(x, y)| {
            Circle::new((x, y), 4, PAUL_TOL_BRIGHT[0].filled())
        }))
        .ok()?;

    chart
        .configure_series_labels()
        .border_style(BLACK)
        .background_style(WHITE.mix(0.8))
        .label_font(("serif", 12))
        .draw()
        .ok()?;

    root.present().ok()?;
    Some(output_path.to_path_buf())
}

// ---------------------------------------------------------------------------
// generate_all_charts
// ---------------------------------------------------------------------------

/// Orchestrator: scan a run directory for data files and generate all
/// applicable charts, returning the paths to generated PNGs.
pub fn generate_all_charts(
    run_dir: &Path,
    output_dir: Option<&Path>,
    metric_key: &str,
    _metric_direction: &str,
) -> Vec<PathBuf> {
    let charts_dir = output_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| run_dir.join("charts"));
    let _ = std::fs::create_dir_all(&charts_dir);

    let mut generated: Vec<PathBuf> = Vec::new();

    // Try to load pipeline_summary.json for timeline chart.
    let summary_path = run_dir.join("pipeline_summary.json");
    if summary_path.exists() {
        if let Ok(text) = std::fs::read_to_string(&summary_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(stages_arr) = val.get("stages").and_then(|s| s.as_array()) {
                    let entries: Vec<StageTimingEntry> = stages_arr
                        .iter()
                        .filter_map(|s| {
                            Some(StageTimingEntry {
                                stage_name: s.get("stage")?.as_str()?.to_string(),
                                elapsed_secs: s.get("elapsed_secs")?.as_f64()?,
                                status: s
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("done")
                                    .to_string(),
                            })
                        })
                        .collect();
                    if let Some(p) =
                        plot_pipeline_timeline(&entries, &charts_dir.join("timeline.png"), "")
                    {
                        generated.push(p);
                    }
                }
            }
        }
    }

    // Try to load experiment results for condition comparison / heatmap / ablation.
    let results_path = run_dir.join("experiment_summary.json");
    if results_path.exists() {
        if let Ok(text) = std::fs::read_to_string(&results_path) {
            if let Ok(summaries) = serde_json::from_str::<Vec<ConditionSummary>>(&text) {
                if let Some(p) = plot_condition_comparison(
                    &summaries,
                    &charts_dir.join("condition_comparison.png"),
                    metric_key,
                    "",
                ) {
                    generated.push(p);
                }
                if let Some(p) =
                    plot_metric_heatmap(&summaries, &charts_dir.join("heatmap.png"), "", 12)
                {
                    generated.push(p);
                }
                if let Some(p) = plot_ablation_deltas(
                    &summaries,
                    &charts_dir.join("ablation_deltas.png"),
                    metric_key,
                    "",
                    "",
                    true,
                ) {
                    generated.push(p);
                }
                if let Some(p) =
                    plot_experiment_comparison(&summaries, &charts_dir.join("comparison.png"), "")
                {
                    generated.push(p);
                }
            }
        }
    }

    generated
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
