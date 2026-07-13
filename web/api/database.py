"""CareerTwin API — database setup, migrations, and Pydantic request models."""

from __future__ import annotations

import contextlib
import json
import sqlite3
from typing import Any
from urllib.parse import urlparse

from pydantic import BaseModel, Field, field_validator

from config import DB_PATH

_ALLOWED_URL_SCHEMES = frozenset({"http", "https"})
_SSRF_BLOCKED_HOSTS = frozenset({
    "localhost", "127.0.0.1", "0.0.0.0", "::1",
    "169.254.169.254",
    "metadata.google.internal",
    "host.docker.internal",
})


def _validate_public_url(url: str) -> str:
    """Reject internal/SSRF-prone URLs; allow only http/https to public hosts."""
    parsed = urlparse(url)
    if parsed.scheme not in _ALLOWED_URL_SCHEMES:
        raise ValueError(f"URL scheme {parsed.scheme!r} is not allowed")
    hostname = (parsed.hostname or "").lower().rstrip(".")
    if hostname in _SSRF_BLOCKED_HOSTS:
        raise ValueError(f"URL host {hostname!r} is blocked")
    if hostname.startswith("192.168.") or hostname.startswith("10.") or hostname.startswith("172."):
        raise ValueError("Private-range URLs are not allowed")
    return url




def _init_db() -> None:
    with contextlib.closing(sqlite3.connect(str(DB_PATH))) as conn:
        conn.execute("""
            CREATE TABLE IF NOT EXISTS user_profiles (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                full_name TEXT,
                email TEXT,
                phone TEXT,
                location TEXT,
                github TEXT,
                linkedin TEXT,
                website TEXT,
                summary TEXT,
                skills TEXT DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS studio_sessions (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                first_name TEXT,
                last_name TEXT,
                linkedin_url TEXT,
                linkedin_email TEXT,
                github TEXT,
                website TEXT,
                preferences TEXT DEFAULT '{}',
                profile TEXT DEFAULT '{}',
                screenshots TEXT DEFAULT '[]',
                completed_at TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS companies (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                description TEXT DEFAULT '',
                industry TEXT DEFAULT '',
                employee_count TEXT DEFAULT '',
                website TEXT DEFAULT '',
                careers TEXT DEFAULT '',
                internships TEXT DEFAULT '',
                certifications TEXT DEFAULT '',
                learning TEXT DEFAULT '',
                github TEXT DEFAULT '',
                linkedin TEXT DEFAULT '',
                glassdoor TEXT DEFAULT '',
                wellfound TEXT DEFAULT '',
                linkedin_raw TEXT DEFAULT '{}',
                ats_platform TEXT DEFAULT '',
                cached_at TEXT DEFAULT CURRENT_TIMESTAMP,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS collected_jobs (
                id TEXT PRIMARY KEY,
                source TEXT,
                url TEXT,
                title TEXT,
                company TEXT,
                location TEXT,
                description TEXT,
                match_score REAL DEFAULT 0,
                matching_skills TEXT DEFAULT '[]',
                missing_skills TEXT DEFAULT '[]',
                collected_at TEXT,
                status TEXT DEFAULT 'new'
            )
        """)
        conn.commit()


def _migrate_db() -> None:
    """Add missing columns to existing tables (atomic — single commit)."""
    with contextlib.closing(sqlite3.connect(str(DB_PATH))) as conn:
        cols = {row[1] for row in conn.execute("PRAGMA table_info(studio_sessions)").fetchall()}
        needed = []
        if "screenshots" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN screenshots TEXT DEFAULT '[]'")
        if "completed_at" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN completed_at TEXT")
        if "first_name" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN first_name TEXT")
        if "last_name" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN last_name TEXT")
        for stmt in needed:
            conn.execute(stmt)

        comp_cols = {row[1] for row in conn.execute("PRAGMA table_info(companies)").fetchall()}
        if "ats_platform" not in comp_cols:
            conn.execute("ALTER TABLE companies ADD COLUMN ats_platform TEXT DEFAULT ''")

        if needed or "ats_platform" not in comp_cols:
            conn.commit()




def _db_conn() -> contextlib.AbstractContextManager[sqlite3.Connection]:
    return contextlib.closing(sqlite3.connect(str(DB_PATH)))


def _row_to_dict(row: sqlite3.Row | None) -> dict[str, Any] | None:
    if row is None:
        return None
    data = dict(row)
    if "skills" in data and isinstance(data["skills"], str):
        try:
            data["skills"] = json.loads(data["skills"])
        except (json.JSONDecodeError, TypeError):
            data["skills"] = []
    for json_col in ("preferences", "profile", "screenshots"):
        if json_col in data and isinstance(data[json_col], str):
            try:
                data[json_col] = json.loads(data[json_col])
            except (json.JSONDecodeError, TypeError):
                data[json_col] = {} if json_col != "screenshots" else []
    return data




_VALID_SOURCES = frozenset({"remoteok", "hn", "justjoin", "nofluffjobs", "linkedin"})
_VALID_PROVIDERS = frozenset({"lmstudio", "ollama", "groq", "openai", "openrouter"})
_PLACEHOLDER_NAMES = frozenset({
    "unknown", "linkedin user", "candidate", "applicant",
    "your name", "your twin", "professional",
})


def is_real_profile_name(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    normalized = " ".join(value.split()).lower()
    return len(normalized) >= 2 and normalized not in _PLACEHOLDER_NAMES


def resolve_profile_name(
    profile_name: Any,
    first_name: Any = "",
    last_name: Any = "",
) -> str:
    if is_real_profile_name(profile_name):
        return " ".join(str(profile_name).split())
    composed = " ".join(
        str(part).strip()
        for part in (first_name, last_name)
        if isinstance(part, str) and part.strip()
    )
    return composed if is_real_profile_name(composed) else ""


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
        return _validate_public_url(v)


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
        return _validate_public_url(v)


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
