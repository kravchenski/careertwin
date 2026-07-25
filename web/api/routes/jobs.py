"""CareerTwin API — job routes."""

from __future__ import annotations

import asyncio
import json
import logging
import sqlite3
from typing import Any

from fastapi import APIRouter, File, Form, UploadFile
from fastapi.responses import StreamingResponse

from careertwin.agents.job_matcher import JobMatcher
from careertwin.graph.schema import CareerGraph, Education, Experience, Profile, Skill, SkillCategory

from browser import (
    _agent_browser_available,
    _agent_browser_browser_session,
    _agent_browser_open,
    _agent_browser_run,
    _agent_browser_session,
    _llm_browser_controller,
)
from database import (
    JobApplyRequest,
    JobCollectRequest,
    JobSearchRequest,
    ScrapeRequest,
    _db_conn,
    _row_to_dict,
)
from graph_builder import build_career_graph
from job_scraper import _web_search_jobs, scrape_job_url, _build_search_query, _is_intern_related
from resume_service import discover_jobs_from_graph, render_markdown_resume_pdf
from seniority import is_senior_or_mid

MIN_MATCH_SCORE = 80

logger = logging.getLogger("careertwin.api")

router = APIRouter()


@router.post("/v1/jobs/scrape")
async def scrape_job(req: ScrapeRequest) -> dict[str, Any]:
    return await scrape_job_url(req.url)


@router.post("/v1/jobs/discover")
async def discover_jobs(
    mode: str = Form("manual"),
    resume_file: UploadFile | None = File(None),
    fullName: str = Form(""),
    email: str = Form(""),
    phone: str = Form(""),
    location: str = Form(""),
    github: str = Form(""),
    linkedin: str = Form(""),
    website: str = Form(""),
    summary: str = Form(""),
    limit: int = Form(12),
    sources: str = Form("linkedin,justjoin,nofluffjobs,remoteok"),
) -> dict[str, Any]:
    graph, build_diagnostics = await build_career_graph(
        mode=mode,
        resume_file=resume_file,
        full_name=fullName,
        email=email,
        phone=phone,
        location=location,
        github=github,
        linkedin=linkedin,
        website=website,
        summary_text=summary,
    )
    search_sources = [item.strip().lower() for item in sources.split(",") if item.strip()]
    result = await discover_jobs_from_graph(
        graph=graph,
        limit=max(1, min(limit, 20)),
        sources=search_sources,
    )
    result["agent_diagnostics"] = {
        **build_diagnostics,
        **result.get("agent_diagnostics", {}),
    }
    return result


_SCRAPE_SEM = asyncio.Semaphore(5)


async def _scrape_limited(url: str) -> dict[str, Any]:
    async with _SCRAPE_SEM:
        return await scrape_job_url(url)


@router.post("/v1/jobs/search")
async def search_jobs(req: JobSearchRequest) -> dict[str, Any]:
    with _db_conn() as conn:
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            "SELECT * FROM user_profiles WHERE username = ?", (req.username,)
        ).fetchone()

    profile = _row_to_dict(row) if row else {}
    profile_skills = profile.get("skills", [])
    summary = profile.get("summary", "")

    if isinstance(profile_skills, str):
        try:
            profile_skills = json.loads(profile_skills)
        except Exception:
            profile_skills = [s.strip() for s in profile_skills.split(",") if s.strip()]
    if not isinstance(profile_skills, list):
        profile_skills = []
    profile_skills = [s for s in profile_skills if s]

    if not profile_skills:
        with _db_conn() as conn:
            row = conn.execute(
                "SELECT profile FROM studio_sessions WHERE username = ?", (req.username,)
            ).fetchone()
            if row and row[0]:
                try:
                    sp = json.loads(row[0])
                    profile_skills = sp.get("skills", [])
                    if not summary:
                        summary = sp.get("summary", "")
                    if not profile.get("full_name"):
                        profile["full_name"] = sp.get("fullName", "")
                except Exception:
                    pass

    query = req.query
    if not query and profile_skills:
        query = " ".join(profile_skills[:5])
    elif not query:
        query = "python developer"

    jobs = await _web_search_jobs(query=query, limit=req.limit, sources=req.sources)

    graph = CareerGraph(
        profile=Profile(
            full_name=profile.get("full_name", ""),
            summary=summary,
        ),
        skills=[Skill(name=s, category=SkillCategory.TOOL) for s in profile_skills],
    )
    matcher = JobMatcher()

    details_list = await asyncio.gather(
        *(_scrape_limited(job["url"]) for job in jobs),
        return_exceptions=True,
    )

    scored_jobs = []
    for job, details in zip(jobs, details_list):
        if isinstance(details, Exception):
            logger.warning("scrape failed url=%s: %s", job.get("url"), details)
            details = {}
        try:
            description = details.get("description", "") or job.get("title", "")
            match_text = description
            if len(description) < 50 and job.get("title"):
                match_text = f"{job['title']} {description}"
            match = matcher.match(graph, match_text)
            job["match_score"] = match.match_score
            job["matching_skills"] = match.matching_skills
            job["missing_skills"] = match.missing_skills
            job["description"] = description[:500]
            job["company"] = job.get("company") or details.get("company", "")
        except Exception as exc:
            logger.warning("match failed url=%s: %s", job.get("url"), exc)
            job["match_score"] = 0
            job["matching_skills"] = []
            job["missing_skills"] = []
            job["description"] = ""
        scored_jobs.append(job)

    scored_jobs.sort(key=lambda j: j.get("match_score", 0), reverse=True)

    return {
        "success": True,
        "username": req.username,
        "query": query,
        "total": len(scored_jobs),
        "jobs": scored_jobs,
    }


@router.post("/v1/jobs/apply")
async def apply_to_job(req: JobApplyRequest) -> dict[str, Any]:
    if not _agent_browser_available():
        return {
            "success": False,
            "error": "agent-browser is not installed or not discoverable by the backend.",
            "browser_session": {"provider": "agent-browser"},
        }

    session = _agent_browser_session("job-apply")
    open_ok, _open_out, open_err = _agent_browser_open(req.job_url, session=session, wait_ms=4000)
    if not open_ok:
        return {
            "success": False,
            "error": f"agent-browser failed to open the vacancy: {open_err or 'unknown error'}",
            "browser_session": _agent_browser_browser_session(
                session,
                "agent-browser session could not be opened.",
                status="error",
            ),
        }

    def get_count(selector: str) -> int:
        ok, stdout, _stderr = _agent_browser_run(["get", "count", selector], session=session, timeout=10, headed=False)
        if not ok:
            return 0
        try:
            return int(stdout.strip())
        except Exception:
            return 0

    controller_result = await _llm_browser_controller(
        session=session,
        job_url=req.job_url,
        full_name=req.full_name,
        email=req.email,
        phone=req.phone,
        cover_letter=req.cover_letter,
        provider=req.provider,
        model=req.model,
        auto_submit=req.auto_submit,
    )
    filled_fields = []
    for item in controller_result.get("trace", []):
        if item.get("action") == "fill":
            label = item.get("target") or item.get("value_preview") or "field"
            if label not in filled_fields:
                filled_fields.append(str(label))

    return {
        "success": True,
        "url": req.job_url,
        "page_info": {
            "forms_found": get_count("form"),
            "inputs_found": get_count("input, textarea"),
            "apply_buttons": get_count('button, a[href*="apply" i]'),
        },
        "filled_fields": filled_fields,
        "status": controller_result.get("status", "done"),
        "message": controller_result.get("message") or (
            f"LLM-controlled agent-browser opened {req.job_url} and prepared the form for review."
        ),
        "agent_trace": controller_result.get("trace", []),
        "browser_session": _agent_browser_browser_session(
            session,
            "Visible LLM-controlled agent-browser session is active.",
        ),
    }


@router.post("/v1/jobs/collect")
async def collect_jobs(req: JobCollectRequest) -> dict[str, Any]:
    """Collect jobs using SearXNG, Tavily, company career pages, and fallback search."""
    with _db_conn() as conn:
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            "SELECT * FROM user_profiles WHERE username = ?", (req.username,)
        ).fetchone()
        pref_row = conn.execute(
            "SELECT * FROM studio_sessions WHERE username = ?", (req.username,)
        ).fetchone()

    profile = _row_to_dict(row) if row else {}
    prefs = {}
    if pref_row:
        try:
            prefs = json.loads(pref_row["preferences"] or "{}")
        except Exception:
            prefs = {}

    query = req.query
    if not query and prefs.get("jobTitles"):
        query = prefs["jobTitles"][0]
    if not query and profile.get("skills"):
        query = " ".join(profile["skills"][:3])
    if not query:
        query = "python developer"

    intern = req.intern
    if req.experience_level == "internship":
        intern = True
    if not intern and prefs.get("experienceLevel") in ("entry", "first_role"):
        intern = True

    experience = req.experience_level
    if experience == "any" and prefs.get("experienceLevel"):
        lvl = prefs["experienceLevel"]
        if lvl in ("entry", "first_role"):
            experience = "entry level"
        elif lvl == "junior":
            experience = "associate level"
        elif lvl in ("mid", "senior", "director", "executive"):
            experience = f"{lvl} level"

    if req.source in ("any", "all"):
        sources = ["linkedin"]
    else:
        sources = [req.source]

    search_query = _build_search_query(
        query, experience=experience, location=req.location, intern=intern
    )

    all_jobs = []
    screenshots: list[str] = []

    companies = req.companies or []

    try:
        web_jobs = await _web_search_jobs(
            query=search_query,
            limit=req.limit,
            sources=sources,
            intern=intern,
            companies=companies,
        )
        all_jobs.extend(web_jobs)
    except Exception as exc:
        logger.warning("Collection failed: %s", exc)

    matcher = JobMatcher()
    graph = CareerGraph(
        profile=Profile(
            full_name=profile.get("full_name", ""),
            summary=profile.get("summary", ""),
        ),
        skills=[Skill(name=s, category=SkillCategory.TOOL) for s in profile.get("skills", [])],
    )

    capped = all_jobs[: req.limit]
    details_list = await asyncio.gather(
        *(_scrape_limited(job.get("url", "")) for job in capped),
        return_exceptions=True,
    )

    scored_jobs = []
    for job, details in zip(capped, details_list):
        if isinstance(details, Exception):
            logger.warning("collect scrape failed url=%s: %s", job.get("url"), details)
            details = {}
        description = details.get("description", "") or job.get("title", "")

        if is_senior_or_mid(f"{job.get('title', '')} {description}"):
            continue

        job["is_intern"] = _is_intern_related(job.get("title", ""), description)

        try:
            match_text = description
            if len(description) < 50 and job.get("title"):
                match_text = f"{job['title']} {description}"
            match = matcher.match(graph, match_text)
            job["match_score"] = match.match_score
            job["matching_skills"] = match.matching_skills
            job["missing_skills"] = match.missing_skills
            job["description"] = description[:500]
            job["company"] = job.get("company") or details.get("company", "")
            job["location"] = job.get("location") or details.get("location", "")
        except Exception as exc:
            logger.warning("collect match failed url=%s: %s", job.get("url"), exc)
            job["match_score"] = 0
            job["matching_skills"] = []
            job["missing_skills"] = []
            job["description"] = job.get("description", "")[:500]
        scored_jobs.append(job)

    # Internships are exempt from MIN_MATCH_SCORE — see the same carve-out in
    # jobs_mcp.py's _score_mcp_jobs for why.
    scored_jobs = [
        j for j in scored_jobs
        if j.get("match_score", 0) >= MIN_MATCH_SCORE or j.get("is_intern")
    ]
    scored_jobs.sort(key=lambda j: j.get("match_score", 0), reverse=True)
    logger.info(
        "Collected %d jobs (query=%s, intern=%s, companies=%s)",
        len(scored_jobs), query, intern, companies,
    )

    return {
        "success": True,
        "username": req.username,
        "source": req.source,
        "query": query,
        "intern": intern,
        "total": len(scored_jobs),
        "jobs": scored_jobs,
        "screenshots": screenshots,
    }



from pydantic import BaseModel as PydanticBaseModel


class JobScanRequest(PydanticBaseModel):
    username: str = "default"
    firstName: str = ""
    lastName: str = ""
    jobTitles: list[str] = []
    skills: list[str] = []
    experienceLevel: str = ""
    workTypes: list[str] = []
    onsiteCities: list[str] = []
    remoteCities: list[str] = []
    salary: dict = {}
    region: str = "all"
    maxCompanies: int = 143
    maxJobs: int = 50
    provider: str = "lmstudio"
    extraCompanies: list[str] = []


@router.post("/v1/jobs/scan")
async def scan_jobs(req: JobScanRequest):
    """Scan 140+ company career pages for matching jobs using crawl4ai.

    `extraCompanies` accepts arbitrary company names not in the curated list
    (e.g. "EPAM", "Sii") — their careers pages are resolved via web search.
    """
    from job_scanner import scan_all_companies

    user_profile = {
        "firstName": req.firstName,
        "lastName": req.lastName,
        "jobTitles": req.jobTitles,
        "skills": req.skills,
        "experienceLevel": req.experienceLevel,
        "workTypes": req.workTypes,
        "onsiteCities": req.onsiteCities,
        "remoteCities": req.remoteCities,
        "salary": req.salary,
    }

    result = await scan_all_companies(
        user_profile,
        region=req.region,
        max_companies=req.maxCompanies,
        max_jobs=req.maxJobs,
        provider=req.provider,
        extra_companies=req.extraCompanies,
    )

    return {
        "success": True,
        "username": req.username,
        "jobs": result["jobs"],
        "scanned_companies": result["scanned_companies"],
        "total_found": result["total_found"],
        "scan_time_seconds": result["scan_time_seconds"],
    }


@router.get("/v1/jobs/scan/status")
async def scan_status():
    """Health check for the job scanner."""
    from job_scanner import load_companies
    companies = load_companies()
    return {
        "status": "ok",
        "total_companies": len(companies),
        "regions": list({c.get("region", "unknown") for c in companies}),
    }


class AutoApplyRequest(PydanticBaseModel):
    username: str = "default"
    firstName: str = ""
    lastName: str = ""
    email: str = ""
    phone: str = ""
    title: str = ""
    location: str = ""
    github: str = ""
    linkedin: str = ""
    website: str = ""
    summary: str = ""
    jobTitles: list[str] = []
    skills: list[str] = []
    languages: list[str] = []
    experienceLevel: str = ""
    onsiteCities: list[str] = []
    remoteCities: list[str] = []
    salary: dict = {}
    experience: list[dict] = []
    education: list[dict] = []
    certificates: list[dict] = []
    jobs: list[dict] = []
    maxApplications: int = 5


def _load_stored_graph(username: str, fallback_name: str, fallback_skills: list[str]) -> tuple[CareerGraph | None, dict[str, Any] | None]:
    """Fallback only: build a CareerGraph from the onboarding-wizard profile
    (studio_sessions). In practice this is usually empty — the wizard doesn't
    collect work history — so _build_profile_graph (request-supplied data)
    is what actually powers cover letters and tailored CVs."""
    try:
        with _db_conn() as conn:
            row = conn.execute(
                "SELECT profile FROM studio_sessions WHERE username = ?", (username,)
            ).fetchone()
        if not row or not row[0]:
            return None, None
        sp = json.loads(row[0])
    except Exception as exc:
        logger.warning("Failed to load stored profile for %s: %s", username, exc)
        return None, None

    raw_experience = [e for e in sp.get("experience", []) if isinstance(e, dict)]
    raw_education = [e for e in sp.get("education", []) if isinstance(e, dict)]
    if not raw_experience and not raw_education:
        return None, None
    skills = sp.get("skills") or fallback_skills

    user_resume = {
        "summary": sp.get("summary", ""),
        "experience": [
            {
                "title": e.get("role", ""),
                "company": e.get("company", ""),
                "duration": f"{e.get('start', '')} - {e.get('end') or 'Present'}",
                "description": e.get("description", ""),
            }
            for e in raw_experience
        ],
        "education": [
            {"degree": e.get("degree", ""), "school": e.get("school", ""), "field": e.get("field", "")}
            for e in raw_education
        ],
    }

    graph = CareerGraph(
        profile=Profile(
            full_name=sp.get("fullName") or fallback_name or "Candidate",
            email=sp.get("email") or None,
            phone=sp.get("phone") or None,
            location=sp.get("location") or None,
            github=sp.get("github") or None,
            linkedin=sp.get("linkedin") or None,
            website=sp.get("website") or None,
            summary=sp.get("summary") or None,
        ),
        experience=[
            Experience(company=e.get("company", ""), position=e.get("role", ""), description=e.get("description", ""))
            for e in raw_experience
        ],
        education=[
            Education(institution=e.get("school", ""), degree=e.get("degree", ""), field=e.get("field") or None)
            for e in raw_education
        ],
        skills=[Skill(name=s, category=SkillCategory.TOOL) for s in skills],
    )
    return graph, user_resume


def _build_profile_graph(req: "AutoApplyRequest") -> tuple[CareerGraph | None, dict[str, Any] | None]:
    """Build a CareerGraph from the profile data sent in the request — this is
    the candidate's real work history from the Profile/Resume page, which
    only exists in browser localStorage and has to travel with the request."""
    if not req.experience and not req.education:
        return None, None

    full_name = f"{req.firstName} {req.lastName}".strip() or "Candidate"
    summary = req.summary
    if req.certificates:
        cert_lines = "; ".join(
            f"{c.get('name', '')} ({c.get('issuer', '')}, {c.get('year', '')})"
            for c in req.certificates if isinstance(c, dict) and c.get("name")
        )
        if cert_lines:
            summary = f"{summary}\n\nCertifications: {cert_lines}".strip()

    user_resume = {
        "summary": summary,
        "languages": req.languages,
        "experience": [
            {
                "title": e.get("role", ""),
                "company": e.get("company", ""),
                "duration": f"{e.get('start', '')} - {e.get('end') or ('Present' if e.get('current') else '')}".strip(" -"),
                "description": e.get("description", ""),
            }
            for e in req.experience if isinstance(e, dict)
        ],
        "education": [
            {"degree": e.get("degree", ""), "school": e.get("school", ""), "field": e.get("field", "")}
            for e in req.education if isinstance(e, dict)
        ],
    }

    graph = CareerGraph(
        profile=Profile(
            full_name=full_name,
            email=req.email or None,
            phone=req.phone or None,
            location=req.location or None,
            github=req.github or None,
            linkedin=req.linkedin or None,
            website=req.website or None,
            summary=summary or None,
        ),
        experience=[
            Experience(company=e.get("company", ""), position=e.get("role", ""), description=e.get("description", ""))
            for e in req.experience if isinstance(e, dict)
        ],
        education=[
            Education(institution=e.get("school", ""), degree=e.get("degree", ""), field=e.get("field") or None)
            for e in req.education if isinstance(e, dict)
        ],
        skills=[Skill(name=s, category=SkillCategory.TOOL) for s in req.skills],
    )
    return graph, user_resume


async def _run_generate_application(req: AutoApplyRequest) -> dict[str, Any]:
    """The actual generation work — cover letter + tailored CV per job. Takes
    minutes against a local model, so it runs as a background task instead of
    blocking the HTTP request (see generate_application / _APPLICATION_TASKS)."""
    from job_scanner import auto_apply_to_jobs

    user_profile = {
        "firstName": req.firstName,
        "lastName": req.lastName,
        "email": req.email,
        "phone": req.phone,
        "jobTitles": req.jobTitles,
        "skills": req.skills,
        "experienceLevel": req.experienceLevel,
        "onsiteCities": req.onsiteCities,
        "remoteCities": req.remoteCities,
        "salary": req.salary,
    }

    graph, user_resume = _build_profile_graph(req)
    if graph is None:
        graph, user_resume = _load_stored_graph(
            req.username, f"{req.firstName} {req.lastName}".strip(), req.skills
        )

    result = await auto_apply_to_jobs(
        req.jobs,
        user_profile,
        user_resume=user_resume,
        max_applications=req.maxApplications,
    )

    if user_resume is not None:
        from config import OUTPUT_DIR
        import uuid as _uuid

        full_name = f"{req.firstName} {req.lastName}".strip() or "Candidate"
        jobs_to_tailor = req.jobs[: req.maxApplications]

        async def _tailor_one(job: dict) -> str:
            pdf_name = f"resume-{_uuid.uuid4().hex[:8]}.pdf"
            pdf_path = OUTPUT_DIR / pdf_name
            await render_markdown_resume_pdf(
                output_path=pdf_path,
                user_resume=user_resume,
                full_name=full_name,
                title=req.title,
                email=req.email,
                phone=req.phone,
                location=req.location,
                github=req.github,
                linkedin=req.linkedin,
                skills=req.skills,
                job_title=job.get("title", ""),
                company=job.get("company", ""),
                job_description=job.get("description", ""),
            )
            return f"/outputs/{pdf_name}"

        packages = await asyncio.gather(
            *(_tailor_one(job) for job in jobs_to_tailor),
            return_exceptions=True,
        )
        for job, pdf_url in zip(jobs_to_tailor, packages):
            if isinstance(pdf_url, Exception):
                logger.warning("Tailored CV generation failed for %r: %s", job.get("title"), pdf_url)
                continue
            for r in result:
                if r["job"].get("url") == job.get("url"):
                    r["application"]["resume_pdf_url"] = pdf_url
                    break

    return {
        "success": True,
        "applications": [r["application"] for r in result],
        "jobs": [r["job"] for r in result],
        "count": len(result),
    }


_APPLICATION_TASKS: dict[str, dict[str, Any]] = {}


async def _generate_application_task(task_id: str, req: AutoApplyRequest) -> None:
    try:
        result = await _run_generate_application(req)
        _APPLICATION_TASKS[task_id] = {"status": "done", "result": result}
    except Exception as exc:
        logger.warning("Application generation task %s failed: %s", task_id, exc)
        _APPLICATION_TASKS[task_id] = {"status": "error", "error": str(exc)}


@router.post("/v1/jobs/generate-application")
async def generate_application(req: AutoApplyRequest):
    """Start generating tailored application materials (cover letter + a CV
    tailored to each job) in the background and return a task id to poll —
    this can take minutes against a local model, so it must survive the
    client disconnecting or the page reloading mid-generation."""
    import uuid

    task_id = uuid.uuid4().hex
    _APPLICATION_TASKS[task_id] = {"status": "pending"}
    asyncio.create_task(_generate_application_task(task_id, req))
    return {"task_id": task_id, "status": "pending"}


@router.get("/v1/jobs/generate-application/status/{task_id}")
async def generate_application_status(task_id: str):
    return _APPLICATION_TASKS.get(task_id, {"status": "not_found"})


@router.get("/v1/jobs/scan/logs")
async def scan_logs():
    """SSE endpoint that streams scan log messages in real-time.

    Deliberately does NOT replay _scan_log_history on connect: the frontend
    opens this connection right before firing the search request, so
    replaying history here meant every fresh "Collect" click first dumped
    the *previous* run's tail-end log lines before the new run's own lines
    arrived a moment later — producing exactly the duplicated/jumbled log
    output this was rewritten to fix. New connections only ever see genuinely
    new entries from here on.
    """
    from job_scanner import get_scan_log_queue

    async def event_generator():
        queue = get_scan_log_queue()
        waited = 0
        while queue is None and waited < 120:
            await asyncio.sleep(1)
            waited += 1
            queue = get_scan_log_queue()

        if queue is None:
            yield f"data: {json.dumps({'msg': '[INFO] No scan started', 'level': 'info', 'ts': 0})}\n\n"
            return

        while True:
            # The frontend opens this SSE connection right before firing the
            # search request that actually calls reset_scan_log() — which
            # swaps in a brand-new queue object. If that swap happens after
            # we grabbed our reference above, we'd be stuck listening on an
            # orphaned queue forever. Re-check on every iteration and switch
            # to the current queue if a reset replaced it.
            current_queue = get_scan_log_queue()
            if current_queue is not None and current_queue is not queue:
                queue = current_queue
            try:
                entry = await asyncio.wait_for(queue.get(), timeout=1.0)
                yield f"data: {json.dumps(entry)}\n\n"
            except asyncio.TimeoutError:
                yield f": keepalive\n\n"

    return StreamingResponse(
        event_generator(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


@router.get("/v1/jobs/collected")
async def get_collected_jobs(
    status: str | None = None,
    limit: int = 500,
    offset: int = 0,
) -> dict[str, Any]:
    """Load collected jobs from the database."""
    with _db_conn() as conn:
        if status:
            rows = conn.execute(
                "SELECT id, source, url, title, company, location, description, "
                "match_score, matching_skills, missing_skills, collected_at, status "
                "FROM collected_jobs WHERE status = ? ORDER BY match_score DESC LIMIT ? OFFSET ?",
                (status, limit, offset),
            ).fetchall()
        else:
            rows = conn.execute(
                "SELECT id, source, url, title, company, location, description, "
                "match_score, matching_skills, missing_skills, collected_at, status "
                "FROM collected_jobs ORDER BY match_score DESC LIMIT ? OFFSET ?",
                (limit, offset),
            ).fetchall()

    jobs = []
    for r in rows:
        if r[11] != "applied" and (
            is_senior_or_mid(f"{r[3] or ''} {r[6] or ''}") or (r[7] or 0) < MIN_MATCH_SCORE
        ):
            continue
        jobs.append({
            "id": r[0],
            "source": r[1],
            "url": r[2],
            "title": r[3],
            "company": r[4],
            "location": r[5],
            "description": r[6],
            "match_score": r[7],
            "matching_skills": json.loads(r[8]) if r[8] else [],
            "missing_skills": json.loads(r[9]) if r[9] else [],
            "collected_at": r[10],
            "status": r[11],
        })

    return {
        "success": True,
        "total": len(jobs),
        "jobs": jobs,
    }


@router.post("/v1/jobs/api/search")
async def api_search_jobs(body: dict[str, Any]) -> dict[str, Any]:
    """Search jobs via direct company APIs (EPAM, etc.)."""
    from api_fetcher import fetch_jobs_from_api

    companies = body.get("companies", [])
    query = body.get("query", "")
    limit = body.get("limit", 50)

    all_jobs = []
    for company in companies:
        jobs = await fetch_jobs_from_api(company, query, limit)
        all_jobs.extend(jobs)

    return {
        "success": True,
        "total": len(all_jobs),
        "jobs": all_jobs,
    }


@router.post("/v1/jobs/scrape-page")
async def scrape_career(body: dict[str, Any]) -> dict[str, Any]:
    """Scrape any career page for job listings using Playwright."""
    from career_scraper import scrape_career_page

    url = body.get("url", "")
    company = body.get("company", "")

    if not url:
        return {"success": False, "error": "URL is required"}

    jobs = await scrape_career_page(url, company)
    return {
        "success": True,
        "total": len(jobs),
        "jobs": jobs,
    }


@router.post("/v1/jobs/fetch-description")
async def fetch_description(body: dict[str, Any]) -> dict[str, Any]:
    """Fetch full job description from the job's URL via Playwright."""
    from api_fetcher import _fetch_epam_job_description, _strip_html

    url = body.get("url", "")
    if not url:
        return {"success": False, "error": "URL is required"}

    if "epam.com" in url:
        desc = await _fetch_epam_job_description(url)
        if desc:
            return {"success": True, "description": desc}

    try:
        from playwright.async_api import async_playwright
        async with async_playwright() as p:
            browser = await p.chromium.launch(headless=True, args=["--no-sandbox", "--disable-dev-shm-usage"])
            page = await browser.new_page()
            try:
                await page.goto(url, wait_until="networkidle", timeout=20000)
                await page.wait_for_timeout(2000)
                desc = await page.evaluate("""() => {
                    const selectors = ['article', 'main', '[role="main"]', '.content', '.description'];
                    for (const sel of selectors) {
                        const el = document.querySelector(sel);
                        if (el && el.textContent.trim().length > 100) {
                            return el.textContent.trim();
                        }
                    }
                    return document.body.innerText.substring(0, 5000);
                }""")
                return {"success": True, "description": _strip_html(desc)[:3000]}
            finally:
                await browser.close()
    except Exception as e:
        return {"success": False, "error": str(e)}
