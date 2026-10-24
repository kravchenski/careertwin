"""CareerTwin API — job scraping, caching, deduplication, and web search."""

from __future__ import annotations

import hashlib
import json
import logging
import re
import time
import urllib.parse
from typing import Any

import httpx

from careertwin.net import safe_async_client
from bs4 import BeautifulSoup

from careertwin.browser import (
    _agent_browser_available,
    _agent_browser_eval,
    _agent_browser_open,
    _agent_browser_read,
    _agent_browser_session,
    _agent_browser_snapshot,
)
from careertwin.settings import (
    CACHE_DIR,
    CACHE_TTL,
    COMPANY_WORKERS,
    JOB_SOURCE_DOMAINS,
    SEARXNG_URL,
    SKILLS,
    TAVILY_API_KEY,
)

logger = logging.getLogger("careertwin.scraper")


_SAFE_SOURCE_RE = re.compile(r"^[a-z0-9_-]{1,40}$")


def _cache_key(source: str, query: str) -> Any:
    if not _SAFE_SOURCE_RE.match(source):
        raise ValueError(f"Invalid source name: {source!r}")
    h = hashlib.md5(f"{source}:{query}".encode()).hexdigest()[:12]
    return CACHE_DIR / f"{source}_{h}.json"


def _cache_get(source: str, query: str, ttl: int | None = None) -> list[dict] | None:
    path = _cache_key(source, query)
    if not path.exists():
        return None
    max_age = ttl or CACHE_TTL.get(source, 3600)
    if time.time() - path.stat().st_mtime > max_age:
        return None
    try:
        return json.loads(path.read_text())
    except Exception:
        return None


def _cache_set(source: str, query: str, data: list[dict]) -> None:
    path = _cache_key(source, query)
    path.write_text(json.dumps(data, ensure_ascii=False))




def _normalize_for_dedup(text: str) -> str:
    text = text.lower().strip()
    for suffix in [
        " sp. z o.o.", " sp. z oo.", " inc.", " llc", " ltd.", " ltd",
        " gmbh", " s.a.", " s.a.s.", " corp.", " corporation",
    ]:
        text = text.replace(suffix, "")
    text = re.sub(r"\s+", " ", text)
    return text


def _fuzzy_match(a: str, b: str) -> float:
    a, b = _normalize_for_dedup(a), _normalize_for_dedup(b)
    if a == b:
        return 100.0
    shorter = min(len(a), len(b))
    if shorter == 0:
        return 0.0
    longer = max(len(a), len(b))
    matches = sum(1 for ca, cb in zip(a, b) if ca == cb)
    return (matches / longer) * 100


def deduplicate_jobs(jobs: list[dict]) -> list[dict]:
    if not jobs:
        return jobs

    source_priority = {
        "company_career": 0,
        "tavily": 1,
        "searxng": 2,
        "linkedin": 3,
        "indeed": 4,
        "glassdoor": 5,
        "ziprecruiter": 6,
        "justjoin": 7,
        "nofluffjobs": 8,
        "remoteok": 9,
        "hn": 10,
    }
    kept: list[dict] = []

    for job in sorted(jobs, key=lambda j: source_priority.get(j.get("source", ""), 15)):
        title = job.get("title", "")
        company = job.get("company", "")
        is_dup = False
        for existing in kept:
            t_sim = _fuzzy_match(title, existing.get("title", ""))
            c_sim = _fuzzy_match(company, existing.get("company", ""))
            if c_sim >= 90 and t_sim >= 80:
                is_dup = True
                break
        if not is_dup:
            kept.append(job)

    return kept




def _clean_text_block(text: str, max_length: int | None = None) -> str:
    text = re.sub(r"<[^>]+>", " ", text)
    text = re.sub(r"data-[\w-]+=\"[^\"]*\"", " ", text)
    text = re.sub(r"\b(?:aria|role|class|style|id)=\"[^\"]*\"", " ", text)
    noise_patterns = [
        r"linkedin respects your privacy.*?(?:cookie policy|learn more)",
        r"we use cookies.*?(?:privacy policy|accept|decline)",
        r"select accept to consent.*",
        r"by clicking continue.*",
        r"posted\s+\d{1,2}:\d{2}:\d{2}\s*(?:AM|PM)\.?",
        r"see this and similar jobs on linkedin\.?",
    ]
    for pattern in noise_patterns:
        text = re.sub(pattern, " ", text, flags=re.I | re.S)
    text = re.sub(r"\s+", " ", text).strip()
    if max_length:
        return text[:max_length]
    return text


def _html_to_visible_text(html: str, max_length: int = 4000) -> str:
    soup = BeautifulSoup(html, "lxml")
    for tag in soup(["script", "style", "noscript", "svg"]):
        tag.decompose()
    text = soup.get_text(separator=" ", strip=True)
    return _clean_text_block(text, max_length=max_length)


def _extract_company_from_text(title: str, description: str, url: str = "") -> str:
    title = _clean_text_block(title, max_length=220)
    description = _clean_text_block(description, max_length=1200)
    combined = f"{title}\n{description}"
    patterns = [
        r"\bat\s+([A-Z][A-Za-z0-9&.,'() -]{2,80})",
        r"\b([A-Z][A-Za-z0-9&.,'() -]{2,80})\s+hiring\b",
        r"\bcompany:\s*([A-Z][A-Za-z0-9&.,'() -]{2,80})",
    ]
    for pattern in patterns:
        match = re.search(pattern, combined, re.I)
        if match:
            company = _clean_text_block(match.group(1), max_length=100)
            if company and len(company.split()) <= 8:
                return company

    if "linkedin.com" in url and " - " in title:
        parts = [part.strip() for part in title.split(" - ") if part.strip()]
        if len(parts) >= 2 and len(parts[-1]) <= 80:
            return parts[-1]

    return ""


def _skills_from_text(text: str) -> list[str]:
    lowered = text.lower()
    return [skill for skill in SKILLS if skill.lower() in lowered][:15]


async def _safe_fetch_text(url: str) -> str:
    try:
        async with safe_async_client(follow_redirects=True, timeout=12.0) as client:
            response = await client.get(
                url,
                headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"},
            )
            response.raise_for_status()
            return response.text
    except Exception:
        return ""




def _build_search_query(
    base_query: str,
    experience: str = "",
    location: str = "",
    intern: bool = False,
) -> str:
    """Build clean, search-engine-friendly queries."""
    parts = [base_query.strip()]
    if intern or experience == "internship":
        parts.append("intern")
    elif experience and experience != "any":
        parts.append(experience)
    if location and location != "any":
        parts.append(location)
    return " ".join(parts)


_INTERN_MARKER_RE = re.compile(
    r"\bintern(s|ship)?\b|\bco-?op\b|\bstudent\b|\bapprentice\b|\btrainee\b|"
    r"\bgraduate\s+programme?\b|\bentry[\s-]?level\b|\bno experience\b|\bnew grad\b",
    re.IGNORECASE,
)


def _is_intern_related(title: str, description: str = "") -> bool:
    """Check if a job is intern-related or otherwise requires no prior
    professional experience. Word-boundary matching — a naive substring
    check on "intern" also fires on "Internal", "International", etc."""
    combined = f"{title} {description}"
    return bool(_INTERN_MARKER_RE.search(combined))




_SEARCH_PAGE_PATTERNS = [
    "/jobs?", "keywords=", "/jobs/all", "/jobs/", "search?",
    "/jobs/search", "/jobs/new", "/jobs/recent",
]

_LISTING_TITLE_PREFIXES = [
    "1,000+", "10,000+", "18,000+", "19,000+", "17,000+",
    "1000+", "10000+", "18000+", "19000+", "17000+",
    "100+", "200+", "300+", "400+", "500+", "600+", "700+",
    "21,000+", "22,000+", "50,000+", "100,000+",
]

_JOB_URL_PATTERNS = ["/view/", "/jd/", "/job/", "/opening/", "/position/"]

_JOB_BOARD_SELECTORS: dict[str, dict[str, str]] = {
    "linkedin": {
        "container": "li.jobs-search__result-card, .base-card[data-entity-urn]",
        "title": ".base-search-card__title, h3",
        "company": ".base-search-card__subtitle, h4",
        "link": "a[href]",
    },
    "indeed": {
        "container": ".jobsearch-ResultsList .result, .jobsearch-DesktopStyling",
        "title": "h2.jobTitle a, a.jcs-JobTitle",
        "company": ".companyLocation, .companyName",
        "link": "a[href]",
    },
    "glassdoor": {
        "container": ".JobCard_jobCardWrapper__arQkV, [data-test='job-card']",
        "title": ".JobCard_jobTitle__GLyJ1, .jobTitle",
        "company": ".EmployerProfile_compactEmployerName__LE242, .company",
        "link": "a[href]",
    },
    "ziprecruiter": {
        "container": ".job_card, .result_content",
        "title": ".job_title, h2 a",
        "company": ".company_name, .company",
        "link": "a[href]",
    },
}


def _is_job_listing_url(url: str, title: str) -> bool:
    """Check if URL points to an actual job listing, not a search results page."""
    url_lower = url.lower()

    for pat in _JOB_URL_PATTERNS:
        if pat in url_lower:
            return True

    if any(num in url_lower for num in ["-4", "-5", "-6", "-7", "-8", "-9"]):
        if "/jobs/" in url_lower and url_lower.split("/jobs/")[-1].strip("/"):
            return True

    for pat in _SEARCH_PAGE_PATTERNS:
        if pat in url_lower:
            return False

    title_lower = title.lower()
    if any(title_lower.startswith(p) for p in _LISTING_TITLE_PREFIXES):
        return False

    if "jobs in" in title_lower and ("united states" in title_lower or "location" in title_lower):
        return False

    return True


async def _scrape_job_board(source: str, query: str, limit: int = 20) -> list[dict[str, Any]]:
    """Scrape job listings directly from a job board search page."""
    domain = JOB_SOURCE_DOMAINS.get(source, "")
    if not domain:
        return []

    urls: dict[str, str] = {
        "linkedin": f"https://www.linkedin.com/jobs/search?keywords={urllib.parse.quote_plus(query)}&f_TPR=r604800",
        "indeed": f"https://www.indeed.com/jobs?q={urllib.parse.quote_plus(query)}&sort=date",
        "glassdoor": f"https://www.glassdoor.com/Job/jobs.htm?sc.keyword={urllib.parse.quote_plus(query)}",
        "ziprecruiter": f"https://www.ziprecruiter.com/jobs-search?search={urllib.parse.quote_plus(query)}",
    }

    url = urls.get(source, "")
    if not url:
        return []

    html = await _safe_fetch_text(url)
    if not html:
        return []

    soup = BeautifulSoup(html, "lxml")
    selectors = _JOB_BOARD_SELECTORS.get(source, {})
    if not selectors:
        return []

    jobs: list[dict[str, Any]] = []
    for card in soup.select(selectors["container"])[:limit]:
        title_el = card.select_one(selectors["title"])
        company_el = card.select_one(selectors["company"])
        link_el = card.select_one(selectors["link"])

        title = _clean_text_block(title_el.get_text(" ", strip=True) if title_el else "", max_length=160)
        company = _clean_text_block(company_el.get_text(" ", strip=True) if company_el else "", max_length=100)
        href = link_el.get("href", "") if link_el else ""

        if not title or len(title) < 4:
            continue
        if href and not href.startswith("http"):
            href = f"https://{domain}{href}"
        if href and domain not in href:
            continue

        jobs.append({
            "source": source,
            "url": href,
            "title": title,
            "company": company,
            "description": "",
        })

    return jobs[:limit]


async def _searxng_search(
    query: str, limit: int = 20, sites: list[str] | None = None
) -> list[dict[str, Any]]:
    """Search using SearXNG metasearch engine."""
    if not SEARXNG_URL:
        return []

    params: dict[str, Any] = {
        "q": query,
        "format": "json",
        "categories": "general",
        "language": "en",
        "pageno": 1,
    }

    try:
        async with httpx.AsyncClient(timeout=15.0, follow_redirects=True) as client:
            resp = await client.get(f"{SEARXNG_URL}/search", params=params)
            if resp.status_code != 200:
                logger.warning("SearXNG returned %d", resp.status_code)
                return []
            data = resp.json()
    except Exception as exc:
        logger.warning("SearXNG search failed: %s", exc)
        return []

    jobs: list[dict[str, Any]] = []
    for result in data.get("results", [])[:limit * 3]:
        url = result.get("url", "")
        title = _clean_text_block(result.get("title", ""), max_length=160)
        snippet = _clean_text_block(result.get("content", ""), max_length=300)
        if not url or not title:
            continue

        if not _is_job_listing_url(url, title):
            continue

        source = "searxng"
        for src_name, domain in JOB_SOURCE_DOMAINS.items():
            if domain in url:
                source = src_name
                break

        company = _extract_company_from_text(title, snippet, url)

        jobs.append({
            "source": source,
            "url": url,
            "title": title,
            "company": company,
            "description": snippet,
        })
        if len(jobs) >= limit:
            break

    return jobs




async def _tavily_search(query: str, limit: int = 20) -> list[dict[str, Any]]:
    """Search using Tavily API for high-quality job results."""
    if not TAVILY_API_KEY:
        return []

    try:
        async with safe_async_client(timeout=15.0) as client:
            resp = await client.post(
                "https://api.tavily.com/search",
                json={
                    "api_key": TAVILY_API_KEY,
                    "query": query,
                    "search_depth": "advanced",
                    "include_answer": False,
                    "max_results": limit,
                },
            )
            if resp.status_code != 200:
                logger.warning("Tavily returned %d: %s", resp.status_code, resp.text[:200])
                return []
            data = resp.json()
    except Exception as exc:
        logger.warning("Tavily search failed: %s", exc)
        return []

    jobs: list[dict[str, Any]] = []
    for result in data.get("results", []):
        url = result.get("url", "")
        title = _clean_text_block(result.get("title", ""), max_length=160)
        content = _clean_text_block(result.get("content", ""), max_length=300)
        if not url or not title:
            continue

        source = "tavily"
        for src_name, domain in JOB_SOURCE_DOMAINS.items():
            if domain in url:
                source = src_name
                break

        company = _extract_company_from_text(title, content, url)

        jobs.append({
            "source": source,
            "url": url,
            "title": title,
            "company": company,
            "description": content,
        })

    return jobs




async def _company_career_search(
    company_key: str, query: str, limit: int = 10
) -> list[dict[str, Any]]:
    """Search a specific company's career page using SearXNG site: queries."""
    worker = COMPANY_WORKERS.get(company_key)
    if not worker:
        return []

    company_name = worker["name"]
    career_domain = worker["career_domain"]
    jobs: list[dict[str, Any]] = []

    job_url_patterns = ["/job/", "/position/", "/role/", "/requisition/", "/jd/", "/opening/", "/career/"]
    nav_patterns = ["/login", "/signin", "/register", "#", "/about", "/teams", "/locations",
                    "/home", "/profile", "/settings", "/account", "/applications", "/categories"]

    search_q = f"site:{career_domain} {query} intern"
    try:
        site_jobs = await _searxng_search(search_q, limit=limit * 2)
        for j in site_jobs:
            url = j.get("url", "")
            title = j.get("title", "")
            is_nav = any(p in url.lower() for p in nav_patterns)
            is_job = any(p in url.lower() for p in job_url_patterns)
            if is_nav and not is_job:
                continue
            if len(title) < 8:
                continue
            j["source"] = "company_career"
            j["company"] = company_name
            jobs.append(j)
            if len(jobs) >= limit:
                break
    except Exception as exc:
        logger.warning("SearXNG site search failed for %s: %s", company_key, exc)

    return jobs[:limit]


async def _all_companies_search(query: str, limit: int = 5) -> dict[str, list[dict]]:
    """Search all company career pages in parallel."""
    tasks = {
        key: _company_career_search(key, query, limit)
        for key in COMPANY_WORKERS
    }
    results: dict[str, list[dict]] = {}
    for key, coro in tasks.items():
        try:
            results[key] = await coro
        except Exception:
            results[key] = []
    return results




def _agent_browser_collect_jobs(
    search_url: str, *, source: str, session: str, host: str
) -> list[dict[str, Any]]:
    opened, _stdout, _stderr = _agent_browser_open(search_url, session=session, wait_ms=3500)
    if not opened:
        return []
    _agent_browser_snapshot(session=session, interactive=True)
    payload = _agent_browser_eval(
        """
(() => JSON.stringify(
  Array.from(document.querySelectorAll('a[href]'))
    .map((anchor) => ({
      href: anchor.href || '',
      text: (anchor.innerText || anchor.textContent || '').trim()
    }))
    .filter((item) => item.href && item.text && item.text.length > 3)
    .slice(0, 400)
))()
        """.strip(),
        session=session,
        timeout=25,
    )
    if not payload:
        return []
    try:
        items = json.loads(payload)
    except Exception:
        return []

    jobs: list[dict[str, Any]] = []
    seen: set[str] = set()
    for item in items:
        href = str(item.get("href", "")).strip()
        text = _clean_text_block(str(item.get("text", "")), max_length=180)
        if host not in href or href in seen or len(text) < 4:
            continue
        lower_href = href.lower()
        if source == "justjoin" and "/offers/" not in lower_href:
            continue
        if source == "nofluffjobs" and "/job/" not in lower_href:
            continue
        seen.add(href)
        parts = [part.strip() for part in re.split(r"\n|·|\\|/|-", text) if part.strip()]
        title = parts[0] if parts else text
        company = parts[1] if len(parts) > 1 else ""
        jobs.append({
            "source": source,
            "url": href,
            "title": title[:160],
            "company": company[:120],
        })
        if len(jobs) >= 20:
            break
    return jobs




async def _duckduckgo_search(query: str, domain: str, source: str) -> list[dict[str, Any]]:
    """Fallback DuckDuckGo HTML search for a specific domain."""
    try:
        async with safe_async_client(timeout=15.0, follow_redirects=True) as client:
            search_q = f"site:{domain} {query}"
            response = await client.get(
                "https://html.duckduckgo.com/html/",
                params={"q": search_q},
                headers={
                    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
                },
            )
            response.raise_for_status()
            soup = BeautifulSoup(response.text, "lxml")
            jobs: list[dict[str, Any]] = []
            for link in soup.select("a.result__a"):
                href = link.get("href", "").strip()
                if "duckduckgo" in href:
                    parsed = urllib.parse.parse_qs(urllib.parse.urlparse(href).query)
                    href = parsed.get("uddg", [href])[0]
                title = _clean_text_block(link.get_text(" ", strip=True), max_length=160)
                if href:
                    jobs.append({
                        "source": source,
                        "url": href,
                        "title": title,
                        "company": "",
                    })
            return jobs
    except Exception:
        return []




async def _web_search_jobs(
    query: str,
    limit: int,
    sources: list[str],
    intern: bool = False,
    companies: list[str] | None = None,
) -> list[dict[str, Any]]:
    """Search across multiple backends with fallback chain.

    Priority: SearXNG → Tavily → DuckDuckGo fallback.
    Company workers run in parallel when specified.
    """
    search_sources = sources or list(JOB_SOURCE_DOMAINS.keys())
    all_jobs: list[dict[str, Any]] = []

    if companies:
        company_results = await _all_companies_search(query, limit=8)
        for key in companies:
            for job in company_results.get(key, []):
                all_jobs.append(job)

    for source in search_sources:
        cached = _cache_get(source, query)
        if cached is not None:
            all_jobs.extend(cached)
            continue

        jobs: list[dict[str, Any]] = []
        used_direct_scraping = False

        if source == "remoteok":
            try:
                async with safe_async_client(timeout=15.0) as client:
                    resp = await client.get(
                        "https://remoteok.com/api",
                        headers={"User-Agent": "CareerTwin/2.0"},
                    )
                    if resp.status_code == 200:
                        data = resp.json()
                        query_words = query.lower().split()
                        for item in data[1:]:
                            if not isinstance(item, dict) or "id" not in item:
                                continue
                            title = item.get("position", "")
                            company = item.get("company", "")
                            url = item.get("url", "")
                            tags = " ".join(item.get("tags", []))
                            searchable = f"{title} {company} {tags}".lower()
                            if url and any(w in searchable for w in query_words if len(w) > 2):
                                jobs.append({
                                    "source": "remoteok",
                                    "url": url,
                                    "title": title,
                                    "company": company,
                                })
            except Exception:
                pass

        elif source == "hn":
            try:
                async with safe_async_client(timeout=15.0) as client:
                    resp = await client.get(
                        "https://hn.algolia.com/api/v1/search_by_date",
                        params={"query": query or "hiring", "tags": "comment", "hitsPerPage": 30},
                    )
                    if resp.status_code == 200:
                        for hit in resp.json().get("hits", []):
                            text = hit.get("comment_text", "") or hit.get("story_text", "")
                            title = hit.get("story_title", "") or hit.get("title", "")
                            story_url = hit.get("story_url", "")
                            comment_url = f"https://news.ycombinator.com/item?id={hit.get('objectID', '')}"
                            url = story_url or comment_url
                            if not title and text:
                                match = re.search(r"([^<]+?)(?:\s*(?:—|\||at)\s*)", text[:200])
                                if match:
                                    title = match.group(1).strip()
                            if not title:
                                title = "HN Who's Hiring"
                            searchable = f"{title} {text[:500]}".lower()
                            query_words = query.lower().split()
                            if not query or any(w in searchable for w in query_words if len(w) > 2):
                                jobs.append({
                                    "source": "hn",
                                    "url": url,
                                    "title": title[:120],
                                    "company": "",
                                })
            except Exception:
                pass

        elif source in ("justjoin", "nofluffjobs"):
            try:
                if _agent_browser_available():
                    domain = JOB_SOURCE_DOMAINS.get(source, "")
                    search_url = f"https://{domain}/all-locations?keyword={urllib.parse.quote_plus(query)}" if source == "justjoin" else f"https://{domain}/pl/jobs?criteria=keyword%3D{urllib.parse.quote_plus(query)}"
                    jobs.extend(
                        _agent_browser_collect_jobs(
                            search_url,
                            source=source,
                            session=_agent_browser_session(f"{source}-search"),
                            host=domain,
                        )
                    )
                if not jobs:
                    domain = JOB_SOURCE_DOMAINS.get(source, "")
                    jobs = await _searxng_search(
                        f"site:{domain} {query}", limit=20
                    )
                    for j in jobs:
                        j["source"] = source
                if not jobs:
                    domain = JOB_SOURCE_DOMAINS.get(source, "")
                    jobs = await _duckduckgo_search(query, domain, source)
            except Exception:
                pass

        else:
            domain = JOB_SOURCE_DOMAINS.get(source, "")

            if not jobs and source in _JOB_BOARD_SELECTORS:
                jobs = await _scrape_job_board(source, query, limit=20)
                if jobs:
                    used_direct_scraping = True

            if not jobs:
                search_q = f"{query} job apply {source}" if source != "all" else f"{query} job apply"
                jobs = await _searxng_search(search_q, limit=20)
                for j in jobs:
                    if not j.get("company"):
                        j["company"] = _extract_company_from_text(
                            j.get("title", ""), j.get("description", ""), j.get("url", "")
                        )

            if not jobs and TAVILY_API_KEY:
                search_q = f"site:{domain} {query}" if domain else query
                jobs = await _tavily_search(search_q, limit=20)
                for j in jobs:
                    j["source"] = source
                    if not j.get("company"):
                        j["company"] = _extract_company_from_text(
                            j.get("title", ""), j.get("description", ""), j.get("url", "")
                        )

            if not jobs and domain:
                jobs = await _duckduckgo_search(query, domain, source)

        if intern and jobs and not used_direct_scraping:
            jobs = [j for j in jobs if _is_intern_related(
                j.get("title", ""), j.get("description", "")
            )]

        _cache_set(source, query, jobs)
        all_jobs.extend(jobs)

    deduped = deduplicate_jobs(all_jobs)
    seen: set[str] = set()
    result: list[dict[str, Any]] = []
    for job in deduped:
        url = job.get("url", "")
        if url and url not in seen:
            seen.add(url)
            result.append(job)
        if len(result) >= limit:
            break
    return result




async def scrape_job_url(url: str) -> dict[str, Any]:
    html = await _safe_fetch_text(url)
    if not html and _agent_browser_available():
        session = _agent_browser_session("job-read")
        opened, _stdout, _stderr = _agent_browser_open(url, session=session, wait_ms=2500)
        if opened:
            html = _agent_browser_eval(
                "document.documentElement.outerHTML",
                session=session,
                timeout=20,
            ) or _agent_browser_read(session=session, max_output=15000)
    if not html:
        return {"title": "", "description": "", "skills": [], "company": "", "url": url}

    title_match = (
        re.search(r'<meta[^>]*property="og:title"[^>]*content="([^"]+)"', html, re.I)
        or re.search(r"<h1[^>]*>([^<]+)</h1>", html, re.I)
        or re.search(r"<title[^>]*>([^<]+)</title>", html, re.I)
    )
    title = title_match.group(1).strip()[:120] if title_match else ""
    desc_match = re.search(r'<meta[^>]*name="description"[^>]*content="([^"]+)"', html, re.I)
    description = (
        desc_match.group(1)
        if desc_match
        else _html_to_visible_text(html[:15000], max_length=3000)
    )
    description = _clean_text_block(description, max_length=3000)
    company = ""
    if " at " in title.lower():
        company = title.split(" at ", 1)[-1].strip()
    if not company:
        company = _extract_company_from_text(title, description, url=url)
    return {
        "title": title,
        "description": description,
        "skills": _skills_from_text(description),
        "company": company,
        "url": url,
    }




async def detect_company_profile(
    company_name: str,
    job_url: str,
    job_description: str,
    provider: str,
    model: str,
) -> dict[str, Any]:
    from careertwin.llm import call_ai, parse_json_object
    from careertwin.settings import LANGUAGE_LABELS

    domain_hint = ""
    page_text = ""
    if job_url:
        domain_hint = job_url
        page_text = await _safe_fetch_text(job_url)

    base_language, reason = _language_from_signals(
        "\n".join([company_name, job_description, page_text[:2000]]),
        domain_hint,
    )

    summary = ""
    if page_text or company_name:
        prompt = (
            "Analyze the company context and infer the best language for a CV and cover letter. "
            "Return JSON with keys: language, company_summary, reasoning. "
            "Use ISO language codes like en or pl.\n\n"
            f"Company name: {company_name}\n"
            f"Job URL: {job_url}\n"
            f"Job description: {job_description[:2500]}\n"
            f"Page excerpt: {page_text[:2500]}"
        )
        response = await call_ai(
            system_prompt="You are a company research agent. Be concise and return JSON only.",
            user_prompt=prompt,
            provider=provider,
            model=model,
        )
        parsed = parse_json_object(response)
        if parsed:
            language = str(parsed.get("language") or base_language).lower()
            if language not in LANGUAGE_LABELS:
                language = base_language
            return {
                "language": language,
                "reasoning": parsed.get("reasoning") or reason,
                "company_summary": parsed.get("company_summary") or "",
            }

    if page_text:
        summary = _html_to_visible_text(page_text[:15000], max_length=500)

    return {"language": base_language, "reasoning": reason, "company_summary": summary[:400]}


def _language_from_signals(text: str, domain_hint: str = "") -> tuple[str, str]:
    haystack = f"{text}\n{domain_hint}".lower()
    polish_markers = [
        " poland", " warsaw", "krakow", "wrocław", "język polski", "zespół",
        "bank", "spółka", ".pl", "polski", "millennium",
    ]
    german_markers = [" deutschland", "berlin", "münchen", ".de", "german", "deutsch"]
    if any(marker in haystack for marker in polish_markers):
        return "pl", "Detected Polish market signals"
    if any(marker in haystack for marker in german_markers):
        return "de", "Detected German market signals"
    return "en", "Defaulted to English"
