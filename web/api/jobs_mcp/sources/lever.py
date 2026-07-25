"""Lever job board adapter — public JSON API."""

from __future__ import annotations

import aiohttp

from ..base import BaseSourceAdapter, JobListing, clean_location, is_internship_title


class LeverSource(BaseSourceAdapter):
    source_name = "lever"

    BASE_URL = "https://api.lever.co/v0"

    def detect_ats(self, url: str) -> bool:
        return "lever.co" in url

    async def _fetch_postings(self, company_id: str) -> list[dict]:
        url = f"{self.BASE_URL}/postings/{company_id}?mode=json"
        async with aiohttp.ClientSession() as session:
            async with session.get(url, timeout=aiohttp.ClientTimeout(total=15)) as resp:
                if resp.status == 200:
                    return await resp.json()
        return []

    def _parse(self, company_id: str, postings: list[dict], intern_only: bool) -> list[JobListing]:
        results = []
        for p in postings:
            title = p.get("text", "")
            is_intern = is_internship_title(title)
            if intern_only and not is_intern:
                continue

            categories = p.get("categories", {})
            department = categories.get("department", "")
            team = categories.get("team", "")
            location = clean_location(categories.get("location", ""))

            results.append(JobListing(
                source=self.source_name,
                company=company_id,
                title=title,
                url=p.get("hostedUrl", ""),
                location=location,
                description=p.get("descriptionPlain", "")[:2000],
                employment_type=department or team,
                is_intern=is_intern,
                raw=p,
            ))
        return results

    async def search_company_jobs(
        self, company_identifier: str, query: str = "", limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        company_id = company_identifier.lower().replace(" ", "")
        postings = await self._fetch_postings(company_id)
        results = self._parse(company_id, postings, intern_only)
        if query:
            q = query.lower()
            results = [j for j in results if q in j.title.lower() or q in j.description.lower()]
        return results[:limit]

    async def search_all_jobs(
        self, query: str, limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        return []
