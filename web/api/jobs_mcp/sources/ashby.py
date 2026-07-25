"""Ashby job board adapter — public GraphQL API."""

from __future__ import annotations

import aiohttp

from ..base import BaseSourceAdapter, JobListing, clean_location, is_internship_title


class AshbySource(BaseSourceAdapter):
    source_name = "ashby"

    API_URL = "https://jobs.ashbyhq.com/api/non-user-graphql"

    def detect_ats(self, url: str) -> bool:
        return "ashbyhq.com" in url

    async def _fetch_jobs(self, company_slug: str) -> list[dict]:
        payload = {
            "operationName": "ApiJobBoardWithTeams",
            "variables": {"organizationHostedJobsPageName": slug},
            "query": (
                "query ApiJobBoardWithTeams($organizationHostedJobsPageName: String!) {"
                "  jobBoard: jobBoardWithTeams(organizationHostedJobsPageName: $organizationHostedJobsPageName) {"
                "    teams { id name jobs { id title locationName employmentType } }"
                "  }"
                "}"
            ),
        }
        async with aiohttp.ClientSession() as session:
            async with session.post(self.API_URL, json=payload, timeout=aiohttp.ClientTimeout(total=15)) as resp:
                if resp.status == 200:
                    data = await resp.json()
                    board = data.get("data", {}).get("jobBoard", {})
                    teams = board.get("teams", [])
                    jobs = []
                    for team in teams:
                        for j in team.get("jobs", []):
                            j["teamName"] = team.get("name", "")
                            jobs.append(j)
                    return jobs
        return []

    def _parse(self, company_slug: str, jobs: list[dict], intern_only: bool) -> list[JobListing]:
        results = []
        for j in jobs:
            title = j.get("title", "")
            is_intern = is_internship_title(title)
            if intern_only and not is_intern:
                continue

            results.append(JobListing(
                source=self.source_name,
                company=company_slug,
                title=title,
                url=f"https://jobs.ashbyhq.com/{company_slug}/{j.get('id', '')}",
                location=clean_location(j.get("locationName", "")),
                employment_type=j.get("employmentType", "") or j.get("teamName", ""),
                is_intern=is_intern,
                raw=j,
            ))
        return results

    async def search_company_jobs(
        self, company_identifier: str, query: str = "", limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        slug = company_identifier.lower().replace(" ", "")
        jobs = await self._fetch_jobs(slug)
        results = self._parse(slug, jobs, intern_only)
        if query:
            q = query.lower()
            results = [j for j in results if q in j.title.lower()]
        return results[:limit]

    async def search_all_jobs(
        self, query: str, limit: int = 50, intern_only: bool = False
    ) -> list[JobListing]:
        return []
