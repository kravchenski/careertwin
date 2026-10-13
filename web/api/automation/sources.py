from __future__ import annotations

import asyncio
import json
from dataclasses import asdict
from typing import Any
from urllib.parse import urljoin, urlsplit

import httpx
from bs4 import BeautifulSoup
from database import _validate_public_url

from .models import Settings, Vacancy

SOURCES = {
    "remote": "remote.co",
    "nodesk": "nodesk.co",
    "mokaru": "mokaru.co",
    "workew": "workew.com",
    "flexjobs": "flexjobs.com",
    "remoters": "remoters.net",
    "pangian": "pangian.com",
    "remotive": "remotive.com",
    "justremote": "justremote.co",
    "jobspresso": "jobspresso.co",
    "jobgether": "jobgether.com",
    "wellfound": "wellfound.com",
    "remoteok": "remoteok.com",
    "powertofly": "powertofly.com",
    "findyourcareer": "findyourcareer.io",
    "hiddenremote": "hiddenremote.io",
    "citizenremote": "citizenremote.com",
    "euremotejobs": "euremotejobs.com",
    "workingnomads": "workingnomads.com",
    "virtualvocations": "virtualvocations.com",
    "weworkremotely": "weworkremotely.com",
    "inclusivelyremote": "inclusivelyremote.com",
    "remotenomadjobs": "remotenomadjobs.com",
    "opentoworkremote": "opentoworkremote.com",
    "remotehealthcarejobs": "remotehealthcarejobs.com",
    "linkedin": "linkedin.com",
    "glassdoor": "glassdoor.com",
    "ziprecruiter": "ziprecruiter.com",
    "justjoin": "justjoin.it",
    "nofluffjobs": "nofluffjobs.com",
}


def text(html: str) -> str:
    soup = BeautifulSoup(html, "html.parser")
    for element in soup.select("script,style,nav,footer,header"):
        element.decompose()
    return soup.get_text("\n", strip=True)


async def fetch(url: str) -> str:
    async with httpx.AsyncClient(timeout=25) as client:
        for _ in range(6):
            _validate_public_url(url)
            response = await client.get(url, headers={"User-Agent": "CareerTwin/2.1"})
            if response.is_redirect:
                url = urljoin(url, response.headers["location"])
                continue
            response.raise_for_status()
            return response.text[:2_000_000]
    raise ValueError("Too many redirects")


def parse_posting(html: str, original: Vacancy) -> Vacancy:
    soup = BeautifulSoup(html, "html.parser")

    def find(value: Any) -> dict[str, Any] | None:
        if isinstance(value, list):
            return next((found for item in value if (found := find(item))), None)
        if isinstance(value, dict):
            if value.get("@type") == "JobPosting" or "JobPosting" in value.get("@type", []):
                return value
            return find(value.get("@graph", []))
        return None

    for script in soup.select('script[type="application/ld+json"]'):
        try:
            posting = find(json.loads(script.string or script.get_text()))
        except ValueError, TypeError:
            continue
        if posting:
            organization = posting.get("hiringOrganization") or {}
            return original.model_copy(
                update={
                    "title": posting.get("title") or original.title,
                    "company": organization.get("name", original.company),
                    "description": text(posting.get("description", ""))[:100_000],
                }
            )
    main = soup.select_one("main,article,[role=main]")
    description = text(str(main or soup))
    if len(description) < 100:
        raise ValueError("The vacancy page did not expose a full description; import it manually")
    return original.model_copy(update={"description": description[:100_000]})


class SearchAgent:
    async def discover(self, settings: Settings) -> tuple[list[Vacancy], list[dict[str, str]]]:
        semaphore = asyncio.Semaphore(4)
        failures: list[dict[str, str]] = []

        async def board(source: str) -> list[Vacancy]:
            async with semaphore:
                try:
                    if source not in SOURCES:
                        raise ValueError("Unknown source")
                    if source in {"remoteok", "remotive"}:
                        return await self.feed(source, settings)
                    from job_scraper import _searxng_search, _tavily_search

                    found: list[Vacancy] = []
                    for query in settings.queries:
                        search = f"site:{SOURCES[source]} {query} job"
                        items = await _searxng_search(search, limit=settings.limit)
                        if not items:
                            items = await _tavily_search(search, limit=settings.limit)
                        for item in items:
                            hostname = urlsplit(item.get("url", "")).hostname or ""
                            domain = SOURCES[source]
                            if hostname == domain or hostname.endswith("." + domain):
                                found.append(Vacancy(**{**item, "source": source}))
                    if not found:
                        failures.append(
                            {
                                "source": source,
                                "message": (
                                    "No indexed vacancies returned; check search configuration or"
                                    " import links"
                                ),
                            }
                        )
                    return found
                except Exception as exc:
                    failures.append({"source": source, "message": str(exc)})
                    return []

        results = await asyncio.gather(
            *(board(source) for source in dict.fromkeys(settings.sources))
        )
        jobs = [job for result in results for job in result]
        if settings.companies:
            from jobs_mcp import search_all_companies

            try:
                for query in settings.queries:
                    found = await search_all_companies(
                        settings.companies,
                        query=query,
                        limit_per_company=settings.limit,
                        max_concurrent=4,
                    )
                    for item in found:
                        values = asdict(item)
                        values["description"] = text(values.get("description", ""))
                        jobs.append(Vacancy(**values))
            except Exception as exc:
                failures.append({"source": "companies", "message": str(exc)})
        return jobs, failures

    async def feed(self, source: str, settings: Settings) -> list[Vacancy]:
        endpoint = (
            "https://remotive.com/api/remote-jobs"
            if source == "remotive"
            else "https://remoteok.com/api"
        )
        async with httpx.AsyncClient(timeout=25) as client:
            response = await client.get(endpoint, headers={"User-Agent": "CareerTwin/2.1"})
            response.raise_for_status()
            data = response.json()
        items = data.get("jobs", []) if isinstance(data, dict) else data
        jobs = []
        words = {
            word.casefold()
            for query in settings.queries
            for word in query.split()
            if len(word) > 2 and word.casefold() not in {"developer", "engineer"}
        }
        for item in items:
            if not isinstance(item, dict) or not item.get("id"):
                continue
            title = item.get("title") or item.get("position", "")
            description = text(item.get("description", ""))
            if words and not any(
                word in (title + " " + " ".join(item.get("tags", []))).casefold() for word in words
            ):
                continue
            jobs.append(
                Vacancy(
                    url=item["url"],
                    title=title,
                    company=item.get("company_name") or item.get("company", ""),
                    description=description[:100_000],
                    source=source,
                    location=item.get("candidate_required_location") or item.get("location", ""),
                    salary=str(item.get("salary", "")),
                    external_id=str(item["id"]),
                )
            )
        return jobs

    async def hydrate(self, vacancy: Vacancy) -> Vacancy:
        if len(vacancy.description.strip()) >= 500:
            return vacancy
        return parse_posting(await fetch(vacancy.url), vacancy)
