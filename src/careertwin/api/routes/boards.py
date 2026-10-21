"""Jobs MCP endpoint — unified job aggregation from 10+ ATS platforms."""

from __future__ import annotations

import asyncio
import logging
import re

from fastapi import APIRouter
from pydantic import BaseModel, Field

from careertwin.db import db_conn
from careertwin.jobs.boards import (
    ADAPTERS,
    detect_ats_from_url,
    search_all_companies,
    search_by_query,
    search_company_jobs,
)
from careertwin.jobs.boards.base import is_entry_level_title, is_internship_title
from careertwin.jobs.seniority import is_senior_or_mid

logger = logging.getLogger(__name__)
router = APIRouter(tags=["jobs-mcp"])

MIN_MATCH_SCORE = 80


class MCPSearchRequest(BaseModel):
    query: str = ""
    companies: list[str] = Field(default_factory=list)
    sources: list[str] = Field(default_factory=list)
    intern_only: bool = False
    limit: int = 50


class JobResult(BaseModel):
    source: str
    company: str
    title: str
    url: str
    location: str = ""
    description: str = ""
    employment_type: str = ""
    is_intern: bool = False
    match_score: float = 0
    matching_skills: list[str] = Field(default_factory=list)
    missing_skills: list[str] = Field(default_factory=list)


class MCPSearchResponse(BaseModel):
    jobs: list[JobResult]
    total: int
    sources_queried: list[str]


def _strip_html(text: str) -> str:
    """Remove HTML tags, preserving block structure for section parsing."""
    import html as _html
    import re
    clean = _html.unescape(text)
    clean = re.sub(r'<style[^>]*>.*?</style>', '', clean, flags=re.I | re.S)
    clean = re.sub(r'<script[^>]*>.*?</script>', '', clean, flags=re.I | re.S)
    clean = re.sub(r'<br\s*/?>', '\n', clean, flags=re.I)
    clean = re.sub(r'</(p|div|section|article)>', '\n\n', clean, flags=re.I)
    clean = re.sub(r'</li>', '\n', clean, flags=re.I)
    clean = re.sub(r'<li[^>]*>', '\n• ', clean, flags=re.I)
    clean = re.sub(r'<h[1-6][^>]*>', '\n## ', clean, flags=re.I)
    clean = re.sub(r'</h[1-6]>', '\n', clean, flags=re.I)
    clean = re.sub(r'<strong>([^<]+)</strong>', r'\1', clean, flags=re.I)
    clean = re.sub(r'<[^>]+>', '', clean)

    clean = re.sub(r'Back to Search', '', clean, flags=re.I)
    clean = re.sub(r'apply\s*(Facebook)?\s*(LinkedIn)?\s*Send via email', '', clean, flags=re.I)
    clean = re.sub(
        r"Looking for something else\?.*?Find me a job",
        '',
        clean,
        flags=re.I | re.S,
    )
    clean = re.sub(r'&\s*\d+\s*others?', '', clean, flags=re.I)
    clean = re.sub(r'Location-specific conditions\s*&\s*benefits\*?', '', clean, flags=re.I)
    clean = re.sub(r'Choose an option', '', clean, flags=re.I)

    clean = re.sub(r'\n{3,}', '\n\n', clean).strip()

    from careertwin.jobs.scanner import _strip_leading_nav_debris

    return _strip_leading_nav_debris(clean)


def _persist_collected_jobs(jobs: list[JobResult]) -> None:
    """Save MCP search results to `collected_jobs` so they survive a page
    refresh. Without this, the results lived only in the browser's memory —
    reloading the page called GET /jobs/collected, which fell back to a
    years-old legacy table (13k+ stale rows from before the student-focused
    redesign) instead of showing what was just searched for.

    Untouched rows from a *previous* search are cleared first so results
    don't get diluted by old, unrelated jobs, but anything the user actually
    acted on (applied/skipped/failed) is left alone.
    """
    import hashlib
    import json as _json
    from datetime import datetime, timezone

    now = datetime.now(timezone.utc).isoformat()
    with db_conn() as conn:
        conn.execute("DELETE FROM collected_jobs WHERE status = 'new'")
        for j in jobs:
            job_id = hashlib.sha1(j.url.encode()).hexdigest()[:16]
            conn.execute(
                "INSERT OR IGNORE INTO collected_jobs "
                "(id, source, url, title, company, location, description, "
                "match_score, matching_skills, missing_skills, collected_at, status) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'new')",
                (
                    job_id, j.source, j.url, j.title, j.company, j.location, j.description,
                    j.match_score, _json.dumps(j.matching_skills), _json.dumps(j.missing_skills),
                    now,
                ),
            )
        conn.commit()


def _score_mcp_jobs(jobs: list, query: str = "") -> list[JobResult]:
    """Score MCP jobs against user's knowledge graph and onboarding preferences.

    `query` (the searched title, e.g. "Python") is enforced as a hard title
    filter — company-wide ATS searches return every open role regardless of
    the query, and skill-overlap scoring alone let unrelated roles (e.g. an
    auditor posting that happens to share a couple of generic skill words)
    cross the match threshold.
    """
    import json as _json

    from careertwin.agents.job_matcher import JobMatcher
    from careertwin.graph.schema import CareerGraph, Experience, Profile, Skill, SkillCategory

    matcher = JobMatcher()

    profile_data = {}
    experience_list = []
    preferences = {}
    with db_conn() as conn:
        row = conn.execute("SELECT full_name, summary, skills FROM user_profiles WHERE username = 'default'").fetchone()
        if row:
            profile_data = {"full_name": row[0], "summary": row[1], "skills": row[2]}
        row = conn.execute("SELECT profile, preferences FROM studio_sessions WHERE username = 'default'").fetchone()
        if row:
            if row[0]:
                try:
                    sp = _json.loads(row[0])
                    if not profile_data.get("full_name"):
                        profile_data["full_name"] = sp.get("fullName", "")
                    if not profile_data.get("summary"):
                        profile_data["summary"] = sp.get("summary", "")
                    for exp in sp.get("experience", []):
                        if isinstance(exp, dict):
                            experience_list.append(Experience(
                                position=exp.get("role", exp.get("position", exp.get("title", ""))),
                                company=exp.get("company", ""),
                                description=exp.get("description", ""),
                            ))
                except Exception:
                    pass
            if row[1]:
                try:
                    preferences = _json.loads(row[1]) if isinstance(row[1], str) else row[1]
                except Exception:
                    pass

    profile_skills = profile_data.get("skills", "")
    if isinstance(profile_skills, str):
        try:
            profile_skills = _json.loads(profile_skills)
        except Exception:
            profile_skills = [s.strip() for s in profile_skills.split(",") if s.strip()]
    if not isinstance(profile_skills, list):
        profile_skills = []
    profile_skills = [s for s in profile_skills if s]

    summary = profile_data.get("summary", "") or profile_data.get("headline", "")
    graph = CareerGraph(
        profile=Profile(
            full_name=profile_data.get("full_name", ""),
            summary=summary,
        ),
        skills=[Skill(name=s, category=SkillCategory.TOOL) for s in profile_skills],
        experience=experience_list,
    )

    user_exp_level = (preferences.get("experienceLevel", "") or "").lower()
    user_cities = [c.lower() for c in (preferences.get("onsiteCities", []) or [])]
    user_work_modes = [m.lower() for m in (preferences.get("workMode", []) or [])]
    user_job_titles = [t.lower() for t in (preferences.get("jobTitles", []) or [])]

    stop_words = {"a", "an", "the", "in", "of", "for", "and", "or", "to", "at", "intern", "internship"}
    generic_role_words = {
        "developer", "engineer", "manager", "analyst", "specialist",
        "designer", "architect", "consultant", "scientist", "coordinator",
        "associate", "assistant",
    }
    query_keywords = [
        w for w in re.findall(r"[a-z0-9+#]+", query.lower())
        if len(w) > 1 and w not in stop_words
    ]
    distinctive_keywords = [w for w in query_keywords if w not in generic_role_words]
    relevance_keywords = distinctive_keywords or query_keywords

    # Broader than distinctive_keywords, but still tech-scoped — used only to
    # relax the relevance check for internship/entry-level titles, which
    # often say "Software Engineer Intern" or "Research Scientist/Engineer"
    # instead of repeating the exact target title. Deliberately excludes
    # fully generic words like "associate"/"consultant"/"coordinator" so
    # "Brand Associate Internship" or "CEO Founder Associate" still don't
    # qualify just for being internships.
    tech_relevance_words = {
        "engineer", "engineering", "developer", "software", "programmer",
        "programming", "backend", "frontend", "fullstack", "full-stack",
        "coder", "coding",
    }

    scored = []
    for job in jobs:
        description = f"{job.title} {_strip_html(job.description or '')}"
        job_lower = (job.title + " " + (job.description or "")).lower()
        job_location = (job.location or "").lower()
        title_lower = job.title.lower()
        is_internship = is_internship_title(job_lower)
        is_entry = is_entry_level_title(job_lower)

        matches_relevance = not relevance_keywords or any(
            re.search(r"\b" + re.escape(kw) + r"\b", title_lower) for kw in relevance_keywords
        )
        matches_tech_adjacent = (is_internship or is_entry) and any(
            re.search(r"\b" + re.escape(w) + r"\b", title_lower) for w in tech_relevance_words
        )
        if not matches_relevance and not matches_tech_adjacent:
            continue

        # A handful of generic career-page crawls pick up decorative image
        # alt-text or captions instead of a real listing (e.g. a stock photo
        # description like "Multiracial colleagues... coffee break..."). Real
        # job titles don't run this long.
        if len(job.title) > 120:
            continue

        if is_senior_or_mid(description):
            continue

        try:
            match = matcher.match(graph, description)
            base_score = match.match_score

            adjustment = 0

            if user_exp_level:
                job_is_intern = is_internship
                job_is_entry = is_entry or any(w in job_lower for w in ["associate"])
                job_is_mid = any(w in job_lower for w in ["middle", "mid-level"])
                job_is_senior = any(w in job_lower for w in ["senior", "sr.", "lead", "principal", "staff"])

                if user_exp_level in ("internship", "entry", "first_role"):
                    if job_is_intern:
                        adjustment += 15
                    elif job_is_entry:
                        adjustment += 10
                    elif job_is_mid:
                        adjustment -= 5
                    elif job_is_senior:
                        adjustment -= 15
                elif user_exp_level == "junior":
                    if job_is_intern:
                        adjustment += 5
                    elif job_is_entry:
                        adjustment += 10
                    elif job_is_mid:
                        adjustment += 5
                    elif job_is_senior:
                        adjustment -= 10
                elif user_exp_level in ("mid", "experienced"):
                    if job_is_mid:
                        adjustment += 10
                    elif job_is_senior:
                        adjustment += 5
                elif user_exp_level in ("senior", "lead", "executive"):
                    if job_is_senior:
                        adjustment += 15
                    elif job_is_mid:
                        adjustment += 5

            if user_cities:
                job_city_match = any(city in job_location or city in job_lower for city in user_cities)
                if job_city_match:
                    adjustment += 10
                if "remote" in user_work_modes:
                    if any(w in job_lower for w in ["remote", "worldwide"]):
                        adjustment += 5

            if user_work_modes:
                job_hybrid = "hybrid" in job_lower
                job_remote = "remote" in job_lower
                job_onsite = "onsite" in job_lower or "on-site" in job_lower or "in-office" in job_lower
                if job_remote and "remote" in user_work_modes:
                    adjustment += 5
                elif job_hybrid and "hybrid" in user_work_modes:
                    adjustment += 5
                elif job_onsite and "onsite" in user_work_modes:
                    adjustment += 5

            if user_job_titles:
                title_match = any(ut in job_lower for ut in user_job_titles)
                if title_match:
                    adjustment += 5

            final_score = max(0, min(base_score + adjustment, 100))

            if (is_internship or is_entry) and user_exp_level in ("internship", "entry", "first_role") and final_score < 40:
                final_score = 40

            scored.append(JobResult(
                source=job.source,
                company=job.company,
                title=job.title,
                url=job.url,
                location=job.location,
                description=_strip_html((job.description or "")[:4000]),
                employment_type=job.employment_type,
                is_intern=is_internship,
                match_score=round(final_score, 1),
                matching_skills=match.matching_skills,
                missing_skills=match.missing_skills,
            ))
        except Exception:
            scored.append(JobResult(
                source=job.source,
                company=job.company,
                title=job.title,
                url=job.url,
                location=job.location,
                description=_strip_html((job.description or "")[:4000]),
                employment_type=job.employment_type,
                is_intern=is_internship,
            ))

    # Internships AND explicitly no-experience-required roles (entry level,
    # junior, graduate, "no experience") are exempt from MIN_MATCH_SCORE —
    # the keyword-overlap scorer rarely clears a high bar for these postings
    # (thin descriptions, little professional-experience overlap), so a
    # strict threshold silently hid them even though relevance/seniority
    # filters already ran.
    scored = [
        j for j in scored
        if j.match_score >= MIN_MATCH_SCORE
        or is_internship_title(j.title)
        or is_entry_level_title(j.title)
    ]
    scored.sort(key=lambda j: j.match_score, reverse=True)
    return scored


@router.get("/v1/jobs/mcp/platforms")
async def list_platforms():
    """List all supported ATS platforms."""
    return {
        "platforms": [
            {"name": a.source_name, "detectable": True}
            for a in ADAPTERS
        ]
    }


@router.post("/v1/jobs/mcp/search")
async def mcp_search(req: MCPSearchRequest):
    """
    Unified job search across all supported ATS platforms.

    - If `companies` is provided: searches each company's career page via their ATS.
    - If empty: searches all platforms by query (limited — most require company identifier).
    - `sources` filter: only hit specific platforms (e.g. ["greenhouse", "lever"]).
    """
    all_jobs = []

    if req.companies:
        from careertwin.jobs.scanner import _emit_scan_log, get_scan_lock, reset_scan_log

        # Only one company-scan may run at a time process-wide — otherwise
        # two concurrent runs (a double-click, a retried request) both emit
        # into the same shared scan-log queue and their progress counters
        # interleave, each restarting from its own zero.
        async with get_scan_lock():
            companies_data = []
            with db_conn() as conn:
                for name in req.companies:
                    row = conn.execute(
                        "SELECT name, ats_platform, linkedin, website, careers FROM companies WHERE LOWER(name) = LOWER(?)",
                        (name,),
                    ).fetchone()
                    if row:
                        companies_data.append({
                            "name": row[0],
                            "ats_platform": row[1],
                            "url": row[4] or row[3] or "",
                        })
                    else:
                        companies_data.append({"name": name})

            reset_scan_log()
            search_batch = companies_data[:150] if len(companies_data) > 150 else companies_data
            _emit_scan_log(f"[SEARCH] Searching {len(search_batch)} companies for {req.query!r}...")

            def _on_company_done(name: str, done: int, total: int) -> None:
                _emit_scan_log(f"[SEARCH] {name} ({done}/{total})")

            ats_jobs = await search_all_companies(
                search_batch,
                query=req.query,
                limit_per_company=req.limit,
                intern_only=req.intern_only,
                on_progress=_on_company_done,
            )
            all_jobs.extend(ats_jobs)

            found_companies = {j.company.lower() for j in ats_jobs}
            remaining = [c for c in search_batch if c["name"].lower() not in found_companies and c.get("url")]
            use_crawler = True
            api_sem = asyncio.Semaphore(8)
            api_done = 0

            async def _try_api(c: dict) -> tuple[dict, list]:
                nonlocal api_done
                async with api_sem:
                    from careertwin.jobs.api_fetcher import fetch_jobs_from_api

                    try:
                        jobs = await fetch_jobs_from_api(c["name"], req.query, req.limit)
                    finally:
                        api_done += 1
                        _emit_scan_log(f"[SEARCH] {c['name']} ({len(search_batch) + api_done}/{len(search_batch) + len(remaining)})")
                    return c, jobs or []

            needs_crawl: list[dict] = []
            if remaining:
                api_results = await asyncio.gather(*(_try_api(c) for c in remaining))
                for c, api_jobs in api_results:
                    if api_jobs:
                        for j in api_jobs:
                            all_jobs.append(JobResult(
                                source=j.get("source", "api"),
                                company=j.get("company", c["name"]),
                                title=j.get("title", ""),
                                url=j.get("url", ""),
                                location=j.get("location", ""),
                                description=j.get("description", ""),
                                employment_type=j.get("employment_type", ""),
                            ))
                    else:
                        needs_crawl.append(c)

            if use_crawler and needs_crawl:
                from careertwin.jobs.scanner import _fetch_many_career_pages_crawl4ai

                _emit_scan_log(f"[SEARCH] Batch-crawling {len(needs_crawl)} companies with no ATS/API match...")
                crawled = await _fetch_many_career_pages_crawl4ai(needs_crawl)
                for name, jobs in crawled.items():
                    for j in jobs[:req.limit]:
                        all_jobs.append(JobResult(
                            source=j.get("source", "crawl4ai"),
                            company=j.get("company", name),
                            title=j.get("title", ""),
                            url=j.get("url", ""),
                            location=j.get("location", ""),
                            description=j.get("description", ""),
                        ))
    else:
        jobs = await search_by_query(
            req.query,
            sources=req.sources or None,
            limit=req.limit,
            intern_only=req.intern_only,
        )
        all_jobs.extend(jobs)

    max_results = req.limit * max(len(req.companies), 1) if req.companies else req.limit
    all_jobs = all_jobs[:max_results]

    sources_hit = list({j.source for j in all_jobs})
    scored = _score_mcp_jobs(all_jobs, query=req.query)
    _persist_collected_jobs(scored)
    return MCPSearchResponse(
        jobs=scored,
        total=len(scored),
        sources_queried=sources_hit,
    )


@router.get("/v1/jobs/mcp/detect")
async def detect_ats(url: str):
    """Detect which ATS platform a career page URL uses."""
    platform = detect_ats_from_url(url)
    return {"url": url, "platform": platform}


@router.get("/v1/jobs/mcp/company/{company_name}")
async def get_company_jobs(
    company_name: str,
    platform: str | None = None,
    query: str = "",
    limit: int = 50,
    intern_only: bool = False,
):
    """Get all jobs for a specific company via their ATS."""
    jobs = await search_company_jobs(
        company_name,
        ats_platform=platform,
        query=query,
        limit=limit,
        intern_only=intern_only,
    )
    scored = _score_mcp_jobs(jobs, query=query)
    _persist_collected_jobs(scored)
    return {
        "company": company_name,
        "platform": platform,
        "jobs": scored,
        "total": len(scored),
    }


@router.post("/v1/jobs/mcp/fetch-description")
async def fetch_job_description(payload: dict):
    """Fetch the real job description from a job listing URL.

    Uses crawl4ai (JS-rendering) rather than a raw HTML GET — ATS platforms
    like Ashby render the description client-side, so a plain fetch only
    ever sees the empty app shell.
    """
    url = payload.get("url", "")
    if not url:
        return {"description": ""}

    from careertwin.jobs.scanner import _fetch_job_description_crawl4ai

    result = await _fetch_job_description_crawl4ai(url, title=payload.get("title", ""))
    return {"description": result.get("description", ""), "unavailable": result.get("unavailable", False)}
