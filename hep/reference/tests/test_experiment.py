import subprocess

import pytest

from molthep.genome.experiment import ExperimentTracker


@pytest.fixture
def git_repo(tmp_path):
    subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=tmp_path, capture_output=True, check=True)
    (tmp_path / "README.md").write_text("# Test")
    subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "commit", "-m", "init"], cwd=tmp_path, capture_output=True, check=True)
    return tmp_path


def test_fork_branch(git_repo):
    tracker = ExperimentTracker(repo_dir=git_repo)
    branch = tracker.fork_branch("ttbar_analysis", "variant_a")
    assert branch == "molthep/ttbar_analysis/variant_a"

    result = subprocess.run(
        ["git", "branch", "--list", branch], cwd=git_repo, capture_output=True, text=True
    )
    assert branch in result.stdout


def test_record_result(git_repo):
    tracker = ExperimentTracker(repo_dir=git_repo)
    tracker.fork_branch("test", "main")
    tracker.record_result("significance", 3.5)

    results = tracker.get_results()
    assert len(results) == 1
    assert results[0].metric_value == 3.5
