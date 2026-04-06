//! Stage-to-skill mapping for the MetaMol pipeline.
//!
//! Provides a static mapping of which skills apply to each pipeline stage
//! (identified by stage number) and a helper to retrieve them.

/// Maps a numeric pipeline stage to the list of skill identifiers that apply
/// to it.  Skills are identified by the lowercase-hyphenated name used in
/// `~/.metamol/skills/<name>/SKILL.md`.
///
/// Stage numbering follows the AutoResearch pipeline convention:
///  1  – topic_init          6  – knowledge_extract   11 – resource_planning
///  2  – problem_decompose   7  – synthesis            12 – experiment_run
///  3  – search_strategy     8  – hypothesis_gen       13 – iterative_refine
///  4  – literature_collect  9  – experiment_design    14 – result_analysis
///  5  – literature_screen   10 – code_generation      15 – research_decision
/// 16  – paper_outline       17 – paper_draft          18 – peer_review
/// 19  – paper_revision      20 – quality_gate         21 – knowledge_archive
/// 22  – export_publish      23 – citation_verify
pub fn get_skills_for_stage(stage: u32) -> Vec<String> {
    let skills: &[&str] = match stage {
        1 => &["literature-search-strategy"],
        2 => &["research-gap-identification"],
        3 => &["literature-search-strategy"],
        4 => &["literature-search-strategy"],
        5 => &["paper-relevance-screening"],
        6 => &["knowledge-card-extraction"],
        7 => &["research-gap-identification"],
        8 => &["hypothesis-formulation"],
        9 => &["experiment-design-rigor"],
        10 => &["hardware-aware-coding"],
        11 => &[],
        12 => &["experiment-debugging"],
        13 => &["experiment-debugging"],
        14 => &["statistical-analysis"],
        15 => &["research-pivot-decision"],
        16 => &["academic-writing-structure"],
        17 => &["academic-writing-structure"],
        18 => &["peer-review-methodology"],
        19 => &["academic-writing-structure", "peer-review-methodology"],
        20 => &["peer-review-methodology"],
        21 => &[],
        22 => &[],
        23 => &["citation-integrity"],
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
        1..=9 | 14..=15 | 20 | 23 => "research",
        10 | 13 => "coding",
        11 => "productivity",
        12 | 21 | 22 => "automation",
        16..=19 => "communication",
        _ => "research",
    }
}

/// How many skills to inject at the given stage (top-k).
pub fn top_k_for_stage(stage: u32) -> usize {
    match stage {
        3 | 5 | 7 | 8 | 9 | 10 | 13 | 14 | 17 | 18 | 19 => 6,
        21 | 22 => 2,
        11 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_5_returns_screening_skill() {
        let skills = get_skills_for_stage(5);
        assert!(skills.contains(&"paper-relevance-screening".to_string()));
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
