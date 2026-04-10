//! Stage-to-skill mapping for the MetaMol pipeline.
//!
//! Provides a static mapping of which skills apply to each pipeline stage
//! (identified by stage number) and a helper to retrieve them.

/// Maps a numeric pipeline stage to the list of skill identifiers that apply
/// to it.  Skills are identified by the lowercase-hyphenated name used in
/// `~/.metamol/skills/<name>/SKILL.md`.
///
/// Stage numbering follows the consolidated 18-stage pipeline:
///  1  – topic_init            7  – experiment_design   13 – knowledge_summary
///  2  – problem_decompose     8  – codebase_search     14 – paper_outline
///  3  – literature_search     9  – code_develop         15 – paper_write
///  4  – literature_screen    10  – experiment_cycle     16 – peer_review
///  5  – knowledge_extract    11  – result_analysis      17 – quality_gate
///  6  – synthesis_hypotheses 12  – research_decision    18 – publish
pub fn get_skills_for_stage(stage: u32) -> Vec<String> {
    let skills: &[&str] = match stage {
        1 => &["literature-search-strategy"],
        2 => &["research-gap-identification"],
        3 => &["literature-search-strategy"],
        4 => &["paper-relevance-screening"],
        5 => &["knowledge-card-extraction"],
        6 => &["research-gap-identification", "hypothesis-formulation"],
        7 => &["experiment-design-rigor"],
        8 => &["hardware-aware-coding"],
        9 => &["hardware-aware-coding", "experiment-debugging"],
        10 => &["experiment-debugging"],
        11 => &["statistical-analysis"],
        12 => &["research-pivot-decision"],
        13 => &[],
        14 => &["academic-writing-structure"],
        15 => &["academic-writing-structure", "peer-review-methodology"],
        16 => &["peer-review-methodology"],
        17 => &["peer-review-methodology"],
        18 => &["citation-integrity"],
        _ => &[],
    };
    skills.iter().map(|s| s.to_string()).collect()
}

/// Mapping from lesson category strings (as produced by the evolution store)
/// to MetaMol skill category strings.
pub fn lesson_category_to_skill_category(lesson_category: &str) -> &'static str {
    match lesson_category {
        "system" => "automation",
        "experiment" => "coding",
        "writing" => "communication",
        "analysis" => "data_analysis",
        "literature" => "research",
        "pipeline" => "automation",
        _ => "research",
    }
}

/// Returns the task type tag for a given pipeline stage (used when querying
/// MetaMol for relevant skills).
pub fn task_type_for_stage(stage: u32) -> &'static str {
    match stage {
        // Phase 1-2: Strategy + Exploration + ExperimentDesign
        1..=7 | 11 | 13 => "research",
        // Phase 3: CodebaseSearch + CodeDevelop + ExperimentCycle
        8..=10 => "coding",
        // Phase 4: ResearchDecision (automation/orchestration)
        12 => "automation",
        // Phase 5: Documentation (PaperOutline..Publish)
        14..=18 => "communication",
        _ => "research",
    }
}

/// How many skills to inject at the given stage (top-k).
pub fn top_k_for_stage(stage: u32) -> usize {
    match stage {
        // Stages with many applicable skills benefit from more lesson retrieval
        3 | 4 | 6 | 7 | 9 | 10 | 15 | 16 => 6,
        // Lightweight stages
        18 => 2,
        // Codebase search
        8 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_4_returns_screening_skill() {
        // Stage 4 = LITERATURE_SCREEN in the 18-stage pipeline
        let skills = get_skills_for_stage(4);
        assert!(skills.contains(&"paper-relevance-screening".to_string()));
    }

    #[test]
    fn stage_5_returns_extraction_skill() {
        // Stage 5 = KNOWLEDGE_EXTRACT
        let skills = get_skills_for_stage(5);
        assert!(skills.contains(&"knowledge-card-extraction".to_string()));
    }

    #[test]
    fn unknown_stage_returns_empty() {
        assert!(get_skills_for_stage(999).is_empty());
    }

    #[test]
    fn lesson_category_mapping_pipeline() {
        assert_eq!(lesson_category_to_skill_category("pipeline"), "automation");
    }
}
