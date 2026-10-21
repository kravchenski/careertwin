"""Tests for Repair Agent."""

from careertwin.graph.schema import CareerGraph, Profile
from careertwin.rendering import RenderConfig
from careertwin.rendering.repair import RepairAction, RepairAgent, RepairResult
from careertwin.vision import QAIssue


class TestRepairAgent:
    def test_repair_returns_result(self, sample_graph: CareerGraph):
        agent = RepairAgent()
        result = agent.repair(sample_graph)

        assert isinstance(result, RepairResult)
        assert result.iterations >= 1
        assert 0 <= result.final_score <= 100
        assert result.template_used

    def test_repair_improves_score(self, sample_graph: CareerGraph):
        agent = RepairAgent()
        result = agent.repair(sample_graph)

        assert result.final_score >= result.original_score

    def test_repair_empty_graph(self):
        graph = CareerGraph(profile=Profile(full_name="Test"))
        agent = RepairAgent()
        result = agent.repair(graph)

        assert isinstance(result, RepairResult)
        assert result.iterations >= 1

    def test_repair_result_improved_property(self):
        result = RepairResult(
            original_score=50,
            final_score=80,
            iterations=2,
            template_used="modern",
        )
        assert result.improved is True

    def test_repair_result_not_improved(self):
        result = RepairResult(
            original_score=80,
            final_score=70,
            iterations=1,
            template_used="modern",
        )
        assert result.improved is False

    def test_try_template_switch_layout_issue(self):
        agent = RepairAgent()
        config = RenderConfig(template="modern")
        report = type("Report", (), {
            "issues": [QAIssue(category="layout", severity="warning", message="test")]
        })()
        action = agent._try_template_switch(config, report)

        assert action is not None
        assert action.success is True
        assert config.template != "modern"

    def test_try_template_switch_ats_issue(self):
        agent = RepairAgent()
        config = RenderConfig(template="modern")
        report = type("Report", (), {
            "issues": [QAIssue(category="ats", severity="warning", message="test")]
        })()
        action = agent._try_template_switch(config, report)

        assert action is not None
        assert action.success is True

    def test_try_template_switch_no_issues(self):
        agent = RepairAgent()
        config = RenderConfig(template="modern")
        report = type("Report", (), {
            "issues": [QAIssue(category="content", severity="info", message="test")]
        })()
        action = agent._try_template_switch(config, report)

        assert action is None

    def test_fix_missing_content_summary(self, sample_graph: CareerGraph):
        sample_graph.profile.summary = None
        agent = RepairAgent()
        issue = QAIssue(category="content", severity="warning", message="Missing summary")
        action = agent._fix_missing_content(sample_graph, issue)

        assert action.success is True
        assert sample_graph.profile.summary is not None

    def test_repair_action_dataclass(self):
        action = RepairAction(
            issue=QAIssue(category="layout", severity="warning", message="test"),
            fix_applied="Fixed layout",
            success=True,
        )
        assert action.fix_applied == "Fixed layout"
        assert action.success is True
