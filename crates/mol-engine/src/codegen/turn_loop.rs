//! Extended codegen turn loop with compliance checks.
//!
//! Wraps the generic [`crate::turn_loop::AgentTurnLoop`] with codegen-specific
//! verification hooks:
//!   - Anti-simulation gate: ensures the agent actually runs code, not fakes output
//!   - Plan compliance check: ensures the plan's required methods are implemented
//!   - Epistemic honesty gate: blocks derived labels / surrogate scores

use std::path::{Path, PathBuf};

use crate::turn_loop::{AgentTurnLoop, LlmConfig, TurnResult};

use super::types::CodegenContext;

/// Compliance check configuration.
#[derive(Debug, Clone)]
pub struct ComplianceConfig {
    /// Fire the anti-simulation gate after the first tool round.
    pub anti_simulation: bool,
    /// Fire the plan compliance check after half the iterations.
    pub plan_compliance: bool,
    /// Fire the epistemic honesty gate on every tool round.
    pub epistemic_honesty: bool,
    /// Required method names that must appear in produced files.
    pub required_methods: Vec<String>,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            anti_simulation: true,
            plan_compliance: true,
            epistemic_honesty: false,
            required_methods: Vec::new(),
        }
    }
}

/// Extended turn loop for codegen with configurable compliance hooks.
pub struct CodegenTurnLoop {
    inner: AgentTurnLoop,
}

impl CodegenTurnLoop {
    /// Build a codegen turn loop with compliance hooks wired in.
    pub fn new(
        llm_config: LlmConfig,
        workspace: impl Into<PathBuf>,
        system_prompt: impl Into<String>,
        _ctx: &CodegenContext,
        compliance: ComplianceConfig,
    ) -> Self {
        let ws = workspace.into();
        let mut loop_ =
            AgentTurnLoop::new(llm_config, ws.clone(), system_prompt).with_trace("generation");

        // ── Anti-simulation gate ─────────────────────────────────────────
        // Fires once after the first bash/write_file round if the agent
        // has NOT yet produced a main.py, reminding it that it must
        // actually execute code rather than simulating results.
        if compliance.anti_simulation {
            loop_ = loop_.add_hook(Box::new(|_workspace: &Path, tool_uses: &[serde_json::Value], ws_files: &[String]| {
                let has_bash = tool_uses.iter().any(|tu| {
                    tu.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(serde_json::Value::as_str)
                        == Some("bash")
                });
                if !has_bash {
                    return None;
                }
                let has_main = ws_files.iter().any(|f| f == "main.py" || f.ends_with("/main.py"));
                if has_main {
                    return None;
                }
                Some(
                    "ANTI-SIMULATION GATE: You have not yet created main.py. \
                     Do NOT simulate or fabricate experiment results. \
                     You must write real executable code in main.py that actually runs \
                     and produces genuine metric values. \
                     Please create main.py now."
                        .to_owned(),
                )
            }));
        }

        // ── Plan compliance check ────────────────────────────────────────
        // Fires once to remind the agent to include all required methods
        // from the experiment plan.
        if compliance.plan_compliance && !compliance.required_methods.is_empty() {
            let methods = compliance.required_methods.clone();
            loop_ = loop_.add_hook(Box::new(move |_workspace, _tool_uses, ws_files| {
                // Check if main.py exists and contains the required methods
                let main_exists = ws_files.iter().any(|f| f == "main.py" || f.ends_with("/main.py"));
                if !main_exists {
                    return None; // Not yet ready to check
                }
                let missing: Vec<&str> = methods
                    .iter()
                    .filter(|m| !ws_files.iter().any(|f| f.contains(m.as_str())))
                    .map(|m| m.as_str())
                    .collect();
                if missing.is_empty() {
                    return None;
                }
                Some(format!(
                    "PLAN COMPLIANCE CHECK: The following methods from the experiment plan \
                     have not been implemented yet: {}. \
                     Please ensure all methods are implemented in main.py or helper modules. \
                     Do not declare method names without executing them.",
                    missing.join(", ")
                ))
            }));
        }

        // ── Epistemic honesty gate ───────────────────────────────────────
        // Injected as a reminder whenever enabled. Fires once.
        if compliance.epistemic_honesty {
            loop_ = loop_.add_hook(Box::new(|_workspace, _tool_uses, _ws_files| {
                Some(
                    "EPISTEMIC HONESTY REMINDER: \
                     Do NOT derive human labels, semantic ratings, or ground-truth classes \
                     from prompt text, file names, paths, or other heuristics. \
                     If required annotations are not present, mark metrics as `not_implemented` \
                     or emit an explicit `skipped_reason` instead of inventing surrogate scores."
                        .to_owned(),
                )
            }));
        }

        Self { inner: loop_ }
    }

    /// Run the codegen turn loop.
    pub async fn run(&mut self, user_message: &str) -> anyhow::Result<TurnResult> {
        self.inner.run(user_message).await
    }

    /// Reference to the workspace path.
    pub fn workspace(&self) -> &Path {
        self.inner.workspace()
    }
}
