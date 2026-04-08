import json
from pathlib import Path
from molthep.hippocampus.session import Session


def test_store_and_load_artifact(analysis_dir, sample_phase_task, sample_execution_result):
    session = Session(analysis_dir)
    session.store_artifact(sample_phase_task, sample_execution_result)
    loaded = session.load_artifact(1)
    assert loaded is not None
    assert "Strategy" in loaded


def test_load_missing_artifact(analysis_dir):
    session = Session(analysis_dir)
    assert session.load_artifact(1) is None


def test_write_status(analysis_dir):
    session = Session(analysis_dir)
    session.write_status(1, "in_progress")
    status_file = analysis_dir / ".status.json"
    assert status_file.exists()
    status = json.loads(status_file.read_text())
    assert status["phases"]["1"]["status"] == "in_progress"
    assert "updated_at" in status


def test_write_status_preserves_previous_phases(analysis_dir):
    session = Session(analysis_dir)
    session.write_status(1, "completed")
    session.write_status(2, "in_progress")
    status = json.loads((analysis_dir / ".status.json").read_text())
    assert status["phases"]["1"]["status"] == "completed"
    assert status["phases"]["2"]["status"] == "in_progress"


def test_get_phase_summary_returns_none_when_no_artifact(analysis_dir):
    session = Session(analysis_dir)
    assert session.get_phase_summary(1) is None


def test_write_status_atomic(analysis_dir):
    session = Session(analysis_dir)
    session.write_status(1, "in_progress")
    assert not (analysis_dir / ".status.json.tmp").exists()
    assert (analysis_dir / ".status.json").exists()


def test_store_artifact_records_metadata(analysis_dir, sample_phase_task, sample_execution_result):
    session = Session(analysis_dir)
    session.write_status(1, "in_progress")
    session.store_artifact(sample_phase_task, sample_execution_result)
    status = json.loads((analysis_dir / ".status.json").read_text())
    assert status["phases"]["1"]["artifact"] == "STRATEGY.md"
    assert status["phases"]["1"]["executor"] == "claude_code"
    assert "duration_s" in status["phases"]["1"]


def test_write_status_includes_started_at(analysis_dir):
    session = Session(analysis_dir)
    session.write_status(1, "in_progress")
    status = json.loads((analysis_dir / ".status.json").read_text())
    assert "started_at" in status


def test_status_overall_cancelled(analysis_dir):
    session = Session(analysis_dir)
    session.write_status(0, "cancelled")
    status = json.loads((analysis_dir / ".status.json").read_text())
    assert status["overall_status"] == "cancelled"
