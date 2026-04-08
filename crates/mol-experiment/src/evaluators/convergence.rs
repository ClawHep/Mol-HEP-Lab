//! Convergence-order analysis via log-log linear regression.
//!
//! Given a sequence of mesh sizes (h) and corresponding errors, the convergence
//! order is the slope of the best-fit line through (ln h, ln error).

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Core computation
// ---------------------------------------------------------------------------

/// Compute the convergence order via log-log linear regression.
///
/// Returns `Some((order, r_squared))` when at least two positive, finite
/// (h, error) pairs exist; `None` otherwise.
pub fn compute_convergence_order(h_values: &[f64], errors: &[f64]) -> Option<(f64, f64)> {
    // Filter to positive, finite pairs and take their natural logs.
    let pairs: Vec<(f64, f64)> = h_values
        .iter()
        .zip(errors)
        .filter(|(h, e)| h.is_finite() && e.is_finite() && **h > 0.0 && **e > 0.0)
        .map(|(h, e)| (h.ln(), e.ln()))
        .collect();

    if pairs.len() < 2 {
        return None;
    }

    let n = pairs.len() as f64;
    let sum_x: f64 = pairs.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = pairs.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = pairs.iter().map(|(x, y)| x * y).sum();
    let sum_xx: f64 = pairs.iter().map(|(x, _)| x * x).sum();

    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-15 {
        return None;
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n;

    // Coefficient of determination (R²).
    let mean_y = sum_y / n;
    let ss_tot: f64 = pairs.iter().map(|(_, y)| (y - mean_y).powi(2)).sum();
    let ss_res: f64 = pairs
        .iter()
        .map(|(x, y)| (y - (slope * x + intercept)).powi(2))
        .sum();

    let r_squared = if ss_tot > 1e-15 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    Some((slope, r_squared))
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single (h, error) measurement.
#[derive(Debug, Clone)]
pub struct ConvergencePoint {
    pub h: f64,
    pub error: f64,
}

/// Per-method convergence analysis result.
#[derive(Debug, Clone)]
pub struct ConvergenceResult {
    /// Name of the numerical method.
    pub method: String,
    /// Estimated convergence order (slope of log-log fit).
    pub convergence_order: f64,
    /// R² of the log-log fit.
    pub r_squared: f64,
    /// Number of valid data points used.
    pub points: usize,
    /// `true` when order > 0.5 **and** R² > 0.8.
    pub is_converging: bool,
    /// Optional theoretically expected order.
    pub expected_order: Option<f64>,
    /// Whether the measured order matches the expected one (within 0.5).
    pub order_matches_expected: Option<bool>,
}

/// Summary report across all methods.
#[derive(Debug, Clone)]
pub struct ConvergenceReport {
    /// Per-method results.
    pub methods: Vec<ConvergenceResult>,
    /// Method with the highest convergence order, if any.
    pub best_method: Option<String>,
    /// Human-readable summary.
    pub summary: String,
}

// ---------------------------------------------------------------------------
// High-level analysis
// ---------------------------------------------------------------------------

/// Analyse convergence data for one or more methods.
///
/// `convergence_data` maps method names to their (h, error) series.
/// `expected_orders` optionally maps method names to their theoretically
/// expected convergence order.
pub fn analyze_convergence(
    convergence_data: &HashMap<String, Vec<ConvergencePoint>>,
    expected_orders: Option<&HashMap<String, f64>>,
) -> ConvergenceReport {
    let mut results: Vec<ConvergenceResult> = Vec::new();

    for (method, points) in convergence_data {
        let h_values: Vec<f64> = points.iter().map(|p| p.h).collect();
        let errors: Vec<f64> = points.iter().map(|p| p.error).collect();

        if let Some((order, r2)) = compute_convergence_order(&h_values, &errors) {
            let is_converging = order > 0.5 && r2 > 0.8;

            let expected_order = expected_orders.and_then(|eo| eo.get(method)).copied();
            let order_matches_expected =
                expected_order.map(|exp| (order - exp).abs() < 0.5);

            results.push(ConvergenceResult {
                method: method.clone(),
                convergence_order: order,
                r_squared: r2,
                points: h_values.len(),
                is_converging,
                expected_order,
                order_matches_expected,
            });
        }
    }

    // Sort by convergence order descending for deterministic best-method pick.
    results.sort_by(|a, b| {
        b.convergence_order
            .partial_cmp(&a.convergence_order)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let best_method = results.first().map(|r| r.method.clone());

    let summary = if results.is_empty() {
        "No valid convergence data.".to_string()
    } else {
        let lines: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "{}: order={:.2}, R²={:.4}, converging={}",
                    r.method, r.convergence_order, r.r_squared, r.is_converging,
                )
            })
            .collect();
        let mut s = lines.join("; ");
        if let Some(ref best) = best_method {
            s.push_str(&format!(" | best: {best}"));
        }
        s
    };

    ConvergenceReport {
        methods: results,
        best_method,
        summary,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_second_order_convergence() {
        let h = vec![1.0, 0.5, 0.25, 0.125];
        let e = vec![1.0, 0.25, 0.0625, 0.015625];
        let (order, r2) = compute_convergence_order(&h, &e).unwrap();
        assert!((order - 2.0).abs() < 0.01, "order={order}");
        assert!(r2 > 0.999, "r2={r2}");
    }

    #[test]
    fn known_first_order_convergence() {
        let h = vec![1.0, 0.5, 0.25];
        let e = vec![1.0, 0.5, 0.25];
        let (order, r2) = compute_convergence_order(&h, &e).unwrap();
        assert!((order - 1.0).abs() < 0.01);
        assert!(r2 > 0.999);
    }

    #[test]
    fn too_few_points_returns_none() {
        let h = vec![1.0];
        let e = vec![1.0];
        assert!(compute_convergence_order(&h, &e).is_none());
    }

    #[test]
    fn filters_non_positive_values() {
        let h = vec![1.0, 0.5, -1.0, 0.0, 0.25];
        let e = vec![1.0, 0.5, 0.3, 0.2, 0.25];
        // Only 3 valid pairs: (1.0,1.0), (0.5,0.5), (0.25,0.25) => order ~1
        let (order, _r2) = compute_convergence_order(&h, &e).unwrap();
        assert!((order - 1.0).abs() < 0.01);
    }

    #[test]
    fn analyze_convergence_finds_best_method() {
        let mut data = HashMap::new();
        data.insert(
            "euler".to_string(),
            vec![
                ConvergencePoint { h: 1.0, error: 1.0 },
                ConvergencePoint { h: 0.5, error: 0.5 },
            ],
        );
        data.insert(
            "rk4".to_string(),
            vec![
                ConvergencePoint { h: 1.0, error: 1.0 },
                ConvergencePoint {
                    h: 0.5,
                    error: 0.0625,
                },
            ],
        );
        let report = analyze_convergence(&data, None);
        assert_eq!(report.best_method.as_deref(), Some("rk4"));
    }

    #[test]
    fn convergence_result_flags() {
        let mut data = HashMap::new();
        data.insert(
            "good".to_string(),
            vec![
                ConvergencePoint { h: 1.0, error: 1.0 },
                ConvergencePoint { h: 0.5, error: 0.25 },
                ConvergencePoint { h: 0.25, error: 0.0625 },
            ],
        );

        let mut expected = HashMap::new();
        expected.insert("good".to_string(), 2.0);

        let report = analyze_convergence(&data, Some(&expected));
        let res = &report.methods[0];
        assert!(res.is_converging);
        assert_eq!(res.expected_order, Some(2.0));
        assert_eq!(res.order_matches_expected, Some(true));
    }
}
