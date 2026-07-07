"""Tests for ATS compatibility checker."""

from careertwin.ats.checker import ATSChecker, ATSReport
from careertwin.graph.schema import CareerGraph, Experience, Profile, Skill, SkillCategory


class TestATSChecker:
    def test_check_with_sample_graph(self, sample_graph: CareerGraph):
        checker = ATSChecker()
        text = sample_graph.to_search_text()
        report = checker.check(sample_graph, text)

        assert isinstance(report, ATSReport)
        assert 0 <= report.score <= 100

    def test_score_with_all_sections(self, sample_graph: CareerGraph):
        checker = ATSChecker()
        text = sample_graph.to_search_text()
        report = checker.check(sample_graph, text)

        # Should find experience, skills sections
        passed_names = [p for p in report.passed_checks]
        assert any("experience" in p.lower() for p in passed_names)
        assert any("skills" in p.lower() for p in passed_names)

    def test_missing_sections(self):
        graph = CareerGraph(profile=Profile(full_name="Empty"))
        checker = ATSChecker()
        text = ""
        report = checker.check(graph, text)

        critical = [i for i in report.issues if i.severity == "critical"]
        assert len(critical) >= 3  # missing experience, education, skills

    def test_action_verbs(self, sample_graph: CareerGraph):
        checker = ATSChecker()
        text = sample_graph.to_search_text()
        report = checker.check(sample_graph, text)

        # Check either passed or failed - depends on sample data
        all_messages = report.passed_checks + [i.message for i in report.issues]
        has_verb_check = any("action verb" in m.lower() for m in all_messages)
        assert has_verb_check

    def test_report_summary(self, sample_graph: CareerGraph):
        checker = ATSChecker()
        text = sample_graph.to_search_text()
        report = checker.check(sample_graph, text)

        summary = report.summary()
        assert "ATS Score:" in summary
        assert "/100" in summary

    def test_report_is_passing(self, sample_graph: CareerGraph):
        checker = ATSChecker()
        text = sample_graph.to_search_text()
        report = checker.check(sample_graph, text)

        # score >= 70 is passing
        assert isinstance(report.is_passing, bool)
