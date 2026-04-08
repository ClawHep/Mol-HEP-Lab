//! HEP (High Energy Physics) analysis types and scaffold utilities.
//!
//! Provides HEP-specific data structures for analysis orchestration and
//! a scaffold function that creates the directory layout for a new HEP analysis.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// HEP Analysis Types
// ---------------------------------------------------------------------------

/// HEP analysis type — determines which conventions apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HepAnalysisType {
    /// Cross-section measurement (extraction or unfolding).
    Measurement,
    /// New physics search (limit-setting or discovery).
    Search,
}

impl HepAnalysisType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Measurement => "measurement",
            Self::Search => "search",
        }
    }

    /// Return the applicable conventions files for this analysis type.
    pub fn conventions_files(&self) -> &'static [&'static str] {
        match self {
            Self::Measurement => &["conventions/unfolding.md", "conventions/extraction.md"],
            Self::Search => &["conventions/search.md"],
        }
    }
}

impl std::fmt::Display for HepAnalysisType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Model routing configuration for HEP analysis phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HepModelConfig {
    pub planner: String,
    pub executor: String,
    pub reviewer: String,
    /// Phase-specific model overrides (e.g. "phase4" → "claude-opus").
    #[serde(default)]
    pub phase_overrides: std::collections::HashMap<String, String>,
}

impl HepModelConfig {
    /// Get the model for a given role, with optional phase-level override.
    pub fn model_for(&self, role: &str, phase: Option<&str>) -> &str {
        if let Some(p) = phase {
            if let Some(model) = self.phase_overrides.get(p) {
                return model;
            }
        }
        match role {
            "planner" => &self.planner,
            "reviewer" => &self.reviewer,
            _ => &self.executor,
        }
    }
}

impl Default for HepModelConfig {
    fn default() -> Self {
        Self {
            planner: "claude-opus-4".into(),
            executor: "claude-sonnet-4".into(),
            reviewer: "claude-opus-4".into(),
            phase_overrides: Default::default(),
        }
    }
}

/// Configuration for a single HEP analysis run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HepAnalysisConfig {
    pub name: String,
    pub physics_prompt: String,
    pub data_dir: PathBuf,
    pub analysis_type: HepAnalysisType,
    #[serde(default)]
    pub conventions: Vec<String>,
    #[serde(default)]
    pub models: HepModelConfig,
    /// Blinding configuration.
    #[serde(default = "default_blinding")]
    pub blinding_active: bool,
    #[serde(default)]
    pub approved_for_unblinding: bool,
    /// Cost controls.
    #[serde(default = "default_max_review_iterations")]
    pub max_review_iterations: u32,
}

fn default_blinding() -> bool {
    true
}
fn default_max_review_iterations() -> u32 {
    10
}

/// A single review item with A/B/C classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HepReviewItem {
    /// "A" (blocking), "B" (important), "C" (minor).
    pub classification: String,
    pub description: String,
    #[serde(default)]
    pub location: Option<String>,
}

/// Review verdict from the arbiter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HepReviewVerdict {
    /// PASS, ITERATE, or ESCALATE.
    pub verdict: String,
    pub reviewer_name: String,
    pub items: Vec<HepReviewItem>,
    pub iteration: u32,
}

// ---------------------------------------------------------------------------
// HEP Analysis Phases
// ---------------------------------------------------------------------------

/// The 5 canonical HEP analysis phases.
pub const HEP_PHASES: &[&str] = &[
    "phase1_strategy",
    "phase2_exploration",
    "phase3_selection",
    "phase4_inference",
    "phase5_documentation",
];

/// Sub-directories created within each phase directory.
pub const PHASE_SUBDIRS: &[&str] = &[
    "exec",
    "scripts",
    "figures",
    "review",
    "review/plot-validation",
];

// ---------------------------------------------------------------------------
// Scaffold
// ---------------------------------------------------------------------------

/// Scaffold a new HEP analysis directory structure.
///
/// Creates:
/// - Root CLAUDE.md (from `hep/templates/root_claude.md`)
/// - 5 phase directories with per-phase CLAUDE.md
/// - analysis_config.yaml with blinding defaults
/// - STATE.md for pipeline tracking
/// - .analysis_config for isolation hook
///
/// Returns the list of created files/directories, or an error.
pub fn scaffold_analysis(
    analysis_dir: &Path,
    analysis_type: HepAnalysisType,
    hep_root: &Path,
) -> std::io::Result<Vec<PathBuf>> {
    let mut created = Vec::new();
    let templates_dir = hep_root.join("templates");
    let name = analysis_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".into());

    // Create root directory
    std::fs::create_dir_all(analysis_dir)?;

    // Root CLAUDE.md
    let root_claude = analysis_dir.join("CLAUDE.md");
    if !root_claude.exists() {
        let template_path = templates_dir.join("root_claude.md");
        if template_path.exists() {
            let template = std::fs::read_to_string(&template_path)?;
            let content = template
                .replace("{{name}}", &name)
                .replace("{{analysis_type}}", analysis_type.as_str())
                .replace(
                    "{{conventions_files}}",
                    &analysis_type
                        .conventions_files()
                        .iter()
                        .map(|f| format!("- `{f}`"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            std::fs::write(&root_claude, content)?;
            created.push(root_claude);
        }
    }

    // Phase directories
    let phase_templates = [
        ("phase1_strategy", "phase1_claude.md"),
        ("phase2_exploration", "phase2_claude.md"),
        ("phase3_selection", "phase3_claude.md"),
        ("phase4_inference", "phase4_claude.md"),
        ("phase5_documentation", "phase5_claude.md"),
    ];

    for (phase_name, template_name) in &phase_templates {
        let phase_dir = analysis_dir.join(phase_name);
        std::fs::create_dir_all(&phase_dir)?;
        for subdir in PHASE_SUBDIRS {
            std::fs::create_dir_all(phase_dir.join(subdir))?;
        }

        let claude_path = phase_dir.join("CLAUDE.md");
        if !claude_path.exists() {
            let template_path = templates_dir.join(template_name);
            if template_path.exists() {
                let template = std::fs::read_to_string(&template_path)?;
                let content = template
                    .replace("{{name}}", &name)
                    .replace("{{analysis_type}}", analysis_type.as_str());
                std::fs::write(&claude_path, &content)?;
                created.push(claude_path);
            }
        }
    }

    // STATE.md
    let state_path = analysis_dir.join("STATE.md");
    if !state_path.exists() {
        std::fs::write(
            &state_path,
            format!(
                "# Analysis State\n\n\
                 - **Analysis**: {name}\n\
                 - **Current phase**: 1\n\
                 - **Status**: initialized\n\
                 - **Last updated**: (not started)\n\n\
                 ## Phase History\n\n\
                 | Phase | Status | Artifact | Review | Iterations | Notes |\n\
                 |-------|--------|----------|--------|------------|-------|\n\n\
                 ## Blockers\n- (none)\n\n\
                 ## Regression Log\n- (none)\n"
            ),
        )?;
        created.push(state_path);
    }

    // analysis_config.yaml
    let config_path = analysis_dir.join("analysis_config.yaml");
    if !config_path.exists() {
        std::fs::write(
            &config_path,
            format!(
                "analysis_name: {name}\n\
                 analysis_type: {atype}\n\
                 physics_prompt_path: prompt.md\n\
                 model_tier: auto\n\
                 channels: []  # populated during Phase 1\n\
                 calibrations: []  # populated during Phase 1\n\
                 cost_controls:\n\
                 \x20 max_review_iterations: 10\n\
                 \x20 review_warn_threshold: 3\n\
                 blinding:\n\
                 \x20 active: true\n\
                 \x20 approved_for_unblinding: false\n",
                atype = analysis_type,
            ),
        )?;
        created.push(config_path);
    }

    // .analysis_config (isolation hook)
    let isolation_config = analysis_dir.join(".analysis_config");
    if !isolation_config.exists() {
        std::fs::write(
            &isolation_config,
            "# The isolation hook allows access to these directories.\n\
             # Set data_dir to the path where your input ROOT files live.\n\
             data_dir=\n\
             # allow=/path/to/mc/samples\n\
             # allow=/path/to/calibration\n",
        )?;
        created.push(isolation_config);
    }

    Ok(created)
}

// ---------------------------------------------------------------------------
// Leaderboard
// ---------------------------------------------------------------------------

/// Metrics where higher values are better.
const HIGHER_IS_BETTER: &[&str] = &[
    "significance",
    "accuracy",
    "auc",
    "signal_efficiency",
    "background_rejection",
];

/// A single experiment result for leaderboard ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HepExperimentResult {
    pub analysis_id: String,
    pub variant: String,
    pub branch: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub timestamp: String,
    pub executor_used: String,
}

/// Leaderboard for ranking HEP experiment results.
#[derive(Debug, Default)]
pub struct HepLeaderboard {
    results: Vec<HepExperimentResult>,
}

impl HepLeaderboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, result: HepExperimentResult) {
        self.results.push(result);
    }

    /// Return results sorted by metric (best first).
    pub fn rankings(&self) -> Vec<&HepExperimentResult> {
        if self.results.is_empty() {
            return vec![];
        }
        let metric = &self.results[0].metric_name;
        let higher = HIGHER_IS_BETTER.contains(&metric.as_str());
        let mut refs: Vec<&HepExperimentResult> = self.results.iter().collect();
        refs.sort_by(|a, b| {
            if higher {
                b.metric_value.partial_cmp(&a.metric_value).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.metric_value.partial_cmp(&b.metric_value).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        refs
    }

    /// Render the leaderboard as a markdown table.
    pub fn render_markdown(&self) -> String {
        let rankings = self.rankings();
        if rankings.is_empty() {
            return "# Leaderboard\n\nNo experiments recorded yet.\n".into();
        }

        let mut lines = vec![
            "# Leaderboard\n".into(),
            format!("**Metric:** {}\n", rankings[0].metric_name),
            "| Rank | Analysis | Variant | Value | Executor | Timestamp |".into(),
            "|------|----------|---------|-------|----------|-----------|".into(),
        ];

        for (i, r) in rankings.iter().enumerate() {
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} |",
                i + 1,
                r.analysis_id,
                r.variant,
                r.metric_value,
                r.executor_used,
                r.timestamp,
            ));
        }

        lines.join("\n") + "\n"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_type_conventions() {
        assert_eq!(
            HepAnalysisType::Measurement.conventions_files(),
            &["conventions/unfolding.md", "conventions/extraction.md"]
        );
        assert_eq!(
            HepAnalysisType::Search.conventions_files(),
            &["conventions/search.md"]
        );
    }

    #[test]
    fn model_config_override() {
        let config = HepModelConfig {
            planner: "opus".into(),
            executor: "sonnet".into(),
            reviewer: "opus".into(),
            phase_overrides: [("phase4".into(), "opus-special".into())]
                .into_iter()
                .collect(),
        };
        assert_eq!(config.model_for("executor", None), "sonnet");
        assert_eq!(config.model_for("executor", Some("phase4")), "opus-special");
        assert_eq!(config.model_for("executor", Some("phase1")), "sonnet");
    }

    #[test]
    fn leaderboard_ranking_higher_better() {
        let mut lb = HepLeaderboard::new();
        lb.update(HepExperimentResult {
            analysis_id: "a".into(), variant: "v1".into(), branch: "main".into(),
            metric_name: "significance".into(), metric_value: 3.5,
            timestamp: "2026-04-08".into(), executor_used: "claude".into(),
        });
        lb.update(HepExperimentResult {
            analysis_id: "a".into(), variant: "v2".into(), branch: "main".into(),
            metric_name: "significance".into(), metric_value: 5.0,
            timestamp: "2026-04-08".into(), executor_used: "claude".into(),
        });
        let ranks = lb.rankings();
        assert_eq!(ranks[0].metric_value, 5.0); // higher first
    }

    #[test]
    fn leaderboard_ranking_lower_better() {
        let mut lb = HepLeaderboard::new();
        lb.update(HepExperimentResult {
            analysis_id: "a".into(), variant: "v1".into(), branch: "main".into(),
            metric_name: "cls_upper_limit".into(), metric_value: 0.8,
            timestamp: "2026-04-08".into(), executor_used: "claude".into(),
        });
        lb.update(HepExperimentResult {
            analysis_id: "a".into(), variant: "v2".into(), branch: "main".into(),
            metric_name: "cls_upper_limit".into(), metric_value: 0.3,
            timestamp: "2026-04-08".into(), executor_used: "claude".into(),
        });
        let ranks = lb.rankings();
        assert_eq!(ranks[0].metric_value, 0.3); // lower first
    }

    #[test]
    fn leaderboard_markdown() {
        let mut lb = HepLeaderboard::new();
        lb.update(HepExperimentResult {
            analysis_id: "dimuon".into(), variant: "bdt".into(), branch: "main".into(),
            metric_name: "significance".into(), metric_value: 4.2,
            timestamp: "2026-04-08".into(), executor_used: "claude".into(),
        });
        let md = lb.render_markdown();
        assert!(md.contains("Leaderboard"));
        assert!(md.contains("dimuon"));
        assert!(md.contains("4.2"));
    }

    #[test]
    fn scaffold_creates_structure() {
        let dir = std::env::temp_dir().join("mol_hep_test_scaffold");
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }

        // Create a minimal templates dir
        let hep_root = std::env::temp_dir().join("mol_hep_test_root");
        let templates = hep_root.join("templates");
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::write(
            templates.join("root_claude.md"),
            "# {{name}} ({{analysis_type}})\n",
        )
        .unwrap();

        let created = scaffold_analysis(&dir, HepAnalysisType::Search, &hep_root).unwrap();
        assert!(!created.is_empty());
        assert!(dir.join("CLAUDE.md").exists());
        assert!(dir.join("STATE.md").exists());
        assert!(dir.join("analysis_config.yaml").exists());
        assert!(dir.join("phase1_strategy").is_dir());
        assert!(dir.join("phase4_inference/review/plot-validation").is_dir());

        // Cleanup
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&hep_root).unwrap();
    }
}
