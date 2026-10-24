"""Auto-discovers a company's internal jobs-search API — or, failing that,
their server-rendered job data — without any per-company hardcoding.

Most companies without a known ATS (Greenhouse/Lever/Ashby/SmartRecruiters/
Workday) still serve jobs from a private API — Storyblok CDN, a bespoke
Elasticsearch proxy (Accenture), etc. A markdown/HTML scrape never sees this
because the listing is rendered client-side after the initial page load.

Some platforms don't even use a separate API call for the initial result
set: Phenom People (Adobe, Activision, and many other enterprise career
sites) server-renders the full job list straight into the page HTML inside
a JS variable (`phApp.ddo...`) for SEO purposes, and only uses XHR/fetch for
secondary widgets (recommendations, status checks, translations) — a
network sniffer never sees the real data because it was never a network
response to begin with. `discover_via_ssr()` handles this: a plain HTTP GET
(no browser) plus proper JSON-aware brace matching (not a fragile regex,
since the blob can contain nested escaped JSON strings) to pull out the
embedded state, then the same generic job-array classifier used everywhere
else in this module.

For everything else, `discover_jobs_api()` watches real network traffic in
a headless browser and classifies which response looks like a jobs list,
generically, without per-company hardcoding.

Cache-and-replay strategy: a discovery run is expensive (5-20s of real
browser time). Once an endpoint is found, callers should persist it
(see routes/jobs_mcp.py) and replay it directly via httpx on future runs —
`replay_jobs_api()` handles that. Replay is only valid when the endpoint's
auth is stateless (e.g. a public API token baked into the URL, like
Storyblok's CDN token). Endpoints whose discovered request carried a
session cookie or CSRF token (`requires_session=True`) invalidate on replay
from a fresh HTTP client and must go through a fresh browser discovery
each time they're needed.
"""

from __future__ import annotations

import json
import logging
import re
from dataclasses import dataclass
from typing import Any
from urllib.parse import urljoin

import httpx

from careertwin.db import db_conn

logger = logging.getLogger("careertwin.job_api_discovery")

_TITLE_KEYS = {"title", "jobtitle", "position", "name", "reqtitle", "jobname", "postingtitle"}
_LOCATION_KEYS = {"location", "city", "joblocation", "primarylocation", "country", "locations", "geo", "state", "citystate"}
_ID_KEYS = {"id", "jobid", "reqid", "jobseqno", "requisitionid", "guid"}
_URL_KEYS = {
    "url", "link", "slug", "applyurl", "detailurl", "joburl", "fullslug",
    "path", "jobdetailurl", "htmlurl", "urlpath",
}
# Extraction priority — "detailurl" and similar frequently point at an
# internal API/introspection endpoint (as seen on Vention's Wagtail-based
# site: meta.detail_url is a JSON API URL, meta.html_url is the actual page)
# rather than a page a human should be sent to, so human-facing URL fields
# are tried first.
_URL_KEY_PRIORITY = [
    "htmlurl", "url", "joburl", "applyurl", "jobdetailurl", "link",
    "urlpath", "fullslug", "path", "slug", "detailurl",
]
_DESCRIPTION_KEYS = {
    "description", "jobdescription", "jobdescriptionclean", "descriptionteaser",
    "summary", "jobsummary", "qualificationclean", "qualification", "joboverview",
    "staticextractivesummary",
}


def _normalize_key(key: str) -> str:
    return re.sub(r"[^a-z0-9]", "", key.lower())

_SESSION_AUTH_MARKERS = ("csrf",)


def _looks_like_cms_page_list(items: list[dict]) -> bool:
    """Reject generic CMS page-tree listings (Wagtail sitemap/children APIs
    and similar) that otherwise satisfy the title+id+slug heuristic just as
    well as a real job posting does — e.g. a "type": "join_job_openings.
    JobOpeningsPage" field on an "Events"/"Contact us"/"Cookie Policy" page
    entry. The literal string "page" inside a type/content-type field is a
    strong, platform-agnostic tell that this is page metadata, not a job."""
    for item in items[:5]:
        for key in ("type", "content_type", "contenttype", "model"):
            value = item.get(key)
            if isinstance(value, str) and "page" in value.lower():
                return True
    return False


def _categories_hit(keys: set[str]) -> int:
    hits = 0
    if keys & _ID_KEYS:
        hits += 1
    if keys & _URL_KEYS:
        hits += 1
    return hits


def _find_job_arrays(obj: Any, path: tuple = ()) -> list[tuple[list[str], list[dict]]]:
    """Recursively search a parsed JSON body for an array of dicts that looks
    like a jobs list — title AND location (a real job posting almost always
    carries one; generic CMS/nav content essentially never does) plus at
    least 1 of {id, url} — with no per-company hardcoding of field names.
    """
    found: list[tuple[list[str], list[dict]]] = []
    if isinstance(obj, list) and obj and all(isinstance(i, dict) for i in obj[:5]):
        keys = {_normalize_key(k) for item in obj[:5] for k in item.keys()}
        has_location = bool(keys & _LOCATION_KEYS)
        if (
            (keys & _TITLE_KEYS)
            and has_location
            and _categories_hit(keys) >= 1
            and not _looks_like_cms_page_list(obj)
        ):
            found.append((list(path), obj))
    if isinstance(obj, dict):
        for k, v in obj.items():
            found.extend(_find_job_arrays(v, path + (k,)))
    elif isinstance(obj, list):
        for i, v in enumerate(obj[:20]):
            found.extend(_find_job_arrays(v, path + (str(i),)))
    return found


# JS variable-assignment markers known to embed a full page-state blob
# (including job search results) directly in the initial server-rendered
# HTML. Extend this list as more platforms are identified — it's the only
# per-platform knowledge this module hardcodes, and it's a marker string,
# not a parser.
_SSR_STATE_MARKERS = (
    "phApp.ddo = ",  # Phenom People (Adobe, Activision, many enterprise ATS sites)
)


async def discover_via_ssr(url: str, timeout: float = 15.0) -> list[dict] | None:
    """Try to pull a jobs list straight out of server-rendered HTML via a
    plain HTTP GET — no browser needed. Much cheaper than discover_jobs_api()
    and catches platforms (like Phenom People) that render the initial
    result set into the page itself rather than fetching it via XHR."""
    try:
        async with httpx.AsyncClient(timeout=timeout, follow_redirects=True) as client:
            resp = await client.get(
                url,
                headers={
                    "User-Agent": (
                        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                        "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
                    )
                },
            )
            resp.raise_for_status()
            html = resp.text
    except Exception as e:
        logger.warning("discover_via_ssr fetch failed for %s: %s", url, e)
        return None

    decoder = json.JSONDecoder()
    for marker in _SSR_STATE_MARKERS:
        idx = html.find(marker)
        if idx < 0:
            continue
        try:
            obj, _ = decoder.raw_decode(html, idx + len(marker))
        except (ValueError, json.JSONDecodeError):
            continue

        best_arr: list[dict] | None = None
        best_count = 0
        for _path, arr in _find_job_arrays(obj):
            if len(arr) > best_count:
                best_count = len(arr)
                best_arr = arr
        if best_arr:
            return best_arr
    return None


@dataclass
class DiscoveredJobsAPI:
    url: str
    method: str
    request_headers: dict[str, str]
    post_data: str | None
    jobs_path: list[str]
    sample_jobs: list[dict]
    requires_session: bool


async def discover_jobs_api(url: str, timeout_ms: int = 20000, settle_ms: int = 6000) -> DiscoveredJobsAPI | None:
    """Load `url` in headless Chromium, capture background XHR/fetch JSON
    responses, and return the one that looks most like a jobs-search API.

    Uses `domcontentloaded` + a fixed settle window rather than `networkidle`
    — some career sites (Phenom People-based ones especially) poll
    continuously in the background and never go network-idle.
    """
    from playwright.async_api import async_playwright

    candidates: list[dict] = []

    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True, args=["--no-sandbox"])
        context = await browser.new_context(
            user_agent=(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
            ),
        )
        page = await context.new_page()

        async def _on_response(response):
            try:
                ctype = response.headers.get("content-type", "")
                if "json" not in ctype:
                    return
                req = response.request
                if req.resource_type not in ("xhr", "fetch"):
                    return
                body = await response.json()
                candidates.append({
                    "url": req.url,
                    "method": req.method,
                    "headers": await req.all_headers(),
                    "post_data": req.post_data,
                    "body": body,
                })
            except Exception:
                pass

        page.on("response", _on_response)

        try:
            await page.goto(url, wait_until="domcontentloaded", timeout=timeout_ms)
            await page.wait_for_timeout(settle_ms)
        except Exception as e:
            logger.warning("discover_jobs_api navigation failed for %s: %s", url, e)
        finally:
            await context.close()
            await browser.close()

    best: DiscoveredJobsAPI | None = None
    best_count = 0
    for c in candidates:
        for path, arr in _find_job_arrays(c["body"]):
            if len(arr) > best_count:
                best_count = len(arr)
                headers = {k: v for k, v in c["headers"].items() if k.lower() != "cookie"}
                requires_session = "cookie" in c["headers"] or any(
                    marker in k.lower() for k in c["headers"] for marker in _SESSION_AUTH_MARKERS
                )
                best = DiscoveredJobsAPI(
                    url=c["url"],
                    method=c["method"],
                    request_headers=headers,
                    post_data=c["post_data"],
                    jobs_path=path,
                    sample_jobs=arr,
                    requires_session=requires_session,
                )
    return best


def _dig(obj: Any, path: list[str]) -> Any:
    cur = obj
    for key in path:
        if isinstance(cur, list):
            try:
                cur = cur[int(key)]
            except (ValueError, IndexError):
                return None
        elif isinstance(cur, dict):
            cur = cur.get(key)
        else:
            return None
    return cur


def _first_matching(item: dict, keys, _depth: int = 0) -> str:
    """Look for a matching key at the top level first; if nothing matches,
    dig one level into nested dicts (e.g. Wagtail-style APIs put the real
    URL under item["meta"]["html_url"] instead of a flat top-level field).

    `keys` may be an unordered set (any match wins) or an ordered list
    (first matching key in list order wins, for fields where multiple
    candidates can exist and one is preferable — see _URL_KEY_PRIORITY)."""
    normalized = {_normalize_key(k): v for k, v in item.items()}
    if isinstance(keys, (list, tuple)):
        for key in keys:
            v = normalized.get(key)
            if isinstance(v, str) and v:
                return v
    else:
        for k, v in normalized.items():
            if k in keys and isinstance(v, str) and v:
                return v
    if _depth == 0:
        for v in item.values():
            if isinstance(v, dict):
                found = _first_matching(v, keys, _depth=1)
                if found:
                    return found
    return ""


def _extract_location(item: dict) -> str:
    """Location is frequently a nested object ({"name": "Vilnius", "country":
    {"name": "Lithuania"}}) rather than a flat string — build a readable
    "City, Country" out of it instead of giving up."""
    flat = _first_matching(item, _LOCATION_KEYS)
    if flat:
        return flat
    for k, v in item.items():
        if _normalize_key(k) in _LOCATION_KEYS and isinstance(v, dict):
            city = v.get("name") or v.get("city") or ""
            country_obj = v.get("country")
            country = country_obj.get("name", "") if isinstance(country_obj, dict) else (country_obj or "")
            parts = [p for p in (city, country) if isinstance(p, str) and p]
            if parts:
                return ", ".join(parts)
    return ""


def normalize_jobs(raw_jobs: list[dict], base_url: str = "") -> list[dict]:
    """Best-effort extraction of title/location/url/description from an
    arbitrary company's job-object shape, using fuzzy field-name matching
    instead of a hardcoded per-company schema."""
    normalized = []
    for item in raw_jobs:
        if not isinstance(item, dict):
            continue
        title = _first_matching(item, _TITLE_KEYS)
        if not title:
            continue
        location = _extract_location(item)
        raw_url = _first_matching(item, _URL_KEY_PRIORITY)
        job_url = urljoin(base_url, raw_url) if raw_url and base_url else raw_url
        description = _first_matching(item, _DESCRIPTION_KEYS)
        normalized.append({"title": title, "location": location, "url": job_url, "description": description})
    return normalized


async def replay_jobs_api(discovered: DiscoveredJobsAPI) -> list[Any] | None:
    """Re-call a previously discovered endpoint directly via httpx, skipping
    a full browser relaunch. Only viable when the endpoint's auth is
    stateless — callers must check `requires_session` first."""
    if discovered.requires_session:
        return None
    try:
        async with httpx.AsyncClient(timeout=15, follow_redirects=True) as client:
            if discovered.method.upper() == "GET":
                resp = await client.get(discovered.url, headers=discovered.request_headers)
            else:
                resp = await client.request(
                    discovered.method,
                    discovered.url,
                    headers=discovered.request_headers,
                    content=discovered.post_data,
                )
            resp.raise_for_status()
            body = resp.json()
    except Exception as e:
        logger.warning("replay_jobs_api failed for %s: %s", discovered.url, e)
        return None

    arr = _dig(body, discovered.jobs_path)
    return arr if isinstance(arr, list) else None


def _load_cached(company: str) -> DiscoveredJobsAPI | None:
    with db_conn() as conn:
        row = conn.execute(
            "SELECT url, method, request_headers, post_data, jobs_path, requires_session "
            "FROM discovered_job_apis WHERE company = ?",
            (company,),
        ).fetchone()
    if not row:
        return None
    url, method, headers_json, post_data, jobs_path_json, requires_session = row
    return DiscoveredJobsAPI(
        url=url,
        method=method,
        request_headers=json.loads(headers_json),
        post_data=post_data,
        jobs_path=json.loads(jobs_path_json),
        sample_jobs=[],
        requires_session=bool(requires_session),
    )


def _save_cache(company: str, d: DiscoveredJobsAPI) -> None:
    with db_conn() as conn:
        conn.execute(
            """
            INSERT INTO discovered_job_apis
                (company, url, method, request_headers, post_data, jobs_path, requires_session,
                 fail_count, discovered_at, last_used_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(company) DO UPDATE SET
                url=excluded.url, method=excluded.method, request_headers=excluded.request_headers,
                post_data=excluded.post_data, jobs_path=excluded.jobs_path,
                requires_session=excluded.requires_session, fail_count=0,
                discovered_at=CURRENT_TIMESTAMP, last_used_at=CURRENT_TIMESTAMP
            """,
            (
                company, d.url, d.method, json.dumps(d.request_headers), d.post_data,
                json.dumps(d.jobs_path), int(d.requires_session),
            ),
        )
        conn.commit()


def _bump_fail(company: str) -> int:
    with db_conn() as conn:
        conn.execute(
            "UPDATE discovered_job_apis SET fail_count = fail_count + 1, last_used_at = CURRENT_TIMESTAMP "
            "WHERE company = ?",
            (company,),
        )
        conn.commit()
        row = conn.execute(
            "SELECT fail_count FROM discovered_job_apis WHERE company = ?", (company,)
        ).fetchone()
    return row[0] if row else 0


# After this many consecutive replay failures, stop retrying the cached
# endpoint on every collection run — assume the site changed and wait for a
# manual/background re-discovery rather than eating a browser launch per
# collect click indefinitely.
_MAX_REPLAY_FAILURES_BEFORE_REDISCOVER = 3


async def get_company_jobs_via_discovery(company_name: str, careers_url: str) -> list[dict]:
    """Cache-and-replay entrypoint used by the collection pipeline for
    companies with no known ATS adapter.

    1. Try the cheap path first: is the job list server-rendered right into
       the HTML (Phenom People and similar)? Plain HTTP GET, no browser.
    2. Cached + stateless endpoint -> fast httpx replay.
    3. No cache, or cache requires a live session, or replay failed enough
       times -> run a fresh browser discovery and refresh the cache.
    """
    if not careers_url:
        return []

    ssr_jobs = await discover_via_ssr(careers_url)
    if ssr_jobs is not None:
        return normalize_jobs(ssr_jobs, base_url=careers_url)

    cached = _load_cached(company_name)
    if cached and not cached.requires_session:
        jobs = await replay_jobs_api(cached)
        if jobs is not None:
            return normalize_jobs(jobs, base_url=careers_url)
        fail_count = _bump_fail(company_name)
        if fail_count < _MAX_REPLAY_FAILURES_BEFORE_REDISCOVER:
            return []

    discovered = await discover_jobs_api(careers_url)
    if not discovered:
        return []
    _save_cache(company_name, discovered)

    if discovered.requires_session:
        # Already have the full response from this discovery run — use it
        # directly rather than immediately re-hitting via replay (which
        # we know will fail for session-bound endpoints).
        return normalize_jobs(discovered.sample_jobs, base_url=careers_url)

    jobs = await replay_jobs_api(discovered)
    if jobs is None:
        return []
    return normalize_jobs(jobs, base_url=careers_url)
