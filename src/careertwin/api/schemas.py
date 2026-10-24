"""Request models for the HTTP API."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, Field, field_validator

from careertwin.net import validate_public_url

_VALID_SOURCES = frozenset({"remoteok", "hn", "justjoin", "nofluffjobs", "linkedin"})
_VALID_PROVIDERS = frozenset({"lmstudio", "ollama", "groq", "openai", "openrouter"})


class UserProfileCreate(BaseModel):
    username: str = Field(..., min_length=1, max_length=64, pattern=r"^[a-zA-Z0-9_-]+$")
    full_name: str = Field("", max_length=200)
    email: str = Field("", max_length=254)
    phone: str = Field("", max_length=40)
    location: str = Field("", max_length=200)
    github: str = Field("", max_length=500)
    linkedin: str = Field("", max_length=500)
    website: str = Field("", max_length=500)
    summary: str = Field("", max_length=8000)
    skills: list[str] = Field(default_factory=list, max_length=200)


class UserProfileUpdate(BaseModel):
    full_name: str | None = Field(None, max_length=200)
    email: str | None = Field(None, max_length=254)
    phone: str | None = Field(None, max_length=40)
    location: str | None = Field(None, max_length=200)
    github: str | None = Field(None, max_length=500)
    linkedin: str | None = Field(None, max_length=500)
    website: str | None = Field(None, max_length=500)
    summary: str | None = Field(None, max_length=8000)
    skills: list[str] | None = Field(None, max_length=200)


class StudioSubmit(BaseModel):
    username: str = Field(..., min_length=1, max_length=64, pattern=r"^[a-zA-Z0-9_-]+$")
    first_name: str | None = Field(None, max_length=100)
    last_name: str | None = Field(None, max_length=100)
    linkedin_url: str | None = None
    linkedin_email: str | None = Field(None, max_length=254)
    github: str | None = Field(None, max_length=500)
    website: str | None = Field(None, max_length=500)
    preferences: dict[str, Any] = {}
    profile: dict[str, Any] = {}
    screenshots: list[str] = []
    completed_at: str | None = None


class ScrapeRequest(BaseModel):
    url: str

    @field_validator("url")
    @classmethod
    def url_must_be_public(cls, v: str) -> str:
        return validate_public_url(v)


class JobSearchRequest(BaseModel):
    username: str = Field(..., min_length=1, max_length=64)
    query: str = Field("", max_length=500)
    sources: list[str] = Field(default=["remoteok", "hn", "justjoin", "nofluffjobs"])
    limit: int = Field(20, ge=1, le=100)

    @field_validator("sources")
    @classmethod
    def sources_allowlist(cls, v: list[str]) -> list[str]:
        invalid = [s for s in v if s not in _VALID_SOURCES]
        if invalid:
            raise ValueError(f"Unknown sources: {invalid}")
        return v


class JobApplyRequest(BaseModel):
    job_url: str
    username: str = Field(..., min_length=1, max_length=64)
    full_name: str = Field("", max_length=200)
    email: str = Field("", max_length=254)
    phone: str = Field("", max_length=40)
    cover_letter: str = Field("", max_length=20_000)
    provider: str = Field("lmstudio", max_length=40)
    model: str = Field("", max_length=200)
    auto_submit: bool = False

    @field_validator("provider")
    @classmethod
    def provider_allowlist(cls, v: str) -> str:
        if v not in _VALID_PROVIDERS:
            raise ValueError(f"Unknown provider: {v!r}")
        return v

    @field_validator("job_url")
    @classmethod
    def job_url_must_be_public(cls, v: str) -> str:
        return validate_public_url(v)


_VALID_APPLICATION_STATUSES = frozenset({"applied", "phone_screen", "technical", "offer", "rejected"})


class JobApplicationCreate(BaseModel):
    username: str = Field("default", min_length=1, max_length=64)
    company: str = Field(..., min_length=1, max_length=200)
    position: str = Field(..., min_length=1, max_length=200)
    domain: str = Field("", max_length=200)
    job_url: str = Field("", max_length=2000)


class JobApplicationUpdate(BaseModel):
    status: str | None = None
    note: str | None = Field(None, max_length=2000)

    @field_validator("status")
    @classmethod
    def status_allowlist(cls, v: str | None) -> str | None:
        if v is not None and v not in _VALID_APPLICATION_STATUSES:
            raise ValueError(f"Unknown status: {v!r}")
        return v


class JobCollectRequest(BaseModel):
    username: str = Field("default", min_length=1, max_length=64)
    source: str = Field("linkedin", max_length=40)
    query: str = Field("", max_length=500)
    location: str = Field("any", max_length=200)
    experience_level: str = Field("any", max_length=40)
    work_type: str = Field("any", max_length=40)
    posted_within: str = Field("week", max_length=20)
    limit: int = Field(20, ge=1, le=100)
    intern: bool = Field(False)
    companies: list[str] = Field(default_factory=list)


class CompanySearchRequest(BaseModel):
    query: str = Field(..., min_length=1, max_length=200)
    limit: int = Field(20, ge=1, le=100)


class CompanyCollectRequest(BaseModel):
    query: str = Field("software engineer", max_length=200)
    limit: int = Field(50, ge=1, le=200)
