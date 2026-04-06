//! Mol-HEP-Lab: Meta-level reasoning and self-improvement loops
//!
//! The `mol-metamol` crate provides the MetaMol integration layer:
//!
//! - **session** – MetaMol proxy session lifecycle management
//! - **prm_gate** – PRM (Process Reward Model) quality gate with majority-vote LLM judging
//! - **skill_feedback** – Skill effectiveness tracking across pipeline runs
//! - **lesson_to_skill** – Convert failure lessons into reusable MetaMol skill files
//! - **stage_skill_map** – Static mapping of pipeline stages to applicable skills

pub mod lesson_to_skill;
pub mod prm_gate;
pub mod session;
pub mod skill_feedback;
pub mod stage_skill_map;

// Convenient re-exports for the most commonly used types.
pub use lesson_to_skill::{
    LessonEntry, MetaMolLessonToSkillConfig, Severity, SkillDraft, convert_lessons,
    write_skill_draft,
};
pub use prm_gate::{PRMConfig, PRMResult, ResearchPRMGate};
pub use session::{MetaMolConfig, MetaMolSession};
pub use skill_feedback::{SkillEffectivenessRecord, SkillFeedbackStore, SkillStats};
pub use stage_skill_map::{
    get_skills_for_stage, lesson_category_to_skill_category, task_type_for_stage,
    top_k_for_stage,
};
