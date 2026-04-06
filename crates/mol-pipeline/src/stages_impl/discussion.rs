//! Discussion stage executor.
//!
//! Generates a simulated multi-agent discussion transcript covering synthesis,
//! hypotheses, and experiment findings.

use crate::executor::{
    read_prior_artifact_pub, utcnow_iso, StageContext, StageResult,
};
use crate::stages::{Stage, StageStatus};
use std::fs;

// ---------------------------------------------------------------------------
// Discussion
// ---------------------------------------------------------------------------

/// Execute the Discussion stage.
///
/// Reads synthesis, hypotheses, and experiment data; produces
/// `discussion_notes.md` with a simulated multi-agent discussion transcript.
pub async fn execute_discussion(stage: Stage, ctx: &StageContext) -> StageResult {
    let stage_dir = ctx.stage_dir(stage);
    if let Err(e) = fs::create_dir_all(&stage_dir) {
        return StageResult::failure(stage, format!("create stage dir: {e}"));
    }

    let topic = ctx.config.topic.as_str();
    let _synthesis = read_prior_artifact_pub(&ctx.run_dir, "synthesis_report.md").unwrap_or_default();
    let _hypotheses = read_prior_artifact_pub(&ctx.run_dir, "hypotheses.md").unwrap_or_default();
    let _analysis = read_prior_artifact_pub(&ctx.run_dir, "analysis_report.md").unwrap_or_default();

    // ---- discussion_notes.md -----------------------------------------------
    let discussion_notes = format!(
        r#"# Multi-Agent Discussion: {topic}

**Session Date**: {ts}
**Participants**: Coordinator, Literature Agent, Experiment Agent, Writing Agent, Critic Agent

---

## Session Opening

**Coordinator**: Welcome to our research discussion on "{topic}". Let us review
the key findings from each phase and decide on the path forward.

---

## Phase 1: Literature Review Summary

**Literature Agent**: Our systematic review identified 5 key papers and 3 major
research clusters. The primary gap we found is the lack of cross-domain
generalization methods. Most existing approaches achieve strong in-domain
performance but degrade significantly under distribution shift.

**Critic Agent**: I'd like to challenge that finding. Is the cross-domain
generalization gap really the most impactful problem, or are there more
fundamental issues with the current benchmarks themselves?

**Literature Agent**: That is a fair point. The benchmarks do have limitations —
many use synthetic or curated datasets. However, the community is actively
working to address benchmark quality, and the generalization gap persists
even on the best available benchmarks.

---

## Phase 2: Hypothesis Evaluation

**Experiment Agent**: We tested 4 hypotheses. H1 (Cross-Domain Transfer) was
confirmed with >5% improvement over baselines (p < 0.01). H2 (Sample Efficiency)
was partially confirmed — we need only 50% of labeled data to match the baseline.
H3 (Computational Efficiency) was confirmed at 1.2x baseline inference cost.

**Coordinator**: Excellent. Are there any hypotheses that were not confirmed?

**Experiment Agent**: H4 (Theoretical Convergence) was not fully tested — we
deferred the theoretical analysis to future work. The experiments converged
empirically, but formal proofs are missing.

**Critic Agent**: The lack of theoretical grounding is a weakness that reviewers
will likely flag. Should we include at least a sketch proof in the paper?

**Writing Agent**: I suggest including a 1-paragraph intuitive explanation in
Section 5 (Discussion) and noting formal analysis as future work. This is more
honest than a rushed proof that may contain errors.

---

## Phase 3: Experimental Design Review

**Experiment Agent**: The experiment ran cleanly across all 5 seeds. The mean
accuracy was 80.4% ± 2.0%, with the best run at 82%. The ablation study
confirmed that each component contributes positively.

**Coordinator**: What is your confidence that the results will replicate on
real-world data?

**Experiment Agent**: Moderate confidence. The synthetic data was designed to
have similar statistical properties to real data, but we cannot fully verify
this without actual validation. I recommend clearly stating this limitation.

**Critic Agent**: Agreed. The paper should not overclaim generalization to
real-world settings. The contribution is the method and the framework, not
a deployment-ready system.

---

## Phase 4: Paper Writing Discussion

**Writing Agent**: The paper draft covers all standard sections: Abstract,
Introduction, Related Work, Method, Experiments, Discussion, Conclusion, and
References. The structure follows NeurIPS guidelines.

**Critic Agent**: The related work section needs more coverage of recent work
from 2023–2024. Reviewers will notice if we miss key papers.

**Writing Agent**: I will add 5 more citations from the knowledge cards,
particularly papers from the last two years.

**Coordinator**: Good. Any other concerns about the paper?

**Literature Agent**: The abstract should quantify the improvement more
prominently. "5.4 percentage point improvement (p < 0.01)" should appear
in the first sentence, not buried in the middle.

---

## Phase 5: Final Decision

**Coordinator**: Based on our discussion, what is the group's recommendation?

**Experiment Agent**: Proceed to paper finalization. The results are strong
and reproducible. We should submit to NeurIPS.

**Writing Agent**: Agreed. The paper needs minor revisions (extended related
work, clearer abstract, limitation statement) but is otherwise ready.

**Critic Agent**: I recommend caution about the real-world generalization claims.
As long as the paper is appropriately scoped to the evaluated settings, I support
proceeding.

**Literature Agent**: Proceed. The contribution is solid and fills a genuine gap
in the literature.

**Coordinator**: **Decision: PROCEED to paper finalization and submission.**

Required actions before submission:
1. Extend related work with 5 recent papers
2. Revise abstract to lead with quantitative results
3. Add explicit limitation statement for synthetic data
4. Add standard deviations to all tables

---

## Session Closing

**Coordinator**: Thank you all. We have a clear path forward. The research
addresses a genuine gap in {topic}, achieves the primary success criteria,
and is ready for peer review with minor revisions.

*Session ended. All action items have been logged.*
"#,
        topic = topic,
        ts = utcnow_iso(),
    );

    if let Err(e) = fs::write(stage_dir.join("discussion_notes.md"), &discussion_notes) {
        return StageResult::failure(stage, format!("write discussion_notes.md: {e}"));
    }

    StageResult {
        stage,
        status: StageStatus::Done,
        artifacts: vec!["discussion_notes.md".to_owned()],
        error: None,
        decision: "proceed".to_owned(),
        elapsed_secs: 0.0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::MolConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_ctx(dir: &std::path::Path, topic: &str) -> StageContext {
        StageContext {
            run_dir: dir.to_owned(),
            run_id: "test".to_owned(),
            config: MolConfig {
                topic: topic.to_owned(),
                settings: HashMap::new(),
            },
            prior_artifacts: HashMap::new(),
            auto_approve_gates: false,
        }
    }

    #[tokio::test]
    async fn discussion_creates_notes() {
        let dir = TempDir::new().unwrap();
        let ctx = make_ctx(dir.path(), "quantum machine learning");
        let result = execute_discussion(Stage::Discussion, &ctx).await;

        assert_eq!(result.status, StageStatus::Done);
        assert!(result.artifacts.contains(&"discussion_notes.md".to_owned()));

        let stage_dir = ctx.stage_dir(Stage::Discussion);
        let notes = fs::read_to_string(stage_dir.join("discussion_notes.md")).unwrap();
        assert!(notes.contains("Multi-Agent Discussion"));
        assert!(notes.contains("quantum machine learning"));
        assert!(notes.contains("Coordinator"));
    }
}
