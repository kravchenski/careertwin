"""Tests for career trajectory recommender."""

from careertwin.agents.career_recommender import CareerRecommender, CareerTrajectory
from careertwin.graph.schema import CareerGraph, Profile


class TestCareerRecommender:
    def test_recommend_returns_trajectory(self, sample_graph: CareerGraph):
        recommender = CareerRecommender()
        trajectory = recommender.recommend(sample_graph)

        assert isinstance(trajectory, CareerTrajectory)
        assert trajectory.current_level

    def test_recommend_has_paths(self, sample_graph: CareerGraph):
        recommender = CareerRecommender()
        trajectory = recommender.recommend(sample_graph)

        assert len(trajectory.suggested_paths) > 0

    def test_recommend_has_summary(self, sample_graph: CareerGraph):
        recommender = CareerRecommender()
        trajectory = recommender.recommend(sample_graph)

        assert trajectory.summary
        assert len(trajectory.summary) > 20

    def test_recommend_senior_level(self):
        graph = CareerGraph(
            profile=Profile(full_name="Senior Dev"),
            experience=[],
        )
        recommender = CareerRecommender()
        trajectory = recommender.recommend(graph)

        assert trajectory.current_level in ("junior", "mid-level", "senior")

    def test_recommend_empty_experience(self):
        graph = CareerGraph(profile=Profile(full_name="New Dev"))
        recommender = CareerRecommender()
        trajectory = recommender.recommend(graph)

        assert trajectory.current_level
        assert len(trajectory.suggested_paths) >= 0

    def test_recommend_paths_have_steps(self, sample_graph: CareerGraph):
        recommender = CareerRecommender()
        trajectory = recommender.recommend(sample_graph)

        for path in trajectory.suggested_paths:
            if path:
                step = path[0]
                assert step.role
                assert step.timeline
                assert step.skills_needed
