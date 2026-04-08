import json
import tempfile
from pathlib import Path

from molthep.types import AnalysisConfig, PhaseTask, ReviewVerdict, ReviewItem, ModelConfig


def test_analysis_config_from_json():
    data = {
        "name": "ttbar_xsec_13tev",
        "physics_prompt": "Measure ttbar cross-section",
        "data_dir": "/data/ttbar",
        "executors": ["claude_code"],
        "analysis_type": "extraction",
        "conventions": ["extraction"],
    }
    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        json.dump(data, f)
        path = f.name

    config = AnalysisConfig.from_json(path)
    assert config.name == "ttbar_xsec_13tev"
    assert config.physics_prompt == "Measure ttbar cross-section"
    assert config.data_dir == Path("/data/ttbar")
    assert config.executors == ["claude_code"]
    assert config.analysis_type == "extraction"


def test_analysis_config_to_json(tmp_path):
    config = AnalysisConfig(
        name="test", physics_prompt="prompt", data_dir=Path("/data"),
        executors=["claude_code"], analysis_type="search", conventions=["search"],
    )
    out = tmp_path / "config.json"
    config.to_json(out)
    loaded = AnalysisConfig.from_json(str(out))
    assert loaded.name == config.name


def test_model_config_get_model_for_role():
    mc = ModelConfig(
        planner="anthropic/claude-opus-4",
        executor="anthropic/claude-sonnet-4",
        reviewer="anthropic/claude-opus-4",
        phase_overrides={"2": "anthropic/claude-sonnet-4"},
    )
    assert mc.get_model_for("planner") == "anthropic/claude-opus-4"
    assert mc.get_model_for("executor") == "anthropic/claude-sonnet-4"
    assert mc.get_model_for("executor", phase="2") == "anthropic/claude-sonnet-4"
    assert mc.get_model_for("planner", phase="1") == "anthropic/claude-opus-4"


def test_model_config_phase_override_takes_precedence():
    mc = ModelConfig(planner="a", executor="b", reviewer="c", phase_overrides={"1": "override"})
    assert mc.get_model_for("planner", phase="1") == "override"
    assert mc.get_model_for("planner", phase="3") == "a"


def test_analysis_config_with_models(tmp_path):
    import json as json_mod
    data = {
        "name": "test", "physics_prompt": "Test", "data_dir": "/data",
        "executors": ["claude_code"], "analysis_type": "extraction",
        "conventions": ["extraction"],
        "models": {"planner": "opus", "executor": "sonnet", "reviewer": "opus", "phase_overrides": {}},
    }
    p = tmp_path / "c.json"
    p.write_text(json_mod.dumps(data))
    from molthep.types import AnalysisConfig
    config = AnalysisConfig.from_json(str(p))
    assert config.models is not None
    assert config.models.planner == "opus"


def test_analysis_config_without_models(tmp_path):
    import json as json_mod
    data = {"name": "t", "physics_prompt": "T", "data_dir": "/d", "executors": ["c"], "analysis_type": "e", "conventions": []}
    p = tmp_path / "c.json"
    p.write_text(json_mod.dumps(data))
    from molthep.types import AnalysisConfig
    config = AnalysisConfig.from_json(str(p))
    assert config.models is None


def test_phase_task_with_review_feedback():
    task = PhaseTask(
        phase_number=1, phase_name="strategy", sub_phase=None,
        template_path=Path("templates/phase1_claude.md"),
        upstream_artifacts=[], upstream_summaries=["Initial context"],
        expected_artifact="STRATEGY.md", num_reviewers=4, parallel_allowed=False,
    )
    verdict = ReviewVerdict(
        passed=False, reviewer_name="reviewer_1",
        items=[ReviewItem(classification="A", description="Missing systematic uncertainties")],
        iteration=1,
    )
    fixed_task = task.with_review_feedback(verdict)
    assert len(fixed_task.upstream_summaries) == 2
    assert "Missing systematic uncertainties" in fixed_task.upstream_summaries[1]
    assert fixed_task.phase_number == task.phase_number
