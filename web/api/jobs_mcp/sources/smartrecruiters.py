"""SmartRecruiters job board adapter — public JSON API."""

from __future__ import annotations

import aiohttp

from ..base import BaseSourceAdapter, JobListing


class SmartRecruitersSource(BaseSourceAdapter):
    source_name = "smartrecruiters"

    BASE_URL = "https://api.smartrecruiters.com/v1/companies"

    def detect_ats(self, url: str) -> bool:
        return "smartrecruiters.com" in url

    async def _fetch_postings(self, company_id: str) -> list[dict]:
        url = f"{self.BASE_URL}/{company_id}/postings"
        results = []
        params = {"limit": 100, "offset": 0}
        async with aiohttp.ClientSession() as session:
            while True:
                async with session.get(url, params=params, timeout=aiohttp.ClientTimeout(total=15)) as resp:
                    if resp.status == 200:
                        data = await resp.json()
                        content = data.get("content", [])
                        results.extend(content)
                        if len(content) < 100:
                            break
                        params["offset"] += 100
                    else:
                        break
        return results

    def _parse(self, company_id: str, postings: list[dict], intern_only: bool) -> list[JobListing]:
        results = []
        for p in postings:
            title = p.get("name", "")
            is_intern = "intern" in title.lower()
            if intern_only and not is_intern:
                continue

            loc = p.get("location", {})
            location_name = loc.get("city", "") + ", " + loc.get("country", "") if isinstance(loc, dict) else ""
            ref = p.get("ref", "")
            board_url = p.get("boardUrl", "")

            results.append(JobListing(
                source=self.source_name,
                company=company_id,
                title=title,
                url=board_url or f"https://smartrecruiters.com/{company_id}/{ref}",
                location=location_name.strip(", "),
                description=p.get("JobDescription", {}).get("plainTextDescription", "")[:2000],
                employment_type=p.get("type", ""),
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
