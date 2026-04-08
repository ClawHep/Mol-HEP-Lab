//! Renderer agent — executes generated figure scripts to produce image files.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::figure::codegen::GeneratedCode;

// ---------------------------------------------------------------------------
// RenderResult
// ---------------------------------------------------------------------------

/// Outcome of rendering a single figure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    /// Figure identifier.
    pub figure_id: String,
    /// Path to the rendered image file, when successful.
    pub output_path: Option<PathBuf>,
    /// Whether rendering succeeded.
    pub success: bool,
    /// Standard output from the rendering process.
    pub stdout: String,
    /// Standard error from the rendering process.
    pub stderr: String,
    /// Exit code of the rendering subprocess.
    pub exit_code: i32,
}

// ---------------------------------------------------------------------------
// RendererAgent
// ---------------------------------------------------------------------------

/// Executes matplotlib / TikZ scripts and collects rendered image paths.
pub struct RendererAgent {
    /// Working directory where scripts are written and executed.
    pub workdir: PathBuf,

    /// Path to the Python interpreter.
    pub python_path: String,

    /// Per-figure rendering timeout.
    pub timeout: Duration,

    /// Whether to use Docker for isolation (currently informational only).
    pub use_docker: bool,

    /// Docker image name when `use_docker` is `true`.
    pub docker_image: String,
}

impl RendererAgent {
    /// Create a renderer agent.
    pub fn new(
        workdir: impl Into<PathBuf>,
        python_path: impl Into<String>,
        timeout_sec: u64,
        use_docker: bool,
        docker_image: impl Into<String>,
    ) -> Self {
        Self {
            workdir: workdir.into(),
            python_path: python_path.into(),
            timeout: Duration::from_secs(timeout_sec),
            use_docker,
            docker_image: docker_image.into(),
        }
    }

    /// Write code to a temp file and execute it.
    async fn render_script(&self, code: &GeneratedCode) -> RenderResult {
        let script_name = format!("{}.py", code.figure_id);
        let script_path = self.workdir.join(&script_name);

        // Write script.
        if let Err(e) = tokio::fs::write(&script_path, &code.code).await {
            return RenderResult {
                figure_id: code.figure_id.clone(),
                output_path: None,
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to write script: {e}"),
                exit_code: -1,
            };
        }

        info!(
            figure = %code.figure_id,
            script = %script_path.display(),
            "Rendering figure"
        );

        // Run the script.
        let mut child = match Command::new(&self.python_path)
            .arg(&script_path)
            .current_dir(&self.workdir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return RenderResult {
                    figure_id: code.figure_id.clone(),
                    output_path: None,
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Failed to spawn Python: {e}"),
                    exit_code: -1,
                };
            }
        };

        // Apply timeout.
        let status = match tokio::time::timeout(self.timeout, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return RenderResult {
                    figure_id: code.figure_id.clone(),
                    output_path: None,
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Process error: {e}"),
                    exit_code: -1,
                };
            }
            Err(_) => {
                warn!(figure = %code.figure_id, "Render timeout");
                let _ = child.kill().await;
                return RenderResult {
                    figure_id: code.figure_id.clone(),
                    output_path: None,
                    success: false,
                    stdout: String::new(),
                    stderr: "Rendering timed out.".to_owned(),
                    exit_code: -2,
                };
            }
        };

        let exit_code = status.code().unwrap_or(-1);
        let success = exit_code == 0;

        // Collect stdout/stderr.
        let stdout = if let Some(mut o) = child.stdout.take() {
            let mut buf = String::new();
            let _ = o.read_to_string(&mut buf).await;
            buf
        } else {
            String::new()
        };
        let stderr = if let Some(mut e) = child.stderr.take() {
            let mut buf = String::new();
            let _ = e.read_to_string(&mut buf).await;
            buf
        } else {
            String::new()
        };

        let output_path = if success {
            let p = self.workdir.join(&code.output_path);
            if p.exists() { Some(p) } else { None }
        } else {
            None
        };

        RenderResult {
            figure_id: code.figure_id.clone(),
            output_path,
            success,
            stdout,
            stderr,
            exit_code,
        }
    }
}

#[async_trait]
impl BaseAgent for RendererAgent {
    fn name(&self) -> &str {
        "renderer"
    }

    async fn plan(&self, _context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Figure rendering",
            vec![
                "Write each generated script to disk".to_owned(),
                "Execute scripts via Python subprocess".to_owned(),
                "Collect rendered image paths".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!("RendererAgent running");

        let generated: Vec<GeneratedCode> = plan
            .metadata
            .get("generated_code")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Ensure workdir exists.
        tokio::fs::create_dir_all(&self.workdir).await?;

        let mut results: Vec<RenderResult> = Vec::with_capacity(generated.len());
        for code in &generated {
            results.push(self.render_script(code).await);
        }

        let passed = results.iter().filter(|r| r.success).count();
        let output = format!("Renderer: {}/{} figures rendered.", passed, results.len());

        let mut step = AgentStepResult::ok(output).with_artifact("render_results", &results);
        if passed < results.len() {
            step.next_action = "retry".to_owned();
        }

        Ok(step)
    }
}
