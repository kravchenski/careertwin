from __future__ import annotations

import os
from typing import Literal
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

from pydantic import BaseModel, Field, field_validator

from careertwin.net import validate_public_url


class Settings(BaseModel):
    context: str = Field(default="", max_length=150_000)
    queries: list[str] = Field(
        default_factory=lambda: ["Python Developer", "AI Engineer", "Frontend Developer"]
    )
    sources: list[str] = Field(default_factory=lambda: ["remotive", "remoteok", "weworkremotely"])
    companies: list[dict[str, str]] = Field(default_factory=list)
    preferences: str = "Work from Poland, remote or in Tricity; no relocation."
    provider: str = "switchyard"  # the agents talk to the Switchyard gateway and to nothing else
    model: str = "agent"
    jev_model: str = "jev-latest"
    decision_backend: Literal["switchyard", "typesafe"] = Field(
        default_factory=lambda: os.getenv("DECISION_BACKEND", "switchyard")
    )
    decision_model: str = Field(default_factory=lambda: os.getenv("DECISION_MODEL", "auto"))
    threshold: float = Field(default=80, ge=0, le=100)
    confidence: float = Field(default=0.7, ge=0, le=1)
    auto_prepare: bool = True
    auto_submit: bool = False
    scheduled: bool = False
    interval_hours: int = Field(default=12, ge=1, le=168)
    limit: int = Field(default=30, ge=1, le=100)
    daily_limit: int = Field(default=10, ge=1, le=100)
    mail_daily_limit: int = Field(default=10, ge=1, le=50)
    mail_scheduled: bool = True  # read the inbox on a timer when mail is configured
    mail_check_minutes: int = Field(default=30, ge=5, le=1440)
    mail_auto_send: bool = False  # the user's standing permission to send drafts without asking again

    @field_validator("provider", mode="before")
    @classmethod
    def only_switchyard(cls, _value: object) -> str:
        """Candidate facts, vacancy pages and answers go to the gateway; a provider chosen in an older
        settings file or sent over the API is ignored."""
        return "switchyard"

    @field_validator("queries")
    @classmethod
    def queries_not_empty(cls, values: list[str]) -> list[str]:
        values = [v.strip() for v in values if v.strip()]
        if not 1 <= len(values) <= 10:
            raise ValueError("Provide 1–10 search queries")
        return values


class Vacancy(BaseModel):
    url: str
    title: str = Field(min_length=1, max_length=300)
    company: str = ""
    source: str = "import"
    location: str = ""
    description: str = Field(default="", max_length=100_000)
    salary: str = ""
    external_id: str = ""

    @field_validator("url")
    @classmethod
    def public_url(cls, value: str) -> str:
        return validate_public_url(value)


class Answer(BaseModel):
    question: str = Field(min_length=1, max_length=1000)
    answer: str = Field(min_length=1, max_length=20_000)


class Outcome(BaseModel):
    status: Literal["interview", "offer", "hired", "rejected", "withdrawn"]
    note: str = Field(default="", max_length=20_000)


def canonical_url(url: str) -> str:
    parts = urlsplit(url)
    kept = [
        (k, v)
        for k, v in parse_qsl(parts.query, keep_blank_values=True)
        if not k.lower().startswith("utm_")
        and k.lower()
        not in {"trackingid", "refid", "ebp", "source", "origin", "origintolandingjobpostings"}
    ]
    # LinkedIn search links must deduplicate against the actual vacancy, not the search query.
    if parts.hostname == "linkedin.com" or (parts.hostname or "").endswith(".linkedin.com"):
        job_id = dict(kept).get("currentJobId")
        if job_id:
            return f"https://www.linkedin.com/jobs/view/{job_id}"
    return urlunsplit(
        (
            parts.scheme.lower(),
            parts.netloc.lower(),
            parts.path.rstrip("/"),
            urlencode(sorted(kept)),
            "",
        )
    )
