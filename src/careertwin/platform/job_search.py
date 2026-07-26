"""Job Search MCP — integrates with job boards and career platforms."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum


class JobSource(str, Enum):
    LINKEDIN = "linkedin"
    INDEED = "indeed"
    GLASSDOOR = "glassdoor"
    REMOTE_OK = "remote_ok"
    HN_WHO_IS_HIRING = "hn_who_is_hiring"
    CUSTOM = "custom"


@dataclass
class JobListing:
    id: str
    title: str
    company: str
    location: str
    source: JobSource
    url: str
    description: str = ""
    salary_min: int | None = None
    salary_max: int | None = None
    remote: bool = False
    tags: list[str] = field(default_factory=list)
    posted_at: str = ""


@dataclass
class JobSearchQuery:
    keywords: str = ""
    location: str = ""
    remote_only: bool = False
    sources: list[JobSource] = field(default_factory=list)
    min_salary: int | None = None
    max_salary: int | None = None
    limit: int = 20


@dataclass
class JobSearchResult:
    query: JobSearchQuery
    jobs: list[JobListing] = field(default_factory=list)
    total: int = 0
    sources_checked: list[JobSource] = field(default_factory=list)


class JobSearchAgent:
    """Searches job boards and aggregates results."""

    def search(self, query: JobSearchQuery) -> JobSearchResult:
        jobs: list[JobListing] = []
        sources_checked: list[JobSource] = []

        sources = query.sources or [JobSource.LINKEDIN, JobSource.INDEED]

        for source in sources:
            try:
                results = self._search_source(source, query)
                jobs.extend(results)
                sources_checked.append(source)
            except Exception:
                continue

        jobs.sort(key=lambda j: j.posted_at, reverse=True)

        return JobSearchResult(
            query=query,
            jobs=jobs[: query.limit],
            total=len(jobs),
            sources_checked=sources_checked,
        )

    def _search_source(self, source: JobSource, query: JobSearchQuery) -> list[JobListing]:
        if source == JobSource.HN_WHO_IS_HIRING:
            return self._search_hn(query)
        if source == JobSource.REMOTE_OK:
            return self._search_remote_ok(query)
        return []

    def _search_hn(self, query: JobSearchQuery) -> list[JobListing]:
        import httpx

        jobs: list[JobListing] = []
        try:
            response = httpx.get(
                "https://hacker-news.firebaseio.com/v0/jobstories.json",
                timeout=10.0,
            )
            response.raise_for_status()
            story_ids = response.json()[:10]

            for sid in story_ids:
                job_response = httpx.get(
                    f"https://hacker-news.firebaseio.com/v0/item/{sid}.json",
                    timeout=5.0,
                )
                if job_response.status_code == 200:
                    data = job_response.json()
                    title = data.get("title", "")

                    if query.keywords and query.keywords.lower() not in title.lower():
                        continue

                    jobs.append(
                        JobListing(
                            id=str(sid),
                            title=title,
                            company=data.get("by", "Unknown"),
                            location="Remote",
                            source=JobSource.HN_WHO_IS_HIRING,
                            url=data.get("url", f"https://news.ycombinator.com/item?id={sid}"),
                            description=data.get("text", ""),
                            remote=True,
                            tags=["hn", "who-is-hiring"],
                        )
                    )
        except Exception:
            pass

        return jobs

    def _search_remote_ok(self, query: JobSearchQuery) -> list[JobListing]:
        import httpx

        jobs: list[JobListing] = []
        try:
            response = httpx.get(
                "https://remoteok.com/api",
                headers={"User-Agent": "CareerTwin/1.0"},
                timeout=10.0,
            )
            response.raise_for_status()
            data = response.json()

            for item in data[:20]:
                if not isinstance(item, dict) or "id" not in item:
                    continue

                title = item.get("position", "")
                if query.keywords and query.keywords.lower() not in title.lower():
                    continue

                tags = item.get("tags", []) or []

                jobs.append(
                    JobListing(
                        id=str(item["id"]),
                        title=title,
                        company=item.get("company", "Unknown"),
                        location=item.get("location", "Remote"),
                        source=JobSource.REMOTE_OK,
                        url=item.get("url", ""),
                        description=item.get("description", ""),
                        salary_min=item.get("salary_min"),
                        salary_max=item.get("salary_max"),
                        remote=True,
                        tags=tags,
                        posted_at=item.get("date", ""),
                    )
                )
        except Exception:
            pass

        return jobs
