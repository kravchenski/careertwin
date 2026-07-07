"""GitHub data collector — extracts repos, languages, commits, stars."""

from __future__ import annotations

import logging
import os
import time
from typing import Optional

from github import Github, GithubException, RateLimitExceededException, Repository

from careertwin.graph.schema import (
    Achievement,
    CareerGraph,
    Evidence,
    Profile,
    Project,
    Skill,
    SkillCategory,
    SourceType,
    Technology,
)

logger = logging.getLogger(__name__)


class GitHubCollector:
    """Collects professional data from GitHub.

    Uses public API by default (60 req/hour).
    Set GITHUB_TOKEN env var or pass token for 5000 req/hour.
    """

    def __init__(self, token: Optional[str] = None):
        self.token = token or os.getenv("GITHUB_TOKEN")
        self.client = Github(
            self.token,
            retry=None,  # disable auto-retry
            seconds_between_requests=0.5,
        ) if self.token else Github(
            retry=None,
            seconds_between_requests=1,
        )
        self._request_count = 0

    def collect(self, username: str) -> CareerGraph:
        self._request_count = 0
        user = self._safe_call(self.client.get_user, username)
        if not user:
            return self._empty_graph(username)

        profile = self._build_profile(user)
        projects = self._collect_repos(user)
        skills = self._extract_skills(projects)

        return CareerGraph(
            profile=profile,
            projects=projects,
            skills=skills,
        )

    def _safe_call(self, func, *args, **kwargs):
        """Execute GitHub API call with rate limit handling."""
        try:
            result = func(*args, **kwargs)
            self._request_count += 1
            return result
        except RateLimitExceededException:
            logger.warning("Rate limited. Set GITHUB_TOKEN for higher limits.")
            return None
        except GithubException as e:
            logger.warning(f"GitHub API error: {e}")
            return None
        except Exception as e:
            logger.warning(f"Unexpected error: {e}")
            return None

    def _empty_graph(self, username: str) -> CareerGraph:
        return CareerGraph(
            profile=Profile(full_name=username, github=f"https://github.com/{username}"),
        )

    def _build_profile(self, user) -> dict:
        return {
            "full_name": user.name or user.login,
            "headline": user.bio or "",
            "email": user.email or "",
            "location": user.location or "",
            "website": user.blog or "",
            "github": user.html_url,
            "summary": user.bio or "",
        }

    def _collect_repos(self, user) -> list[Project]:
        repos = self._safe_call(user.get_repos, type="public")
        if not repos:
            return []

        sorted_repos = sorted(repos, key=lambda r: r.stargazers_count, reverse=True)

        projects = []
        for repo in sorted_repos[:30]:  # Limit to top 30 repos
            if repo.fork:
                continue
            project = self._repo_to_project(repo)
            if project:
                projects.append(project)

        return projects

    def _repo_to_project(self, repo: Repository) -> Optional[Project]:
        evidence = Evidence(
            source=SourceType.GITHUB,
            url=repo.html_url,
            description=f"Repository: {repo.full_name}",
            raw_data={
                "stars": repo.stargazers_count,
                "forks": repo.forks_count,
                "watchers": repo.watchers_count,
                "open_issues": repo.open_issues_count,
                "created_at": repo.created_at.isoformat() if repo.created_at else None,
                "updated_at": repo.updated_at.isoformat() if repo.updated_at else None,
            },
        )

        achievements = []
        if repo.stargazers_count > 100:
            achievements.append(Achievement(
                title=f"{repo.stargazers_count} stars",
                metric=f"{repo.stargazers_count} stars",
                evidence=[evidence],
            ))
        if repo.forks_count > 50:
            achievements.append(Achievement(
                title=f"{repo.forks_count} forks",
                metric=f"{repo.forks_count} forks",
                evidence=[evidence],
            ))

        languages = []
        lang_data = self._safe_call(repo.get_languages)
        if lang_data:
            for lang in lang_data:
                languages.append(Technology(
                    name=lang,
                    category="language",
                    evidence=[evidence],
                ))

        topics = []
        topics_data = self._safe_call(repo.get_topics)
        if topics_data:
            topics = [Technology(name=t, category="topic") for t in topics_data]

        return Project(
            name=repo.name,
            description=repo.description or "",
            url=repo.html_url,
            technologies=languages + topics,
            achievements=achievements,
            start_date=repo.created_at.date() if repo.created_at else None,
            end_date=repo.updated_at.date() if repo.updated_at else None,
            evidence=[evidence],
        )

    def _extract_skills(self, projects: list[Project]) -> list[Skill]:
        lang_count: dict[str, int] = {}
        for proj in projects:
            for tech in proj.technologies:
                if tech.category == "language":
                    lang_count[tech.name] = lang_count.get(tech.name, 0) + 1

        skills = []
        for lang, count in sorted(lang_count.items(), key=lambda x: -x[1]):
            proficiency = "expert" if count >= 5 else "advanced" if count >= 3 else "intermediate"
            skills.append(Skill(
                name=lang,
                category=SkillCategory.LANGUAGE,
                proficiency=proficiency,
            ))
        return skills

    @property
    def requests_remaining(self) -> Optional[int]:
        try:
            return self.client.rate_limit.remaining
        except Exception:
            return None
