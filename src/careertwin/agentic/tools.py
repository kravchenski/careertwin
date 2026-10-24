"""The CareerTwin tool catalogue: one async function per agent capability."""

from __future__ import annotations

import asyncio
import os
import time
from typing import Annotated, Any, Literal

from pydantic import Field

from careertwin.agentic.client import ApiError, CareerTwinClient
from careertwin.agentic.registry import tool

TRANSIENT_STATES = frozenset({"ranking", "preparing", "filling", "submitting", "verifying"})
SETTINGS_FIELDS = (
    "queries", "sources", "preferences", "threshold", "confidence", "auto_prepare",
    "limit", "daily_limit", "companies",
)


def _client() -> CareerTwinClient:
    return CareerTwinClient()


def _brief(job: dict[str, Any]) -> dict[str, Any]:
    vacancy = job.get("vacancy") or {}
    evaluation = job.get("evaluation") or {}
    return {
        "id": job.get("id"),
        "title": vacancy.get("title"),
        "company": vacancy.get("company"),
        "url": vacancy.get("url"),
        "state": job.get("state"),
        "fit_score": evaluation.get("score"),
        "category": evaluation.get("category"),
        "error": job.get("error") or None,
        "updated_at": job.get("updated_at"),
    }


@tool("API health and what the agents are doing")
async def get_status() -> dict[str, Any]:
    """Check that the CareerTwin API is up and report what the automation agents are doing:
    whether a search cycle runs, which vacancies are being processed, applications submitted today
    and whether the Switchyard browser is configured."""
    client = _client()
    health = await client.get("/v1/health")
    return {"api": health, "automation": await client.get("/v1/automation/status")}


@tool("Search settings and your ABOUT context")
async def get_candidate_context(
    max_chars: Annotated[int, Field(ge=200, le=20000)] = 3000,
) -> dict[str, Any]:
    """Return the candidate's search settings and the start of the private ABOUT context
    (skills, experience, salary policy). Use it to ground every document and answer in facts."""
    settings = await _client().get("/v1/automation/settings")
    context = settings.get("context", "")
    return {
        **{key: settings.get(key) for key in SETTINGS_FIELDS},
        "context_chars": len(context),
        "context": context[:max_chars],
    }


@tool("Sources the search agent can use")
async def list_job_sources() -> list[dict[str, Any]]:
    """List the job sources the search agent can use (public feeds and indexed boards)."""
    return await _client().get("/v1/automation/sources")


@tool("Search company career boards via their ATS")
async def search_company_boards(
    companies: Annotated[list[str], Field(min_length=1, max_length=20)],
    query: str = "",
    sources: list[str] | None = None,
    limit: Annotated[int, Field(ge=1, le=100)] = 25,
    intern_only: bool = False,
) -> dict[str, Any]:
    """Search the career boards of specific companies through their applicant tracking system
    (greenhouse, lever, ashby, smartrecruiters, workday). Use company names known to
    CareerTwin; `sources` optionally restricts the ATS platforms. Returns vacancies you can
    import with import_vacancy."""
    result = await _client().post(
        "/v1/jobs/mcp/search",
        {
            "query": query,
            "companies": companies,
            "sources": sources or [],
            "intern_only": intern_only,
            "limit": limit,
        },
        timeout=300,
    )
    jobs = result.get("jobs", [])
    return {
        "total": result.get("total", len(jobs)),
        "sources_queried": result.get("sources_queried", []),
        "jobs": [
            {key: job.get(key) for key in ("company", "title", "url", "location", "source")}
            for job in jobs[:limit]
        ],
    }


@tool("Full text of a vacancy page")
async def fetch_job_description(url: str, title: str = "") -> dict[str, Any]:
    """Fetch the full text of a vacancy page (JavaScript pages included). Use it before
    importing a vacancy so ranking sees the real requirements."""
    return await _client().post(
        "/v1/jobs/mcp/fetch-description", {"url": url, "title": title}, timeout=120
    )


@tool("Add a vacancy to the pipeline", mutating=True)
async def import_vacancy(
    url: str,
    title: str,
    company: str = "",
    description: str = "",
    location: str = "",
    salary: str = "",
) -> dict[str, Any]:
    """Add a vacancy to the pipeline (state `new`). Duplicates are merged by URL. Pass the
    full description when you have it, then run process_vacancy with `rank`."""
    job = await _client().post(
        "/v1/automation/jobs",
        {
            "url": url,
            "title": title,
            "company": company,
            "description": description,
            "location": location,
            "salary": salary,
            "source": "agent",
        },
    )
    return _brief(job)


@tool("Vacancies in the pipeline, by state")
async def list_vacancies(
    state: str | None = None,
    limit: Annotated[int, Field(ge=1, le=200)] = 50,
) -> list[dict[str, Any]]:
    """List vacancies in the pipeline, newest first. `state` filters by pipeline state:
    new, ranking, priority, stretch, needs_info, skip, preparing, prepared, filling,
    awaiting_answers, form_ready, submitting, submitted, submission_uncertain, verifying,
    interview, offer, hired, rejected, withdrawn."""
    jobs = await _client().get("/v1/automation/jobs")
    if state:
        jobs = [job for job in jobs if job.get("state") == state]
    return [_brief(job) for job in jobs[:limit]]


@tool("One vacancy with evaluation, files, events")
async def get_vacancy(job_id: str) -> dict[str, Any]:
    """Full detail of one vacancy: state, fit evaluation with reasons, prepared documents
    and the latest events. Use it to read why a vacancy was skipped or what a form needs."""
    detail = await _client().get(f"/v1/automation/jobs/{job_id}")
    job = detail["job"]
    return {
        **_brief(job),
        "description": (job.get("vacancy") or {}).get("description", "")[:4000],
        "evaluation": job.get("evaluation"),
        "package": job.get("package"),
        "files": detail.get("files"),
        "events": detail.get("events", [])[-15:],
    }


@tool("Queue rank, prepare, fill or verify", mutating=True)
async def process_vacancy(
    job_id: str,
    operation: Literal["rank", "prepare", "fill", "verify"],
) -> dict[str, Any]:
    """Queue one agent step for a vacancy and return immediately: `rank` scores fit against the
    candidate, `prepare` writes the tailored CV and cover letter, `fill` completes the application
    form in the signed-in browser without sending it, `verify` re-reads the confirmation page.
    Steps run in the background, so follow with wait_for_vacancy."""
    return await _client().post(f"/v1/automation/jobs/{job_id}/actions", {"operation": operation})


@tool("Wait for a queued step to finish")
async def wait_for_vacancy(
    job_id: str,
    timeout_seconds: Annotated[int, Field(ge=5, le=600)] = 120,
) -> dict[str, Any]:
    """Wait until a queued step finishes (the vacancy leaves ranking, preparing, filling,
    submitting or verifying) or the timeout passes, then return its brief state."""
    client = _client()
    deadline = time.monotonic() + timeout_seconds
    while True:
        detail = await client.get(f"/v1/automation/jobs/{job_id}")
        job = detail["job"]
        if job.get("state") not in TRANSIENT_STATES:
            return {**_brief(job), "finished": True}
        if time.monotonic() >= deadline:
            return {**_brief(job), "finished": False}
        await asyncio.sleep(2)


@tool("Run the search agent", mutating=True)
async def start_search_cycle() -> dict[str, Any]:
    """Start the search agent: it runs the saved queries across the saved sources, imports new
    vacancies, ranks them and, when auto_prepare is on, prepares documents for strong matches.
    Poll get_status and list_vacancies for progress."""
    return await _client().post("/v1/automation/run")


@tool("Cancel running agents and scheduling", mutating=True)
async def stop_agents() -> dict[str, Any]:
    """Cancel the running search cycle and every queued vacancy step, and switch off scheduling."""
    return await _client().post("/v1/automation/stop")


@tool("Queries, sources, threshold, limits", mutating=True)
async def update_search_settings(
    queries: list[str] | None = None,
    sources: list[str] | None = None,
    preferences: str | None = None,
    threshold: Annotated[float | None, Field(ge=0, le=100)] = None,
    auto_prepare: bool | None = None,
    limit: Annotated[int | None, Field(ge=1, le=100)] = None,
    daily_limit: Annotated[int | None, Field(ge=1, le=100)] = None,
) -> dict[str, Any]:
    """Change search settings; omitted fields keep their value. Submitting is deliberately not
    settable here: only the candidate turns on auto_submit or scheduling."""
    client = _client()
    settings = await client.get("/v1/automation/settings")
    changes = {
        "queries": queries,
        "sources": sources,
        "preferences": preferences,
        "threshold": threshold,
        "auto_prepare": auto_prepare,
        "limit": limit,
        "daily_limit": daily_limit,
    }
    settings.update({key: value for key, value in changes.items() if value is not None})
    saved = await client.put("/v1/automation/settings", settings)
    return {key: saved.get(key) for key in SETTINGS_FIELDS}


@tool("Save your answer to a form question", mutating=True)
async def answer_question(job_id: str, question: str, answer: str) -> dict[str, Any]:
    """Save the candidate's own answer to a question an application form asked (state
    `awaiting_answers`). Only pass answers the candidate gave or that the profile states;
    never invent one. Then run process_vacancy with `fill` again."""
    return await _client().post(
        f"/v1/automation/jobs/{job_id}/answers", {"question": question, "answer": answer}
    )


@tool("Press submit on a prepared form", mutating=True, outward=True)
async def submit_application(job_id: str, confirm: bool = False) -> dict[str, Any]:
    """Press submit on a prepared application form. This sends the application to the employer
    and cannot be undone. It runs only with `confirm=true` AND when the candidate set
    CAREERTWIN_AGENT_ALLOW_SUBMIT=1. Ask the candidate first, every time."""
    if not confirm:
        raise ApiError("Not submitted: ask the candidate, then call again with confirm=true.")
    if os.getenv("CAREERTWIN_AGENT_ALLOW_SUBMIT") != "1":
        raise ApiError(
            "Not submitted: submitting from an agent is disabled. The candidate can submit in "
            "the browser, or set CAREERTWIN_AGENT_ALLOW_SUBMIT=1 for this agent."
        )
    return await _client().post(f"/v1/automation/jobs/{job_id}/actions", {"operation": "submit"})


@tool("Companies the monitor watches and their health")
async def list_monitored_companies() -> dict[str, Any]:
    """The companies whose career sites are watched, when each was last read, how many vacancies
    are open and any error. New relevant vacancies from them arrive in the pipeline by themselves."""
    return await _client().get("/v1/automation/monitor")


@tool("Watch one company's career site", mutating=True)
async def watch_company(
    name: str,
    url: str = "",
    ats_platform: Literal["", "greenhouse", "lever", "ashby", "smartrecruiters", "workday"] = "",
) -> dict[str, Any]:
    """Add a company to the monitor. `url` is its careers page (the application system is detected from
    it); `name` is the board identifier when the company uses Greenhouse, Lever, Ashby, SmartRecruiters
    or Workday."""
    return await _client().post("/v1/automation/monitor/companies", {"name": name, "url": url, "ats_platform": ats_platform})


@tool("Visit the watched career sites now", mutating=True)
async def run_monitor(everything: bool = False) -> dict[str, Any]:
    """Visit the companies that are due (all of them with `everything=true`). Runs in the background;
    read list_monitored_companies and recent_events for what it found."""
    return await _client().post(f"/v1/automation/monitor/run?everything={'true' if everything else 'false'}")


@tool("Read the inbox for replies to applications", mutating=True)
async def check_mail() -> dict[str, Any]:
    """Read the candidate's mailbox once (read-only) and file the messages that are about an
    application: interview invitations, rejections, requests for information. Returns counts by
    kind; call list_mail for the messages."""
    return await _client().post("/v1/automation/mail/check", timeout=120)


@tool("Messages the mail agent filed")
async def list_mail(
    job_id: str | None = None, state: str | None = None, direction: str | None = None
) -> list[dict[str, Any]]:
    """Messages the mail agent filed, newest first. `state` is new (needs the candidate), handled,
    draft, approved, sent. `direction` is in or out."""
    mails = await _client().get("/v1/automation/mail", job_id=job_id, state=state, direction=direction)
    return [{key: value for key, value in mail.items() if key != "body"} | {"body": mail["body"][:600]} for mail in mails]


@tool("Draft an application or follow-up email", mutating=True)
async def draft_email(
    job_id: str, kind: Literal["application", "follow_up"], to: str | None = None
) -> dict[str, Any]:
    """Prepare an email for one vacancy and leave it as a draft for the candidate to approve.
    Nothing is sent. The address must be one the vacancy names, one on the employer's domain or a
    sender in the thread; give `to` only for an address the candidate has checked."""
    return await _client().post(f"/v1/automation/jobs/{job_id}/mail", {"kind": kind, "to": to})


@tool("Send a drafted email", mutating=True, outward=True)
async def send_email(mail_id: str, confirm: bool = False) -> dict[str, Any]:
    """Approve and send a drafted email to the employer. This cannot be undone. It runs only with
    `confirm=true` AND when the candidate set CAREERTWIN_AGENT_ALLOW_MAIL=1. Ask the candidate first,
    every time, and show them the draft."""
    if not confirm:
        raise ApiError("Not sent: ask the candidate, then call again with confirm=true.")
    if os.getenv("CAREERTWIN_AGENT_ALLOW_MAIL") != "1":
        raise ApiError("Not sent: sending mail from an agent is disabled (CAREERTWIN_AGENT_ALLOW_MAIL=1 enables it).")
    await _client().post(f"/v1/automation/mail/{mail_id}/approve")
    return await _client().post(f"/v1/automation/mail/{mail_id}/send")


@tool("Interview, offer, hired, rejected, withdrawn", mutating=True)
async def record_outcome(
    job_id: str,
    status: Literal["interview", "offer", "hired", "rejected", "withdrawn"],
    note: str = "",
) -> dict[str, Any]:
    """Record what happened after applying, as reported by the candidate."""
    job = await _client().post(
        f"/v1/automation/jobs/{job_id}/outcome", {"status": status, "note": note}
    )
    return _brief(job)


@tool("Newest events from every agent")
async def recent_events(limit: Annotated[int, Field(ge=1, le=200)] = 30) -> list[dict[str, Any]]:
    """The newest events from every agent (ranked, prepared, answers saved, submissions)."""
    events = await _client().get("/v1/automation/events")
    return events[-limit:]


@tool("Grounded talking points for one question")
async def suggest_interview_answer(
    question: str,
    job_id: str | None = None,
    notes: str = "",
    language: str | None = None,
) -> dict[str, Any]:
    """Talking points and a short spoken answer for one interviewer question, grounded in the
    candidate's ABOUT context (and the vacancy when `job_id` is given). Never invents experience:
    when the context has none it says so and offers an honest framing. Use it for practice or
    when the candidate pastes a question. `language` is the interview language (en, ru, pl, de, ...);
    answers are written in it."""
    return await _client().post(
        "/v1/interview/suggest",
        {"question": question, "job_id": job_id, "notes": notes, "language": language},
        timeout=180,
    )


@tool("Likely questions with honest outlines")
async def prepare_interview_questions(
    job_id: str | None = None,
    count: Annotated[int, Field(ge=3, le=15)] = 8,
    focus: str = "",
    language: str | None = None,
) -> dict[str, Any]:
    """Likely interview questions for a vacancy, each with an outline of an honest answer built
    from the candidate's facts. `focus` narrows the topics, for example "system design". `language` is
    the interview language (en, ru, pl, de, ...)."""
    return await _client().post(
        "/v1/interview/practice",
        {"job_id": job_id, "count": count, "focus": focus, "language": language},
        timeout=180,
    )


@tool("Review the CV sent for a vacancy")
async def review_cv_for_vacancy(job_id: str, language: str | None = None) -> dict[str, Any]:
    """Compare the CV prepared for a pipeline vacancy with that vacancy: a short summary, the real
    strengths to lead with, honest gaps with a way to address each in the interview, the questions
    those gaps invite, and concrete fixes to the CV text. `language` is the language of the review
    (en, ru, pl, de, ...). Run process_vacancy `prepare` first so the vacancy has a CV."""
    return await _client().post(
        "/v1/interview/cv-review", {"job_id": job_id, "language": language}, timeout=180
    )
