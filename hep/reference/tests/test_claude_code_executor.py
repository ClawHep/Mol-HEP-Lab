import asyncio
from pathlib import Path
from unittest.mock import AsyncMock, patch, MagicMock

import pytest

from molthep.executors.claude_code import ClaudeCodeExecutor
from molthep.types import PhaseTask, SessionContext, AnalysisConfig


@pytest.fixture
def executor():
    return ClaudeCodeExecutor(command="claude", args=["--dangerously-skip-permissions"])


@pytest.fixture
def phase_task(tmp_path):
    template = tmp_path / "phase1_claude.md"
    template.write_text("# Phase 1 Instructions")
    return PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=template, upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )


@pytest.fixture
def session_context(tmp_path):
    return SessionContext(
        analysis_dir=tmp_path, methodology_dir=tmp_path,
        conventions_dir=tmp_path, current_phase=1,
        config=AnalysisConfig(
            name="test", physics_prompt="Test", data_dir=Path("/data"),
            executors=["claude_code"], analysis_type="extraction", conventions=[],
        ),
    )


@pytest.mark.asyncio
async def test_health_check_success(executor):
    with patch("shutil.which", return_value="/usr/local/bin/claude"):
        result = await executor.health_check()
    assert result is True


@pytest.mark.asyncio
async def test_health_check_missing(executor):
    with patch("shutil.which", return_value=None):
        result = await executor.health_check()
    assert result is False


@pytest.mark.asyncio
async def test_execute_success(executor, phase_task, session_context, tmp_path):
    strategy = tmp_path / "STRATEGY.md"

    async def mock_create_subprocess(*args, **kwargs):
        strategy.write_text("# Strategy\nTest output")
        mock_proc = MagicMock()
        mock_proc.communicate = AsyncMock(return_value=(b"Done", b""))
        mock_proc.returncode = 0
        return mock_proc

    with patch("asyncio.create_subprocess_exec", side_effect=mock_create_subprocess):
        result = await executor.execute(phase_task, session_context)

    assert result.success is True
    assert result.executor_name == "claude_code"
    assert result.artifact_content == "# Strategy\nTest output"
