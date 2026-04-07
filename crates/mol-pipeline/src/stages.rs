//! 26-stage Mol-HEP-Lab pipeline state machine.
//!
//! Defines the stage sequence, status transitions, gate logic, and rollback
//! rules for the complete research pipeline.

use anyhow::{bail, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Stage
// ---------------------------------------------------------------------------

/// All pipeline stages in execution order.
///
/// The discriminants match the Python `Stage(IntEnum)` values exactly so that
/// checkpoints and JSON wire format remain compatible.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[repr(i32)]
pub enum Stage {
    // Phase A: Research Scoping
    TopicInit = 1,
    ProblemDecompose = 2,

    // Phase B: Literature Discovery
    SearchStrategy = 3,
    LiteratureCollect = 4,
    LiteratureScreen = 5, // GATE
    KnowledgeExtract = 6,

    // Phase C: Knowledge Synthesis
    Synthesis = 7,
    HypothesisGen = 8,

    // Phase D: Experiment Design
    ExperimentDesign = 9, // GATE
    CodebaseSearch = 10,
    CodeGeneration = 11,
    SanityCheck = 12,
    ResourcePlanning = 13,

    // Phase E: Experiment Execution
    ExperimentRun = 14,
    IterativeRefine = 15,

    // Phase F: Analysis & Decision
    ResultAnalysis = 16,
    ResearchDecision = 17,
    KnowledgeSummary = 18,

    // Phase G: Paper Writing
    PaperOutline = 19,
    PaperDraft = 20,
    PeerReview = 21,
    PaperRevision = 22,

    // Phase H: Finalization
    QualityGate = 23, // GATE
    KnowledgeArchive = 24,
    ExportPublish = 25,
    CitationVerify = 26,

    // Special
    Discussion = 100,
}

impl Stage {
    /// Human-readable name matching the Python enum attribute name.
    pub fn name(self) -> &'static str {
        match self {
            Stage::TopicInit => "TOPIC_INIT",
            Stage::ProblemDecompose => "PROBLEM_DECOMPOSE",
            Stage::SearchStrategy => "SEARCH_STRATEGY",
            Stage::LiteratureCollect => "LITERATURE_COLLECT",
            Stage::LiteratureScreen => "LITERATURE_SCREEN",
            Stage::KnowledgeExtract => "KNOWLEDGE_EXTRACT",
            Stage::Synthesis => "SYNTHESIS",
            Stage::HypothesisGen => "HYPOTHESIS_GEN",
            Stage::ExperimentDesign => "EXPERIMENT_DESIGN",
            Stage::CodebaseSearch => "CODEBASE_SEARCH",
            Stage::CodeGeneration => "CODE_GENERATION",
            Stage::SanityCheck => "SANITY_CHECK",
            Stage::ResourcePlanning => "RESOURCE_PLANNING",
            Stage::ExperimentRun => "EXPERIMENT_RUN",
            Stage::IterativeRefine => "ITERATIVE_REFINE",
            Stage::ResultAnalysis => "RESULT_ANALYSIS",
            Stage::ResearchDecision => "RESEARCH_DECISION",
            Stage::KnowledgeSummary => "KNOWLEDGE_SUMMARY",
            Stage::PaperOutline => "PAPER_OUTLINE",
            Stage::PaperDraft => "PAPER_DRAFT",
            Stage::PeerReview => "PEER_REVIEW",
            Stage::PaperRevision => "PAPER_REVISION",
            Stage::QualityGate => "QUALITY_GATE",
            Stage::KnowledgeArchive => "KNOWLEDGE_ARCHIVE",
            Stage::ExportPublish => "EXPORT_PUBLISH",
            Stage::CitationVerify => "CITATION_VERIFY",
            Stage::Discussion => "DISCUSSION",
        }
    }

    /// Numeric value as `i32`.
    pub fn as_i32(self) -> i32 {
        self.into()
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ---------------------------------------------------------------------------
// StageStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a single pipeline stage execution.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    #[default]
    Pending,
    Running,
    BlockedApproval,
    Approved,
    Rejected,
    Paused,
    Retrying,
    Failed,
    Done,
}

impl StageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::BlockedApproval => "blocked_approval",
            StageStatus::Approved => "approved",
            StageStatus::Rejected => "rejected",
            StageStatus::Paused => "paused",
            StageStatus::Retrying => "retrying",
            StageStatus::Failed => "failed",
            StageStatus::Done => "done",
        }
    }
}

impl std::fmt::Display for StageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// TransitionEvent
// ---------------------------------------------------------------------------

/// Events that drive state machine transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionEvent {
    Start,
    Succeed,
    Approve,
    Reject,
    Timeout,
    Fail,
    Retry,
    Resume,
    Pause,
}

impl TransitionEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            TransitionEvent::Start => "start",
            TransitionEvent::Succeed => "succeed",
            TransitionEvent::Approve => "approve",
            TransitionEvent::Reject => "reject",
            TransitionEvent::Timeout => "timeout",
            TransitionEvent::Fail => "fail",
            TransitionEvent::Retry => "retry",
            TransitionEvent::Resume => "resume",
            TransitionEvent::Pause => "pause",
        }
    }
}

impl std::fmt::Display for TransitionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// TransitionOutcome
// ---------------------------------------------------------------------------

/// Result of a state machine transition computation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionOutcome {
    /// The current or next active stage after this transition.
    pub stage: Stage,
    /// The new status for `stage`.
    pub status: StageStatus,
    /// The stage the runner should execute next (may equal `stage` for
    /// loops/retries, or a rollback target).
    pub next_stage: Option<Stage>,
    /// Non-`None` only on rollback transitions.
    pub rollback_stage: Option<Stage>,
    /// Whether the runner must write a checkpoint after this transition.
    pub checkpoint_required: bool,
    /// Human-readable decision token: `"proceed"`, `"block"`, `"retry"`,
    /// `"pivot"`, `"degraded"`.
    pub decision: String,
}

// ---------------------------------------------------------------------------
// Stage navigation constants
// ---------------------------------------------------------------------------

/// The canonical execution order of all pipeline stages (excluding
/// `Discussion` which is out-of-band).
pub const STAGE_SEQUENCE: &[Stage] = &[
    Stage::TopicInit,
    Stage::ProblemDecompose,
    Stage::SearchStrategy,
    Stage::LiteratureCollect,
    Stage::LiteratureScreen,
    Stage::KnowledgeExtract,
    Stage::Synthesis,
    Stage::HypothesisGen,
    Stage::ExperimentDesign,
    Stage::CodebaseSearch,
    Stage::CodeGeneration,
    Stage::SanityCheck,
    Stage::ResourcePlanning,
    Stage::ExperimentRun,
    Stage::IterativeRefine,
    Stage::ResultAnalysis,
    Stage::ResearchDecision,
    Stage::KnowledgeSummary,
    Stage::PaperOutline,
    Stage::PaperDraft,
    Stage::PeerReview,
    Stage::PaperRevision,
    Stage::QualityGate,
    Stage::KnowledgeArchive,
    Stage::ExportPublish,
    Stage::CitationVerify,
];

/// Gate stages — require human-in-the-loop approval before proceeding.
pub const GATE_STAGES: &[Stage] = &[
    Stage::LiteratureScreen,  // 5
    Stage::ExperimentDesign,  // 9
    Stage::QualityGate,       // 23
];

/// Maximum number of PIVOT/REFINE decision loops permitted.
/// Mirroring Python: `MAX_DECISION_PIVOTS = 0` (loops disabled).
pub const MAX_DECISION_PIVOTS: u32 = 0;

/// Stages that can be skipped on failure without aborting the pipeline.
pub const NONCRITICAL_STAGES: &[Stage] = &[
    Stage::QualityGate,      // Low quality should warn, not block deliverables
    Stage::KnowledgeArchive, // Archival doesn't affect paper output
    // Note: CitationVerify is NOT here — hallucinated citations MUST block export
];

// ---------------------------------------------------------------------------
// Phase map
// ---------------------------------------------------------------------------

/// Phase label → stages.  Ordered for deterministic iteration.
pub fn phase_map() -> Vec<(&'static str, &'static [Stage])> {
    vec![
        ("A: Research Scoping", &[Stage::TopicInit, Stage::ProblemDecompose] as &[Stage]),
        (
            "B: Literature Discovery",
            &[
                Stage::SearchStrategy,
                Stage::LiteratureCollect,
                Stage::LiteratureScreen,
                Stage::KnowledgeExtract,
            ],
        ),
        ("C: Knowledge Synthesis", &[Stage::Synthesis, Stage::HypothesisGen]),
        (
            "D: Experiment Design",
            &[
                Stage::ExperimentDesign,
                Stage::CodebaseSearch,
                Stage::CodeGeneration,
                Stage::SanityCheck,
                Stage::ResourcePlanning,
            ],
        ),
        ("E: Experiment Execution", &[Stage::ExperimentRun, Stage::IterativeRefine]),
        (
            "F: Analysis & Decision",
            &[Stage::ResultAnalysis, Stage::ResearchDecision, Stage::KnowledgeSummary],
        ),
        (
            "G: Paper Writing",
            &[Stage::PaperOutline, Stage::PaperDraft, Stage::PeerReview, Stage::PaperRevision],
        ),
        (
            "H: Finalization",
            &[
                Stage::QualityGate,
                Stage::KnowledgeArchive,
                Stage::ExportPublish,
                Stage::CitationVerify,
            ],
        ),
    ]
}

// ---------------------------------------------------------------------------
// Navigation helpers
// ---------------------------------------------------------------------------

/// Build the `next_stage` map from the sequence.
pub fn next_stage_map() -> HashMap<Stage, Option<Stage>> {
    let mut map = HashMap::new();
    for (i, &stage) in STAGE_SEQUENCE.iter().enumerate() {
        map.insert(stage, STAGE_SEQUENCE.get(i + 1).copied());
    }
    map
}

/// Build the `previous_stage` map from the sequence.
pub fn previous_stage_map() -> HashMap<Stage, Option<Stage>> {
    let mut map = HashMap::new();
    for (i, &stage) in STAGE_SEQUENCE.iter().enumerate() {
        map.insert(stage, if i > 0 { Some(STAGE_SEQUENCE[i - 1]) } else { None });
    }
    map
}

/// Return the stage that follows `stage` in the sequence, or `None` if last.
pub fn next_stage(stage: Stage) -> Option<Stage> {
    STAGE_SEQUENCE
        .iter()
        .position(|&s| s == stage)
        .and_then(|i| STAGE_SEQUENCE.get(i + 1))
        .copied()
}

/// Return the stage that precedes `stage` in the sequence, or `None` if first.
pub fn previous_stage(stage: Stage) -> Option<Stage> {
    STAGE_SEQUENCE
        .iter()
        .position(|&s| s == stage)
        .and_then(|i| if i > 0 { Some(STAGE_SEQUENCE[i - 1]) } else { None })
}

// ---------------------------------------------------------------------------
// Gate and rollback helpers
// ---------------------------------------------------------------------------

/// Gate rollback targets: when a gate rejects, where to roll back.
///
/// ```text
/// LiteratureScreen  → LiteratureCollect   (reject → re-collect)
/// ExperimentDesign  → HypothesisGen       (reject → re-hypothesize)
/// QualityGate       → PaperOutline        (reject → rewrite paper)
/// ```
pub fn gate_rollback(stage: Stage) -> Option<Stage> {
    match stage {
        Stage::LiteratureScreen => Some(Stage::LiteratureCollect),
        Stage::ExperimentDesign => Some(Stage::HypothesisGen),
        Stage::QualityGate => Some(Stage::PaperOutline),
        _ => None,
    }
}

/// Decision rollback targets for RESEARCH_DECISION outcomes.
///
/// ```text
/// "pivot"  → HypothesisGen     (discard hypotheses, re-generate)
/// "refine" → IterativeRefine   (keep hypotheses, re-run experiments)
/// ```
pub fn decision_rollback(decision: &str) -> Option<Stage> {
    match decision {
        "pivot" => Some(Stage::HypothesisGen),
        "refine" => Some(Stage::IterativeRefine),
        _ => None,
    }
}

/// Return `true` if `stage` is a gate stage requiring human approval.
///
/// The optional `hitl_required_stages` slice allows callers to restrict which
/// gate stages actually block; when `None` all gate stages return `true`.
pub fn gate_required(stage: Stage, hitl_required_stages: Option<&[i32]>) -> bool {
    if !GATE_STAGES.contains(&stage) {
        return false;
    }
    match hitl_required_stages {
        Some(required) => required.contains(&stage.as_i32()),
        None => true, // Default: all gate stages require approval
    }
}

/// Return the configured rollback target for `stage`, falling back to the
/// previous stage in the sequence, then `stage` itself.
pub fn default_rollback_stage(stage: Stage) -> Stage {
    gate_rollback(stage)
        .or_else(|| previous_stage(stage))
        .unwrap_or(stage)
}

// ---------------------------------------------------------------------------
// Core state machine
// ---------------------------------------------------------------------------

/// Compute the next state given current stage, status, and transition event.
///
/// Returns `Err` for unsupported (stage, status, event) combinations,
/// mirroring the Python `ValueError`.
pub fn advance(
    stage: Stage,
    status: StageStatus,
    event: TransitionEvent,
) -> Result<TransitionOutcome> {
    advance_with_opts(stage, status, event, None, None)
}

/// Like [`advance`] but with optional overrides for gate behaviour and rollback
/// target — useful for tests and the runner.
pub fn advance_with_opts(
    stage: Stage,
    status: StageStatus,
    event: TransitionEvent,
    hitl_required_stages: Option<&[i32]>,
    rollback_override: Option<Stage>,
) -> Result<TransitionOutcome> {
    let target_rollback = rollback_override.unwrap_or_else(|| default_rollback_stage(stage));

    // START → RUNNING
    if event == TransitionEvent::Start
        && matches!(
            status,
            StageStatus::Pending | StageStatus::Retrying | StageStatus::Paused
        )
    {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Running,
            next_stage: Some(stage),
            rollback_stage: None,
            checkpoint_required: false,
            decision: "proceed".to_owned(),
        });
    }

    // SUCCEED while RUNNING
    if event == TransitionEvent::Succeed && status == StageStatus::Running {
        if gate_required(stage, hitl_required_stages) {
            return Ok(TransitionOutcome {
                stage,
                status: StageStatus::BlockedApproval,
                next_stage: Some(stage),
                rollback_stage: None,
                checkpoint_required: false,
                decision: "block".to_owned(),
            });
        }
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Done,
            next_stage: next_stage(stage),
            rollback_stage: None,
            checkpoint_required: true,
            decision: "proceed".to_owned(),
        });
    }

    // APPROVE while BLOCKED
    if event == TransitionEvent::Approve && status == StageStatus::BlockedApproval {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Done,
            next_stage: next_stage(stage),
            rollback_stage: None,
            checkpoint_required: true,
            decision: "proceed".to_owned(),
        });
    }

    // REJECT while BLOCKED → rollback
    if event == TransitionEvent::Reject && status == StageStatus::BlockedApproval {
        return Ok(TransitionOutcome {
            stage: target_rollback,
            status: StageStatus::Pending,
            next_stage: Some(target_rollback),
            rollback_stage: Some(target_rollback),
            checkpoint_required: true,
            decision: "pivot".to_owned(),
        });
    }

    // TIMEOUT while BLOCKED → pause
    if event == TransitionEvent::Timeout && status == StageStatus::BlockedApproval {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Paused,
            next_stage: Some(stage),
            rollback_stage: None,
            checkpoint_required: true,
            decision: "block".to_owned(),
        });
    }

    // FAIL while RUNNING
    if event == TransitionEvent::Fail && status == StageStatus::Running {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Failed,
            next_stage: Some(stage),
            rollback_stage: None,
            checkpoint_required: true,
            decision: "retry".to_owned(),
        });
    }

    // RETRY while FAILED
    if event == TransitionEvent::Retry && status == StageStatus::Failed {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Retrying,
            next_stage: Some(stage),
            rollback_stage: None,
            checkpoint_required: false,
            decision: "retry".to_owned(),
        });
    }

    // RESUME while PAUSED
    if event == TransitionEvent::Resume && status == StageStatus::Paused {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Running,
            next_stage: Some(stage),
            rollback_stage: None,
            checkpoint_required: false,
            decision: "proceed".to_owned(),
        });
    }

    // PAUSE while FAILED
    if event == TransitionEvent::Pause && status == StageStatus::Failed {
        return Ok(TransitionOutcome {
            stage,
            status: StageStatus::Paused,
            next_stage: Some(stage),
            rollback_stage: None,
            checkpoint_required: true,
            decision: "block".to_owned(),
        });
    }

    bail!(
        "Unsupported transition: {} + {} for stage {}",
        status.as_str(),
        event.as_str(),
        stage.as_i32()
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_sequence_length() {
        // 26 main stages + no Discussion in sequence
        assert_eq!(STAGE_SEQUENCE.len(), 26);
    }

    #[test]
    fn stage_sequence_starts_at_topic_init() {
        assert_eq!(STAGE_SEQUENCE[0], Stage::TopicInit);
        assert_eq!(STAGE_SEQUENCE[25], Stage::CitationVerify);
    }

    #[test]
    fn next_stage_wraps_correctly() {
        assert_eq!(next_stage(Stage::TopicInit), Some(Stage::ProblemDecompose));
        assert_eq!(next_stage(Stage::CitationVerify), None);
    }

    #[test]
    fn previous_stage_first_is_none() {
        assert_eq!(previous_stage(Stage::TopicInit), None);
        assert_eq!(previous_stage(Stage::ProblemDecompose), Some(Stage::TopicInit));
    }

    #[test]
    fn gate_stages_are_correct() {
        assert!(gate_required(Stage::LiteratureScreen, None));
        assert!(gate_required(Stage::ExperimentDesign, None));
        assert!(gate_required(Stage::QualityGate, None));
        assert!(!gate_required(Stage::TopicInit, None));
    }

    #[test]
    fn gate_rollback_targets() {
        assert_eq!(gate_rollback(Stage::LiteratureScreen), Some(Stage::LiteratureCollect));
        assert_eq!(gate_rollback(Stage::ExperimentDesign), Some(Stage::HypothesisGen));
        assert_eq!(gate_rollback(Stage::QualityGate), Some(Stage::PaperOutline));
    }

    #[test]
    fn advance_start_pending() {
        let out = advance(Stage::TopicInit, StageStatus::Pending, TransitionEvent::Start).unwrap();
        assert_eq!(out.status, StageStatus::Running);
        assert_eq!(out.next_stage, Some(Stage::TopicInit));
    }

    #[test]
    fn advance_succeed_non_gate() {
        let out =
            advance(Stage::TopicInit, StageStatus::Running, TransitionEvent::Succeed).unwrap();
        assert_eq!(out.status, StageStatus::Done);
        assert_eq!(out.next_stage, Some(Stage::ProblemDecompose));
        assert!(out.checkpoint_required);
    }

    #[test]
    fn advance_succeed_gate_blocks() {
        let out = advance(
            Stage::LiteratureScreen,
            StageStatus::Running,
            TransitionEvent::Succeed,
        )
        .unwrap();
        assert_eq!(out.status, StageStatus::BlockedApproval);
        assert_eq!(out.decision, "block");
    }

    #[test]
    fn advance_approve_gate() {
        let out = advance(
            Stage::LiteratureScreen,
            StageStatus::BlockedApproval,
            TransitionEvent::Approve,
        )
        .unwrap();
        assert_eq!(out.status, StageStatus::Done);
        assert_eq!(out.next_stage, Some(Stage::KnowledgeExtract));
        assert!(out.checkpoint_required);
    }

    #[test]
    fn advance_reject_gate_rollback() {
        let out = advance(
            Stage::LiteratureScreen,
            StageStatus::BlockedApproval,
            TransitionEvent::Reject,
        )
        .unwrap();
        assert_eq!(out.status, StageStatus::Pending);
        assert_eq!(out.rollback_stage, Some(Stage::LiteratureCollect));
        assert_eq!(out.decision, "pivot");
    }

    #[test]
    fn advance_fail_then_retry() {
        let fail_out =
            advance(Stage::SanityCheck, StageStatus::Running, TransitionEvent::Fail).unwrap();
        assert_eq!(fail_out.status, StageStatus::Failed);
        assert_eq!(fail_out.decision, "retry");

        let retry_out =
            advance(Stage::SanityCheck, StageStatus::Failed, TransitionEvent::Retry).unwrap();
        assert_eq!(retry_out.status, StageStatus::Retrying);
    }

    #[test]
    fn advance_unsupported_transition_errors() {
        let result = advance(Stage::TopicInit, StageStatus::Done, TransitionEvent::Start);
        assert!(result.is_err());
    }

    #[test]
    fn noncritical_stages_correct() {
        assert!(NONCRITICAL_STAGES.contains(&Stage::QualityGate));
        assert!(NONCRITICAL_STAGES.contains(&Stage::KnowledgeArchive));
        assert!(!NONCRITICAL_STAGES.contains(&Stage::CitationVerify));
    }

    #[test]
    fn stage_discriminants() {
        assert_eq!(Stage::TopicInit.as_i32(), 1);
        assert_eq!(Stage::CitationVerify.as_i32(), 26);
        assert_eq!(Stage::Discussion.as_i32(), 100);
    }
}
