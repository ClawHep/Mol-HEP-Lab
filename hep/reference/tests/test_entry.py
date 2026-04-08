from pathlib import Path

import pytest

from molthep.entry import select_best_result
from molthep.types import ExecutionResult


def test_select_best_result_picks_successful():
    results = [
        ExecutionResult(
            executor_name="a", artifact_path=None, artifact_content="",
            code_files=[], stdout="fail", success=False, duration_seconds=1.0,
        ),
        ExecutionResult(
            executor_name="b", artifact_path=Path("STRATEGY.md"),
            artifact_content="content", code_files=[], stdout="ok",
            success=True, duration_seconds=2.0, token_usage=500,
        ),
    ]
    best = select_best_result(results)
    assert best.executor_name == "b"


def test_select_best_result_empty_raises():
    with pytest.raises(ValueError, match="No executor results"):
        select_best_result([])


def test_select_best_result_prefers_lower_tokens():
    results = [
        ExecutionResult(
            executor_name="a", artifact_path=Path("X.md"),
            artifact_content="c", code_files=[], stdout="",
            success=True, duration_seconds=1.0, token_usage=1000,
        ),
        ExecutionResult(
            executor_name="b", artifact_path=Path("X.md"),
            artifact_content="c", code_files=[], stdout="",
            success=True, duration_seconds=1.0, token_usage=500,
        ),
    ]
    best = select_best_result(results)
    assert best.executor_name == "b"
