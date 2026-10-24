"""Shared test fixtures."""

import os
from pathlib import Path

os.environ["CAREERTWIN_NO_DOTENV"] = "1"

import pytest

from careertwin.graph.schema import (
    Achievement,
    CareerGraph,
    Experience,
    Profile,
    Project,
    Skill,
    SkillCategory,
    Technology,
)

FIXTURES_DIR = Path(__file__).parent / "fixtures"


@pytest.fixture(autouse=True)
def private_run_log(tmp_path, monkeypatch):
    """No test writes into the real interview log (it sits next to the user's own data)."""
    from careertwin.interview import runlog

    monkeypatch.setattr(runlog, "path", lambda: tmp_path / "run-log" / "interview.log")


@pytest.fixture
def sample_profile() -> Profile:
    return Profile(
        full_name="John Doe",
        headline="Senior Software Engineer",
        email="john@example.com",
        phone="+1 (555) 123-4567",
        location="San Francisco, CA",
        website="https://johndoe.dev",
        linkedin="https://linkedin.com/in/johndoe",
        github="https://github.com/johndoe",
        summary="Senior Software Engineer with 5+ years of experience.",
    )


@pytest.fixture
def sample_experience() -> list[Experience]:
    return [
        Experience(
            company="Google",
            position="Senior Software Engineer",
            description="Led development of data pipeline.",
            start_date=None,
            end_date=None,
            technologies=[
                Technology(name="Python", category="language"),
                Technology(name="Go", category="language"),
            ],
            achievements=[
                Achievement(
                    title="Reduced latency by 40%",
                    metric="40%",
                ),
            ],
        ),
        Experience(
            company="Stripe",
            position="Software Engineer",
            description="Built payment APIs.",
            technologies=[
                Technology(name="Ruby", category="language"),
            ],
        ),
    ]


@pytest.fixture
def sample_skills() -> list[Skill]:
    return [
        Skill(name="Python", category=SkillCategory.LANGUAGE, proficiency="expert"),
        Skill(name="React", category=SkillCategory.FRAMEWORK, proficiency="advanced"),
        Skill(name="Docker", category=SkillCategory.TOOL),
    ]


@pytest.fixture
def sample_graph(
    sample_profile: Profile,
    sample_experience: list[Experience],
    sample_skills: list[Skill],
) -> CareerGraph:
    return CareerGraph(
        profile=sample_profile,
        experience=sample_experience,
        skills=sample_skills,
        projects=[
            Project(
                name="awesome-project",
                description="A cool project",
                technologies=[Technology(name="Python", category="language")],
            ),
        ],
    )


@pytest.fixture
def sample_resume_path() -> Path:
    return FIXTURES_DIR / "sample_resume.txt"


@pytest.fixture
def minimal_resume_path() -> Path:
    return FIXTURES_DIR / "minimal_resume.txt"


@pytest.fixture(autouse=True)
def isolated_candidate_context(tmp_path, monkeypatch):
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(tmp_path / "ABOUT.md"))
    monkeypatch.chdir(tmp_path)
