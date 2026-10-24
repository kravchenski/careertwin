"""Direct JSON job-board API access, bypassing HTML/markdown scraping.

Career pages almost always render their job list client-side from a JSON API
the browser calls after page load — scraping the rendered HTML (what
job_scanner.py's crawl4ai path does) is fragile (misses paginated results,
breaks on markup changes) when the real data is one HTTP call away.

Two tiers, in order of preference:
1. Known ATS platforms (Greenhouse, Lever, Ashby, SmartRecruiters) expose a
   documented public JSON API at a predictable URL derived from the careers
   URL — call it directly via httpx, no browser needed.
2. Everyone else: open the careers page in a real (async) Playwright browser,
   listen to network responses, and pick out whichever JSON response looks
   like a job listing — then call that same endpoint directly next time.

Either tier can fail (site changed its API, discovered endpoint 404s/500s
later) — callers must treat `None`/`[]` as "fall back to markdown scraping",
never as "this company has no jobs".
"""

from __future__ import annotations

from typing import Any
from urllib.parse import urljoin, urlparse

import httpx

from careertwin.net import guard_page
from careertwin.net import safe_async_client

_TIMEOUT = 15.0
_USER_AGENT = "Mozilla/5.0 (compatible; CareerTwinJobBot/1.0)"

_discovered_api_cache: dict[str, dict[str, Any]] = {}


def _job(company: str, title: str, url: str, location: str = "", source: str = "") -> dict:
    return {
        "company": company,
        "title": (title or "").strip()[:160],
        "url": url,
        "location": (location or "").strip()[:100],
        "source": source,
    }




def _path_slug(url: str, index: int = 0) -> str | None:
    parts = [p for p in urlparse(url).path.split("/") if p]
    return parts[index] if len(parts) > index else None


async def _fetch_greenhouse(url: str, company_name: str, client: httpx.AsyncClient) -> list[dict] | None:
    board = _path_slug(url)
    if not board:
        return None
    api_url = f"https://boards-api.greenhouse.io/v1/boards/{board}/jobs?content=true"
    resp = await client.get(api_url)
    if resp.status_code >= 400:
        return None
    data = resp.json()
    jobs = data.get("jobs")
    if not isinstance(jobs, list):
        return None
    return [
        _job(
            company_name,
            j.get("title", ""),
            j.get("absolute_url", ""),
            (j.get("location") or {}).get("name", ""),
            "ats_greenhouse",
        )
        for j in jobs
        if j.get("absolute_url")
    ]


async def _fetch_lever(url: str, company_name: str, client: httpx.AsyncClient) -> list[dict] | None:
    company = _path_slug(url)
    if not company:
        return None
    api_url = f"https://api.lever.co/v0/postings/{company}?mode=json"
    resp = await client.get(api_url)
    if resp.status_code >= 400:
        return None
    data = resp.json()
    if not isinstance(data, list):
        return None
    return [
        _job(
            company_name,
            j.get("text", ""),
            j.get("hostedUrl", ""),
            (j.get("categories") or {}).get("location", ""),
            "ats_lever",
        )
        for j in data
        if j.get("hostedUrl")
    ]


async def _fetch_ashby(url: str, company_name: str, client: httpx.AsyncClient) -> list[dict] | None:
    org = _path_slug(url)
    if not org:
        return None
    api_url = f"https://api.ashbyhq.com/posting-api/job-board/{org}"
    resp = await client.get(api_url)
    if resp.status_code >= 400:
        return None
    data = resp.json()
    jobs = data.get("jobs")
    if not isinstance(jobs, list):
        return None
    return [
        _job(company_name, j.get("title", ""), j.get("jobUrl", ""), j.get("location", ""), "ats_ashby")
        for j in jobs
        if j.get("jobUrl")
    ]


async def _fetch_smartrecruiters(url: str, company_name: str, client: httpx.AsyncClient) -> list[dict] | None:
    company = _path_slug(url)
    if not company:
        return None
    jobs: list[dict] = []
    offset = 0
    limit = 100
    for _ in range(10):
        api_url = f"https://api.smartrecruiters.com/v1/companies/{company}/postings?limit={limit}&offset={offset}"
        resp = await client.get(api_url)
        if resp.status_code >= 400:
            return jobs or None
        data = resp.json()
        content = data.get("content")
        if not isinstance(content, list):
            break
        for p in content:
            posting_id = p.get("id", "")
            jobs.append(
                _job(
                    company_name,
                    p.get("name", ""),
                    f"https://jobs.smartrecruiters.com/{company}/{posting_id}",
                    (p.get("location") or {}).get("city", ""),
                    "ats_smartrecruiters",
                )
            )
        total = data.get("totalFound", len(content))
        offset += limit
        if offset >= total or not content:
            break
    return jobs


_KNOWN_ATS_FETCHERS = {
    "greenhouse": _fetch_greenhouse,
    "lever": _fetch_lever,
    "ashby": _fetch_ashby,
    "smartrecruiters": _fetch_smartrecruiters,
}


async def fetch_jobs_from_known_ats(url: str, company_name: str, ats_platform: str | None) -> list[dict] | None:
    """Call a known ATS's public JSON API directly. Returns None (not []) on
    any failure/mismatch so the caller knows to fall back — [] only means
    "the API works and genuinely has zero open postings"."""
    fetcher = _KNOWN_ATS_FETCHERS.get(ats_platform or "")
    if not fetcher:
        return None
    try:
        async with safe_async_client(timeout=_TIMEOUT, headers={"User-Agent": _USER_AGENT}) as client:
            return await fetcher(url, company_name, client)
    except Exception:
        return None



_JOB_KEY_HINTS = ("title", "position", "jobtitle", "name", "role")
_LIST_KEY_HINTS = ("jobs", "postings", "positions", "results", "data", "items", "openings", "vacancies")


def _looks_like_job_list(payload: Any) -> list[dict] | None:
    """Heuristic: does this JSON payload contain a list of job-like dicts?
    Checked against both bare arrays and {"jobs": [...]}-style wrappers."""
    candidates: list[Any] = []
    if isinstance(payload, list):
        candidates.append(payload)
    elif isinstance(payload, dict):
        for key in _LIST_KEY_HINTS:
            val = payload.get(key)
            if isinstance(val, list):
                candidates.append(val)
        for val in payload.values():
            if isinstance(val, dict):
                for key in _LIST_KEY_HINTS:
                    inner = val.get(key)
                    if isinstance(inner, list):
                        candidates.append(inner)

    for arr in candidates:
        if len(arr) < 2 or not all(isinstance(x, dict) for x in arr):
            continue
        sample = arr[0]
        keys_lower = {k.lower() for k in sample.keys()}
        if any(hint in k for k in keys_lower for hint in _JOB_KEY_HINTS):
            return arr
    return None


async def discover_jobs_api_via_network(url: str, company_name: str) -> dict[str, Any] | None:
    """Load the careers page in a real browser and watch for a JSON response
    that looks like the job list — this is what actually powers the page,
    so calling it directly next time is faster and gets full pagination for
    free (most such APIs return everything in one call already)."""
    from playwright.async_api import async_playwright

    candidates: list[dict[str, Any]] = []

    async def on_response(response):
        try:
            ctype = response.headers.get("content-type", "")
            if "json" not in ctype:
                return
            if response.status >= 400:
                return
            body = await response.json()
        except Exception:
            return
        arr = _looks_like_job_list(body)
        if arr:
            candidates.append({"url": response.url, "method": response.request.method, "count": len(arr)})

    try:
        async with async_playwright() as p:
            browser = await p.chromium.launch()
            page = await browser.new_page()
            await guard_page(page)
            page.on("response", on_response)
            try:
                await page.goto(url, timeout=20000, wait_until="domcontentloaded")
                await page.wait_for_timeout(3000)
            except Exception:
                pass
            await browser.close()
    except Exception:
        return None

    if not candidates:
        return None
    best = max(candidates, key=lambda c: c["count"])
    return {"url": best["url"], "method": best["method"]}


async def fetch_jobs_via_discovered_api(api_info: dict[str, Any], company_name: str, base_url: str) -> list[dict] | None:
    """Re-call a previously discovered JSON endpoint directly via httpx.
    Returns None on any failure so the caller invalidates the cache entry."""
    try:
        async with safe_async_client(timeout=_TIMEOUT, headers={"User-Agent": _USER_AGENT}) as client:
            resp = await client.request(api_info.get("method", "GET"), api_info["url"])
            if resp.status_code >= 400:
                return None
            arr = _looks_like_job_list(resp.json())
            if arr is None:
                return None
    except Exception:
        return None

    jobs = []
    for item in arr:
        title = next((item.get(k) for k in item if k.lower() in _JOB_KEY_HINTS and item.get(k)), "")
        link = next(
            (item.get(k) for k in item if "url" in k.lower() and isinstance(item.get(k), str)),
            "",
        )
        if link and not link.startswith("http"):
            link = urljoin(base_url, link)
        location = next(
            (item.get(k) for k in item if "location" in k.lower() and isinstance(item.get(k), str)),
            "",
        )
        if title and link:
            jobs.append(_job(company_name, str(title), link, str(location), "discovered_api"))
    return jobs




async def fetch_jobs_smart(url: str, company_name: str, ats_platform: str | None) -> list[dict]:
    """Try the fastest, most complete source first; return [] (never raise)
    so the caller's existing markdown-scrape fallback always still runs."""
    domain = urlparse(url).netloc

    known = await fetch_jobs_from_known_ats(url, company_name, ats_platform)
    if known is not None:
        return known

    cached = _discovered_api_cache.get(domain)
    if cached:
        jobs = await fetch_jobs_via_discovered_api(cached, company_name, url)
        if jobs is not None:
            return jobs
        _discovered_api_cache.pop(domain, None)

    discovered = await discover_jobs_api_via_network(url, company_name)
    if discovered:
        jobs = await fetch_jobs_via_discovered_api(discovered, company_name, url)
        if jobs is not None:
            _discovered_api_cache[domain] = discovered
            return jobs

    return []
