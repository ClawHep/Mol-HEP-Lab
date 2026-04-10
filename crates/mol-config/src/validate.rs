//! Configuration validation for Mol-HEP-Lab.
//!
//! Ports `validate_config()` from
//! `backend/agent/mol/config.py`.

use std::path::Path;

use serde_yaml::Value;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Fields that must be non-blank in a valid config.
const REQUIRED_FIELDS: &[&str] = &[
    "project.name",
    "research.topic",
    "runtime.timezone",
    "notifications.channel",
    "knowledge_base.root",
    "llm.api_key_env",
];

/// Recommended knowledge-base sub-directories.
const KB_SUBDIRS: &[&str] = &[
    "questions",
    "literature",
    "experiments",
    "findings",
    "decisions",
    "reviews",
];

const VALID_PROJECT_MODES: &[&str] = &["docs-first", "semi-auto", "full-auto"];
const VALID_KB_BACKENDS: &[&str] = &["markdown", "obsidian"];
const VALID_EXPERIMENT_MODES: &[&str] =
    &["simulated", "sandbox", "docker", "ssh_remote", "colab_drive"];
const VALID_METRIC_DIRECTIONS: &[&str] = &["minimize", "maximize"];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of validating a raw config map.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Validate the raw YAML `Value` parsed from a config file.
///
/// * `project_root` – if provided, path existence checks are performed.
/// * `check_paths`  – set to `false` to skip filesystem checks entirely.
pub fn validate_config(
    data: &Value,
    project_root: Option<&Path>,
    check_paths: bool,
) -> ValidationResult {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let llm_provider = get_by_path(data, "llm.provider")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // --- Required fields ---
    for &key in REQUIRED_FIELDS {
        // ACP provider doesn't need api_key_env
        if llm_provider == "acp" && key == "llm.api_key_env" {
            continue;
        }
        if is_blank(get_by_path(data, key)) {
            errors.push(format!("Missing required field: {key}"));
        }
    }

    // --- project.mode ---
    if let Some(mode) = get_by_path(data, "project.mode").and_then(|v| v.as_str()) {
        if !mode.is_empty() && !VALID_PROJECT_MODES.contains(&mode) {
            errors.push(format!("Invalid project.mode: {mode}"));
        }
    }

    // --- knowledge_base.backend ---
    if let Some(backend) = get_by_path(data, "knowledge_base.backend").and_then(|v| v.as_str()) {
        if !backend.is_empty() && !VALID_KB_BACKENDS.contains(&backend) {
            errors.push(format!("Invalid knowledge_base.backend: {backend}"));
        }
    }

    // --- security.hitl_required_stages ---
    if let Some(stages_val) = get_by_path(data, "security.hitl_required_stages") {
        match stages_val.as_sequence() {
            None => errors.push("security.hitl_required_stages must be a list".to_owned()),
            Some(seq) => {
                for item in seq {
                    match item.as_i64() {
                        Some(n) if (1..=18).contains(&n) => {}
                        Some(n) => errors.push(format!(
                            "Invalid security.hitl_required_stages entry: {n} (must be 1-18)"
                        )),
                        None => errors.push(format!(
                            "Invalid security.hitl_required_stages entry: {item:?} (must be integer)"
                        )),
                    }
                }
            }
        }
    }

    // --- experiment.mode ---
    if let Some(mode) = get_by_path(data, "experiment.mode").and_then(|v| v.as_str()) {
        if !mode.is_empty() && !VALID_EXPERIMENT_MODES.contains(&mode) {
            errors.push(format!("Invalid experiment.mode: {mode}"));
        }
    }

    // --- experiment.metric_direction ---
    if let Some(dir) = get_by_path(data, "experiment.metric_direction").and_then(|v| v.as_str()) {
        if !dir.is_empty() && !VALID_METRIC_DIRECTIONS.contains(&dir) {
            errors.push(format!("Invalid experiment.metric_direction: {dir}"));
        }
    }

    // --- Path checks ---
    if check_paths {
        if let Some(root_str) = get_by_path(data, "knowledge_base.root").and_then(|v| v.as_str()) {
            if !root_str.is_empty() {
                if let Some(project_root) = project_root {
                    let kb_root = project_root.join(root_str);
                    if !kb_root.exists() {
                        errors.push(format!("Missing path: {}", kb_root.display()));
                    } else {
                        for subdir in KB_SUBDIRS {
                            let candidate = kb_root.join(subdir);
                            if !candidate.exists() {
                                warnings.push(format!(
                                    "Missing recommended kb subdir: {}",
                                    candidate.display()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    ValidationResult {
        ok: errors.is_empty(),
        errors,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Navigate a dotted key path through a `serde_yaml::Value`.
fn get_by_path<'a>(data: &'a Value, dotted_key: &str) -> Option<&'a Value> {
    let mut cur = data;
    for part in dotted_key.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

/// Return `true` if the value is `null` or a blank string.
fn is_blank(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::from_str;

    fn minimal_yaml() -> Value {
        from_str(
            r#"
project:
  name: "test"
research:
  topic: "HEP"
runtime:
  timezone: "UTC"
notifications:
  channel: "slack"
knowledge_base:
  root: "kb"
llm:
  provider: "openai-compatible"
  api_key_env: "OPENAI_API_KEY"
"#,
        )
        .unwrap()
    }

    #[test]
    fn valid_minimal_config_passes() {
        let data = minimal_yaml();
        let result = validate_config(&data, None, false);
        assert!(result.ok, "errors: {:?}", result.errors);
    }

    #[test]
    fn missing_required_field_fails() {
        let mut data = minimal_yaml();
        // Remove project.name
        data["project"]["name"] = Value::Null;
        let result = validate_config(&data, None, false);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("project.name")));
    }

    #[test]
    fn invalid_project_mode_fails() {
        let mut data = minimal_yaml();
        data["project"]["mode"] = Value::String("invalid".to_owned());
        let result = validate_config(&data, None, false);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("project.mode")));
    }

    #[test]
    fn invalid_hitl_stage_out_of_range_fails() {
        let mut data = minimal_yaml();
        data["security"]["hitl_required_stages"] =
            from_str("[1, 99]").unwrap();
        let result = validate_config(&data, None, false);
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("hitl_required_stages")));
    }

    #[test]
    fn acp_provider_skips_api_key_env_check() {
        let yaml: Value = from_str(
            r#"
project:
  name: "test"
research:
  topic: "HEP"
runtime:
  timezone: "UTC"
notifications:
  channel: "slack"
knowledge_base:
  root: "kb"
llm:
  provider: "acp"
"#,
        )
        .unwrap();
        let result = validate_config(&yaml, None, false);
        // api_key_env missing but provider is acp → should pass
        assert!(result.ok, "errors: {:?}", result.errors);
    }
}
