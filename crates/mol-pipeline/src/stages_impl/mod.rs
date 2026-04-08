//! Stage implementations for all 26 pipeline steps across 5 phases.
//!
//! - Phase 1: Strategy (1.1 TopicInit, 1.2 ProblemDecompose)
//! - Phase 2: Exploration (2.1–2.6: SearchStrategy → HypothesisGen)
//! - Phase 3: Processing (3.1–3.7: ExperimentDesign → IterativeRefine)
//! - Phase 4: Inference (4.1–4.3: ResultAnalysis → KnowledgeSummary)
//! - Phase 5: Documentation (5.1–5.8: PaperOutline → CitationVerify)
//! - Discussion: Multi-agent discussion (out-of-band)

pub mod discussion;
pub mod phase1;
pub mod phase2;
pub mod phase3;
pub mod phase4;
pub mod phase5;
