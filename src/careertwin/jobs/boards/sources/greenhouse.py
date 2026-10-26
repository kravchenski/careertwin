"""Greenhouse job board adapter — public JSON API."""

from __future__ import annotations

import aiohttp

from ..base import BaseSourceAdapter, JobListing, clean_location, is_internship_title, to_iso_date


class GreenhouseSource(BaseSourceAdapter):
    source_name = "greenhouse"

    BASE_URL = "https://boards-api.greenhouse.io/v1/boards"

    def detect_ats(self, url: str) -> bool:
        return "greenhouse.io" in url or "boards.greenhouse.io" in url

    async def _fetch_board(self, board_id: str) -> list[dict]:
        url = f"{self.BASE_URL}/{board_id}/jobs?content=true"
        async with aiohttp.ClientSession() as session:
            async with session.get(url, timeout=aiohttp.ClientTimeout(total=15)) as resp:
                if resp.status == 200:
                    data = await resp.json()
                    return data.get("jobs", [])
        return []

    def _parse(self, board_id: str, jobs: list[dict], intern_only: bool) -> list[JobListing]:
        results = []
        for j in jobs:
            title = j.get("title", "")
            is_intern = is_internship_title(title)
            if intern_only and not is_intern:
                continue

            loc = j.get("location", {})
            location_name = clean_location(loc.get("name", "") if isinstance(loc, dict) else str(loc))

            departments = [d.get("name", "") for d in j.get("departments", []) if isinstance(d, dict)]

            results.append(JobListing(
                source=self.source_name,
                company=board_id,
                title=title,
                url=f"https://boards.greenhouse.io/{board_id}/jobs/{j['id']}",
                location=location_name,
                posted_at=to_iso_date(j.get("first_published") or j.get("updated_at")),
                description=j.get("content", ""),
                employment_type=", ".join(departments) if departments else "",
                is_intern=is_intern,
                raw=j,
            ))
        return results

    async def search_company_jobs(
        self, company_identifier: str, query: str = "", limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        board_id = company_identifier.lower().replace(" ", "")
        jobs = await self._fetch_board(board_id)
        results = self._parse(board_id, jobs, intern_only)
        if query:
            q = query.lower()
            results = [j for j in results if q in j.title.lower() or q in j.description.lower()]
        return results[:limit]

    async def search_all_jobs(
        self, query: str, limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        return []
