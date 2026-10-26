"""Workday career sites, read through the JSON that the sites themselves load: no login, no HTML scraping.

Any link to a posting or a career site on `*.myworkdayjobs.com` is enough (a web search for
`site:myworkdayjobs.com python intern poland` finds them): the tenant and the site name are in the address.
The newest vacancies of a country or a city come back with the day they were published.
"""

from __future__ import annotations

import asyncio
import re
from dataclasses import dataclass
from datetime import date, datetime, timedelta, timezone
from typing import Any

import httpx
from bs4 import BeautifulSoup

from careertwin.jobs.boards.base import to_iso_date
from careertwin.net import safe_async_client

HOST = re.compile(r"^[a-z0-9][a-z0-9-]*\.wd\d+\.myworkdayjobs\.com$", re.I)
ADDRESS = re.compile(
    r"https?://(?P<tenant>[a-z0-9][a-z0-9-]*)\.(?P<wd>wd\d+)\.myworkdayjobs\.com"
    r"/(?:wday/cxs/[^/]+/)?(?:[a-z]{2}(?:-[A-Z]{2})?/)?(?P<site>[^/?#]+)",
    re.I,
)
PAGE = 20  # the most a site returns in one answer
PAUSE = 0.15  # seconds between two requests to the same site


@dataclass(frozen=True)
class Site:
    host: str  # rockwellautomation.wd1.myworkdayjobs.com
    tenant: str  # rockwellautomation
    name: str  # External_Rockwell_Automation

    @property
    def api(self) -> str:
        return f"https://{self.host}/wday/cxs/{self.tenant}/{self.name}"

    @property
    def company(self) -> str:
        return self.tenant.replace("-", " ").title()


def parse_site(url: str) -> Site | None:
    """The Workday site behind a link, or None when the link is not a Workday one."""
    match = ADDRESS.match(url.strip())
    if not match or match["site"].lower() in {"job", "wday", "login"}:
        return None
    host = f"{match['tenant']}.{match['wd']}.myworkdayjobs.com".lower()
    return Site(host, match["tenant"].lower(), match["site"]) if HOST.match(host) else None


def find_facet(facets: list[dict[str, Any]], label: str, parameter: str = "") -> tuple[str, str] | None:
    """(facet parameter, id) of the value called `label` ("Poland", "Gdańsk") in a site's filter tree."""
    wanted = label.casefold()

    def walk(items: list[dict[str, Any]], group: str) -> tuple[str, str] | None:
        for item in items:
            name = item.get("facetParameter") or group
            values = item.get("values") or []
            if values and any("values" in value or "facetParameter" in value for value in values):
                if found := walk(values, name):
                    return found
                continue
            if parameter and name != parameter:
                continue
            for value in values:
                descriptor = str(value.get("descriptor", "")).casefold()
                if value.get("id") and (descriptor == wanted or descriptor.startswith(wanted + ",")):
                    return name, value["id"]
        return None

    return walk(facets, "")


def plain(html: str) -> str:
    return " ".join(BeautifulSoup(html or "", "html.parser").get_text(" ").split())


async def _post(client: httpx.AsyncClient, site: Site, body: dict[str, Any]) -> dict[str, Any]:
    response = await client.post(f"{site.api}/jobs", json=body, headers={"Accept": "application/json"})
    response.raise_for_status()
    return response.json()


async def search(
    site: Site,
    query: str = "",
    *,
    place: str = "Poland",
    limit: int = 20,
    client: httpx.AsyncClient | None = None,
) -> list[dict[str, Any]]:
    """Postings of a site that match `query` in `place` (a country or a city), newest first as the site
    orders them. Each is `{title, path, location, posted_on}`; `place=""` means anywhere."""
    if not HOST.match(site.host):
        raise ValueError("not a Workday host")
    own = client is None
    client = client or safe_async_client(timeout=25)
    try:
        body: dict[str, Any] = {"appliedFacets": {}, "limit": min(PAGE, limit), "offset": 0, "searchText": query}
        first = await _post(client, site, body)
        if place:
            facet = find_facet(first.get("facets", []), place)
            if facet is None:
                return []  # the site has nothing there
            body["appliedFacets"] = {facet[0]: [facet[1]]}
            first = await _post(client, site, body)
        postings = list(first.get("jobPostings", []))
        total = int(first.get("total") or 0)
        while len(postings) < min(limit, total):
            body["offset"] = len(postings)
            more = (await _post(client, site, body)).get("jobPostings", [])
            if not more:
                break
            postings += more
            await asyncio.sleep(PAUSE)
        return [
            {"title": p.get("title", ""), "path": p.get("externalPath", ""), "location": p.get("locationsText", ""), "posted_on": p.get("postedOn", "")}
            for p in postings[:limit]
        ]
    finally:
        if own:
            await client.aclose()


async def detail(site: Site, path: str, client: httpx.AsyncClient) -> dict[str, Any]:
    response = await client.get(f"{site.api}{path}", headers={"Accept": "application/json"})
    response.raise_for_status()
    return response.json().get("jobPostingInfo", {})


async def newest(
    site: Site,
    query: str = "",
    *,
    place: str = "Poland",
    limit: int = 20,
    days: int = 0,
    client: httpx.AsyncClient | None = None,
    today: date | None = None,
) -> list[dict[str, Any]]:
    """Vacancies (fields of a `Vacancy`) of a site, newest first, with the exact day each was published.
    `days` > 0 keeps only those published in that many days."""
    own = client is None
    client = client or safe_async_client(timeout=25)
    today = today or datetime.now(timezone.utc).date()
    try:
        found = []
        for posting in await search(site, query, place=place, limit=limit, client=client):
            try:
                info = await detail(site, posting["path"], client)
            except httpx.HTTPError:
                info = {}
            posted = to_iso_date(info.get("startDate")) or to_iso_date(posting["posted_on"], today)
            location = info.get("location") or posting["location"]
            found.append(
                {
                    "url": info.get("externalUrl") or f"https://{site.host}/{site.name}{posting['path']}",
                    "title": info.get("title") or posting["title"],
                    "company": site.company,
                    "location": location + (f" ({info['remoteType']})" if info.get("remoteType") else ""),
                    "description": plain(info.get("jobDescription", ""))[:20_000],
                    "posted_at": posted,
                    "source": "workday",
                    "external_id": str(info.get("jobReqId", "")),
                }
            )
            await asyncio.sleep(PAUSE)
        if days:
            floor = (today - timedelta(days=days)).isoformat()
            found = [item for item in found if item["posted_at"] >= floor or not item["posted_at"]]
        return sorted(found, key=lambda item: item["posted_at"], reverse=True)
    finally:
        if own:
            await client.aclose()
