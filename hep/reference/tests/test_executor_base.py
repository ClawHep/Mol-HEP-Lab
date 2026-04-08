from pathlib import Path

from molthep.executors.base import build_prompt
from molthep.types import PhaseTask, SessionContext, AnalysisConfig


def test_build_prompt_includes_physics_prompt(tmp_path):
    template = tmp_path / "phase1_claude.md"
    template.write_text("# Phase 1 Instructions\nExecute strategy phase.")

    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=template, upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    context = SessionContext(
        analysis_dir=tmp_path, methodology_dir=tmp_path, conventions_dir=tmp_path,
        current_phase=1,
        config=AnalysisConfig(
            name="test", physics_prompt="Measure ttbar cross-section at 13 TeV",
            data_dir=Path("/data"), executors=["claude_code"],
            analysis_type="extraction", conventions=["extraction"],
        ),
    )

    prompt = build_prompt(task, context)
    assert "Measure ttbar cross-section at 13 TeV" in prompt
    assert "Phase 1 Instructions" in prompt
    assert "strategy" in prompt.lower()


def test_build_prompt_includes_upstream_summaries(tmp_path):
    template = tmp_path / "phase2_claude.md"
    template.write_text("# Phase 2")

    task = PhaseTask(
        phase_number=2, phase_name="exploration", sub_phase=None,
        template_path=template, upstream_artifacts=[],
        upstream_summaries=["Phase 1 produced STRATEGY.md with key decisions."],
        expected_artifact="EXPLORATION.md", num_reviewers=1, parallel_allowed=True,
    )
    context = SessionContext(
        analysis_dir=tmp_path, methodology_dir=tmp_path, conventions_dir=tmp_path,
        current_phase=2,
        config=AnalysisConfig(
            name="test", physics_prompt="Test", data_dir=Path("/data"),
            executors=["claude_code"], analysis_type="extraction", conventions=[],
        ),
    )

    prompt = build_prompt(task, context)
    assert "Phase 1 produced STRATEGY.md" in prompt
