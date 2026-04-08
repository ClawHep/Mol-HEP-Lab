import pytest
from pathlib import Path
from molthep.types import AnalysisConfig, PhaseTask, ExecutionResult


@pytest.fixture
def analysis_dir(tmp_path):
    d = tmp_path / "test_analysis"
    d.mkdir()
    return d


@pytest.fixture
def sample_config():
    return AnalysisConfig(
        name="test_analysis", physics_prompt="Test prompt",
        data_dir=Path("/data/test"), executors=["claude_code"],
        analysis_type="extraction", conventions=["extraction"],
    )


@pytest.fixture
def sample_phase_task():
    return PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("templates/phase1_claude.md"),
        upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )


@pytest.fixture
def sample_execution_result(analysis_dir):
    artifact = analysis_dir / "STRATEGY.md"
    artifact.write_text("# Strategy\nTest strategy content")
    return ExecutionResult(
        executor_name="claude_code", artifact_path=artifact,
        artifact_content="# Strategy\nTest strategy content",
        code_files=[], stdout="Completed successfully",
        success=True, duration_seconds=120.5, token_usage=5000,
    )
