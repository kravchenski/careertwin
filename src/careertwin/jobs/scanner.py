"""CareerTwin API — job scanning with crawl4ai for career page extraction."""

from __future__ import annotations

import asyncio
import json
import re
import time
from collections import deque
from urllib.parse import parse_qs, urljoin, urlparse


from careertwin.net import safe_async_client
from bs4 import BeautifulSoup

from careertwin.jobs.api_client import fetch_jobs_from_known_ats, fetch_jobs_smart
from careertwin.settings import COMPANIES_FILE

COMPANIES_FILE = COMPANIES_FILE


_scan_log_queue: asyncio.Queue | None = None
_scan_log_history: deque = deque(maxlen=200)

# Serializes company-scan runs so two concurrent /v1/jobs/mcp/search calls
# (a double-click, a stuck request retried, multiple tabs) can never both be
# emitting progress into the shared scan-log queue at once — that was the
# actual cause of counters resetting mid-stream and companies repeating with
# a lower count than a moment before.
_scan_lock = asyncio.Lock()


def get_scan_lock() -> asyncio.Lock:
    return _scan_lock


def _emit_scan_log(msg: str, level: str = "info"):
    """Push a log message to the SSE queue and history."""
    entry = {"msg": msg, "level": level, "ts": time.time()}
    _scan_log_history.append(entry)
    if _scan_log_queue and not _scan_log_queue.full():
        _scan_log_queue.put_nowait(entry)


def reset_scan_log():
    """Create a fresh queue for the next scan."""
    global _scan_log_queue
    _scan_log_queue = asyncio.Queue(maxsize=500)
    _scan_log_history.clear()


def get_scan_log_queue():
    return _scan_log_queue



def load_companies(region: str | None = None) -> list[dict]:
    """Load companies from companies.json, optionally filtered by region."""
    data = json.loads(COMPANIES_FILE.read_text())
    if region and region != "all":
        data = [c for c in data if c.get("region", "").upper() == region.upper()]
    for c in data:
        if "careers_url" not in c and "careers" in c:
            c["careers_url"] = c["careers"]
    return data



_AGGREGATOR_DOMAINS = {
    "linkedin.com", "indeed.com", "glassdoor.com", "ziprecruiter.com",
    "monster.com", "wikipedia.org", "crunchbase.com", "facebook.com",
    "twitter.com", "x.com", "instagram.com", "youtube.com", "reddit.com",
    "google.com", "bing.com", "duckduckgo.com", "pinterest.com",
}

_careers_url_cache: dict[str, str | None] = {}

_NON_JOB_PATHS = {"", "/", "/privacy", "/legal", "/terms", "/about", "/pricing"}


def _looks_like_job_page(url: str) -> bool:
    path = urlparse(url).path.rstrip("/") or "/"
    return path.lower() not in _NON_JOB_PATHS


async def _search_careers_url(company_name: str) -> str | None:
    """Search the web for a company's careers page and return the first plausible hit."""
    for query in (f"{company_name} careers", f"{company_name} internships"):
        try:
            async with safe_async_client(timeout=15.0, follow_redirects=True) as client:
                resp = await client.get(
                    "https://html.duckduckgo.com/html/",
                    params={"q": query},
                    headers={"User-Agent": "Mozilla/5.0 CareerTwin/2.0"},
                )
                if resp.status_code != 200:
                    continue
                soup = BeautifulSoup(resp.text, "lxml")
                for link in soup.select("a.result__a"):
                    href = link.get("href", "").strip()
                    if not href:
                        continue
                    if "duckduckgo" in href:
                        parsed = parse_qs(urlparse(href).query)
                        href = parsed.get("uddg", [href])[0]
                    domain = urlparse(href).netloc.lower().removeprefix("www.")
                    if not domain or any(domain == d or domain.endswith("." + d) for d in _AGGREGATOR_DOMAINS):
                        continue
                    if not _looks_like_job_page(href):
                        continue
                    _emit_scan_log(f"[DISCOVER] {company_name} -> {href}")
                    return href
        except Exception as e:
            _emit_scan_log(f"[DISCOVER] Search failed for {company_name!r}: {e}", "error")
    _emit_scan_log(f"[DISCOVER] No careers page found for {company_name!r}", "warn")
    return None


def _extract_indeed_jobs(markdown: str, city: str) -> list[dict]:
    """Parse job listings out of an Indeed search-results page's markdown.

    Each listing is a `### [Title](rc/clk?...)` block; the company name is
    reliably present in the "Zobacz wszystkie oferty pracy [praca: X]" /
    "Pokaż wszystkie [praca: X]" link that follows, which is far more robust
    than trying to split the noisy company/location/benefits text above it.
    """
    jobs = []
    blocks = re.split(r"\n(?=### \[)", markdown)
    for block in blocks:
        m = re.match(r"### \[([^\]]+)\]\((https://[a-z.]*indeed\.com/rc/clk\?[^)]+)\)", block)
        if not m:
            continue
        title, url = m.group(1).strip(), m.group(2).strip()
        company_m = re.search(r"\[praca: ([^\]]+)\]", block)
        company = company_m.group(1).strip() if company_m else "Unknown"
        jobs.append({
            "title": title,
            "url": url,
            "company": company,
            "location": city,
            "source": "indeed_search",
        })
    return jobs


async def _search_indeed_jobs(query: str, city: str) -> list[dict]:
    """Search Indeed for jobs matching a query in a specific city.

    Company career pages rarely offer a per-city view, but a job aggregator's
    own search does exactly that — this is the primary mechanism for
    candidates who want an on-site role in a specific city.
    """
    from urllib.parse import quote

    url = f"https://pl.indeed.com/jobs?q={quote(query)}&l={quote(city)}"
    try:
        markdown = await _fetch_markdown_crawl4ai(url, wait_for_js=True)
    except Exception as e:
        _emit_scan_log(f"[INDEED] Error searching {query!r} in {city!r}: {e}", "error")
        return []
    if not markdown:
        return []
    jobs = _extract_indeed_jobs(markdown, city)
    _emit_scan_log(f"[INDEED] {query!r} in {city!r}: {len(jobs)} jobs found")
    return jobs


async def discover_careers_url(company_name: str) -> str | None:
    """Resolve a careers page for a company by name.

    Checks the curated companies.json first (case-insensitive), then falls
    back to a live web search for companies CareerTwin has never seen before.
    """
    name_lower = company_name.strip().lower()
    if not name_lower:
        return None

    for company in load_companies():
        if company["name"].lower() == name_lower:
            return company["careers_url"]

    if name_lower in _careers_url_cache:
        return _careers_url_cache[name_lower]

    url = await _search_careers_url(company_name)
    _careers_url_cache[name_lower] = url
    return url



_crawl4ai_init_lock = asyncio.Lock()
_crawl4ai_crawler = None
_CRAWL4AI_MAX_CONCURRENT = 10
_CRAWL4AI_CONCURRENCY = asyncio.Semaphore(_CRAWL4AI_MAX_CONCURRENT)


async def _get_crawler():
    """Get or create a persistent AsyncWebCrawler instance."""
    global _crawl4ai_crawler
    if _crawl4ai_crawler is None:
        async with _crawl4ai_init_lock:
            if _crawl4ai_crawler is None:
                from crawl4ai import AsyncWebCrawler
                crawler = AsyncWebCrawler()
                await crawler.start()
                _crawl4ai_crawler = crawler
    return _crawl4ai_crawler


async def _fetch_markdown_crawl4ai(url: str, wait_for_js: bool = True) -> str:
    """Fetch a URL using crawl4ai and return clean markdown.

    Args:
        url: The URL to fetch
        wait_for_js: If True, waits 1.5s for JavaScript rendering (for ATS pages)
    """
    try:
        from crawl4ai import CacheMode, CrawlerRunConfig

        js_code = "await new Promise(r => setTimeout(r, 1500));" if wait_for_js else ""

        config = CrawlerRunConfig(
            cache_mode=CacheMode.BYPASS,
            js_code=js_code,
            word_count_threshold=5,
            page_timeout=30000,
        )

        for attempt in range(2):
            async with _CRAWL4AI_CONCURRENCY:
                crawler = await _get_crawler()
                result = await crawler.arun(url=url, config=config)

            if result.success and result.markdown:
                return result.markdown.raw_markdown or ""
            if attempt == 0:
                _emit_scan_log(f"[CRAWL4AI] Empty result for {url}, retrying...", "warn")
        return ""
    except Exception as e:
        _emit_scan_log(f"[CRAWL4AI] Error fetching {url}: {e}", "error")
        return ""


def _parse_markdown_links(markdown: str) -> list[tuple[str, str]]:
    """Extract all [text](url) links from markdown.

    Markdown also allows an optional quoted title after the URL —
    [text](url "title") — which the previous greedy `([^)]+)` swallowed
    whole into the "url" capture, corrupting every link that had one
    (e.g. a job URL ending up as '...jobId=123 "Job Title Here"').
    """
    return [
        (text, url)
        for text, url, _title in re.findall(r'\[([^\]]*)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)', markdown)
    ]


def _is_job_link_from_markdown(text: str, url: str) -> bool:
    """Check if a markdown link looks like a job posting."""
    lower = text.lower().strip()
    parsed = urlparse(url)
    path = parsed.path.rstrip("/")

    if len(lower) < 4:
        return False

    if url.endswith((".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp")):
        return False

    if "/api/images/" in url or "/api/media/" in url:
        return False

    if url.startswith("#") or "/#" in url:
        return False

    parsed_url = urlparse(url)
    if parsed_url.fragment and "ashbyhq" not in parsed_url.netloc:
        return False

    nav_skip = {
        "home", "about", "contact", "blog", "news", "privacy", "terms",
        "cookie", "login", "sign in", "sign up", "careers", "jobs",
        "our culture", "benefits", "what we do", "how we hire",
        "where we work", "sustainability", "learn more", "view all",
        "see all", "explore", "products", "solutions", "pricing",
        "get started", "start free", "join us", "follow us", "share", "download",
    }
    if lower in nav_skip:
        return False

    locations = {
        "amsterdam", "berlin", "london", "paris", "munich", "dublin",
        "madrid", "barcelona", "rome", "milan", "prague", "warsaw",
        "stockholm", "copenhagen", "helsinki", "remote",
    }
    if len(lower) < 25 and lower in locations:
        return False

    if "/products/" in path or "/solutions/" in path or "/industry/" in path or "/news" in path or "/blog" in path:
        return False

    if _is_individual_job_url(url):
        return True

    if _detect_ats_platform(url) and _is_ats_listing_page(url):
        return True

    if _detect_ats_platform(url) and any(s in lower for s in ["sign up", "sign in", "log in", "join", "register"]):
        return False

    job_hints = ["/jobs/", "/positions/", "/roles/", "/openings/",
                 "/vacancies/", "/vacancy/", "/requisition/"]
    for hint in job_hints:
        if hint in path.lower():
            return True

    if re.search(r"[0-9a-f]{8}-[0-9a-f]{4}-", path.lower()):
        return True

    job_words = [
        "engineer", "developer", "manager", "designer", "analyst",
        "scientist", "architect", "lead", "head", "director",
        "senior", "junior", "staff", "principal", "intern",
    ]
    if any(w in lower for w in job_words) and len(lower) >= 8:
        return True

    return False




async def _fetch_html(url: str) -> str:
    """Quick HTTP fetch."""
    try:
        async with safe_async_client(follow_redirects=True, timeout=15.0) as client:
            resp = await client.get(
                url,
                headers={
                    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
                    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                    "Accept-Language": "en-US,en;q=0.9",
                },
            )
            resp.raise_for_status()
            return resp.text
    except Exception:
        return ""


def _clean_text(text: str, max_len: int = 200) -> str:
    """Clean extracted text."""
    text = re.sub(r"\s+", " ", text).strip()
    if max_len:
        return text[:max_len]
    return text



ATS_PLATFORMS = {
    "ashbyhq.com": "ashby",
    "boards.greenhouse.io": "greenhouse",
    "jobs.greenhouse.io": "greenhouse",
    "lever.co": "lever",
    "jobs.lever.co": "lever",
    "myworkdayjobs.com": "workday",
    "wd5.myworkdayjobs.com": "workday",
    "smartrecruiters.com": "smartrecruiters",
    "jobs.smartrecruiters.com": "smartrecruiters",
    "workable.com": "workable",
    "apply.workable.com": "workable",
    "bamboohr.com": "bamboohr",
    "jazz.co": "jazzhr",
    "jobvite.com": "jobvite",
    "icims.com": "icims",
    "taleo.net": "taleo",
    "successfactors.com": "successfactors",
}

SKIP_DOMAINS = {"linkedin.com", "twitter.com", "x.com", "facebook.com", "instagram.com", "glassdoor.com", "google.com", "bing.com"}


def _detect_ats_platform(url: str) -> str | None:
    """Detect if URL is from an ATS platform."""
    parsed = urlparse(url)
    domain = parsed.netloc.lower()
    for ats_domain, ats_name in ATS_PLATFORMS.items():
        if ats_domain in domain:
            return ats_name
    return None


def _is_ats_listing_page(url: str) -> bool:
    """Check if URL is an ATS listing page (not individual job)."""
    parsed = urlparse(url)
    path = parsed.path.rstrip("/")

    if "ashbyhq.com" in parsed.netloc and path.count("/") <= 1:
        return True
    if "greenhouse.io" in parsed.netloc:
        if "/boards/" in path and "/jobs/" not in path:
            return True
        if "boards." in parsed.netloc and path.count("/") <= 1 and "/jobs/" not in path:
            return True
    if "lever.co" in parsed.netloc and path.count("/") <= 1:
        return True
    if "workable.com" in parsed.netloc and "/jobs" in path:
        return True
    return False


def _is_individual_job_url(url: str) -> bool:
    """Check if URL is an individual job posting."""
    parsed = urlparse(url)
    path = parsed.path.rstrip("/")

    if "ashbyhq.com" in parsed.netloc and path.count("/") >= 2:
        return True
    if "greenhouse.io" in parsed.netloc and "/jobs/" in path:
        return True
    if "lever.co" in parsed.netloc and path.count("/") >= 2:
        return True
    if "workable.com" in parsed.netloc and "/jobs/" in path:
        return True

    job_patterns = [
        r"/jobs?/\d+",
        r"/positions?/\d+",
        r"/roles?/\d+",
        r"/openings?/\d+",
        r"/vacancy/\d+",
    ]
    for pattern in job_patterns:
        if re.search(pattern, path, re.I):
            return True
    return False



_STRONG_JOB_WORDS = frozenset([
    "engineer", "developer", "designer", "analyst", "scientist",
    "architect", "researcher", "consultant", "specialist", "coordinator",
    "attorney", "backend", "frontend", "full stack", "fullstack",
    "devops", "sre", "qa", "mobile", "ios", "android",
])

_WEAK_JOB_WORDS = frozenset([
    "manager", "lead", "head", "director", "vp", "intern", "officer",
    "product", "data", "security", "cloud", "ml", "ai", "test",
    "react", "node", "python", "java", "typescript", "golang", "rust",
    "scala", "kotlin", "swift", "staff", "senior", "junior", "principal",
    "associate", "trainee", "apprentice", "graduate", "fellow",
])

_ASHBY_DEPT_SUFFIXES = (
    "Engineering & Infra", "Engineering & Infrastructure",
    "Business", "Research", "Design", "People &",
    "Sales", "Marketing", "Finance", "Operations",
    "Legal", "Product", "Data & Analytics",
)


def _clean_ashby_title(title: str) -> str:
    """Remove department suffixes that Ashby concatenates to job titles."""
    for suffix in _ASHBY_DEPT_SUFFIXES:
        if title.endswith(suffix):
            title = title[: -len(suffix)].strip()
            break
    return title




def _extract_jobs_from_markdown(markdown: str, company_name: str) -> list[dict]:
    """Extract job listings from markdown content.

    Returns list of dicts with title, url, company, location, source.
    """
    links = _parse_markdown_links(markdown)
    jobs = []
    seen = set()

    for text, url in links:
        if url in seen:
            continue
        if any(d in url for d in SKIP_DOMAINS):
            continue
        if not _is_job_link_from_markdown(text, url):
            continue

        seen.add(url)

        title = re.split(r"\s*•\s*", text)[0].strip()
        if not title or len(title) < 4:
            title = text.strip()

        ats = _detect_ats_platform(url)
        if ats == "ashby":
            title = _clean_ashby_title(title)

        location = ""
        parts = text.split("•")
        if len(parts) >= 2:
            location = parts[1].strip()

        jobs.append({
            "company": company_name,
            "title": title[:160],
            "url": url,
            "location": location[:100],
            "source": f"ats_{ats}" if ats else "careers_page",
        })

    return jobs


def _find_pagination_links(markdown: str, base_url: str) -> list[str]:
    """Find 'next page' style links in scraped markdown — numbered pages
    (1, 2, 3...), "Next"/"More" text, or arrow glyphs — so a multi-page job
    listing isn't silently truncated to whatever page 1 happened to show."""
    base_domain = urlparse(base_url).netloc
    found = []
    for text, link_url in _parse_markdown_links(markdown):
        stripped = text.strip().lower()
        is_pagination_text = stripped in (
            "next", "next page", "more", "load more", "show more", ">", "»", "→",
        ) or re.fullmatch(r"\d{1,3}", stripped)
        if not is_pagination_text:
            continue
        full_url = urljoin(base_url, link_url)
        if urlparse(full_url).netloc != base_domain:
            continue
        if full_url != base_url and full_url not in found:
            found.append(full_url)
    return found


async def _fetch_career_page_crawl4ai(url: str, company_name: str) -> list[dict]:
    """Fetch a career page's job listings.

    Tries the site's own JSON job-board API first (either a known ATS's
    documented public API, or one discovered by watching network traffic in
    a real browser) — far more reliable than scraping rendered HTML, and it
    typically returns every posting in one call. Only falls back to
    crawl4ai markdown scraping (with pagination-link following) if no API
    could be found or called successfully.

    For ATS listing pages found via scraping (Ashby, Greenhouse, etc.), also
    fetches individual jobs.
    """
    api_jobs = await fetch_jobs_smart(url, company_name, _detect_ats_platform(url))
    if api_jobs:
        return api_jobs

    markdown = await _fetch_markdown_crawl4ai(url, wait_for_js=True)

    if not markdown or len(markdown) < 100:
        markdown = await _fetch_markdown_crawl4ai(url, wait_for_js=False)

    if not markdown:
        return []

    jobs = _extract_jobs_from_markdown(markdown, company_name)

    seen_urls = {j["url"] for j in jobs}
    visited_pages = {url}
    pending = _find_pagination_links(markdown, url)
    pages_fetched = 0
    while pending and pages_fetched < 5:
        next_url = pending.pop(0)
        if next_url in visited_pages:
            continue
        visited_pages.add(next_url)
        pages_fetched += 1
        page_markdown = await _fetch_markdown_crawl4ai(next_url, wait_for_js=True)
        if not page_markdown:
            continue
        page_jobs = _extract_jobs_from_markdown(page_markdown, company_name)
        new_jobs = [j for j in page_jobs if j["url"] not in seen_urls]
        if not new_jobs:
            break
        seen_urls.update(j["url"] for j in new_jobs)
        jobs.extend(new_jobs)
        pending.extend(u for u in _find_pagination_links(page_markdown, next_url) if u not in visited_pages)

    ats_urls = []
    for job in jobs[:]:
        if _detect_ats_platform(job["url"]) and _is_ats_listing_page(job["url"]):
            ats_urls.append(job["url"])
            jobs.remove(job)

    for ats_url in ats_urls[:2]:
        ats_jobs = await fetch_jobs_smart(ats_url, company_name, _detect_ats_platform(ats_url))
        if not ats_jobs:
            ats_markdown = await _fetch_markdown_crawl4ai(ats_url, wait_for_js=True)
            if ats_markdown:
                ats_jobs = _extract_jobs_from_markdown(ats_markdown, company_name)
                ats = _detect_ats_platform(ats_url)
                if ats:
                    for j in ats_jobs:
                        j["source"] = f"ats_{ats}"
        jobs.extend(ats_jobs)

    return jobs


async def _fetch_many_career_pages_crawl4ai(companies: list[dict]) -> dict[str, list[dict]]:
    """Batch-crawl multiple career pages using a memory-adaptive dispatcher,
    instead of one URL at a time behind a fixed semaphore — lets concurrency
    scale with available memory rather than a hardcoded number, and avoids
    per-URL call overhead across many companies.

    Known-ATS companies (Greenhouse/Lever/Ashby/SmartRecruiters) are pulled
    straight from that platform's public JSON API via httpx first — cheap,
    complete, no browser needed — and only the remainder goes through
    crawl4ai's batch crawl. Generic Playwright network-discovery (tier 2 of
    fetch_jobs_smart) is deliberately skipped here: it launches a browser per
    company, which doesn't scale across a large batch the way it's fine to
    do for a single company in _fetch_career_page_crawl4ai.

    No ATS-listing expansion for the crawl4ai remainder (unlike
    _fetch_career_page_crawl4ai) — this path is for fanning out across many
    companies fast, not a deep single-page crawl.
    """
    from crawl4ai import CacheMode, CrawlerRunConfig
    from crawl4ai.async_dispatcher import MemoryAdaptiveDispatcher

    jobs_by_company: dict[str, list[dict]] = {}
    scrape_needed: list[dict] = []
    for c in companies:
        url, name = c.get("url"), c.get("name")
        if not url:
            continue
        ats = _detect_ats_platform(url)
        api_jobs = await fetch_jobs_from_known_ats(url, name, ats) if ats else None
        if api_jobs is not None:
            jobs_by_company[name] = api_jobs
        else:
            scrape_needed.append(c)

    url_to_company = {c["url"]: c["name"] for c in scrape_needed if c.get("url")}
    urls = list(url_to_company.keys())
    if not urls:
        return jobs_by_company

    run_cfg = CrawlerRunConfig(
        cache_mode=CacheMode.BYPASS,
        js_code="await new Promise(r => setTimeout(r, 1500));",
        word_count_threshold=5,
        page_timeout=30000,
    )
    dispatcher = MemoryAdaptiveDispatcher(
        max_session_permit=_CRAWL4AI_MAX_CONCURRENT,
        memory_threshold_percent=80.0,
    )

    crawler = await _get_crawler()
    for name in url_to_company.values():
        jobs_by_company.setdefault(name, [])
    seen_job_urls: dict[str, set] = {name: set() for name in url_to_company.values()}
    visited_pages: dict[str, set] = {name: {u} for u, name in url_to_company.items()}

    round_urls = url_to_company
    for round_num in range(2):
        if not round_urls:
            break
        try:
            results = await crawler.arun_many(urls=list(round_urls.keys()), config=run_cfg, dispatcher=dispatcher)
        except Exception as e:
            _emit_scan_log(f"[CRAWL4AI] Batch crawl failed (round {round_num + 1}): {e}", "error")
            break

        next_round_urls: dict[str, str] = {}
        for res in results:
            name = round_urls.get(res.url)
            if name is None:
                continue
            if not (res.success and res.markdown and res.markdown.raw_markdown):
                continue
            page_jobs = _extract_jobs_from_markdown(res.markdown.raw_markdown, name)
            new_jobs = [j for j in page_jobs if j["url"] not in seen_job_urls[name]]
            if not new_jobs:
                continue
            seen_job_urls[name].update(j["url"] for j in new_jobs)
            jobs_by_company[name].extend(new_jobs)
            if name in next_round_urls:
                continue
            next_link = next(
                (u for u in _find_pagination_links(res.markdown.raw_markdown, res.url) if u not in visited_pages[name]),
                None,
            )
            if next_link:
                visited_pages[name].add(next_link)
                next_round_urls[next_link] = name
        round_urls = next_round_urls

    return jobs_by_company


_NAV_DEBRIS_WORDS = {
    "polski", "english", "deutsch", "français", "francais", "español",
    "espanol", "italiano", "login", "log in", "sign in", "home", "menu",
    "search", "apply now", "back", "back to search results",
}
_NAV_DEBRIS_LANGS = {
    "polski", "english", "deutsch", "français", "francais", "español",
    "espanol", "italiano", "русский", "русский язык", "čeština", "cestina",
    "slovenčina", "slovencina", "magyar", "română", "romana",
}


def _strip_leading_nav_debris(text: str) -> str:
    """Strip a language switcher + login link etc. rendered as their own
    short lines right before the real content — seen verbatim on
    apply.deloittece.com as (once markdown bullet markers are accounted for)
    "* \\n*  English - US \\n*  Login\\n<actual job text>". Removes one
    leading line at a time until a real paragraph (or nothing) is left."""
    lines = text.split("\n")
    start = 0
    while start < len(lines):
        candidate = lines[start].strip().lstrip("*•-").strip().strip(".").lower()
        candidate = re.sub(r"\s*[-(]\s*[a-z]{2}\)?$", "", candidate).strip()
        if not candidate:
            start += 1
        elif len(candidate) < 30 and (candidate in _NAV_DEBRIS_WORDS or candidate in _NAV_DEBRIS_LANGS):
            start += 1
        else:
            break
    return "\n".join(lines[start:]).strip()


_DEAD_PAGE_SIGNATURES = (
    "can't find the page",
    "cannot find the page",
    "page not found",
    "page you requested could not be found",
    "doesn't exist",
    "this page doesn't exist",
)


def _looks_like_dead_page(markdown: str) -> bool:
    """Detect a 404 / removed-listing page rendered as markdown.

    Job links scraped from a company's careers page can go stale between
    collection and the user expanding the card — the posting gets pulled but
    the homepage link isn't cleaned up. Without this check, the 404 page's
    own nav/error copy gets treated as if it were the job description.
    """
    lowered = markdown.lower()
    has_404_marker = bool(re.search(r"\b404\b", lowered))
    has_dead_phrase = any(sig in lowered for sig in _DEAD_PAGE_SIGNATURES)
    return has_404_marker and has_dead_phrase


async def _fetch_job_description_crawl4ai(url: str, title: str = "") -> dict:
    """Fetch a single job description page using crawl4ai."""
    markdown = await _fetch_markdown_crawl4ai(url, wait_for_js=True)

    if not markdown:
        return {"description": "", "skills": []}

    if _looks_like_dead_page(markdown):
        return {"description": "", "skills": [], "unavailable": True}

    title_norm = re.sub(r"[^a-z0-9]", "", title.lower()) if title else ""
    lines = markdown.split("\n")
    content_lines = []
    in_content = False

    for line in lines:
        stripped = line.strip()
        if not stripped:
            if in_content:
                content_lines.append("")
            continue

        if not in_content:
            text_only = re.sub(r"\]\([^)]*\)", "]", stripped)
            text_only = re.sub(r"https?://\S+", "", text_only)
            line_norm = re.sub(r"[^a-z0-9]", "", text_only.lower())
            if title_norm and len(line_norm) >= 6 and (line_norm in title_norm or title_norm in line_norm):
                in_content = True
            elif stripped.startswith("# ") or stripped.startswith("## "):
                in_content = True

        if in_content:
            content_lines.append(stripped)

    description = "\n".join(content_lines)

    if len(description) < 100:
        description = markdown

    description = re.sub(r"!?\[[^\]]*\]\([^)]+\)", "", description)
    description = re.sub(r"\]\([^)]+\)", "", description)
    description = re.sub(r"Back to Search", "", description, flags=re.I)
    description = re.sub(r"apply\s*(Facebook)?\s*(LinkedIn)?\s*Send via email", "", description, flags=re.I)
    description = re.sub(r"Looking for something else\?.*?(?:Find me a job|personalized offer\.?)", "", description, flags=re.I | re.S)
    description = re.sub(r"&\s*\d+\s*others?", "", description, flags=re.I)
    description = re.sub(r"Location-specific conditions\s*&\s*benefits\*?", "", description, flags=re.I)
    description = re.sub(r"Choose an option", "", description, flags=re.I)
    description = re.sub(r"sign in via.*?careers\s*>>\s*", "", description, flags=re.I | re.S)
    description = re.sub(r"contact our recruiter[^\n]*", "", description, flags=re.I)
    if title:
        description = re.sub(rf"({re.escape(title)})(\s*\1\b)+", r"\1", description, flags=re.I)
    description = re.sub(r"\b\w+\s+careers\s*>>\s*", "", description, flags=re.I)

    footer_markers = [
        r"interested in working with us\??.*",
        r"we use cookies.*",
        r"cookie settings.*",
        r"our brands\b.*",
        r"manage cookies.*",
        r"(©|copyright)\s*\d{4}.*",
        r"\d{4}\s*[-–—]\s*\d{4}\.?\s*\S+\s+all rights reserved.*",
        r"(light|dark) mode\s+.*",
        r"apply for [^\n:]*:.*",
        r"share this opening.*",
        r"share this job:?.*",
        r"similar jobs.*",
        r"\bli-[a-z]{1,3}\d+\b",
        r"udostępnij.*",
        r"podobne oferty.*",
        r"sdílet.*",
        r"podobné nabídky.*",
        r"zdieľať.*",
        r"podobné ponuky.*",
        r"megosztás.*",
        r"hasonló állások.*",
        # Locale-switcher footer widget ("Any questions? Follow us." followed
        # by a giant country-selector dropdown list) — seen verbatim across
        # many career sites, not one company's quirk.
        r"any questions\??\s*follow us\b.*",
    ]
    for pattern in footer_markers:
        description = re.sub(pattern, "", description, flags=re.I | re.S)

    description = re.sub(r"#{1,6}\s*", "", description)
    description = re.sub(r"\*{1,2}([^*\n]+)\*{1,2}", r"\1", description)
    description = re.sub(r"\n{3,}", "\n\n", description)
    description = description.strip()

    description = _strip_leading_nav_debris(description)

    return {
        "description": description[:12000],
        "skills": [],
    }




async def _fetch_job_detail(url: str) -> dict:
    """Fetch a single job URL and extract title, description, skills (httpx fallback)."""
    html = await _fetch_html(url)
    if not html:
        return {"title": "", "description": "", "skills": [], "company": ""}

    soup = BeautifulSoup(html, "lxml")
    for tag in soup(["script", "style", "noscript", "svg"]):
        tag.decompose()

    title = ""
    title_el = soup.find("meta", property="og:title")
    if title_el:
        title = title_el.get("content", "")
    if not title:
        h1 = soup.find("h1")
        if h1:
            title = _clean_text(h1.get_text())
    if not title:
        title_tag = soup.find("title")
        if title_tag:
            title = _clean_text(title_tag.get_text())

    desc = ""
    desc_el = soup.find("meta", attrs={"name": "description"})
    if desc_el:
        desc = desc_el.get("content", "")
    if not desc:
        desc_selectors = [
            "[class*='job-description']",
            "[class*='description']",
            "[id*='description']",
            "article",
        ]
        for sel in desc_selectors:
            el = soup.select_one(sel)
            if el:
                desc = _clean_text(el.get_text(), max_len=3000)
                break

    skills = []
    text = soup.get_text()
    skill_patterns = [
        r"(?:skills?|requirements?|qualifications?|technologies?)[\s:]+([^\n]+)",
    ]
    for pattern in skill_patterns:
        match = re.search(pattern, text, re.I)
        if match:
            skills = [s.strip() for s in re.split(r"[,•·]", match.group(1)) if s.strip()][:10]
            break

    company = ""
    company_el = soup.find("meta", attrs={"name": "author"})
    if company_el:
        company = company_el.get("content", "")

    return {
        "title": title[:200],
        "description": desc[:3000],
        "skills": skills,
        "company": company[:120],
    }




async def _enrich_single_job(job: dict) -> bool:
    """Enrich a single job with description. Returns True if successful."""
    url = job.get("url", "")
    if not url:
        return False

    try:
        detail = await _fetch_job_detail(url)
        if detail.get("description") and len(detail["description"]) > 50:
            job["description"] = detail["description"]
            return True

        detail = await _fetch_job_description_crawl4ai(url)
        if detail.get("description"):
            job["description"] = detail["description"]
            return True
    except Exception as e:
        _emit_scan_log(f"[ENRICH] Error fetching {url}: {e}", "error")
    return False


async def _enrich_jobs_with_descriptions(jobs: list[dict], max_jobs: int = 30) -> list[dict]:
    """Fetch full job descriptions for jobs that don't have them yet.

    Uses crawl4ai with concurrency limit for speed.
    """
    jobs_needing = [j for j in jobs if not j.get("description")]
    if not jobs_needing:
        return jobs

    limit = min(len(jobs_needing), max_jobs)
    _emit_scan_log(f"[ENRICH] Fetching descriptions for {limit} jobs (concurrent)...")

    sem = asyncio.Semaphore(8)

    async def limited_fetch(job):
        async with sem:
            return await _enrich_single_job(job)

    results = await asyncio.gather(
        *[limited_fetch(j) for j in jobs_needing[:limit]],
        return_exceptions=True,
    )

    enriched = sum(1 for r in results if r is True)
    _emit_scan_log(f"[ENRICH] Got descriptions for {enriched}/{limit} jobs")
    return jobs



MIN_JOB_SCORE = 30


def _deterministic_score(job: dict, user_profile: dict) -> tuple[int, str]:
    """Keyword-based scoring when LLM is unavailable. Returns (score, reason)."""
    import re as _re

    title = job.get("title", "").lower()
    desc = job.get("description", "").lower()
    combined = f"{title} {desc}"

    target_titles = [t.lower() for t in user_profile.get("jobTitles", [])]
    skills = [s.lower() for s in user_profile.get("skills", [])]
    experience = user_profile.get("experienceLevel", "").lower()
    work_types = [w.lower() for w in user_profile.get("workTypes", [])]
    onsite_cities = [c.lower() for c in user_profile.get("onsiteCities", [])]

    score = 0
    reasons = []

    if target_titles:
        for t in target_titles:
            title_words = t.split()
            matched_words = sum(1 for w in title_words if _re.search(r'\b' + _re.escape(w) + r'\b', title))
            if matched_words == len(title_words):
                score += 40
                reasons.append(f"Title matches '{t}'")
                break
            elif matched_words > 0:
                score += 20 * matched_words // len(title_words)
                reasons.append(f"Partial title match '{t}'")
                break

    if skills:
        matched_skills = [s for s in skills if _re.search(r'\b' + _re.escape(s) + r'\b', combined)]
        if matched_skills:
            skill_score = min(30, len(matched_skills) * 10)
            score += skill_score
            reasons.append(f"Skills: {', '.join(matched_skills[:3])}")

    level_keywords = {
        "intern": ["intern", "internship", "trainee", "student"],
        "junior": ["junior", "entry level", "entry-level", "associate", "graduate", "0-2 years", "0-1 year"],
        "mid": ["mid level", "mid-level", "intermediate", "2-5 years", "3-5 years"],
        "senior": ["senior", "lead", "principal", "staff", "5+ years"],
    }
    if experience:
        for level_key, keywords in level_keywords.items():
            if level_key in experience:
                if any(_re.search(r'\b' + _re.escape(kw) + r'\b', combined) for kw in keywords):
                    score += 20
                    reasons.append(f"Experience level match ({level_key})")
                break

    senior_only = ["senior", "staff", "principal", "director", "vp ", "head of"]
    if experience in ("intern", "junior"):
        if any(_re.search(r'\b' + _re.escape(s) + r'\b', title) for s in senior_only):
            if not any(_re.search(r'\b' + _re.escape(s) + r'\b', combined) for s in ["junior", "entry", "intern"]):
                score = max(5, score - 30)
                reasons.append("Senior-level role, too advanced")

    if any(w in combined for w in ["startup", "early-stage", "seed"]):
        if experience in ("intern", "junior"):
            score += 10
            reasons.append("Startup (good for juniors)")

    if work_types and "internship" in work_types and "full_time" not in work_types:
        if _re.search(r'\bfull[\s-]?time\b|\bpermanent\b', combined) and "intern" not in combined:
            score = max(5, score - 40)
            reasons.append("Full-time role, candidate wants internship only")

    if onsite_cities:
        is_remote = bool(_re.search(r'\bremote\b', combined))
        job_location = job.get("location", "").lower()
        if not is_remote and job_location and not any(city in job_location for city in onsite_cities):
            score = max(5, score - 30)
            reasons.append(f"On-site in {job.get('location', '?')}, not candidate's city")

    score = max(0, min(100, score))
    reason = "; ".join(reasons) if reasons else "Low relevance to target profile"
    return score, reason


def _filter_jobs_by_titles(jobs: list[dict], target_titles: list[str]) -> list[dict]:
    """Filter and boost jobs matching target titles. Return filtered + scored list."""
    import re as _re

    if not target_titles:
        return jobs

    stop_words = {"a", "an", "the", "in", "of", "for", "and", "or", "to", "at"}
    title_keywords = set()
    for t in target_titles:
        for word in t.lower().split():
            if len(word) > 2 and word not in stop_words:
                title_keywords.add(word)

    if not title_keywords:
        return jobs

    filtered = []
    for job in jobs:
        job_title = job.get("title", "").lower()
        matching_words = sum(
            1 for w in title_keywords
            if _re.search(r'\b' + _re.escape(w) + r'\b', job_title)
        )
        if matching_words > 0:
            job["_title_relevance"] = matching_words / len(title_keywords)
            filtered.append(job)

    filtered.sort(key=lambda j: j.get("_title_relevance", 0), reverse=True)
    return filtered


async def _score_jobs_with_llm(jobs: list[dict], user_profile: dict, provider: str = "lmstudio") -> list[dict]:
    """Score jobs against user profile. Uses LLM if available, falls back to deterministic scoring."""
    if not jobs:
        return []

    titles = user_profile.get("jobTitles", [])
    skills = user_profile.get("skills", [])
    experience = user_profile.get("experienceLevel", "")
    work_types = user_profile.get("workTypes", [])
    onsite_cities = user_profile.get("onsiteCities", [])

    user_summary_parts = []
    if titles:
        user_summary_parts.append(f"Target roles: {', '.join(titles)}")
    if skills:
        user_summary_parts.append(f"Skills: {', '.join(skills[:15])}")
    if experience:
        user_summary_parts.append(f"Level: {experience}")
    if work_types:
        user_summary_parts.append(f"Accepted employment types: {', '.join(work_types)}")
    if onsite_cities:
        user_summary_parts.append(f"Wants on-site work in: {', '.join(onsite_cities)} (remote is fine from anywhere)")
    user_summary = "\n".join(user_summary_parts)

    employment_type_rule = ""
    if work_types and "internship" in work_types and "full_time" not in work_types:
        employment_type_rule = (
            "\nThe candidate ONLY wants internships, not full-time/permanent roles. "
            "If a job's Employment Type is Full time / permanent and it is not explicitly "
            "an internship, score it below 20 even if the title says 'New Grad' or 'Junior'."
        )

    location_rule = ""
    if onsite_cities:
        location_rule = (
            f"\nThe candidate only wants ON-SITE roles in: {', '.join(onsite_cities)}. "
            "If a job is on-site (not remote) and its Location does not match any of these "
            "cities, score it below 20. Remote roles are fine regardless of location."
        )

    llm_available = False
    try:
        from careertwin.llm import call_ai, parse_json_object
        raw = await call_ai(
            "Return the word 'ping' only.",
            "ping",
            provider=provider,
            model="",
            max_tokens=200,
            temperature=0.0,
        )
        if raw and "ping" in raw.lower():
            llm_available = True
            _emit_scan_log("[SCORE] LLM available, using AI scoring")
        else:
            _emit_scan_log("[SCORE] LLM not responding, using keyword scoring")
    except Exception:
        _emit_scan_log("[SCORE] LLM unavailable, using keyword scoring")

    if not llm_available:
        for job in jobs:
            score, reason = _deterministic_score(job, user_profile)
            job["score"] = score
            job["score_reason"] = reason
        return jobs

    from careertwin.llm import call_ai, parse_json_object

    batches = [jobs[i:i + 10] for i in range(0, len(jobs), 10)]
    sem = asyncio.Semaphore(4)

    async def _score_batch(batch: list[dict]) -> list[dict]:
        batch_text = "\n".join(
            f"[{j}] {job['title']} at {job.get('company', '?')} — {job.get('url', '')}\n"
            f"   Location: {job.get('location', 'unknown')}\n"
            f"   Description: {job.get('description', '')[:800]}"
            for j, job in enumerate(batch)
        )

        system_prompt = f"""You are a job matching engine for ENTRY-LEVEL candidates (interns, juniors, recent graduates).
Score each job 0-100 based on how well it matches the candidate's profile.
Focus on: title relevance, skill overlap, experience level compatibility.
Return a JSON object with key "scores" mapping each job index (as string) to {{"score": number, "reason": "one sentence"}}.
Skip jobs that are clearly irrelevant (e.g. senior roles for intern candidates).{employment_type_rule}{location_rule}"""

        user_msg = f"""Candidate profile:
{user_summary}

Jobs to score:
{batch_text}

Return JSON: {{"scores": {{"0": {{"score": 85, "reason": "..."}}}}}}"""

        try:
            async with sem:
                raw = await call_ai(system_prompt, user_msg, provider=provider, model="", max_tokens=4000)
            _emit_scan_log(f"[SCORE] Raw response length: {len(raw)} chars")
            result = parse_json_object(raw)
            if result and "scores" in result:
                scores = result["scores"]
                for idx_str, score_data in scores.items():
                    idx = int(idx_str)
                    if 0 <= idx < len(batch):
                        batch[idx]["score"] = score_data.get("score", 50)
                        batch[idx]["score_reason"] = score_data.get("reason", "")
                for idx in range(len(batch)):
                    if "score" not in batch[idx]:
                        batch[idx]["score"] = 15
                        batch[idx]["score_reason"] = "Not matched by LLM"
            else:
                for job in batch:
                    score, reason = _deterministic_score(job, user_profile)
                    job["score"] = score
                    job["score_reason"] = reason
        except Exception as e:
            _emit_scan_log(f"[SCORE] Error: {e}", "error")
            for job in batch:
                score, reason = _deterministic_score(job, user_profile)
                job["score"] = score
                job["score_reason"] = reason
        return batch

    results = await asyncio.gather(*(_score_batch(b) for b in batches))
    return [job for batch in results for job in batch]




async def generate_job_application(
    job: dict,
    user_profile: dict,
    user_resume: dict | None = None,
) -> dict:
    """Generate a tailored application response for a job using AI.

    Returns dict with:
      - cover_letter: tailored cover letter
      - answers: dict of question -> answer for application forms
      - resume_highlights: relevant experience to highlight
    """
    from careertwin.llm import call_ai, parse_json_object

    job_title = job.get("title", "")
    company = job.get("company", "")
    description = job.get("description", "")

    user_context = f"Name: {user_profile.get('firstName', '')} {user_profile.get('lastName', '')}\n"
    user_context += f"Target: {job_title}\n"

    if user_profile.get("jobTitles"):
        user_context += f"Desired roles: {', '.join(user_profile['jobTitles'])}\n"
    if user_profile.get("skills"):
        user_context += f"Skills: {', '.join(user_profile['skills'][:20])}\n"
    if user_profile.get("experienceLevel"):
        user_context += f"Experience: {user_profile['experienceLevel']}\n"
    salary = user_profile.get("salary") or {}
    if salary.get("amount"):
        user_context += f"Salary expectation: {salary['amount']} {salary.get('currency', '')}\n"
    if user_profile.get("onsiteCities"):
        user_context += f"Open to on-site in: {', '.join(user_profile['onsiteCities'])}\n"
    if user_profile.get("remoteCities"):
        user_context += f"Open to remote from: {', '.join(user_profile['remoteCities'])}\n"

    if user_resume:
        if user_resume.get("summary"):
            user_context += f"Summary: {user_resume['summary'][:500]}\n"
        if user_resume.get("experience"):
            user_context += "Experience:\n"
            for exp in user_resume["experience"][:5]:
                user_context += f"  - {exp.get('title', '')} at {exp.get('company', '')} ({exp.get('duration', '')})\n"
                if exp.get("description"):
                    user_context += f"    {exp['description'][:200]}\n"
        if user_resume.get("education"):
            user_context += "Education:\n"
            for edu in user_resume["education"][:3]:
                user_context += f"  - {edu.get('degree', '')} from {edu.get('school', '')}\n"

    system_prompt = f"""You are a senior technical recruiter writing an application PACKAGE on behalf of a candidate, \
with one goal: get this candidate an interview. Write the way a recruiter who has placed hundreds of \
candidates would — not the way an AI assistant would.

CRITICAL: Detect the language of the job description below. Write the ENTIRE response (cover letter, all answers, everything) in the SAME language as the job description. If the job is in German, write in German. If in Japanese, write in Japanese. If in English, write in English. Match the language exactly.

Company: {company}
Job Title: {job_title}

Job Description:
{description[:2000]}

Hiring rules (violating these gets the application rejected by the ATS or the recruiter):
- Mirror the exact keywords and technologies from the job description wherever the candidate genuinely has \
them — ATS keyword matching is real and this is the single highest-leverage thing you can do.
- Every claim must be traceable to something in the candidate profile below. Never invent employers, titles, \
dates, tools, or numbers that aren't there.
- Quantify wherever the profile gives you a number (scale, team size, time saved, users, percentage). If the \
profile has no number for a bullet, describe scope and outcome concretely instead of inventing a fake metric.
- Lead every bullet and every paragraph with what the candidate DID and the result — not with soft traits \
like "hardworking" or "passionate."
- Zero filler language: no "leverage," "elevate," "seamless," "dynamic," "results-driven," "team player," \
"go-getter," "synergy," "unlock potential." A senior recruiter would strike every one of these on sight.
- The cover letter opens with a specific, concrete reason tied to this company/role, not "I am writing to \
apply for the position of X."
- The cover letter MUST end with a proper professional closing line (the local-language equivalent of \
"Best regards,") followed by the candidate's full name on its own line. A letter that stops after the last \
argument with no sign-off reads as unfinished and gets rejected by both ATS parsers and human reviewers.

Generate a JSON response with:
1. "cover_letter": 3-4 tight paragraphs, no generic filler, ending with a closing + full name, written in the SAME language as the job description
2. "resume_highlights": 3-5 bullets, each one outcome-first and quantified where the profile supports it — \
these should read like lines from a resume a senior recruiter rewrote, not a summary of duties"""

    user_msg = f"""Candidate profile:
{user_context}

IMPORTANT: The job description is written in a specific language. Generate ALL application materials in that SAME language.
Generate the application materials as JSON."""

    try:
        raw = await call_ai(system_prompt, user_msg, provider="lmstudio", model="", max_tokens=8000)
        result = parse_json_object(raw)

        if result:
            cover_letter = result.get("cover_letter", "")

            full_name = f"{user_profile.get('firstName', '')} {user_profile.get('lastName', '')}".strip()
            if cover_letter and full_name and not re.search(
                r"(regards|sincerely|best|kind regards|cordialement|mit freundlichen|atenciosamente)",
                cover_letter[-120:],
                re.I,
            ):
                cover_letter = f"{cover_letter.rstrip()}\n\nBest regards,\n{full_name}"

            pdf_url = ""
            if cover_letter:
                try:
                    import uuid

                    from careertwin.services.resume import _render_cover_letter_pdf
                    from careertwin.settings import OUTPUT_DIR

                    pdf_name = f"cover-letter-{uuid.uuid4().hex[:8]}.pdf"
                    pdf_path = OUTPUT_DIR / pdf_name
                    _render_cover_letter_pdf(
                        output_path=pdf_path,
                        full_name=user_profile.get("firstName", "") + " " + user_profile.get("lastName", ""),
                        email=user_profile.get("email", ""),
                        phone=user_profile.get("phone", ""),
                        company=company,
                        job_title=job_title,
                        language_label="",
                        body=cover_letter,
                    )
                    pdf_url = f"/outputs/{pdf_name}"
                except Exception as e:
                    _emit_scan_log(f"[APPLY] PDF generation error: {e}", "error")

            return {
                "cover_letter": cover_letter,
                "resume_highlights": result.get("resume_highlights", []),
                "pdf_url": pdf_url,
            }
    except Exception as e:
        _emit_scan_log(f"[APPLY] Error generating application: {e}", "error")

    return {
        "cover_letter": "",
        "resume_highlights": [],
        "pdf_url": "",
    }


async def auto_apply_to_jobs(
    jobs: list[dict],
    user_profile: dict,
    user_resume: dict | None = None,
    max_applications: int = 10,
) -> list[dict]:
    """Generate application materials for top-scoring jobs.

    Returns list of dicts with job info + application materials.
    """
    scored_jobs = list(jobs)
    scored_jobs.sort(key=lambda x: x.get("score", 50), reverse=True)

    results = []
    for job in scored_jobs[:max_applications]:
        _emit_scan_log(f"[APPLY] Generating application for: {job.get('title', '')} at {job.get('company', '')}")

        application = await generate_job_application(job, user_profile, user_resume)
        results.append({
            "job": job,
            "application": application,
        })

        await asyncio.sleep(1)

    return results




async def scan_all_companies(
    user_profile: dict,
    *,
    region: str | None = None,
    max_companies: int = 30,
    max_jobs: int = 100,
    provider: str = "lmstudio",
    extra_companies: list[str] | None = None,
) -> dict:
    """Scan multiple company career pages using crawl4ai and return ranked matches.

    `extra_companies` lets callers add company names outside the curated
    companies.json list (e.g. "EPAM", "Sii") — each name is resolved to a
    careers URL via web search (see discover_careers_url).
    """
    start = time.time()
    reset_scan_log()

    all_jobs: list[dict] = []
    scanned_sources: list[str] = []

    target_titles = user_profile.get("jobTitles", [])
    if target_titles:
        _emit_scan_log(f"[FILTER] Targeting: {', '.join(target_titles)}")
    else:
        _emit_scan_log("[FILTER] No target titles — showing all jobs")

    companies = load_companies(region)[:max_companies]

    if extra_companies:
        known_names = {c["name"].lower() for c in companies}
        names_to_resolve = [
            n for n in extra_companies
            if n.strip() and n.strip().lower() not in known_names
        ]
        if names_to_resolve:
            _emit_scan_log(f"[DISCOVER] Resolving careers pages for {len(names_to_resolve)} extra companies...")
            resolved = await asyncio.gather(
                *(discover_careers_url(n) for n in names_to_resolve)
            )
            for name, url in zip(names_to_resolve, resolved):
                if url:
                    companies.append({"name": name, "careers_url": url, "region": region or "custom"})
                else:
                    _emit_scan_log(f"[DISCOVER] Skipping {name!r}: no careers page found", "warn")

    _emit_scan_log(
        f"[SCAN] Scanning {len(companies)} companies concurrently "
        f"(up to {_CRAWL4AI_MAX_CONCURRENT} in flight)..."
    )

    async def _scan_one(company: dict) -> tuple[str, list[dict]]:
        name = company["name"]
        url = company["careers_url"]
        _emit_scan_log(f"[FETCH]... {name} ({url})")
        try:
            jobs = await _fetch_career_page_crawl4ai(url, name)
        except Exception as e:
            _emit_scan_log(f"[SCAN] Error scanning {name}: {e}", "error")
            return name, []

        if not jobs:
            _emit_scan_log(f"[SCAN] {name}: 0 jobs found")
            return name, []

        if target_titles:
            filtered = _filter_jobs_by_titles(jobs, target_titles)
            if filtered:
                _emit_scan_log(f"[SCAN] {name}: {len(filtered)}/{len(jobs)} matching jobs")
                return name, filtered
            _emit_scan_log(f"[SCAN] {name}: {len(jobs)} jobs found, 0 match titles")
            return name, []

        _emit_scan_log(f"[SCAN] {name}: {len(jobs)} jobs")
        return name, jobs

    results = await asyncio.gather(*(_scan_one(c) for c in companies))
    for name, jobs in results:
        if jobs:
            all_jobs.extend(jobs)
            scanned_sources.append(name)

    onsite_cities = user_profile.get("onsiteCities", [])
    if onsite_cities:
        cities = onsite_cities[:2]
        work_types = [w.lower() for w in user_profile.get("workTypes", [])]
        wants_internship = "internship" in work_types and "full_time" not in work_types

        queries = list(target_titles[:2]) if target_titles else ["software engineer"]
        if wants_internship:
            queries = [f"{q} intern" for q in queries]

        _emit_scan_log(f"[INDEED] Searching {len(queries)} queries x {len(cities)} cities...")
        indeed_results = await asyncio.gather(
            *(_search_indeed_jobs(q, city) for q in queries for city in cities)
        )
        for jobs in indeed_results:
            if target_titles:
                jobs = _filter_jobs_by_titles(jobs, target_titles)
            all_jobs.extend(jobs)

    _emit_scan_log(f"[SCAN] Total before dedup: {len(all_jobs)}")

    from careertwin.jobs.scraper import deduplicate_jobs
    all_jobs = deduplicate_jobs(all_jobs)
    _emit_scan_log(f"[SCAN] After dedup: {len(all_jobs)}")

    if all_jobs:
        for j in all_jobs:
            pre_score, _ = _deterministic_score(j, user_profile)
            j["_pre_score"] = pre_score
        all_jobs.sort(key=lambda x: x["_pre_score"], reverse=True)
        for j in all_jobs:
            j.pop("_pre_score", None)

    all_jobs = await _enrich_jobs_with_descriptions(all_jobs, max_jobs=max_jobs)

    LLM_SCORE_LIMIT = 20

    if all_jobs:
        scan_limit = min(len(all_jobs), max_jobs)
        candidates = all_jobs[:scan_limit]

        for j in candidates:
            det_score, det_reason = _deterministic_score(j, user_profile)
            j["_det_score"] = det_score
            j["_det_reason"] = det_reason
        candidates.sort(key=lambda x: x["_det_score"], reverse=True)

        llm_candidates = candidates[:LLM_SCORE_LIMIT]
        rest = candidates[LLM_SCORE_LIMIT:]

        _emit_scan_log(
            f"[SCORE] LLM-scoring top {len(llm_candidates)} candidates, "
            f"deterministic for remaining {len(rest)}..."
        )
        scored_llm = await _score_jobs_with_llm(llm_candidates, user_profile, provider=provider)
        for j in rest:
            j["score"] = j.pop("_det_score")
            j["score_reason"] = j.pop("_det_reason")
        for j in scored_llm:
            j.pop("_det_score", None)
            j.pop("_det_reason", None)

        all_jobs = scored_llm + rest
        all_jobs.sort(key=lambda x: x.get("score", 0), reverse=True)

        before = len(all_jobs)
        all_jobs = [j for j in all_jobs if j.get("score", 0) >= MIN_JOB_SCORE]
        rejected = before - len(all_jobs)
        if rejected:
            _emit_scan_log(f"[FILTER] Rejected {rejected} low-relevance jobs (score < {MIN_JOB_SCORE})")
        _emit_scan_log(f"[DONE] Scan complete: {len(all_jobs[:max_jobs])} jobs returned")

    return {
        "jobs": all_jobs[:max_jobs],
        "scanned_companies": scanned_sources,
        "total_found": len(all_jobs),
        "scan_time_seconds": round(time.time() - start, 1),
    }
