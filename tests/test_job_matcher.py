"""Tests for job matcher agent."""

from careertwin.agents.job_matcher import JobMatch, JobMatcher
from careertwin.graph.schema import CareerGraph, Profile


class TestJobMatcher:
    def test_match_returns_job_match(self, sample_graph: CareerGraph):
        matcher = JobMatcher()
        result = matcher.match(sample_graph, "Python Django PostgreSQL developer")

        assert isinstance(result, JobMatch)
        assert 0 <= result.match_score <= 100

    def test_match_finds_python(self, sample_graph: CareerGraph):
        matcher = JobMatcher()
        result = matcher.match(sample_graph, "Python backend developer")

        assert "python" in result.matching_skills

    def test_match_finds_missing(self, sample_graph: CareerGraph):
        matcher = JobMatcher()
        result = matcher.match(sample_graph, "Rust Kubernetes AWS developer")

        assert len(result.missing_skills) > 0

    def test_match_score_scales(self, sample_graph: CareerGraph):
        matcher = JobMatcher()
        good = matcher.match(sample_graph, "Python React Docker developer")
        bad = matcher.match(sample_graph, "Rust Kubernetes TensorFlow C++ developer")

        assert good.match_score >= bad.match_score

    def test_match_recommends(self, sample_graph: CareerGraph):
        matcher = JobMatcher()
        result = matcher.match(sample_graph, "Python developer")

        assert result.recommendation

    def test_match_empty_graph(self):
        graph = CareerGraph(profile=Profile(full_name="Empty"))
        matcher = JobMatcher()
        result = matcher.match(graph, "Python developer")

        assert result.match_score >= 0
        assert result.recommendation

    def test_match_empty_description(self, sample_graph: CareerGraph):
        matcher = JobMatcher()
        result = matcher.match(sample_graph, "")

        assert result.match_score >= 0
