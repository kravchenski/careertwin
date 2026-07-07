"""Career Knowledge Graph — core data models."""

from __future__ import annotations

from datetime import date
from enum import Enum
from typing import Optional
from uuid import uuid4

from pydantic import BaseModel, Field


def _id() -> str:
    return uuid4().hex[:12]


class SourceType(str, Enum):
    GITHUB = "github"
    LINKEDIN = "linkedin"
    WEBSITE = "website"
    RESUME = "resume"
    MANUAL = "manual"


class Evidence(BaseModel):
    id: str = Field(default_factory=_id)
    source: SourceType
    url: Optional[str] = None
    description: str = ""
    raw_data: dict = Field(default_factory=dict)


class Technology(BaseModel):
    id: str = Field(default_factory=_id)
    name: str
    category: Optional[str] = None  # language, framework, tool, etc.
    evidence: list[Evidence] = Field(default_factory=list)


class Achievement(BaseModel):
    id: str = Field(default_factory=_id)
    title: str
    description: str = ""
    metric: Optional[str] = None  # e.g. "100k users", "2.4k stars"
    evidence: list[Evidence] = Field(default_factory=list)


class Project(BaseModel):
    id: str = Field(default_factory=_id)
    name: str
    description: str = ""
    url: Optional[str] = None
    technologies: list[Technology] = Field(default_factory=list)
    achievements: list[Achievement] = Field(default_factory=list)
    start_date: Optional[date] = None
    end_date: Optional[date] = None
    evidence: list[Evidence] = Field(default_factory=list)


class Experience(BaseModel):
    id: str = Field(default_factory=_id)
    company: str
    position: str
    description: str = ""
    location: Optional[str] = None
    start_date: Optional[date] = None
    end_date: Optional[date] = None
    projects: list[Project] = Field(default_factory=list)
    technologies: list[Technology] = Field(default_factory=list)
    achievements: list[Achievement] = Field(default_factory=list)
    evidence: list[Evidence] = Field(default_factory=list)


class Education(BaseModel):
    id: str = Field(default_factory=_id)
    institution: str
    degree: str
    field: Optional[str] = None
    start_date: Optional[date] = None
    end_date: Optional[date] = None
    evidence: list[Evidence] = Field(default_factory=list)


class SkillCategory(str, Enum):
    LANGUAGE = "language"
    FRAMEWORK = "framework"
    TOOL = "tool"
    SOFT = "soft"
    DOMAIN = "domain"


class Skill(BaseModel):
    id: str = Field(default_factory=_id)
    name: str
    category: SkillCategory
    proficiency: Optional[str] = None  # expert, advanced, intermediate
    evidence: list[Evidence] = Field(default_factory=list)


class Profile(BaseModel):
    id: str = Field(default_factory=_id)
    full_name: str
    headline: Optional[str] = None
    email: Optional[str] = None
    phone: Optional[str] = None
    location: Optional[str] = None
    website: Optional[str] = None
    linkedin: Optional[str] = None
    github: Optional[str] = None
    summary: Optional[str] = None


class CareerGraph(BaseModel):
    """Root node of the Career Knowledge Graph."""

    profile: Profile
    experience: list[Experience] = Field(default_factory=list)
    education: list[Education] = Field(default_factory=list)
    skills: list[Skill] = Field(default_factory=list)
    projects: list[Project] = Field(default_factory=list)  # standalone projects
    technologies: list[Technology] = Field(default_factory=list)
    evidence: list[Evidence] = Field(default_factory=list)

    def all_technologies(self) -> list[Technology]:
        seen: set[str] = set()
        result: list[Technology] = []
        for tech in self.technologies:
            if tech.name not in seen:
                seen.add(tech.name)
                result.append(tech)
        for exp in self.experience:
            for tech in exp.technologies:
                if tech.name not in seen:
                    seen.add(tech.name)
                    result.append(tech)
            for proj in exp.projects:
                for tech in proj.technologies:
                    if tech.name not in seen:
                        seen.add(tech.name)
                        result.append(tech)
        for proj in self.projects:
            for tech in proj.technologies:
                if tech.name not in seen:
                    seen.add(tech.name)
                    result.append(tech)
        return result

    def all_achievements(self) -> list[Achievement]:
        result: list[Achievement] = []
        for exp in self.experience:
            result.extend(exp.achievements)
            for proj in exp.projects:
                result.extend(proj.achievements)
        for proj in self.projects:
            result.extend(proj.achievements)
        return result

    def to_search_text(self) -> str:
        lines = [self.profile.summary or ""]
        for exp in self.experience:
            lines.append(f"{exp.position} at {exp.company}")
            lines.append(exp.description)
            for proj in exp.projects:
                lines.append(proj.name)
                lines.append(proj.description)
        for proj in self.projects:
            lines.append(proj.name)
            lines.append(proj.description)
        for skill in self.skills:
            lines.append(skill.name)
        return "\n".join(lines)
