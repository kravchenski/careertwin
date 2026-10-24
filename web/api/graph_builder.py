"""CareerTwin API — career graph construction and enrichment."""

from __future__ import annotations

import tempfile
from datetime import date
from pathlib import Path
from typing import Any

import httpx
from fastapi import UploadFile

from candidate_context import read_context, studio_snapshot
from config import SKILLS
from config import _agent_result

from careertwin.collectors.resume import ResumeParser
from careertwin.collectors.website import WebsiteCollector
from careertwin.graph.schema import (
    CareerGraph,
    Education,
    Experience,
    Profile,
    Project,
    Skill,
    SkillCategory,
)
from careertwin.rendering import PdfRenderer, RenderConfig

try:
    from careertwin.collectors.github import GitHubCollector
except ModuleNotFoundError:
    GitHubCollector = None


def _extract_username(url: str) -> str:
    cleaned = url.rstrip("/").split("/")[-1]
    return cleaned.split("?")[0].strip()


def _skills_from_text(text: str) -> list[str]:
    lowered = text.lower()
    return [skill for skill in SKILLS if skill.lower() in lowered][:15]


def _merge_graphs(target: CareerGraph, source: CareerGraph) -> CareerGraph:
    target_experience = {(e.company, e.position) for e in target.experience}
    for exp in source.experience:
        key = (exp.company, exp.position)
        if key not in target_experience:
            target.experience.append(exp)
            target_experience.add(key)

    target_projects = {project.name.lower() for project in target.projects}
    for project in source.projects:
        key = project.name.lower()
        if key not in target_projects:
            target.projects.append(project)
            target_projects.add(key)

    target_skills = {skill.name.lower() for skill in target.skills}
    for skill in source.skills:
        key = skill.name.lower()
        if key not in target_skills:
            target.skills.append(skill)
            target_skills.add(key)

    target_tech = {tech.name.lower() for tech in target.technologies}
    for tech in source.technologies:
        key = tech.name.lower()
        if key not in target_tech:
            target.technologies.append(tech)
            target_tech.add(key)

    target.profile.headline = target.profile.headline or source.profile.headline
    target.profile.summary = target.profile.summary or source.profile.summary
    target.profile.website = target.profile.website or source.profile.website
    target.profile.github = target.profile.github or source.profile.github
    target.profile.linkedin = target.profile.linkedin or source.profile.linkedin
    target.profile.location = target.profile.location or source.profile.location
    target.profile.email = target.profile.email or source.profile.email
    target.profile.phone = target.profile.phone or source.profile.phone
    return target


def _build_manual_graph(
    full_name: str,
    email: str,
    phone: str,
    location: str,
    github: str,
    linkedin: str,
    website: str,
    summary_text: str,
) -> CareerGraph:
    shared = studio_snapshot(read_context()).get("profile", {})
    if shared and (
        not full_name or full_name.casefold() == str(shared.get("fullName", "")).casefold()
    ):
        full_name = shared.get("fullName") or full_name
        email, phone, location = (
            shared.get(key) or value
            for key, value in [("email", email), ("phone", phone), ("location", location)]
        )
        github, linkedin, website = (
            shared.get(key) or value
            for key, value in [("github", github), ("linkedin", linkedin), ("website", website)]
        )
        summary_text = shared.get("summary") or summary_text
    else:
        shared = {}
    skill_names = shared.get("skills") or _skills_from_text(summary_text)
    graph = CareerGraph(
        profile=Profile(
            full_name=full_name or "Candidate",
            email=email or None,
            phone=phone or None,
            location=location or None,
            github=github or None,
            linkedin=linkedin or None,
            website=website or None,
            headline=shared.get("title") or None,
            summary=summary_text or None,
        ),
        skills=[Skill(name=str(skill), category=SkillCategory.TOOL) for skill in skill_names],
    )

    def exact_date(value):
        # Do not invent a day for an entry that only supplies a month/year.
        try:
            return date.fromisoformat(value) if isinstance(value, str) else None
        except ValueError:
            return None

    for item in shared.get("experience", []):
        graph.experience.append(
            Experience(
                company=item.get("company", ""),
                position=item.get("role", ""),
                description=item.get("description", ""),
                start_date=exact_date(item.get("start")),
                end_date=None if item.get("current") else exact_date(item.get("end")),
            )
        )
    for item in shared.get("education", []):
        graph.education.append(
            Education(
                institution=item.get("school", ""),
                degree=item.get("degree", ""),
                field=item.get("field") or None,
                start_date=exact_date(item.get("start")),
                end_date=exact_date(item.get("end")),
            )
        )
    for item in shared.get("projects", []):
        if isinstance(item, dict):
            graph.projects.append(
                Project(
                    name=item.get("name", ""),
                    description=item.get("description", ""),
                    url=item.get("url") or None,
                )
            )
    return graph


async def _fallback_collect_github(username: str) -> CareerGraph:
    profile = Profile(full_name=username, github=f"https://github.com/{username}")
    projects = []
    skills: dict[str, int] = {}

    try:
        async with httpx.AsyncClient(timeout=12.0, follow_redirects=True) as client:
            response = await client.get(
                f"https://api.github.com/users/{username}/repos?per_page=100&sort=updated",
                headers={"Accept": "application/vnd.github+json", "User-Agent": "CareerTwin/2.0"},
            )
            response.raise_for_status()
            repos = response.json()
    except Exception:
        return CareerGraph(profile=profile)

    for repo in repos:
        if repo.get("fork"):
            continue
        technologies = []
        language = repo.get("language")
        if language:
            technologies.append({"name": language, "category": "language"})
            skills[language] = skills.get(language, 0) + 1
        topics = repo.get("topics") or []
        for topic in topics[:6]:
            technologies.append({"name": topic, "category": "topic"})

        projects.append(
            {
                "name": repo.get("name") or "repository",
                "description": repo.get("description") or "",
                "url": repo.get("html_url"),
                "technologies": technologies,
            }
        )

    return CareerGraph(
        profile=profile,
        projects=projects,
        skills=[
            Skill(name=name, category=SkillCategory.LANGUAGE)
            for name, _count in sorted(skills.items(), key=lambda item: -item[1])
        ],
    )


async def build_career_graph(
    mode: str,
    resume_file: UploadFile | None,
    full_name: str,
    email: str,
    phone: str,
    location: str,
    github: str,
    linkedin: str,
    website: str,
    summary_text: str,
) -> tuple[CareerGraph, dict[str, Any]]:
    diagnostics: dict[str, Any] = {
        "source": _agent_result("idle", "No source processed"),
        "github": _agent_result("idle", "GitHub enrichment not requested"),
        "website": _agent_result("idle", "Website enrichment not requested"),
    }

    if mode == "upload" and resume_file and resume_file.filename:
        suffix = Path(resume_file.filename).suffix or ".txt"
        with tempfile.NamedTemporaryFile(delete=False, suffix=suffix) as tmp:
            tmp.write(await resume_file.read())
            tmp_path = Path(tmp.name)
        try:
            parser = ResumeParser()
            graph = parser.parse(tmp_path)
            diagnostics["source"] = _agent_result(
                "success",
                f"Parsed uploaded resume: {resume_file.filename}",
                experience=len(graph.experience),
                skills=len(graph.skills),
            )
        except Exception as exc:
            diagnostics["source"] = _agent_result("error", f"Resume parsing failed: {exc}")
            raise
        finally:
            tmp_path.unlink(missing_ok=True)
    else:
        graph = _build_manual_graph(
            full_name=full_name,
            email=email,
            phone=phone,
            location=location,
            github=github,
            linkedin=linkedin,
            website=website,
            summary_text=summary_text,
        )
        diagnostics["source"] = _agent_result(
            "success",
            "Built graph from manual profile input",
            skills=len(graph.skills),
        )

    if full_name:
        graph.profile.full_name = full_name
    graph.profile.email = email or graph.profile.email
    graph.profile.phone = phone or graph.profile.phone
    graph.profile.location = location or graph.profile.location
    graph.profile.github = github or graph.profile.github
    graph.profile.linkedin = linkedin or graph.profile.linkedin
    graph.profile.website = website or graph.profile.website
    if summary_text and not graph.profile.summary:
        graph.profile.summary = summary_text

    if github:
        username = _extract_username(github)
        if GitHubCollector is not None:
            try:
                github_graph = GitHubCollector().collect(username)
                graph = _merge_graphs(graph, github_graph)
                diagnostics["github"] = _agent_result(
                    "success",
                    "GitHub enrichment completed",
                    projects=len(github_graph.projects),
                    skills=len(github_graph.skills),
                )
            except Exception as exc:
                diagnostics["github"] = _agent_result(
                    "error",
                    f"GitHub SDK enrichment failed: {exc}",
                )
        else:
            try:
                fallback_graph = await _fallback_collect_github(username)
                graph = _merge_graphs(graph, fallback_graph)
                diagnostics["github"] = _agent_result(
                    "success",
                    "GitHub REST fallback enrichment completed",
                    projects=len(fallback_graph.projects),
                    skills=len(fallback_graph.skills),
                )
            except Exception as exc:
                diagnostics["github"] = _agent_result(
                    "error",
                    f"GitHub fallback enrichment failed: {exc}",
                )

    if website:
        try:
            website_graph = WebsiteCollector().collect(website)
            graph = _merge_graphs(graph, website_graph)
            diagnostics["website"] = _agent_result(
                "success",
                "Website enrichment completed",
                projects=len(website_graph.projects),
            )
        except Exception as exc:
            diagnostics["website"] = _agent_result(
                "error",
                f"Website enrichment failed: {exc}",
            )

    return graph, diagnostics
