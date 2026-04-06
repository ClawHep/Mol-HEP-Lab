//! Nano Banana agent — generates conceptual/architectural images via Gemini API.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::figure::decision::FigureRequest;

// ---------------------------------------------------------------------------
// ImageGenerationResult
// ---------------------------------------------------------------------------

/// Outcome of generating a single image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationResult {
    /// Figure identifier.
    pub figure_id: String,
    /// Path to the generated image file.
    pub output_path: Option<PathBuf>,
    /// Prompt sent to Gemini.
    pub prompt: String,
    /// Whether generation succeeded.
    pub success: bool,
    /// Error message when `success == false`.
    pub error: String,
}

// ---------------------------------------------------------------------------
// NanoBananaAgent
// ---------------------------------------------------------------------------

/// Generates conceptual diagrams and architectural images using the Gemini API.
///
/// Named "Nano Banana" in the Python codebase.
pub struct NanoBananaAgent {
    /// Gemini API key.  Falls back to `GEMINI_API_KEY` env var when empty.
    pub api_key: String,

    /// Gemini model name (e.g. `"gemini-2.0-flash-preview-image-generation"`).
    pub model: String,

    /// Directory where generated images are saved.
    pub output_dir: PathBuf,
}

impl NanoBananaAgent {
    /// Create a new Nano Banana agent.
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        output_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            output_dir: output_dir.into(),
        }
    }

    /// Resolve the API key from config or environment.
    fn resolved_api_key(&self) -> Option<String> {
        if !self.api_key.is_empty() {
            return Some(self.api_key.clone());
        }
        std::env::var("GEMINI_API_KEY").ok()
    }

    /// Build an image generation prompt from a figure request.
    fn build_prompt(&self, request: &FigureRequest) -> String {
        format!(
            "Create a clean, professional scientific diagram suitable for an academic paper. \
             Title: '{}'. Description: {}. \
             Style: white background, clear labels, publication quality, no watermarks.",
            request.title, request.description
        )
    }

    /// Call the Gemini image generation API.
    async fn call_gemini_api(&self, prompt: &str) -> Result<Vec<u8>> {
        let api_key = self
            .resolved_api_key()
            .ok_or_else(|| anyhow!("No Gemini API key available"))?;

        let client = reqwest::Client::new();
        // Use header-based auth instead of URL query parameter to avoid key leakage
        // in proxy logs, server logs, and browser history.
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );

        let body = serde_json::json!({
            "contents": [{
                "parts": [{
                    "text": prompt
                }]
            }],
            "generationConfig": {
                "responseModalities": ["IMAGE"],
            }
        });

        let resp = client
            .post(&url)
            .header("x-goog-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Gemini API error {status}: {text}"));
        }

        let response: Value = resp.json().await?;

        // Parse the image bytes from the base64-encoded response.
        let b64 = response
            .pointer("/candidates/0/content/parts/0/inlineData/data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No image data in Gemini response"))?;

        // Decode base64.
        let bytes = base64_decode(b64).ok_or_else(|| anyhow!("Base64 decode failed"))?;
        Ok(bytes)
    }

    /// Generate and save an image for a single figure request.
    async fn generate_image(&self, request: &FigureRequest) -> ImageGenerationResult {
        let prompt = self.build_prompt(request);
        let output_filename = format!("{}.png", request.id);
        let output_path = self.output_dir.join(&output_filename);

        info!(figure = %request.id, "NanoBananaAgent generating image");

        match self.call_gemini_api(&prompt).await {
            Ok(bytes) => {
                if let Err(e) = tokio::fs::write(&output_path, &bytes).await {
                    ImageGenerationResult {
                        figure_id: request.id.clone(),
                        output_path: None,
                        prompt,
                        success: false,
                        error: format!("Failed to write image: {e}"),
                    }
                } else {
                    info!(path = %output_path.display(), "Image saved");
                    ImageGenerationResult {
                        figure_id: request.id.clone(),
                        output_path: Some(output_path),
                        prompt,
                        success: true,
                        error: String::new(),
                    }
                }
            }
            Err(e) => {
                warn!(figure = %request.id, "Gemini image generation failed: {e}");
                ImageGenerationResult {
                    figure_id: request.id.clone(),
                    output_path: None,
                    prompt,
                    success: false,
                    error: e.to_string(),
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl BaseAgent for NanoBananaAgent {
    fn name(&self) -> &str {
        "nano_banana"
    }

    async fn plan(&self, context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Gemini image generation",
            vec![
                format!("Generate conceptual images for '{}'", context.topic),
                "Call Gemini API for each image figure".to_owned(),
                "Save results to output directory".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!("NanoBananaAgent running");

        let requests: Vec<FigureRequest> = plan
            .metadata
            .get("image_requests")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Ensure output directory exists.
        tokio::fs::create_dir_all(&self.output_dir).await?;

        let mut results: Vec<ImageGenerationResult> = Vec::with_capacity(requests.len());
        for req in &requests {
            results.push(self.generate_image(req).await);
        }

        let passed = results.iter().filter(|r| r.success).count();
        let output = format!(
            "NanoBanana: {}/{} images generated.",
            passed,
            results.len()
        );

        let mut step = AgentStepResult::ok(output)
            .with_artifact("image_generation_results", &results);
        if passed < results.len() {
            step.next_action = "partial".to_owned();
        }

        Ok(step)
    }
}

// ---------------------------------------------------------------------------
// Minimal base64 decoder
// ---------------------------------------------------------------------------

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'\n' && b != b'\r')
        .collect();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = val(chunk[2])?;
        let d = val(chunk[3])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            out.push(((b & 0x0F) << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((c & 0x03) << 6) | d);
        }
    }
    Some(out)
}
