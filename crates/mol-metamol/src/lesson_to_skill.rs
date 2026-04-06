//! Convert pipeline failure lessons into MetaMol skills.
//!
//! Analyses high-severity lessons and uses an LLM to generate actionable
//! MetaMol skill files (SKILL.md) that prevent future recurrence of the same
//! mistakes.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn};

use crate::stage_skill_map::lesson_category_to_skill_category;

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Severity levels for lessons, ordered from lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "info" => Self::Info,
            "warning" | "warn" => Self::Warning,
            "error" => Self::Error,
            "critical" => Self::Critical,
            _ => Self::Info,
        }
    }
}

/// A failure lesson extracted from a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonEntry {
    /// Human-readable description of the failure.
    pub description: String,

    /// Category tag (e.g. `"experiment"`, `"literature"`, `"writing"`).
    pub category: String,

    /// Severity of the failure.
    pub severity: Severity,

    /// Pipeline stage name where the failure occurred.
    pub stage_name: String,
}

/// A skill draft ready to be persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDraft {
    /// Lowercase-hyphenated skill name prefixed with `"arc-"`.
    pub title: String,

    /// One-line description of when to use the skill.
    pub description: String,

    /// MetaMol skill category (e.g. `"research"`, `"coding"`).
    pub category: String,

    /// Pipeline stage numbers where this skill is applicable.
    pub stage_applicability: Vec<u32>,

    /// Full Markdown body of the SKILL.md file.
    pub prompt_template: String,
}

/// Configuration for lesson-to-skill conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaMolLessonToSkillConfig {
    /// Minimum severity to include in conversion (`"info"`, `"warning"`,
    /// `"error"`, or `"critical"`).
    pub min_severity: String,

    /// Maximum number of skills to generate per conversion call.
    pub max_skills_per_run: usize,

    /// Path to the MetaMol skills directory (default: `~/.metamol/skills`).
    pub skills_dir: String,
}

impl Default for MetaMolLessonToSkillConfig {
    fn default() -> Self {
        Self {
            min_severity: "warning".to_owned(),
            max_skills_per_run: 3,
            skills_dir: "~/.metamol/skills".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

fn conversion_system_prompt(categories: &str) -> String {
    format!(
        "You are a skill designer for an AI agent system. Your job is to convert \
         failure lessons from an automated research pipeline into reusable skill \
         guides that help the agent avoid the same mistakes in the future.\n\n\
         Each skill must include:\n\
         - A descriptive name (lowercase-hyphenated, prefixed with \"arc-\")\n\
         - A one-line description of when to use the skill\n\
         - A category from: {categories}\n\
         - Markdown content with numbered steps and an anti-pattern section\n\n\
         Output a JSON array of skill objects. Each object has:\n  \
         \"name\": \"arc-<slug>\",\n  \
         \"description\": \"<when to use>\",\n  \
         \"category\": \"<category>\",\n  \
         \"content\": \"<markdown body>\""
    )
}

fn conversion_user_prompt(
    max_skills: usize,
    lessons_text: &str,
    existing_skills: &str,
) -> String {
    format!(
        "The following failure lessons were extracted from automated research runs.\n\
         Please generate {max_skills} reusable skills to address these failures.\n\n\
         ## Failure Lessons\n\n{lessons_text}\n\n\
         ## Existing Skills (do not duplicate)\n\n{existing_skills}\n\n\
         Return ONLY a JSON array. No extra text."
    )
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert `lessons` into [`SkillDraft`] objects using an LLM.
///
/// Only lessons whose severity meets or exceeds `config.min_severity` are
/// included.  The LLM call is made against the provided OpenAI-compatible
/// endpoint.
pub async fn convert_lessons(
    lessons: &[LessonEntry],
    config: &MetaMolLessonToSkillConfig,
    api_base: &str,
    api_key: &str,
    model: &str,
) -> Result<Vec<SkillDraft>> {
    let min_sev = Severity::from_str(&config.min_severity);
    let filtered: Vec<&LessonEntry> =
        lessons.iter().filter(|l| l.severity >= min_sev).collect();

    if filtered.is_empty() {
        info!(
            min_severity = config.min_severity,
            total = lessons.len(),
            "No lessons at or above minimum severity; skipping skill conversion"
        );
        return Ok(Vec::new());
    }

    info!(
        count = filtered.len(),
        min_severity = config.min_severity,
        "Converting lessons to MetaMol skills"
    );

    let skills_path = expand_tilde(&config.skills_dir);
    let existing = list_existing_skill_names(&skills_path).await;

    let all_categories =
        "automation, coding, communication, data_analysis, productivity, research";
    let system = conversion_system_prompt(all_categories);
    let lessons_text = format_lessons(&filtered);
    let existing_str = if existing.is_empty() {
        "(none)".to_owned()
    } else {
        existing[..existing.len().min(50)].join(", ")
    };
    let user = conversion_user_prompt(config.max_skills_per_run, &lessons_text, &existing_str);

    let raw = llm_chat(api_base, api_key, model, &system, &user).await?;
    let skill_dicts = parse_skills_response(&raw);

    if skill_dicts.is_empty() {
        warn!("No valid skills parsed from LLM lesson-to-skill response");
        return Ok(Vec::new());
    }

    let drafts: Vec<SkillDraft> = skill_dicts
        .into_iter()
        .take(config.max_skills_per_run)
        .map(|d| {
            let raw_category = d
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("research");
            let category = lesson_category_to_skill_category(raw_category).to_owned();
            SkillDraft {
                title: d
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("arc-unnamed")
                    .to_owned(),
                description: d
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
                category,
                stage_applicability: Vec::new(), // enriched later if needed
                prompt_template: d
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
            }
        })
        .collect();

    Ok(drafts)
}

/// Write a [`SkillDraft`] as a `SKILL.md` file under `skills_dir`.
///
/// Creates `skills_dir/<draft.title>/SKILL.md`.  Returns the path on success.
pub async fn write_skill_draft(draft: &SkillDraft, skills_dir: &Path) -> Result<PathBuf> {
    let sanitized = sanitize_skill_name(&draft.title);
    let skill_dir = skills_dir.join(&sanitized);
    fs::create_dir_all(&skill_dir).await?;

    let mut content = format!(
        "---\nname: {}\ndescription: {}\ncategory: {}\n---\n",
        sanitized, draft.description, draft.category
    );
    if !draft.stage_applicability.is_empty() {
        let stages: Vec<String> = draft.stage_applicability.iter().map(|s| s.to_string()).collect();
        content.push_str(&format!("stages: [{}]\n---\n", stages.join(", ")));
    }
    content.push_str(&draft.prompt_template);
    content.push('\n');

    let path = skill_dir.join("SKILL.md");
    fs::write(&path, &content).await?;
    info!(skill = sanitized, "Wrote MetaMol skill draft");
    Ok(path)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn format_lessons(lessons: &[&LessonEntry]) -> String {
    lessons
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "{}. [{:?}] [{}] Stage {}: {}",
                i + 1,
                l.severity,
                l.category,
                l.stage_name,
                l.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn list_existing_skill_names(skills_dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(mut entries) = fs::read_dir(skills_dir).await else {
        return names;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_owned());
            }
        }
    }
    names
}

fn parse_skills_response(text: &str) -> Vec<serde_json::Value> {
    let text = text.trim();
    // Strip markdown code fences.
    let stripped = if let Some(inner) = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
    {
        inner.trim_end_matches("```").trim()
    } else {
        text
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(stripped) else {
        warn!("Failed to parse lesson-to-skill response as JSON");
        return Vec::new();
    };

    let array = match value {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(ref obj) => {
            for key in &["skills", "results", "data"] {
                if let Some(serde_json::Value::Array(a)) = obj.get(*key) {
                    return a
                        .iter()
                        .filter(|v| is_valid_skill_object(v))
                        .cloned()
                        .collect();
                }
            }
            return Vec::new();
        }
        _ => return Vec::new(),
    };

    array
        .into_iter()
        .filter(|v| is_valid_skill_object(v))
        .collect()
}

fn is_valid_skill_object(v: &serde_json::Value) -> bool {
    ["name", "description", "category", "content"]
        .iter()
        .all(|k| v.get(k).is_some())
}

fn sanitize_skill_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let sanitized: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    sanitized.trim_matches('-').to_owned()
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(path.replacen('~', &home, 1))
    } else {
        PathBuf::from(path)
    }
}

/// Minimal OpenAI-compatible chat completion call.
async fn llm_chat(
    api_base: &str,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_completion_tokens": 3000,
        "temperature": 0.4,
    });

    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    let data: serde_json::Value = resp.json().await?;
    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::Critical > Severity::Error);
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }

    #[test]
    fn sanitize_name_strips_specials() {
        // Spaces and '!' become '-'; the trailing '-' from '!' is stripped.
        assert_eq!(sanitize_skill_name("arc-Test Name!"), "arc-test-name");
        // Leading/trailing dashes are stripped.
        assert_eq!(sanitize_skill_name("---foo---"), "foo");
    }

    #[test]
    fn parse_skills_response_valid_array() {
        let json = r##"[{"name":"arc-foo","description":"bar","category":"research","content":"Foo"}]"##;
        let result = parse_skills_response(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "arc-foo");
    }

    #[test]
    fn parse_skills_response_fenced() {
        let json = "```json\n[{\"name\":\"arc-x\",\"description\":\"d\",\"category\":\"c\",\"content\":\"body\"}]\n```";
        let result = parse_skills_response(json);
        assert_eq!(result.len(), 1);
    }
}
