from datetime import datetime

from molthep.genome.leaderboard import Leaderboard
from molthep.types import ExperimentResult


def test_update_and_rank():
    lb = Leaderboard()
    lb.update(ExperimentResult(
        analysis_id="a", variant="v1", branch="b1",
        metric_name="significance", metric_value=3.5,
        timestamp=datetime.now(), executor_used="claude_code",
    ))
    lb.update(ExperimentResult(
        analysis_id="a", variant="v2", branch="b2",
        metric_name="significance", metric_value=4.2,
        timestamp=datetime.now(), executor_used="codex",
    ))

    rankings = lb.get_rankings()
    assert len(rankings) == 2
    assert rankings[0].metric_value == 4.2


def test_render_markdown():
    lb = Leaderboard()
    lb.update(ExperimentResult(
        analysis_id="test", variant="v1", branch="b1",
        metric_name="significance", metric_value=3.5,
        timestamp=datetime.now(), executor_used="claude_code",
    ))

    md = lb.render_markdown()
    assert "# Leaderboard" in md
    assert "3.5" in md
    assert "claude_code" in md
