//! Acquirer agent — downloads and prepares selected benchmark datasets.

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent};
use crate::benchmark::selector::SelectedBenchmark;

// ---------------------------------------------------------------------------
// AcquisitionRecord
// ---------------------------------------------------------------------------

/// Result of downloading/preparing a single benchmark dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionRecord {
    /// Benchmark name.
    pub name: String,
    /// Local path where the dataset was saved.
    pub local_path: PathBuf,
    /// Python data-loader code snippet ready for injection into experiments.
    pub data_loader_code: String,
    /// pip requirements to load this dataset.
    pub requirements: String,
    /// Whether the acquisition succeeded.
    pub success: bool,
    /// Error message when `success == false`.
    pub error: String,
}

// ---------------------------------------------------------------------------
// AcquirerAgent
// ---------------------------------------------------------------------------

/// Downloads and prepares datasets for selected benchmarks.
pub struct AcquirerAgent {
    /// Root directory where datasets are stored.
    pub data_dir: PathBuf,
}

impl AcquirerAgent {
    /// Create an acquirer that stores data under `data_dir`.
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Generate a Python data-loader snippet for a HuggingFace dataset.
    fn hf_loader_code(&self, hf_id: &str, name: &str) -> String {
        format!(
            r#"# {name} — HuggingFace dataset loader
from datasets import load_dataset

def load_{snake}():
    dataset = load_dataset("{hf_id}")
    return dataset

{snake}_data = load_{snake}()
"#,
            name = name,
            snake = name.to_lowercase().replace(['-', ' '], "_"),
            hf_id = hf_id,
        )
    }

    /// Prepare (but don't actually download) a single benchmark.
    async fn prepare(&self, bench: &SelectedBenchmark) -> AcquisitionRecord {
        let name = bench.candidate.name.clone();
        let local_path = self.data_dir.join(&name);

        if let Some(ref hf_id) = bench.candidate.hf_id {
            info!(name, hf_id, "Preparing HuggingFace dataset");
            let data_loader_code = self.hf_loader_code(hf_id, &name);
            AcquisitionRecord {
                name,
                local_path,
                data_loader_code,
                requirements: "datasets>=2.0\n".to_owned(),
                success: true,
                error: String::new(),
            }
        } else {
            warn!(name, "No HuggingFace ID; cannot auto-acquire");
            AcquisitionRecord {
                name: name.clone(),
                local_path,
                data_loader_code: String::new(),
                requirements: String::new(),
                success: false,
                error: format!("No HuggingFace ID for '{name}'; manual download required."),
            }
        }
    }
}

#[async_trait]
impl BaseAgent for AcquirerAgent {
    fn name(&self) -> &str {
        "acquirer"
    }

    async fn plan(&self, _context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Benchmark acquisition",
            vec![
                "Retrieve selected benchmarks from previous stage".to_owned(),
                "Generate data-loader code for each benchmark".to_owned(),
                "Collect pip requirements".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(plan = %plan.title, "AcquirerAgent running");

        let benchmarks: Vec<SelectedBenchmark> = plan
            .metadata
            .get("selected_benchmarks")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let mut records: Vec<AcquisitionRecord> = Vec::with_capacity(benchmarks.len());
        let mut combined_loader = String::new();
        let mut combined_requirements = String::new();

        for bench in &benchmarks {
            let record = self.prepare(bench).await;
            if record.success {
                combined_loader.push_str(&record.data_loader_code);
                combined_loader.push('\n');
                combined_requirements.push_str(&record.requirements);
            }
            records.push(record);
        }

        let failed: Vec<&AcquisitionRecord> = records.iter().filter(|r| !r.success).collect();
        let output = format!(
            "Acquired {}/{} benchmarks.",
            records.len() - failed.len(),
            records.len()
        );

        let mut result = AgentStepResult::ok(output)
            .with_artifact("acquisition_records", &records)
            .with_artifact("data_loader_code", &combined_loader)
            .with_artifact("requirements", &combined_requirements);

        if !failed.is_empty() {
            result.next_action = "partial".to_owned();
        }

        Ok(result)
    }
}
