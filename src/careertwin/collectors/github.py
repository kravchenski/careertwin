"""GitHub data collector — extracts repos, languages, commits, stars."""

from __future__ import annotations

import os
from datetime import date
from typing import Optional

from github import Github, Repository, AuthenticatedUser

from careertwin.graph.schema import (
    Achievement,
    CareerGraph,
    Evidence,
    Project,
    Skill,
    SkillCategory,
    SourceType,
    Technology,
)


class GitHubCollector:
    """Collects professional data from GitHub."""

    def __init__(self, token: Optional[str] = None):
        self.token = token or os.getenv("GITHUB_TOKEN")
        if not self.token:
            raise ValueError("GITHUB_TOKEN required")
        self.client = Github(self.token)

    def collect(self, username: Optional[str] = None) -> CareerGraph:
        user = self.client.get_user(username) if username else self.client.get_user()
        profile = self._build_profile(user)
        projects = self._collect_repos(user)
        skills = self._extract_skills(projects)

        return CareerGraph(
            profile=profile,
            projects=projects,
            skills=skills,
            evidence=[],
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
        projects = []
        repos = sorted(user.get_repos(), key=lambda r: r.stargazers_count, reverse=True)

        for repo in repos:
            if repo.fork:
                continue
            project = self._repo_to_project(repo)
            projects.append(project)

        return projects

    def _repo_to_project(self, repo: Repository) -> Project:
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
        try:
            lang_data = repo.get_languages()
            for lang, bytes_count in lang_data.items():
                languages.append(Technology(
                    name=lang,
                    category="language",
                    evidence=[evidence],
                ))
        except Exception:
            pass

        topics = []
        try:
            topics = [Technology(name=t, category="topic") for t in repo.get_topics()]
        except Exception:
            pass

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
