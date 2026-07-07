"""Tests for GitHub collector."""

import os
from unittest.mock import MagicMock, patch

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
        assert graph.profile.full_name == "The Octocat"
        assert len(graph.projects) > 0


class TestGitHubCollectorPublic:
    def test_collect_without_token(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        assert isinstance(graph, CareerGraph)
        assert graph.profile.full_name == "The Octocat"

    def test_profile_fields(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        assert graph.profile.github == "https://github.com/octocat"

    def test_projects_extracted(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        assert len(graph.projects) > 0
        for proj in graph.projects:
            assert proj.name
            assert proj.url

    def test_skills_extracted(self):
        collector = GitHubCollector()
        graph = collector.collect("octocat")

        # Should extract languages from repos
        assert len(graph.skills) > 0
        for skill in graph.skills:
            assert skill.category.value == "language"
