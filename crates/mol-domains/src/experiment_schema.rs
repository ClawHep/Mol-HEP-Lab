//! Domain-agnostic experiment plan structures.
//!
//! Provides [`UniversalExperimentPlan`] and supporting types that describe
//! conditions, metrics, and evaluation protocols independently of any
//! particular scientific domain.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The role a condition plays in an experiment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConditionRole {
    Reference,
    #[default]
    Proposed,
    Variant,
}

/// The kind of experiment being run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentType {
    #[default]
    Comparison,
    Convergence,
    ProgressiveSpec,
    Simulation,
    AblationStudy,
}

// ---------------------------------------------------------------------------
// Condition
// ---------------------------------------------------------------------------

/// A single experimental condition (baseline, proposed method, or ablation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub name: String,
    #[serde(default)]
    pub role: ConditionRole,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub varies_from: String,
    #[serde(default)]
    pub variation: String,
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,
}

impl Condition {
    /// Create a new condition with the given name and default (Proposed) role.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: ConditionRole::Proposed,
            description: String::new(),
            varies_from: String::new(),
            variation: String::new(),
            parameters: HashMap::new(),
        }
    }
}

impl Default for Condition {
    fn default() -> Self {
        Self::new("")
    }
}

// ---------------------------------------------------------------------------
// MetricSpec
// ---------------------------------------------------------------------------

/// Specification for a single metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricSpec {
    pub name: String,
    #[serde(default = "default_direction")]
    pub direction: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub description: String,
}

fn default_direction() -> String {
    "minimize".into()
}

impl Default for MetricSpec {
    fn default() -> Self {
        Self {
            name: "primary_metric".into(),
            direction: "minimize".into(),
            unit: String::new(),
            description: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// EvaluationSpec
// ---------------------------------------------------------------------------

/// Evaluation protocol for an experiment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationSpec {
    #[serde(default)]
    pub primary_metric: MetricSpec,
    #[serde(default)]
    pub secondary_metrics: Vec<MetricSpec>,
    #[serde(default)]
    pub protocol: String,
    #[serde(default = "default_statistical_test")]
    pub statistical_test: String,
    #[serde(default = "default_num_seeds")]
    pub num_seeds: u32,
}

fn default_statistical_test() -> String {
    "paired_t_test".into()
}

fn default_num_seeds() -> u32 {
    3
}

impl Default for EvaluationSpec {
    fn default() -> Self {
        Self {
            primary_metric: MetricSpec::default(),
            secondary_metrics: Vec::new(),
            protocol: String::new(),
            statistical_test: "paired_t_test".into(),
            num_seeds: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// UniversalExperimentPlan
// ---------------------------------------------------------------------------

/// A domain-agnostic experiment plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniversalExperimentPlan {
    #[serde(default)]
    pub experiment_type: ExperimentType,
    #[serde(default)]
    pub domain_id: String,
    #[serde(default)]
    pub problem_description: String,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default = "default_input_type")]
    pub input_type: String,
    #[serde(default)]
    pub input_description: String,
    #[serde(default)]
    pub evaluation: EvaluationSpec,
    #[serde(default = "default_main_figure_type")]
    pub main_figure_type: String,
    #[serde(default = "default_main_table_type")]
    pub main_table_type: String,
    #[serde(default)]
    pub raw_yaml: String,
}

fn default_input_type() -> String {
    "generated".into()
}
fn default_main_figure_type() -> String {
    "bar_chart".into()
}
fn default_main_table_type() -> String {
    "comparison_table".into()
}

impl Default for UniversalExperimentPlan {
    fn default() -> Self {
        Self {
            experiment_type: ExperimentType::Comparison,
            domain_id: String::new(),
            problem_description: String::new(),
            conditions: Vec::new(),
            input_type: "generated".into(),
            input_description: String::new(),
            evaluation: EvaluationSpec::default(),
            main_figure_type: "bar_chart".into(),
            main_table_type: "comparison_table".into(),
            raw_yaml: String::new(),
        }
    }
}

impl UniversalExperimentPlan {
    /// Return conditions whose role is [`ConditionRole::Reference`].
    pub fn references(&self) -> Vec<&Condition> {
        self.conditions
            .iter()
            .filter(|c| c.role == ConditionRole::Reference)
            .collect()
    }

    /// Return conditions whose role is [`ConditionRole::Proposed`].
    pub fn proposed(&self) -> Vec<&Condition> {
        self.conditions
            .iter()
            .filter(|c| c.role == ConditionRole::Proposed)
            .collect()
    }

    /// Return conditions whose role is [`ConditionRole::Variant`].
    pub fn variants(&self) -> Vec<&Condition> {
        self.conditions
            .iter()
            .filter(|c| c.role == ConditionRole::Variant)
            .collect()
    }

    /// Convert to the legacy baselines/proposed_methods/ablations JSON format.
    pub fn to_legacy_format(&self) -> serde_json::Value {
        let baselines: Vec<serde_json::Value> = self
            .references()
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "description": c.description,
                })
            })
            .collect();

        let proposed_methods: Vec<serde_json::Value> = self
            .proposed()
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "description": c.description,
                })
            })
            .collect();

        let ablations: Vec<serde_json::Value> = self
            .variants()
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "varies_from": c.varies_from,
                    "variation": c.variation,
                })
            })
            .collect();

        serde_json::json!({
            "baselines": baselines,
            "proposed_methods": proposed_methods,
            "ablations": ablations,
        })
    }

    /// Serialize to a YAML string.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }
}

// ---------------------------------------------------------------------------
// Legacy parsing
// ---------------------------------------------------------------------------

/// Parse a legacy experiment plan (baselines / proposed_methods / ablations)
/// from a YAML string into a [`UniversalExperimentPlan`].
pub fn from_legacy_exp_plan(yaml: &str, domain_id: &str) -> UniversalExperimentPlan {
    let value: serde_yaml::Value =
        serde_yaml::from_str(yaml).unwrap_or(serde_yaml::Value::Mapping(Default::default()));

    let mut conditions = Vec::new();

    // --- baselines → Reference ---
    if let Some(baselines) = value.get("baselines").and_then(|v| v.as_sequence()) {
        for item in baselines {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            conditions.push(Condition {
                name,
                role: ConditionRole::Reference,
                description,
                ..Default::default()
            });
        }
    }

    // --- proposed_methods → Proposed ---
    if let Some(proposed) = value.get("proposed_methods").and_then(|v| v.as_sequence()) {
        for item in proposed {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            conditions.push(Condition {
                name,
                role: ConditionRole::Proposed,
                description,
                ..Default::default()
            });
        }
    }

    // --- ablations → Variant ---
    if let Some(ablations) = value.get("ablations").and_then(|v| v.as_sequence()) {
        for item in ablations {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let varies_from = item
                .get("varies_from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let variation = item
                .get("variation")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            conditions.push(Condition {
                name,
                role: ConditionRole::Variant,
                varies_from,
                variation,
                ..Default::default()
            });
        }
    }

    // --- metrics → EvaluationSpec ---
    let evaluation = if let Some(metrics) = value.get("metrics").and_then(|v| v.as_mapping()) {
        let mut metric_specs: Vec<MetricSpec> = Vec::new();
        for (key, val) in metrics {
            let metric_name = key.as_str().unwrap_or("").to_string();
            let direction = val
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("minimize")
                .to_string();
            let unit = val
                .get("unit")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let description = val
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            metric_specs.push(MetricSpec {
                name: metric_name,
                direction,
                unit,
                description,
            });
        }

        let primary = metric_specs.first().cloned().unwrap_or_default();
        let secondary = if metric_specs.len() > 1 {
            metric_specs[1..].to_vec()
        } else {
            Vec::new()
        };

        EvaluationSpec {
            primary_metric: primary,
            secondary_metrics: secondary,
            ..Default::default()
        }
    } else {
        EvaluationSpec::default()
    };

    UniversalExperimentPlan {
        experiment_type: ExperimentType::Comparison,
        domain_id: domain_id.to_string(),
        conditions,
        evaluation,
        raw_yaml: yaml.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_role_default_is_proposed() {
        let c = Condition::new("test");
        assert_eq!(c.role, ConditionRole::Proposed);
    }

    #[test]
    fn experiment_type_serializes_to_string() {
        let et = ExperimentType::Convergence;
        let s = serde_json::to_string(&et).unwrap();
        assert_eq!(s, "\"convergence\"");
    }

    #[test]
    fn plan_filters_by_role() {
        let plan = UniversalExperimentPlan {
            conditions: vec![
                Condition {
                    name: "baseline".into(),
                    role: ConditionRole::Reference,
                    ..Default::default()
                },
                Condition {
                    name: "ours".into(),
                    role: ConditionRole::Proposed,
                    ..Default::default()
                },
                Condition {
                    name: "no_attn".into(),
                    role: ConditionRole::Variant,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(plan.references().len(), 1);
        assert_eq!(plan.proposed().len(), 1);
        assert_eq!(plan.variants().len(), 1);
    }

    #[test]
    fn from_legacy_parses_baselines() {
        let yaml = r#"
baselines:
  - name: SGD
    description: Standard SGD optimizer
proposed_methods:
  - name: AdamW
    description: Our proposed method
ablations:
  - name: no_warmup
    varies_from: AdamW
    variation: removed warmup
metrics:
  accuracy:
    direction: maximize
"#;
        let plan = from_legacy_exp_plan(yaml, "ml_classification");
        assert_eq!(plan.conditions.len(), 3);
        assert_eq!(plan.references().len(), 1);
        assert_eq!(plan.evaluation.primary_metric.name, "accuracy");
        assert_eq!(plan.evaluation.primary_metric.direction, "maximize");
    }

    #[test]
    fn to_yaml_roundtrip() {
        let plan = UniversalExperimentPlan::default();
        let yaml = plan.to_yaml().unwrap();
        assert!(yaml.contains("experiment"));
    }
}
