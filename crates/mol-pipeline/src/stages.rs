//! 18-stage Mol-HEP-Lab pipeline state machine.
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
/// Organised into 5 HEP analysis phases. Each stage is addressable as
/// `Phase.Step` (e.g. "3.4" = Phase 3 Processing, step 4 Sanity Check).
/// The discriminants are stable for checkpoint / wire-format compatibility.
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
    // Phase 1: Strategy
    TopicInit = 1,              // 1.1
    ProblemDecompose = 2,       // 1.2

    // Phase 2: Exploration
    LiteratureSearch = 3,       // 2.1  ← SearchStrategy + LiteratureCollect
    LiteratureScreen = 4,       // 2.2 GATE
    KnowledgeExtract = 5,       // 2.3
    SynthesisHypotheses = 6,    // 2.4  ← Synthesis + HypothesisGen

    // Phase 3: Execution
    ExperimentDesign = 7,       // 3.1 GATE
    CodebaseSearch = 8,         // 3.2
    CodeDevelop = 9,            // 3.3  ← CodeGeneration + SanityCheck (write→run→check→fix loop)
    ExperimentCycle = 10,       // 3.4  ← ResourcePlanning + ExperimentRun + IterativeRefine

    // Phase 4: Inference
    ResultAnalysis = 11,        // 4.1
    ResearchDecision = 12,      // 4.2
    KnowledgeSummary = 13,      // 4.3

    // Phase 5: Documentation
    PaperOutline = 14,          // 5.1
    PaperWrite = 15,            // 5.2  ← PaperDraft + PaperRevision
    PeerReview = 16,            // 5.3
    QualityGate = 17,           // 5.4 GATE
    Publish = 18,               // 5.5  ← KnowledgeArchive + ExportPublish + CitationVerify

    // Special
    Discussion = 100,
}

impl Stage {
    /// Human-readable name matching the Python enum attribute name.
    pub fn name(self) -> &'static str {
        match self {
            Stage::TopicInit => "TOPIC_INIT",
            Stage::ProblemDecompose => "PROBLEM_DECOMPOSE",
            Stage::LiteratureSearch => "LITERATURE_SEARCH",
            Stage::LiteratureScreen => "LITERATURE_SCREEN",
            Stage::KnowledgeExtract => "KNOWLEDGE_EXTRACT",
            Stage::SynthesisHypotheses => "SYNTHESIS_HYPOTHESES",
            Stage::ExperimentDesign => "EXPERIMENT_DESIGN",
            Stage::CodebaseSearch => "CODEBASE_SEARCH",
            Stage::CodeDevelop => "CODE_DEVELOP",
            Stage::ExperimentCycle => "EXPERIMENT_CYCLE",
            Stage::ResultAnalysis => "RESULT_ANALYSIS",
            Stage::ResearchDecision => "RESEARCH_DECISION",
            Stage::KnowledgeSummary => "KNOWLEDGE_SUMMARY",
            Stage::PaperOutline => "PAPER_OUTLINE",
            Stage::PaperWrite => "PAPER_WRITE",
            Stage::PeerReview => "PEER_REVIEW",
            Stage::QualityGate => "QUALITY_GATE",
            Stage::Publish => "PUBLISH",
            Stage::Discussion => "DISCUSSION",
        }
    }

    /// Numeric value as `i32`.
    pub fn as_i32(self) -> i32 {
        self.into()
    }

    /// Which HEP phase this stage belongs to.
    pub fn phase(self) -> Phase {
        match self {
            Stage::TopicInit | Stage::ProblemDecompose => Phase::Strategy,
            Stage::LiteratureSearch
            | Stage::LiteratureScreen
            | Stage::KnowledgeExtract
            | Stage::SynthesisHypotheses => Phase::Exploration,
            Stage::ExperimentDesign
            | Stage::CodebaseSearch
            | Stage::CodeDevelop
            | Stage::ExperimentCycle => Phase::Processing,
            Stage::ResultAnalysis
            | Stage::ResearchDecision
            | Stage::KnowledgeSummary => Phase::Inference,
            Stage::PaperOutline
            | Stage::PaperWrite
            | Stage::PeerReview
            | Stage::QualityGate
            | Stage::Publish => Phase::Documentation,
            Stage::Discussion => Phase::Strategy, // special: assign to first phase
        }
    }

    /// 1-based step index within the parent phase (e.g. SanityCheck → 4).
    pub fn phase_step(self) -> u8 {
        let stages = self.phase().stages();
        stages
            .iter()
            .position(|&s| s == self)
            .map(|i| (i + 1) as u8)
            .unwrap_or(0)
    }

    /// Display label in `Phase.Step` format (e.g. "3.4 Sanity Check").
    pub fn phase_label(self) -> String {
        if self == Stage::Discussion {
            return "Discussion".to_owned();
        }
        format!("{}.{} {}", self.phase().number(), self.phase_step(), self.display_name())
    }

    /// Human-friendly display name (e.g. "Sanity Check" instead of "SANITY_CHECK").
    pub fn display_name(self) -> &'static str {
        match self {
            Stage::TopicInit => "Topic Init",
            Stage::ProblemDecompose => "Problem Decompose",
            Stage::LiteratureSearch => "Literature Search",
            Stage::LiteratureScreen => "Literature Screen",
            Stage::KnowledgeExtract => "Knowledge Extract",
            Stage::SynthesisHypotheses => "Synthesis & Hypotheses",
            Stage::ExperimentDesign => "Experiment Design",
            Stage::CodebaseSearch => "Codebase Search",
            Stage::CodeDevelop => "Code Develop",
            Stage::ExperimentCycle => "Experiment Cycle",
            Stage::ResultAnalysis => "Result Analysis",
            Stage::ResearchDecision => "Research Decision",
            Stage::KnowledgeSummary => "Knowledge Summary",
            Stage::PaperOutline => "Paper Outline",
            Stage::PaperWrite => "Paper Write",
            Stage::PeerReview => "Peer Review",
            Stage::QualityGate => "Quality Gate",
            Stage::Publish => "Publish",
            Stage::Discussion => "Discussion",
        }
    }

    /// Parse a stage from its human-readable name (e.g. `"TOPIC_INIT"`).
    pub fn from_name(name: &str) -> Result<Self> {
        let upper = name.to_uppercase();
        for &stage in STAGE_SEQUENCE {
            if stage.name() == upper {
                return Ok(stage);
            }
        }
        if upper == "DISCUSSION" {
            return Ok(Stage::Discussion);
        }
        bail!("unknown stage name: {name}")
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
    Stage::LiteratureSearch,
    Stage::LiteratureScreen,
    Stage::KnowledgeExtract,
    Stage::SynthesisHypotheses,
    Stage::ExperimentDesign,
    Stage::CodebaseSearch,
    Stage::CodeDevelop,
    Stage::ExperimentCycle,
    Stage::ResultAnalysis,
    Stage::ResearchDecision,
    Stage::KnowledgeSummary,
    Stage::PaperOutline,
    Stage::PaperWrite,
    Stage::PeerReview,
    Stage::QualityGate,
    Stage::Publish,
];

/// Gate stages — require human-in-the-loop approval before proceeding.
pub const GATE_STAGES: &[Stage] = &[
    Stage::LiteratureScreen,  // 4
    Stage::ExperimentDesign,  // 7
    Stage::QualityGate,       // 17
];

/// Maximum number of PIVOT/REFINE decision loops permitted.
/// Allows the pipeline to respond to ResearchDecision signals by looping
/// back to SynthesisHypotheses (pivot) or ExperimentCycle (refine).
pub const MAX_DECISION_PIVOTS: u32 = 2;

/// Stages that can be skipped on failure without aborting the pipeline.
pub const NONCRITICAL_STAGES: &[Stage] = &[
    Stage::QualityGate,      // Low quality should warn, not block deliverables
    // Note: Publish includes citation verification — hallucinated citations block
];

// ---------------------------------------------------------------------------
// Phase system
// ---------------------------------------------------------------------------

/// The 5 HEP analysis phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Phase {
    Strategy = 1,
    Exploration = 2,
    Processing = 3,
    Inference = 4,
    Documentation = 5,
}

impl Phase {
    pub fn number(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Phase::Strategy => "Strategy",
            Phase::Exploration => "Exploration",
            Phase::Processing => "Processing",
            Phase::Inference => "Inference",
            Phase::Documentation => "Documentation",
        }
    }

    /// Stages belonging to this phase, in execution order.
    pub fn stages(self) -> &'static [Stage] {
        match self {
            Phase::Strategy => &[Stage::TopicInit, Stage::ProblemDecompose],
            Phase::Exploration => &[
                Stage::LiteratureSearch,
                Stage::LiteratureScreen,
                Stage::KnowledgeExtract,
                Stage::SynthesisHypotheses,
            ],
            Phase::Processing => &[
                Stage::ExperimentDesign,
                Stage::CodebaseSearch,
                Stage::CodeDevelop,
                Stage::ExperimentCycle,
            ],
            Phase::Inference => &[
                Stage::ResultAnalysis,
                Stage::ResearchDecision,
                Stage::KnowledgeSummary,
            ],
            Phase::Documentation => &[
                Stage::PaperOutline,
                Stage::PaperWrite,
                Stage::PeerReview,
                Stage::QualityGate,
                Stage::Publish,
            ],
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Phase {}: {}", self.number(), self.name())
    }
}

/// All phases in order.
pub const PHASES: &[Phase] = &[
    Phase::Strategy,
    Phase::Exploration,
    Phase::Processing,
    Phase::Inference,
    Phase::Documentation,
];

/// Phase label → stages. Ordered for deterministic iteration.
pub fn phase_map() -> Vec<(&'static str, &'static [Stage])> {
    PHASES
        .iter()
        .map(|p| {
            let label: &'static str = match p {
                Phase::Strategy => "1: Strategy",
                Phase::Exploration => "2: Exploration",
                Phase::Processing => "3: Processing",
                Phase::Inference => "4: Inference",
                Phase::Documentation => "5: Documentation",
            };
            (label, p.stages())
        })
        .collect()
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
/// LiteratureScreen  → LiteratureSearch       (reject → re-search)
/// ExperimentDesign  → SynthesisHypotheses    (reject → re-hypothesize)
/// QualityGate       → PaperOutline           (reject → rewrite paper)
/// ```
pub fn gate_rollback(stage: Stage) -> Option<Stage> {
    match stage {
        Stage::LiteratureScreen => Some(Stage::LiteratureSearch),
        Stage::ExperimentDesign => Some(Stage::SynthesisHypotheses),
        Stage::QualityGate => Some(Stage::PaperOutline),
        _ => None,
    }
}

/// Decision rollback targets for RESEARCH_DECISION outcomes.
///
/// ```text
/// "pivot"  → SynthesisHypotheses  (discard hypotheses, re-generate)
/// "refine" → ExperimentCycle      (keep hypotheses, re-run experiments)
/// ```
pub fn decision_rollback(decision: &str) -> Option<Stage> {
    match decision {
        "pivot" => Some(Stage::SynthesisHypotheses),
        "refine" => Some(Stage::ExperimentCycle),
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
        assert_eq!(STAGE_SEQUENCE.len(), 18);
    }

    #[test]
    fn stage_sequence_starts_at_topic_init() {
        assert_eq!(STAGE_SEQUENCE[0], Stage::TopicInit);
        assert_eq!(STAGE_SEQUENCE[17], Stage::Publish);
    }

    #[test]
    fn next_stage_wraps_correctly() {
        assert_eq!(next_stage(Stage::TopicInit), Some(Stage::ProblemDecompose));
        assert_eq!(next_stage(Stage::Publish), None);
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
        assert_eq!(gate_rollback(Stage::LiteratureScreen), Some(Stage::LiteratureSearch));
        assert_eq!(gate_rollback(Stage::ExperimentDesign), Some(Stage::SynthesisHypotheses));
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
        assert_eq!(out.rollback_stage, Some(Stage::LiteratureSearch));
        assert_eq!(out.decision, "pivot");
    }

    #[test]
    fn advance_fail_then_retry() {
        let fail_out =
            advance(Stage::CodeDevelop, StageStatus::Running, TransitionEvent::Fail).unwrap();
        assert_eq!(fail_out.status, StageStatus::Failed);
        assert_eq!(fail_out.decision, "retry");

        let retry_out =
            advance(Stage::CodeDevelop, StageStatus::Failed, TransitionEvent::Retry).unwrap();
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
        assert!(!NONCRITICAL_STAGES.contains(&Stage::Publish));
    }

    #[test]
    fn stage_discriminants() {
        assert_eq!(Stage::TopicInit.as_i32(), 1);
        assert_eq!(Stage::Publish.as_i32(), 18);
        assert_eq!(Stage::Discussion.as_i32(), 100);
    }

    #[test]
    fn phase_assignment() {
        assert_eq!(Stage::TopicInit.phase(), Phase::Strategy);
        assert_eq!(Stage::SynthesisHypotheses.phase(), Phase::Exploration);
        assert_eq!(Stage::ExperimentCycle.phase(), Phase::Processing);
        assert_eq!(Stage::ResultAnalysis.phase(), Phase::Inference);
        assert_eq!(Stage::QualityGate.phase(), Phase::Documentation);
    }

    #[test]
    fn phase_step_numbering() {
        assert_eq!(Stage::TopicInit.phase_step(), 1);
        assert_eq!(Stage::ProblemDecompose.phase_step(), 2);
        assert_eq!(Stage::CodeDevelop.phase_step(), 3); // 3.3
        assert_eq!(Stage::Publish.phase_step(), 5); // 5.5
    }

    #[test]
    fn phase_label_format() {
        assert_eq!(Stage::CodeDevelop.phase_label(), "3.3 Code Develop");
        assert_eq!(Stage::TopicInit.phase_label(), "1.1 Topic Init");
        assert_eq!(Stage::Publish.phase_label(), "5.5 Publish");
        assert_eq!(Stage::Discussion.phase_label(), "Discussion");
    }

    #[test]
    fn all_stages_covered_by_phases() {
        let mut covered: Vec<Stage> = PHASES.iter().flat_map(|p| p.stages().iter().copied()).collect();
        covered.sort();
        let mut all: Vec<Stage> = STAGE_SEQUENCE.to_vec();
        all.sort();
        assert_eq!(covered, all, "every stage must belong to exactly one phase");
    }

    #[test]
    fn phase_map_has_five_entries() {
        assert_eq!(phase_map().len(), 5);
    }
}
