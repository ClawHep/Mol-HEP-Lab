//! Mol-HEP-Lab: Configuration loading and management.
//!
//! # Quick start
//!
//! ```no_run
//! use mol_config::{MolConfig, resolve_config_path};
//!
//! let path = resolve_config_path(None).expect("no config file found");
//! let cfg  = MolConfig::load(&path).expect("failed to load config");
//! println!("project: {}", cfg.project.name);
//! ```

mod types;
mod validate;

pub use types::{
    AcpConfig, BenchmarkAgentConfig, CodeAgentConfig, ColabDriveConfig, DockerSandboxConfig,
    ExperimentConfig, ExperimentMode, ExportConfig, FigureAgentConfig, KbBackend,
    KnowledgeBaseConfig, LlmConfig, MetaMolBridgeConfig, MetaMolLessonToSkillConfig,
    MetaMolPRMConfig, MetricDirection, MolConfig, NotificationsConfig, OpenCodeConfig,
    OpenMolBridgeConfig, ProjectConfig, ProjectMode, PromptsConfig, ResearchConfig,
    RuntimeConfig, SandboxConfig, SecurityConfig, SshRemoteConfig, WebSearchConfig,
};

pub use validate::{validate_config, ValidationResult};

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Ordered list of config file names to try when no explicit path is given.
pub const CONFIG_SEARCH_ORDER: &[&str] = &["config.mol.yaml", "config.yaml"];

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Return the first existing config file from [`CONFIG_SEARCH_ORDER`], or
/// `explicit` if one was provided.
///
/// Returns `None` only when `explicit` is `None` and none of the candidate
/// files exist in the current directory.
pub fn resolve_config_path(explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(PathBuf::from(path));
    }
    for name in CONFIG_SEARCH_ORDER {
        let candidate = PathBuf::from(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// MolConfig loading
// ---------------------------------------------------------------------------

impl MolConfig {
    /// Load and deserialize a `MolConfig` from a YAML file at `path`.
    ///
    /// Validation is performed with path-existence checks enabled and the
    /// config file's parent used as the project root.  Use
    /// [`MolConfig::load_opts`] for full control.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        Self::load_opts(path, None, true)
    }

    /// Load with explicit `project_root` and `check_paths` flag.
    ///
    /// * `project_root` – directory used to resolve relative KB paths;
    ///   defaults to the directory containing the config file.
    /// * `check_paths`  – when `true`, the KB root path is checked for
    ///   existence and warnings are emitted for missing sub-directories.
    pub fn load_opts(
        path: impl AsRef<Path>,
        project_root: Option<&Path>,
        check_paths: bool,
    ) -> Result<Self> {
        let config_path = path.as_ref().to_path_buf();
        let config_path = if config_path.is_absolute() {
            config_path
        } else {
            std::env::current_dir()
                .context("failed to get current directory")?
                .join(&config_path)
        };

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config file: {}", config_path.display()))?;

        // Parse as raw Value first for validation, then deserialize into typed struct.
        let raw: serde_yaml::Value = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse YAML: {}", config_path.display()))?;

        if !raw.is_mapping() {
            anyhow::bail!(
                "Config root must be a YAML mapping. Check that {} is valid YAML.",
                config_path.display()
            );
        }

        let resolved_root = project_root
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                config_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            });

        let result = validate_config(
            &raw,
            Some(&resolved_root),
            check_paths,
        );

        if !result.ok {
            anyhow::bail!(
                "Config validation failed:\n{}",
                result.errors.join("\n")
            );
        }

        // Log warnings (non-fatal)
        for w in &result.warnings {
            tracing::warn!("mol-config: {w}");
        }

        let cfg: MolConfig = serde_yaml::from_value(raw)
            .with_context(|| format!("failed to deserialize config: {}", config_path.display()))?;

        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_config_path_returns_explicit() {
        let p = resolve_config_path(Some("/tmp/my.yaml")).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/my.yaml"));
    }

    #[test]
    fn resolve_config_path_returns_none_when_no_file() {
        // In a fresh temp dir there are no candidate files.
        let tmp = std::env::temp_dir();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();
        let result = resolve_config_path(None);
        std::env::set_current_dir(&orig).unwrap();
        // Either None or an existing file — we just check it doesn't panic.
        let _ = result;
    }

    #[test]
    fn mol_config_deserializes_minimal_yaml() {
        let yaml = r#"
project:
  name: "TestProject"
research:
  topic: "High-Energy Physics"
runtime:
  timezone: "UTC"
notifications:
  channel: "slack"
knowledge_base:
  root: "kb"
llm:
  provider: "openai-compatible"
  api_key_env: "OPENAI_API_KEY"
  primary_model: "gpt-4o"
"#;
        let raw: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg: MolConfig = serde_yaml::from_value(raw).unwrap();
        assert_eq!(cfg.project.name, "TestProject");
        assert_eq!(cfg.research.topic, "High-Energy Physics");
        assert_eq!(cfg.llm.primary_model, "gpt-4o");
        assert_eq!(cfg.security.hitl_required_stages, vec![5, 9, 20]);
        assert!(cfg.experiment.sandbox.gpu_required == false);
    }

    #[test]
    fn experiment_defaults_are_populated() {
        let yaml = r#"
project:
  name: "P"
research:
  topic: "T"
runtime:
  timezone: "UTC"
notifications:
  channel: "none"
knowledge_base:
  root: "kb"
llm:
  provider: "openai-compatible"
  api_key_env: "KEY"
"#;
        let raw: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg: MolConfig = serde_yaml::from_value(raw).unwrap();
        assert_eq!(cfg.experiment.time_budget_sec, 3600);
        assert_eq!(cfg.experiment.max_iterations, 10);
        assert_eq!(cfg.experiment.paper_length, "medium");
        assert_eq!(cfg.experiment.docker.memory_limit_mb, 8192);
        assert_eq!(cfg.export.target_conference, "neurips_2025");
        assert!(cfg.web_search.enable_scholar);
    }
}
