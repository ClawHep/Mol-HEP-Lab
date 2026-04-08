import asyncio
from pathlib import Path
from unittest.mock import AsyncMock

import pytest

from molthep.prefrontal.dispatcher import Dispatcher
from molthep.executors.base import Executor
from molthep.types import PhaseTask, SessionContext, ExecutionResult, AnalysisConfig


class MockExecutor(Executor):
    name = "mock"

    def __init__(self, result: ExecutionResult):
        self._result = result

    async def execute(self, task, context, model=None):
        return self._result

    async def health_check(self):
        return True


@pytest.fixture
def mock_result():
    return ExecutionResult(
        executor_name="mock", artifact_path=Path("STRATEGY.md"),
        artifact_content="content", code_files=[], stdout="ok",
        success=True, duration_seconds=1.0, token_usage=100,
    )


@pytest.fixture
def context(tmp_path):
    return SessionContext(
        analysis_dir=tmp_path, methodology_dir=tmp_path,
        conventions_dir=tmp_path, current_phase=1,
        config=AnalysisConfig(
            name="test", physics_prompt="Test", data_dir=Path("/data"),
            executors=["mock"], analysis_type="extraction", conventions=[],
        ),
    )


@pytest.mark.asyncio
async def test_dispatch_single_executor(mock_result, context):
    executors = [MockExecutor(mock_result)]
    dispatcher = Dispatcher(executors, context)
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    results = await dispatcher.dispatch(task)
    assert len(results) == 1
    assert results[0].success is True


@pytest.mark.asyncio
async def test_dispatch_parallel(mock_result, context):
    executors = [MockExecutor(mock_result), MockExecutor(mock_result)]
    dispatcher = Dispatcher(executors, context)
    task = PhaseTask(
        phase_number=2, phase_name="exploration", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="EXPLORATION.md", num_reviewers=1, parallel_allowed=True,
    )
    results = await dispatcher.dispatch(task)
    assert len(results) == 2


@pytest.mark.asyncio
async def test_dispatch_single_even_with_multiple_executors(mock_result, context):
    executors = [MockExecutor(mock_result), MockExecutor(mock_result)]
    dispatcher = Dispatcher(executors, context)
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("t.md"), upstream_artifacts=[], upstream_summaries=[],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    results = await dispatcher.dispatch(task)
    assert len(results) == 1
