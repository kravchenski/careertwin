"""Jobs MCP — unified job board aggregation across 10+ ATS platforms."""

from __future__ import annotations

import asyncio
import logging
from typing import Callable, Sequence

from careertwin.jobs.api_discovery import get_company_jobs_via_discovery

from .base import BaseSourceAdapter, JobListing
from .sources.ashby import AshbySource
from .sources.greenhouse import GreenhouseSource
from .sources.lever import LeverSource
from .sources.smartrecruiters import SmartRecruitersSource
from .sources.workday import WorkdaySource

logger = logging.getLogger(__name__)

ADAPTERS: list[BaseSourceAdapter] = [
    GreenhouseSource(),
    LeverSource(),
    AshbySource(),
    SmartRecruitersSource(),
    WorkdaySource(),
]


def detect_ats_from_url(url: str) -> str | None:
    """Detect which ATS platform a URL belongs to."""
    for adapter in ADAPTERS:
        if adapter.detect_ats(url):
            return adapter.source_name
    return None


def get_adapter(source_name: str) -> BaseSourceAdapter | None:
    """Get adapter by source name."""
    for adapter in ADAPTERS:
        if adapter.source_name == source_name:
            return adapter
    return None


async def search_company_jobs(
    company_identifier: str,
    ats_platform: str | None = None,
    query: str = "",
    limit: int = 50,
    intern_only: bool = False,
) -> list[JobListing]:
    """
    Search jobs at a specific company.

    If ats_platform is provided, only that adapter is used.
    Otherwise, all adapters are tried in parallel.
    """
    if ats_platform:
        adapter = get_adapter(ats_platform)
        if adapter:
            return await adapter.search_company_jobs(company_identifier, query, limit, intern_only)
        logger.warning("Unknown ATS platform: %s", ats_platform)
        return []

    tasks = [
        adapter.search_company_jobs(company_identifier, query, limit, intern_only)
        for adapter in ADAPTERS
    ]
    results = await asyncio.gather(*tasks, return_exceptions=True)
    for result in results:
        if isinstance(result, list) and result:
            return result
    return []


async def _search_via_discovered_api(company_name: str, careers_url: str) -> list[JobListing]:
    """Fallback for companies with no known ATS: try the cache-and-replay
    jobs-API discovery (see job_api_discovery.py) before giving up on them."""
    try:
        raw_jobs = await get_company_jobs_via_discovery(company_name, careers_url)
    except Exception as e:
        logger.warning("discovered-api search failed for %s: %s", company_name, e)
        return []
    return [
        JobListing(
            source="discovered_api",
            company=company_name,
            title=j["title"],
            url=j.get("url", ""),
            location=j.get("location", ""),
            description=j.get("description", ""),
        )
        for j in raw_jobs
    ]


async def search_all_companies(
    companies: list[dict],
    query: str = "",
    limit_per_company: int = 20,
    intern_only: bool = False,
    max_concurrent: int = 10,
    on_progress: Callable[[str, int, int], None] | None = None,
) -> list[JobListing]:
    """
    Search jobs across multiple companies.

    Each company dict should have:
      - name: company identifier
      - ats_platform: optional ATS platform name
      - url: optional career page URL (used for ATS detection)

    `on_progress(company_name, completed_count, total_count)` fires after each
    company resolves (success or failure) — lets a caller stream real progress
    instead of the UI sitting at 0% until the whole batch finishes.
    """
    semaphore = asyncio.Semaphore(max_concurrent)
    total = sum(1 for c in companies if c.get("name"))
    completed = 0

    async def _search_one(company: dict) -> list[JobListing]:
        nonlocal completed
        async with semaphore:
            name = company.get("name", "")
            platform = company.get("ats_platform")
            if not platform and company.get("url"):
                platform = detect_ats_from_url(company["url"])
            try:
                jobs = await search_company_jobs(name, platform, query, limit_per_company, intern_only)
                if not jobs and company.get("url"):
                    jobs = await _search_via_discovered_api(name, company["url"])
                return jobs
            finally:
                completed += 1
                if on_progress:
                    on_progress(name, completed, total)

    tasks = [_search_one(c) for c in companies if c.get("name")]
    results = await asyncio.gather(*tasks, return_exceptions=True)
    all_jobs: list[JobListing] = []
    for result in results:
        if isinstance(result, list):
            all_jobs.extend(result)
    return all_jobs


async def search_by_query(
    query: str,
    sources: list[str] | None = None,
    limit: int = 50,
    intern_only: bool = False,
) -> list[JobListing]:
    """
    Search all platforms by query (no company filter).

    Currently only LinkedIn supports cross-company search;
    this hits platform-specific search endpoints where available.
    """
    target_adapters = [a for a in ADAPTERS if not sources or a.source_name in sources]
    tasks = [
        adapter.search_all_jobs(query, limit, intern_only)
        for adapter in target_adapters
    ]
    results = await asyncio.gather(*tasks, return_exceptions=True)
    all_jobs: list[JobListing] = []
    for result in results:
        if isinstance(result, list):
            all_jobs.extend(result)
    return all_jobs[:limit]
