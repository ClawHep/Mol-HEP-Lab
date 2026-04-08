//! Metric parsing utilities for experiment output.
//!
//! Supports three formats (in priority order):
//!   1. `results.json` — structured JSON (recommended)
//!   2. stdout `key: value` lines — legacy regex format
//!   3. `epoch N loss V` style lines for learning curves

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

// ---------------------------------------------------------------------------
// MetricValue
// ---------------------------------------------------------------------------

/// A single parsed metric value.
#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Float(f64),
    Int(i64),
    String(String),
}

impl MetricValue {
    /// Convert to f64, returning `None` for string variants.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            MetricValue::Float(v) => Some(*v),
            MetricValue::Int(v) => Some(*v as f64),
            MetricValue::String(_) => None,
        }
    }
}

impl std::fmt::Display for MetricValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricValue::Float(v) => write!(f, "{v}"),
            MetricValue::Int(v) => write!(f, "{v}"),
            MetricValue::String(s) => write!(f, "{s}"),
        }
    }
}

// ---------------------------------------------------------------------------
// parse_metrics_from_stdout
// ---------------------------------------------------------------------------

/// Parse `metric: value` or `condition=X metric: value` lines from stdout.
///
/// Skips NaN / Inf values — they indicate training divergence.
pub fn parse_metrics_from_stdout(stdout: &str) -> HashMap<String, MetricValue> {
    let mut metrics: HashMap<String, MetricValue> = HashMap::new();
    // Accumulate per-condition composite keys so duplicates get averaged.
    let mut accum: HashMap<String, Vec<f64>> = HashMap::new();

    let float_re = r"[+-]?\d+\.?\d*(?:[eE][+-]?\d+)?";

    // Ratio format: "condition=X [tags] metric: N/M"
    let ratio_pat = regex::Regex::new(&format!(
        r"^condition=(\S+)\s+((?:\S+=\S+\s+)*)(\w[\w.]*)\s*:\s*({float})/({float})\s*$",
        float = float_re
    ))
    .expect("ratio regex");

    // Condition-prefixed: "condition=X [tags] metric: value"
    let cond_pat = regex::Regex::new(&format!(
        r"^condition=(\S+)\s+((?:\S+=\S+\s+)*)(\w[\w.]*)\s*:\s*({float})\s*$",
        float = float_re
    ))
    .expect("condition metric regex");

    // Plain: "metric: value"
    let plain_pat = regex::Regex::new(&format!(
        r"^(?:\S+=\S+\s+)?(\w[\w.]*)\s*:\s*({float})\s*$",
        float = float_re
    ))
    .expect("plain metric regex");

    for line in stdout.lines() {
        let stripped = line.trim();

        // --- Ratio format ---
        if let Some(cap) = ratio_pat.captures(stripped) {
            let cond_name = &cap[1];
            let extra_tags = &cap[2];
            let name = &cap[3];
            let num: f64 = cap[4].parse().unwrap_or(0.0);
            let den: f64 = cap[5].parse().unwrap_or(0.0);
            let val = if den != 0.0 { num / den } else { 0.0 };

            let tag_parts = build_tag_parts(cond_name, extra_tags);
            let composite = tag_parts.join("/");

            accum
                .entry(format!("{composite}/{name}"))
                .or_default()
                .push(val);
            accum
                .entry(format!("{cond_name}/{name}"))
                .or_default()
                .push(val);
            metrics.insert(name.to_string(), MetricValue::Float(val));
            continue;
        }

        // --- Condition-prefixed ---
        if let Some(cap) = cond_pat.captures(stripped) {
            let cond_name = &cap[1];
            let extra_tags = &cap[2];
            let name = &cap[3];
            let val: f64 = match cap[4].parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if val.is_nan() || val.is_infinite() {
                tracing::warn!("Skipping non-finite metric {name}={val}");
                continue;
            }

            let tag_parts = build_tag_parts(cond_name, extra_tags);
            let composite = tag_parts.join("/");

            accum
                .entry(format!("{composite}/{name}"))
                .or_default()
                .push(val);
            accum
                .entry(format!("{cond_name}/{name}"))
                .or_default()
                .push(val);
            metrics.insert(name.to_string(), MetricValue::Float(val));
            continue;
        }

        // --- Plain format ---
        if let Some(cap) = plain_pat.captures(stripped) {
            let name = &cap[1];
            let val: f64 = match cap[2].parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if val.is_nan() || val.is_infinite() {
                tracing::warn!("Skipping non-finite metric {name}={val}");
                continue;
            }
            metrics.insert(name.to_string(), MetricValue::Float(val));
        }
    }

    // Resolve accumulated keys: average duplicates.
    for (key, vals) in &accum {
        let avg = vals.iter().sum::<f64>() / vals.len() as f64;
        metrics.insert(key.clone(), MetricValue::Float(avg));
    }

    metrics
}

fn build_tag_parts(cond_name: &str, extra_tags: &str) -> Vec<String> {
    let mut parts = vec![cond_name.to_string()];
    for tag in extra_tags.split_whitespace() {
        if let Some((_, v)) = tag.split_once('=') {
            parts.push(v.to_string());
        }
    }
    parts
}

// ---------------------------------------------------------------------------
// parse_metrics_from_file
// ---------------------------------------------------------------------------

/// Parse a `results.json` file into a metric map.
///
/// Handles the same structured JSON format as the Python `UniversalMetricParser`.
pub fn parse_metrics_from_file(path: &Path) -> Result<HashMap<String, MetricValue>> {
    let text = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;

    let mut metrics = HashMap::new();
    flatten_json_value("", &value, &mut metrics);
    Ok(metrics)
}

/// Recursively flatten a JSON object into `key -> MetricValue` pairs.
fn flatten_json_value(prefix: &str, val: &Value, out: &mut HashMap<String, MetricValue>) {
    match val {
        Value::Number(n) => {
            let key = prefix.to_string();
            if !key.is_empty() {
                if let Some(i) = n.as_i64() {
                    // Prefer integer representation when lossless.
                    out.insert(key, MetricValue::Int(i));
                } else if let Some(f) = n.as_f64() {
                    if f.is_finite() {
                        out.insert(key, MetricValue::Float(f));
                    }
                }
            }
        }
        Value::String(s) => {
            let key = prefix.to_string();
            if !key.is_empty() {
                out.insert(key, MetricValue::String(s.clone()));
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let new_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}/{k}")
                };
                flatten_json_value(&new_prefix, v, out);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let new_prefix = if prefix.is_empty() {
                    i.to_string()
                } else {
                    format!("{prefix}/{i}")
                };
                flatten_json_value(&new_prefix, v, out);
            }
        }
        Value::Bool(b) => {
            let key = prefix.to_string();
            if !key.is_empty() {
                out.insert(key, MetricValue::Int(i64::from(*b)));
            }
        }
        Value::Null => {}
    }
}

// ---------------------------------------------------------------------------
// extract_loss_curves
// ---------------------------------------------------------------------------

/// Extract `(epoch, loss)` pairs from stdout.
///
/// Matches lines of the form:
///   - `epoch N loss V`
///   - `Epoch N: loss=V`
///   - `[N] loss: V`
pub fn extract_loss_curves(stdout: &str) -> Vec<(u32, f64)> {
    let patterns: &[regex::Regex] = &[
        regex::Regex::new(r"(?i)epoch\s+(\d+)[:\s]+loss[=:\s]+([+-]?\d+\.?\d*(?:[eE][+-]?\d+)?)")
            .expect("epoch/loss regex 1"),
        regex::Regex::new(r"\[(\d+)\]\s+loss[=:\s]+([+-]?\d+\.?\d*(?:[eE][+-]?\d+)?)")
            .expect("epoch/loss regex 2"),
        regex::Regex::new(r"(?i)step\s+(\d+)[:\s]+loss[=:\s]+([+-]?\d+\.?\d*(?:[eE][+-]?\d+)?)")
            .expect("step/loss regex"),
    ];

    let mut pairs = Vec::new();

    for line in stdout.lines() {
        let stripped = line.trim();
        for pat in patterns {
            if let Some(cap) = pat.captures(stripped) {
                let epoch: u32 = cap[1].parse().unwrap_or(0);
                let loss: f64 = cap[2].parse().unwrap_or(f64::NAN);
                if loss.is_finite() {
                    pairs.push((epoch, loss));
                    break;
                }
            }
        }
    }

    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain_metric() {
        let stdout = "accuracy: 0.95\nloss: 0.05\n";
        let m = parse_metrics_from_stdout(stdout);
        assert_eq!(m["accuracy"], MetricValue::Float(0.95));
        assert_eq!(m["loss"], MetricValue::Float(0.05));
    }

    #[test]
    fn test_skips_nan() {
        let stdout = "loss: nan\n";
        let m = parse_metrics_from_stdout(stdout);
        assert!(!m.contains_key("loss"));
    }

    #[test]
    fn test_extract_loss_curve() {
        let stdout = "epoch 1: loss=0.9\nepoch 2: loss=0.7\n";
        let curve = extract_loss_curves(stdout);
        assert_eq!(curve.len(), 2);
        assert_eq!(curve[0], (1, 0.9));
    }
}
