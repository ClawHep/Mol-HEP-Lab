//! Stage input/output contracts.
//!
//! Each pipeline stage declares what artifact names it requires as inputs and
//! what it is expected to produce as outputs.  The runner uses these contracts
//! to validate that prerequisites are satisfied before a stage is dispatched
//! and that the stage produced its expected deliverables afterward.

use crate::stages::Stage;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// StageContract
// ---------------------------------------------------------------------------

/// Declared inputs and outputs for a single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageContract {
    /// Artifact names that must be present before this stage can run.
    pub required_inputs: Vec<String>,
    /// Artifact names that this stage is expected to produce.
    pub expected_outputs: Vec<String>,
}

// ---------------------------------------------------------------------------
// Contract definitions
// ---------------------------------------------------------------------------

/// Return the input/output contract for `stage`.
///
/// Artifact names are conventional file-name stems; the executor and runner
/// use these as keys in the artifact registry.
pub fn get_contract(stage: Stage) -> StageContract {
    match stage {
        // Phase 1: Strategy ------------------------------------------------
        Stage::TopicInit => StageContract {
            required_inputs: vec![],
            expected_outputs: vec!["topic_brief".into(), "research_questions".into()],
        },
        Stage::ProblemDecompose => StageContract {
            required_inputs: vec!["topic_brief".into()],
            expected_outputs: vec!["problem_tree".into(), "sub_problems".into()],
        },

        // Phase 2: Exploration ---------------------------------------------
        Stage::SearchStrategy => StageContract {
            required_inputs: vec!["problem_tree".into()],
            expected_outputs: vec!["search_queries".into(), "source_list".into()],
        },
        Stage::LiteratureCollect => StageContract {
            required_inputs: vec!["search_queries".into()],
            expected_outputs: vec!["raw_papers".into(), "paper_metadata".into()],
        },
        Stage::LiteratureScreen => StageContract {
            required_inputs: vec!["raw_papers".into(), "paper_metadata".into()],
            expected_outputs: vec!["screened_papers".into(), "exclusion_reasons".into()],
        },
        Stage::KnowledgeExtract => StageContract {
            required_inputs: vec!["screened_papers".into()],
            expected_outputs: vec!["knowledge_cards".into(), "citation_map".into()],
        },

        // Phase 2 (cont.): Synthesis + Hypothesis --------------------------
        Stage::Synthesis => StageContract {
            required_inputs: vec!["knowledge_cards".into()],
            expected_outputs: vec!["synthesis_report".into(), "gap_analysis".into()],
        },
        Stage::HypothesisGen => StageContract {
            required_inputs: vec!["synthesis_report".into(), "gap_analysis".into()],
            expected_outputs: vec!["hypotheses".into(), "rationale".into()],
        },

        // Phase 3: Processing ----------------------------------------------
        Stage::ExperimentDesign => StageContract {
            required_inputs: vec!["hypotheses".into()],
            expected_outputs: vec!["experiment_plan".into(), "success_criteria".into()],
        },
        Stage::CodebaseSearch => StageContract {
            required_inputs: vec!["experiment_plan".into()],
            expected_outputs: vec!["codebase_context".into(), "relevant_files".into()],
        },
        Stage::CodeGeneration => StageContract {
            required_inputs: vec!["experiment_plan".into(), "codebase_context".into()],
            expected_outputs: vec!["experiment_code".into(), "code_readme".into()],
        },
        Stage::SanityCheck => StageContract {
            required_inputs: vec!["experiment_code".into()],
            expected_outputs: vec!["sanity_report".into()],
        },
        Stage::ResourcePlanning => StageContract {
            required_inputs: vec!["experiment_plan".into(), "sanity_report".into()],
            expected_outputs: vec!["resource_plan".into(), "compute_estimate".into()],
        },

        // Phase 3 (cont.): Execution ---------------------------------------
        Stage::ExperimentRun => StageContract {
            required_inputs: vec!["experiment_code".into(), "resource_plan".into()],
            expected_outputs: vec!["raw_results".into(), "run_logs".into()],
        },
        Stage::IterativeRefine => StageContract {
            required_inputs: vec!["raw_results".into(), "run_logs".into()],
            expected_outputs: vec!["refined_results".into(), "refinement_log".into()],
        },

        // Phase 4: Inference -----------------------------------------------
        Stage::ResultAnalysis => StageContract {
            required_inputs: vec!["refined_results".into()],
            expected_outputs: vec!["analysis_report".into(), "figures".into()],
        },
        Stage::ResearchDecision => StageContract {
            required_inputs: vec!["analysis_report".into(), "hypotheses".into()],
            expected_outputs: vec!["decision_record".into()],
        },
        Stage::KnowledgeSummary => StageContract {
            required_inputs: vec!["analysis_report".into(), "decision_record".into()],
            expected_outputs: vec!["knowledge_summary".into()],
        },

        // Phase 5: Documentation -------------------------------------------
        Stage::PaperOutline => StageContract {
            required_inputs: vec!["knowledge_summary".into(), "hypotheses".into()],
            expected_outputs: vec!["paper_outline".into()],
        },
        Stage::PaperDraft => StageContract {
            required_inputs: vec!["paper_outline".into(), "analysis_report".into()],
            expected_outputs: vec!["paper_draft".into()],
        },
        Stage::PeerReview => StageContract {
            required_inputs: vec!["paper_draft".into()],
            expected_outputs: vec!["review_comments".into()],
        },
        Stage::PaperRevision => StageContract {
            required_inputs: vec!["paper_draft".into(), "review_comments".into()],
            expected_outputs: vec!["paper_revised".into(), "revision_notes".into()],
        },

        // Phase 5 (cont.): Finalization ------------------------------------
        Stage::QualityGate => StageContract {
            required_inputs: vec!["paper_revised".into()],
            expected_outputs: vec!["quality_report".into()],
        },
        Stage::KnowledgeArchive => StageContract {
            required_inputs: vec!["knowledge_summary".into(), "paper_revised".into()],
            expected_outputs: vec!["archive_manifest".into()],
        },
        Stage::ExportPublish => StageContract {
            required_inputs: vec!["paper_revised".into()],
            expected_outputs: vec!["paper_final".into(), "paper_tex".into()],
        },
        Stage::CitationVerify => StageContract {
            required_inputs: vec!["paper_final".into()],
            expected_outputs: vec!["verification_report".into()],
        },

        // Special ----------------------------------------------------------
        Stage::Discussion => StageContract {
            required_inputs: vec![],
            expected_outputs: vec!["discussion_notes".into()],
        },
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Check if a logical artifact name is satisfied by the available artifacts.
///
/// Accepts both exact name matches and common file-name aliases (e.g.
/// `"topic_brief"` is satisfied by `"goal.md"`).
fn artifact_satisfied(name: &str, available: &[String]) -> bool {
    if available.iter().any(|a| a == name) {
        return true;
    }
    // File-name aliases: logical contract name → acceptable file names
    // Derived from actual stage implementations in stages_impl/
    let aliases: &[(&str, &[&str])] = &[
        // Phase 1: Strategy
        ("topic_brief", &["goal.md"]),
        ("research_questions", &["goal.md"]),
        ("problem_tree", &["problem_tree.md", "topic_evaluation.json"]),
        ("sub_problems", &["problem_tree.md"]),
        // Phase 2: Exploration
        ("search_queries", &["search_plan.yaml", "queries.json", "sources.json"]),
        ("source_list", &["search_plan.yaml", "sources.json"]),
        ("raw_papers", &["candidates.jsonl"]),
        ("paper_metadata", &["candidates.jsonl"]),
        ("screened_papers", &["candidates.jsonl", "screened_papers.jsonl"]),
        ("exclusion_reasons", &["candidates.jsonl", "exclusion_reasons.json"]),
        ("knowledge_cards", &["knowledge_cards.json"]),
        ("citation_map", &["citation_map.json"]),
        // Phase 2 (cont.): Synthesis + Hypothesis
        ("synthesis_report", &["synthesis_report.md"]),
        ("gap_analysis", &["gap_analysis.json"]),
        ("hypotheses", &["hypotheses.md"]),
        ("rationale", &["hypotheses.md"]),
        // Phase 3: Processing
        ("experiment_plan", &["exp_plan.yaml"]),
        ("success_criteria", &["exp_plan.yaml"]),
        ("codebase_context", &["codebase_context.json"]),
        ("relevant_files", &["relevant_files.json"]),
        ("experiment_code", &["experiment/", "experiment_spec.md"]),
        ("code_readme", &["experiment_spec.md"]),
        ("sanity_report", &["sanity_report.json"]),
        ("resource_plan", &["resource_plan.json"]),
        ("compute_estimate", &["schedule.json", "resource_plan.json"]),
        // Phase 3 (cont.): Execution
        ("raw_results", &["runs/"]),
        ("run_logs", &["runs/"]),
        ("refined_results", &["refinement_log.json", "experiment_final/"]),
        ("refinement_log", &["refinement_log.json"]),
        // Phase 4: Inference
        ("analysis_report", &["analysis_report.md", "experiment_summary.json"]),
        ("figures", &["analysis_report.md"]),
        ("decision_record", &["decision_record.json"]),
        ("knowledge_summary", &["knowledge_summary.json"]),
        // Phase 5: Documentation
        ("paper_outline", &["paper_outline.md"]),
        ("paper_draft", &["paper_draft.md"]),
        ("review_comments", &["review_comments.json"]),
        ("paper_revised", &["paper_revised.md"]),
        ("revision_notes", &["revision_notes.md"]),
        // Phase 5 (cont.): Finalization
        ("quality_report", &["quality_report.json"]),
        ("archive_manifest", &["archive_manifest.json"]),
        ("paper_final", &["paper_final.md"]),
        ("paper_tex", &["paper.tex"]),
        ("verification_report", &["verification_report.json"]),
        ("paper_final_verified", &["paper_final_verified.md"]),
    ];
    for &(logical, file_names) in aliases {
        if name == logical {
            return file_names.iter().any(|f| available.iter().any(|a| a == *f));
        }
    }
    false
}

/// Verify that all inputs declared by `stage`'s contract are present in
/// `available`.
///
/// Returns `Ok(())` when all required inputs are satisfied, otherwise an
/// error listing every missing artifact.
pub fn validate_inputs(stage: Stage, available: &[String]) -> Result<()> {
    let contract = get_contract(stage);
    let missing: Vec<&str> = contract
        .required_inputs
        .iter()
        .map(|s| s.as_str())
        .filter(|&req| !artifact_satisfied(req, available))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        bail!(
            "stage {} is missing required inputs: {}",
            stage.name(),
            missing.join(", ")
        )
    }
}

/// Verify that all outputs declared by `stage`'s contract are present in
/// `produced`.
///
/// Returns `Ok(())` when all expected outputs are present, otherwise an error
/// listing every absent artifact.
pub fn validate_outputs(stage: Stage, produced: &[String]) -> Result<()> {
    let contract = get_contract(stage);
    let missing: Vec<&str> = contract
        .expected_outputs
        .iter()
        .map(|s| s.as_str())
        .filter(|&exp| !artifact_satisfied(exp, produced))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        bail!(
            "stage {} did not produce expected outputs: {}",
            stage.name(),
            missing.join(", ")
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_stages_have_contracts() {
        use crate::stages::STAGE_SEQUENCE;
        for &stage in STAGE_SEQUENCE {
            // Should not panic
            let _ = get_contract(stage);
        }
    }

    #[test]
    fn validate_inputs_passes_when_satisfied() {
        let available: Vec<String> = vec!["topic_brief".into()];
        assert!(validate_inputs(Stage::ProblemDecompose, &available).is_ok());
    }

    #[test]
    fn validate_inputs_fails_when_missing() {
        let available: Vec<String> = vec![];
        let err = validate_inputs(Stage::ProblemDecompose, &available).unwrap_err();
        assert!(err.to_string().contains("topic_brief"));
    }

    #[test]
    fn validate_outputs_passes_when_produced() {
        let produced: Vec<String> = vec!["topic_brief".into(), "research_questions".into()];
        assert!(validate_outputs(Stage::TopicInit, &produced).is_ok());
    }

    #[test]
    fn validate_outputs_fails_when_missing() {
        let produced: Vec<String> = vec!["topic_brief".into()];
        let err = validate_outputs(Stage::TopicInit, &produced).unwrap_err();
        assert!(err.to_string().contains("research_questions"));
    }

    #[test]
    fn topic_init_has_no_required_inputs() {
        let contract = get_contract(Stage::TopicInit);
        assert!(contract.required_inputs.is_empty());
    }
}
