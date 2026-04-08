from pathlib import Path

import pytest

from molthep.prefrontal.phase_gate import PhaseGate
from molthep.hippocampus.session import Session
from molthep.types import PhaseTask, ExecutionResult


@pytest.fixture
def session(analysis_dir):
    return Session(analysis_dir)


@pytest.fixture
def gate(session):
    return PhaseGate(session)


def test_check_artifact_exists(gate, analysis_dir):
    (analysis_dir / "STRATEGY.md").write_text("# Strategy")
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    assert gate.check_artifact(task) is True


def test_check_artifact_missing(gate):
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    assert gate.check_artifact(task) is False


@pytest.mark.asyncio
async def test_run_review_passes_when_artifact_exists(gate, analysis_dir):
    content = "# Strategy\n\n" + "This is a detailed strategy document with sufficient content. " * 5
    (analysis_dir / "STRATEGY.md").write_text(content)
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    result = ExecutionResult(
        executor_name="mock", artifact_path=analysis_dir / "STRATEGY.md",
        artifact_content=content, code_files=[], stdout="",
        success=True, duration_seconds=1.0,
    )
    verdict = await gate.run_review(task, result)
    assert verdict.passed is True


@pytest.mark.asyncio
async def test_run_review_fails_when_content_too_short(gate, analysis_dir):
    (analysis_dir / "STRATEGY.md").write_text("# Short")
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    result = ExecutionResult(
        executor_name="mock", artifact_path=analysis_dir / "STRATEGY.md",
        artifact_content="# Short", code_files=[], stdout="",
        success=True, duration_seconds=1.0,
    )
    verdict = await gate.run_review(task, result)
    assert verdict.passed is False
    assert any(item.classification == "B" for item in verdict.items)
