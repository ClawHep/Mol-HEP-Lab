from pathlib import Path

from molthep.prefrontal.planner import Planner
from molthep.types import AnalysisConfig


def test_create_plan_generates_all_phases(tmp_path):
    method_dir = tmp_path / "methodology"
    method_dir.mkdir()
    (method_dir / "03-phases.md").write_text("# Phases")
    conv_dir = tmp_path / "conventions"
    conv_dir.mkdir()
    tmpl_dir = tmp_path / "templates"
    tmpl_dir.mkdir()
    for i in range(1, 6):
        (tmpl_dir / f"phase{i}_claude.md").write_text(f"# Phase {i}")

    config = AnalysisConfig(
        name="test", physics_prompt="Measure cross-section",
        data_dir=Path("/data"), executors=["claude_code"],
        analysis_type="extraction", conventions=["extraction"],
    )

    planner = Planner(method_dir, conv_dir, tmpl_dir)
    plan = planner.create_plan(config)

    assert len(plan.phases) == 7
    assert plan.phases[0].phase_name == "strategy"
    assert plan.phases[0].phase_number == 1
    assert plan.phases[3].sub_phase == "4a"
    assert plan.phases[6].phase_name == "documentation"
    assert plan.analysis_type == "extraction"


def test_create_plan_sets_reviewer_counts(tmp_path):
    method_dir = tmp_path / "methodology"
    method_dir.mkdir()
    conv_dir = tmp_path / "conventions"
    conv_dir.mkdir()
    tmpl_dir = tmp_path / "templates"
    tmpl_dir.mkdir()
    for i in range(1, 6):
        (tmpl_dir / f"phase{i}_claude.md").write_text(f"# Phase {i}")

    config = AnalysisConfig(
        name="test", physics_prompt="Test", data_dir=Path("/data"),
        executors=["claude_code"], analysis_type="search", conventions=["search"],
    )

    planner = Planner(method_dir, conv_dir, tmpl_dir)
    plan = planner.create_plan(config)

    assert plan.phases[0].num_reviewers == 4  # Phase 1
    assert plan.phases[1].num_reviewers == 1  # Phase 2
    assert plan.phases[2].num_reviewers == 1  # Phase 3


def test_create_plan_sets_parallel_flags(tmp_path):
    method_dir = tmp_path / "methodology"
    method_dir.mkdir()
    conv_dir = tmp_path / "conventions"
    conv_dir.mkdir()
    tmpl_dir = tmp_path / "templates"
    tmpl_dir.mkdir()
    for i in range(1, 6):
        (tmpl_dir / f"phase{i}_claude.md").write_text(f"# Phase {i}")

    config = AnalysisConfig(
        name="test", physics_prompt="Test", data_dir=Path("/data"),
        executors=["claude_code", "codex"], analysis_type="extraction", conventions=[],
    )

    planner = Planner(method_dir, conv_dir, tmpl_dir)
    plan = planner.create_plan(config)

    assert plan.phases[0].parallel_allowed is False   # Phase 1
    assert plan.phases[1].parallel_allowed is True    # Phase 2
    assert plan.phases[2].parallel_allowed is False   # Phase 3
