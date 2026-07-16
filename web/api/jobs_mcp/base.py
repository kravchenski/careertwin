"""Base adapter class for job board integrations."""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


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
