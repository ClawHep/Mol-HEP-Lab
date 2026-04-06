//! Validator agent — validates that acquired benchmarks are usable.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::base::{AgentContext, AgentPlan, AgentStepResult, BaseAgent, ReviewOutcome};
use crate::benchmark::acquirer::AcquisitionRecord;

// ---------------------------------------------------------------------------
// ValidationReport
// ---------------------------------------------------------------------------

/// Outcome of validating a single acquisition record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Name of the benchmark validated.
    pub name: String,
    /// Whether the benchmark passed all checks.
    pub passed: bool,
    /// Issues found during validation.
    pub warnings: Vec<String>,
    /// Fatal errors that prevent use.
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// ValidatorAgent
// ---------------------------------------------------------------------------

/// Validates that acquired benchmark datasets and generated code are usable.
pub struct ValidatorAgent {
    /// When `true`, any warning is treated as a failure.
    pub strict_mode: bool,
}

impl ValidatorAgent {
    /// Create a new validator.
    pub fn new(strict_mode: bool) -> Self {
        Self { strict_mode }
    }

    /// Run checks on a single [`AcquisitionRecord`].
    fn validate_record(&self, record: &AcquisitionRecord) -> ValidationReport {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if !record.success {
            errors.push(format!("Acquisition failed: {}", record.error));
        }

        if record.data_loader_code.is_empty() && record.success {
            warnings.push("No data-loader code was generated.".to_owned());
        }

        if record.requirements.is_empty() && record.success {
            warnings.push("No pip requirements were captured.".to_owned());
        }

        let passed = errors.is_empty() && (warnings.is_empty() || !self.strict_mode);

        ValidationReport {
            name: record.name.clone(),
            passed,
            warnings,
            errors,
        }
    }
}

#[async_trait]
impl BaseAgent for ValidatorAgent {
    fn name(&self) -> &str {
        "validator"
    }

    async fn plan(&self, _context: &AgentContext) -> Result<AgentPlan> {
        Ok(AgentPlan::new(
            "Benchmark validation",
            vec![
                "Check each acquisition record for completeness".to_owned(),
                "Verify data-loader code is non-empty".to_owned(),
                "Collect warnings and errors".to_owned(),
            ],
        ))
    }

    async fn run(&self, plan: &AgentPlan) -> Result<AgentStepResult> {
        debug!(plan = %plan.title, "ValidatorAgent running");

        let records: Vec<AcquisitionRecord> = plan
            .metadata
            .get("acquisition_records")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let reports: Vec<ValidationReport> = records
            .iter()
            .map(|r| self.validate_record(r))
            .collect();

        let passed_count = reports.iter().filter(|r| r.passed).count();
        let all_warnings: Vec<String> = reports
            .iter()
            .flat_map(|r| r.warnings.iter().cloned())
            .collect();

        let all_passed = reports.iter().all(|r| r.passed);

        if !all_passed {
            let errors: Vec<String> = reports
                .iter()
                .flat_map(|r| r.errors.iter().cloned())
                .collect();
            warn!(errors = ?errors, "Validation found errors");
        }

        let output = format!(
            "Validation: {}/{} benchmarks passed.",
            passed_count,
            reports.len()
        );

        let mut result = AgentStepResult::ok(output)
            .with_artifact("validation_reports", &reports)
            .with_artifact("validation_warnings", &all_warnings);

        result.success = all_passed;
        if !all_passed {
            result.error = "One or more benchmarks failed validation.".to_owned();
            result.next_action = "retry".to_owned();
        }

        Ok(result)
    }

    async fn review(&self, result: &AgentStepResult) -> Result<ReviewOutcome> {
        if result.success {
            Ok(ReviewOutcome::accept(1.0, "All benchmarks passed validation."))
        } else {
            let warnings: Vec<String> = result
                .artifacts
                .get("validation_warnings")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            Ok(ReviewOutcome::reject(
                0.5,
                "Some benchmarks failed validation.",
                warnings,
            ))
        }
    }
}
