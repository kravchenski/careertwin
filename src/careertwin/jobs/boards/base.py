"""Base adapter class for job board integrations."""

from __future__ import annotations

import re
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any

_INTERNSHIP_RE = re.compile(r"\bintern(s|ship)?\b|\bco-?op\b", re.IGNORECASE)
_ENTRY_LEVEL_RE = re.compile(
    r"\bentry[\s-]?level\b|\bno experience\b|\bjunior\b|\bgraduate\b|\bnew grad\b|\bearly career\b",
    re.IGNORECASE,
)


def is_internship_title(text: str) -> bool:
    """Word-boundary internship detection — a naive substring check
    ("intern" in text.lower()) also matches "Internal", "International",
    "Internet", etc. and silently mislabels ordinary corporate roles as
    internships while contributing nothing toward finding real ones."""
    return bool(_INTERNSHIP_RE.search(text))


def is_entry_level_title(text: str) -> bool:
    """Catches roles that don't literally say "intern" but are explicitly
    aimed at candidates with no professional experience — "entry level",
    "no experience", "junior", "graduate", "early career"."""
    return bool(_ENTRY_LEVEL_RE.search(text))


_NON_GEOGRAPHIC_LOCATIONS = {
    "in-office", "in office", "on-site", "on site", "onsite",
    "remote", "hybrid", "various", "various locations", "tbd",
    "flexible", "flexible location", "worldwide",
}


def clean_location(value: str) -> str:
    """Some ATS configurations put a workplace-arrangement label ("In-Office",
    "Remote", "Hybrid") in the location field instead of an actual place —
    that's a real value the source API returns, not a parsing bug, but it
    isn't a city and shouldn't be shown as one."""
    if value.strip().lower() in _NON_GEOGRAPHIC_LOCATIONS:
        return ""
    return value


@dataclass
class JobListing:
    """Normalized job listing from any source."""
    source: str
    company: str
    title: str
    url: str
    location: str = ""
    description: str = ""
    salary: str = ""
    posted_at: str = ""
    employment_type: str = ""
    is_intern: bool = False
    raw: dict[str, Any] = field(default_factory=dict)


class BaseSourceAdapter(ABC):
    """Base class for all job board adapters."""

    source_name: str = "unknown"

    @abstractmethod
    async def search_company_jobs(
        self,
        company_identifier: str,
        query: str = "",
        limit: int = 50,
        intern_only: bool = False,
    ) -> list[JobListing]:
        """Search for jobs at a specific company."""
        ...

    @abstractmethod
    async def search_all_jobs(
        self,
        query: str,
        limit: int = 50,
        intern_only: bool = False,
    ) -> list[JobListing]:
        """Search across all companies on this platform."""
        ...

    @abstractmethod
    def detect_ats(self, url: str) -> bool:
        """Detect if a URL belongs to this ATS platform."""
        ...
