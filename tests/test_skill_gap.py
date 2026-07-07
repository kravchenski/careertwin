"""Tests for skill gap analyzer."""

from careertwin.agents.skill_gap import SkillGapAnalyzer, SkillGapReport
from careertwin.graph.schema import CareerGraph, Profile


class TestSkillGapAnalyzer:
    def test_analyze_returns_report(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Python Django PostgreSQL developer",
            target_role="Backend Developer",
        )

        assert isinstance(report, SkillGapReport)
        assert 0 <= report.overall_readiness <= 100

    def test_analyze_finds_gaps(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Rust Kubernetes TensorFlow developer",
        )

        assert len(report.gaps) > 0

    def test_analyze_no_gaps(self):
        graph = CareerGraph(
            profile=Profile(full_name="Test"),
            skills=[],
        )
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=graph,
            job_description="Unknown skill xyz",
        )

        assert report.gaps == []
        assert len(report.required_skills) == 0

    def test_analyze_includes_resources(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Docker Kubernetes developer",
        )

        for gap in report.gaps:
            assert gap.recommended_resources

    def test_analyze_readiness_scale(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Python React Docker",
        )

        assert 0 <= report.overall_readiness <= 100

    def test_analyze_target_role(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Python developer",
            target_role="Senior Python Engineer",
        )

        assert report.target_role == "Senior Python Engineer"

    def test_analyze_role_from_description(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Django Backend Developer\nBuild scalable APIs",
        )

        assert report.target_role

    def test_analyze_recommendations(self, sample_graph: CareerGraph):
        analyzer = SkillGapAnalyzer()
        report = analyzer.analyze(
            graph=sample_graph,
            job_description="Rust Kubernetes Docker",
        )

        assert len(report.recommendations) > 0
