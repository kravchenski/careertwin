"""Workday job board adapter — HTML scraping (no public API)."""

from __future__ import annotations

import re

import aiohttp
from bs4 import BeautifulSoup

from ..base import BaseSourceAdapter, JobListing


class WorkdaySource(BaseSourceAdapter):
    source_name = "workday"

    def detect_ats(self, url: str) -> bool:
        return "myworkdayjobs.com" in url or "workday.com" in url

    def _extract_domain(self, url: str) -> str:
        """Extract company domain from Workday URL. e.g. 'https://nvidia.wd5.myworkdayjobs.com' -> 'nvidia'"""
        m = re.search(r"https?://([^.]+)\.wd\d*\.myworkdayjobs\.com", url)
        if m:
            return m.group(1)
        return ""

    async def _fetch_jobs_from_url(self, workday_url: str) -> list[dict]:
        """Fetch job listings from a Workday career page."""
        if not workday_url.endswith("/"):
            workday_url += "/"
        api_url = workday_url.rstrip("/") + "search" if "/search" not in workday_url else workday_url
        results = []
        try:
            async with aiohttp.ClientSession() as session:
                headers = {"User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36"}
                async with session.get(api_url, headers=headers, timeout=aiohttp.ClientTimeout(total=15)) as resp:
                    if resp.status == 200:
                        html = await resp.text()
                        soup = BeautifulSoup(html, "html.parser")
                        for card in soup.select('[data-automation-id="jobTitle"], .WGAG'):
                            link = card.find("a", href=True)
                            if link:
                                title = link.get_text(strip=True)
                                href = link["href"]
                                if not href.startswith("http"):
                                    href = api_url.rstrip("/") + href
                                results.append({"title": title, "url": href})
        except Exception:
            pass
        return results

    async def _fetch_jobs_from_api(self, company_domain: str) -> list[dict]:
        """Try to hit Workday JSON endpoint."""
        url = f"https://{company_domain}.wd5.myworkdayjobs.com/wday/cxs/{company_domain}/{company_domain}/jobs"
        results = []
        try:
            payload = {"appliedFacets": {}, "limit": 20, "offset": 0, "searchText": ""}
            async with aiohttp.ClientSession() as session:
                async with session.post(url, json=payload, timeout=aiohttp.ClientTimeout(total=15)) as resp:
                    if resp.status == 200:
                        data = await resp.json()
                        for j in data.get("jobPostings", []):
                            results.append({
                                "title": j.get("title", ""),
                                "url": j.get("externalPath", ""),
                                "location": j.get("locationsText", ""),
                                "posted": j.get("postedOn", ""),
                            })
        except Exception:
            pass
        return results

    def _parse(self, company_domain: str, jobs: list[dict], intern_only: bool) -> list[JobListing]:
        results = []
        for j in jobs:
            title = j.get("title", "")
            is_intern = "intern" in title.lower()
            if intern_only and not is_intern:
                continue

            url = j.get("url", "")
            if not url.startswith("http"):
                url = f"https://{company_domain}.wd5.myworkdayjobs.com/en-US{url}"

            results.append(JobListing(
                source=self.source_name,
                company=company_domain,
                title=title,
                url=url,
                location=j.get("location", ""),
                posted_at=j.get("posted", ""),
                is_intern=is_intern,
                raw=j,
            ))
        return results

    async def search_company_jobs(
        self, company_identifier: str, query: str = "", limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        domain = company_identifier.lower().replace(" ", "")

        jobs = await self._fetch_jobs_from_api(domain)
        if not jobs:
            fallback_url = f"https://{domain}.wd5.myworkdayjobs.com/en-US/{domain}"
            jobs = await self._fetch_jobs_from_url(fallback_url)

        results = self._parse(domain, jobs, intern_only)
        if query:
            q = query.lower()
            results = [j for j in results if q in j.title.lower()]
        return results[:limit]

    async def search_all_jobs(
        self, query: str, limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        return []
