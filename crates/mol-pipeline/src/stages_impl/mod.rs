//! Stage implementations for all 26 pipeline stages.
//!
//! Each sub-module handles a phase of the research pipeline:
//! - Phase A: Research scoping (TopicInit, ProblemDecompose)
//! - Phase B: Literature discovery (SearchStrategy, LiteratureCollect, LiteratureScreen, KnowledgeExtract)
//! - Phase C: Knowledge synthesis (Synthesis, HypothesisGen)
//! - Phase D: Experiment design (ExperimentDesign, CodebaseSearch, CodeGeneration, SanityCheck, ResourcePlanning)
//! - Phase E: Experiment execution (ExperimentRun, IterativeRefine)
//! - Phase F: Analysis & decision (ResultAnalysis, ResearchDecision, KnowledgeSummary)
//! - Phase G: Paper writing (PaperOutline, PaperDraft, PeerReview, PaperRevision)
//! - Phase H: Finalization (QualityGate, KnowledgeArchive, ExportPublish, CitationVerify)
//! - Discussion: Multi-agent discussion

pub mod discussion;
pub mod phase_a;
pub mod phase_b;
pub mod phase_c;
pub mod phase_d;
pub mod phase_e;
pub mod phase_f;
pub mod phase_g;
pub mod phase_h;
