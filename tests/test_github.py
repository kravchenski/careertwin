"""Tests for GitHub collector."""

import os

import pytest

from careertwin.collectors.github import GitHubCollector
from careertwin.graph.schema import CareerGraph


@pytest.mark.skipif(
    not os.getenv("GITHUB_TOKEN"),
    reason="GITHUB_TOKEN not set"
)
class TestGitHubCollectorWithToken:
    def test_collect_own_profile(self):
        from github import Github
        g = Github(os.getenv("GITHUB_TOKEN"))
        user = g.get_user()
        collector = GitHubCollector()
        graph = collector.collect(user.login)

        assert isinstance(graph, CareerGraph)
        assert graph.profile.full_name
        assert len(graph.projects) > 0

    def test_collect_public_profile(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        assert isinstance(graph, CareerGraph)
        assert graph.profile.full_name
        assert len(graph.projects) > 0


@pytest.mark.integration
class TestGitHubCollectorPublic:
    @pytest.mark.skipif(
        os.getenv("CI") == "true" and not os.getenv("GITHUB_TOKEN"),
        reason="Skip live API tests in CI without token"
    )
    def test_collect_without_token(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        assert isinstance(graph, CareerGraph)
        assert graph.profile.github == "https://github.com/octocat"

    @pytest.mark.skipif(
        os.getenv("CI") == "true" and not os.getenv("GITHUB_TOKEN"),
        reason="Skip live API tests in CI without token"
    )
    def test_profile_fields(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        assert graph.profile.github == "https://github.com/octocat"

    @pytest.mark.skipif(
        os.getenv("CI") == "true" and not os.getenv("GITHUB_TOKEN"),
        reason="Skip live API tests in CI without token"
    )
    def test_projects_extracted(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        if len(graph.projects) > 0:
            for proj in graph.projects:
                assert proj.name
                assert proj.url

    @pytest.mark.skipif(
        os.getenv("CI") == "true" and not os.getenv("GITHUB_TOKEN"),
        reason="Skip live API tests in CI without token"
    )
    def test_skills_extracted(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        if len(graph.skills) > 0:
            for skill in graph.skills:
                assert skill.category.value == "language"
