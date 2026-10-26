from __future__ import annotations

import os
from typing import Literal

import httpx
import asyncio

from fastapi import APIRouter, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel, Field, field_validator

from careertwin.automation import autopilot, guard
from careertwin.automation.engine import Engine
from careertwin.automation.mail import MailError, config as mail_config, configured as mail_configured
from careertwin.automation.excel import workbook
from careertwin.automation.models import Answer, Outcome, Settings, Vacancy
from careertwin.automation.monitor import Target
from careertwin.automation.stats import attention as attention_items
from careertwin.automation.stats import stats as pipeline_stats
from careertwin.automation.salary import SALARY_FIELD
from careertwin.net import validate_public_url
from careertwin.automation.sources import SOURCES

router = APIRouter(prefix="/v1/automation", tags=["automation"])
engine = Engine()


def existing(job_id: str) -> dict:
    try:
        return engine.store.job(job_id)
    except KeyError:
        raise HTTPException(404, "Vacancy not found") from None


@router.get("/settings")
async def get_settings():
    return engine.store.settings()


@router.put("/settings")
async def save_settings(settings: Settings):
    unknown = set(settings.sources) - SOURCES.keys()
    if unknown:
        raise HTTPException(422, f"Unknown sources: {', '.join(sorted(unknown))}")
    engine.store.save_settings(settings)
    return settings


@router.get("/status")
async def status():
    return engine.status()


@router.get("/decision-models")
async def decision_models():
    url = os.getenv("SWITCHYARD_URL", "http://127.0.0.1:3260").rstrip("/") + "/v1/models"
    key = os.getenv("SWITCHYARD_API_KEY", "")
    try:
        async with httpx.AsyncClient(timeout=15) as client:
            response = await client.get(
                url, headers={"Authorization": f"Bearer {key}"} if key else {}
            )
            response.raise_for_status()
            return [
                {"id": item["id"]}
                for item in response.json().get("data", [])
                if isinstance(item.get("id"), str)
            ]
    except (httpx.HTTPError, ValueError, AttributeError, KeyError) as exc:
        raise HTTPException(502, f"Could not list Switchyard models: {type(exc).__name__}") from exc


@router.get("/sources")
async def sources():
    return [
        {
            "id": key,
            "domain": domain,
            "method": "public_api" if key in {"remotive", "remoteok"} else "search_index_or_import",
        }
        for key, domain in SOURCES.items()
    ]


@router.post("/run")
async def run():
    try:
        engine.run()
    except guard.Stopped as exc:
        raise HTTPException(423, str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(409, str(exc)) from exc
    return {"status": "started"}


@router.post("/stop")
async def stop():
    settings = engine.store.settings().model_copy(update={"scheduled": False})
    engine.store.save_settings(settings)
    if engine.cycle and not engine.cycle.done():
        engine.cycle.cancel()
    for task in engine.tasks.values():
        if not task.done():
            task.cancel()
    return {"status": "stopping"}


@router.post("/kill")
async def kill():
    """The kill switch: nothing is submitted or sent until it is lifted. `touch data/STOP` does the same."""
    guard.stop("stopped from the API")
    await stop()
    return {"stopped": True}


@router.delete("/kill")
async def lift_kill():
    guard.resume()
    return {"stopped": False}


@router.get("/jobs")
async def jobs():
    return engine.store.jobs()


@router.post("/jobs")
async def import_job(vacancy: Vacancy):
    job = engine.store.add(vacancy)
    engine.store.event(job["id"], "imported", {"url": vacancy.url})
    return job


@router.get("/jobs/{job_id}")
async def job_detail(job_id: str):
    return {
        "job": existing(job_id),
        "events": engine.store.events(job_id),
        "files": engine.store.files(job_id),
    }


class Operation(BaseModel):
    operation: Literal["rank", "prepare", "fill", "submit", "resume", "verify"]


@router.post("/jobs/{job_id}/actions")
async def action(job_id: str, request: Operation):
    existing(job_id)
    try:
        engine.enqueue(job_id, request.operation)
    except ValueError as exc:
        raise HTTPException(409, str(exc)) from exc
    return {"status": "queued"}


@router.post("/jobs/{job_id}/answers")
async def answer(job_id: str, request: Answer):
    existing(job_id)
    if request.standing and SALARY_FIELD.search(request.question):
        raise HTTPException(422, "A salary is settled per vacancy, not for every form")
    engine.store.answer(request.question, request.answer, "" if request.standing else job_id)
    engine.store.event(job_id, "user_answer", request.model_dump())
    resumed = engine.resume_waiting() if request.standing else engine.resume_waiting(job_id)
    return {"status": "saved", "resumed": resumed}


@router.get("/standing-answers")
async def standing_answers():
    return engine.store.standing()


@router.put("/standing-answers")
async def save_standing_answer(request: Answer):
    if SALARY_FIELD.search(request.question):
        raise HTTPException(422, "A salary is settled per vacancy, not for every form")
    engine.store.answer(request.question, request.answer)
    return {"status": "saved", "resumed": engine.resume_waiting()}


@router.delete("/standing-answers")
async def forget_standing_answer(question: str):
    if not engine.store.forget(question):
        raise HTTPException(404, "No such standing answer")
    return {"status": "forgotten"}


class Autopilot(BaseModel):
    on: bool
    daily_limit: int | None = Field(default=None, ge=1, le=100)
    threshold: float | None = Field(default=None, ge=0, le=100)


@router.get("/autopilot")
async def get_autopilot():
    return autopilot.report(engine.store, engine.store.settings(), gateway=await asyncio.to_thread(autopilot.switchyard_reachable))


@router.put("/autopilot")
async def set_autopilot(request: Autopilot):
    """The candidate's switch. It is not a tool an agent has: only a call with the API token reaches it."""
    settings = engine.store.settings()
    if request.on:
        found = autopilot.blockers(autopilot.readiness(engine.store, settings, gateway=await asyncio.to_thread(autopilot.switchyard_reachable)))
        if found:
            raise HTTPException(409, "; ".join(f"{check.title}: {check.detail}" for check in found))
        engine.store.save_settings(autopilot.enable(settings, daily_limit=request.daily_limit, threshold=request.threshold))
        engine.store.event(None, "autopilot_on", {"daily_limit": request.daily_limit, "threshold": request.threshold})
    else:
        engine.store.save_settings(autopilot.disable(settings))
        engine.store.event(None, "autopilot_off", {})
    return autopilot.report(engine.store, engine.store.settings(), gateway=True)


@router.post("/jobs/{job_id}/outcome")
async def outcome(job_id: str, request: Outcome):
    job = existing(job_id)
    if job_id in engine.status()["active_jobs"]:
        raise HTTPException(409, "Stop this agent before recording an outcome")
    engine.store.update(job_id, state=request.status)
    engine.store.event(
        job_id, "user_outcome", {**request.model_dump(), "previous_state": job["state"]}
    )
    return engine.store.job(job_id)


@router.get("/events")
async def events():
    return engine.store.events()[-200:]


@router.get("/export.xlsx")
async def export():
    return Response(
        workbook(engine.store),
        media_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        headers={"Content-Disposition": 'attachment; filename="CareerTwin-applications.xlsx"'},
    )


class MailDraft(BaseModel):
    kind: Literal["application", "follow_up"]
    to: str | None = None


def mail_failure(exc: Exception) -> HTTPException:
    if isinstance(exc, guard.Stopped):
        return HTTPException(423, str(exc))
    if isinstance(exc, KeyError):
        return HTTPException(404, "Not found")
    return HTTPException(409, str(exc))


@router.get("/mail/status")
async def mail_status():
    """Whether mail works and how far today's limit is used. The password is never part of the answer."""
    if not mail_configured():
        return {"configured": False}
    cfg = mail_config()
    return {
        "configured": True,
        "address": cfg.address,
        "imap_host": cfg.imap_host,
        "smtp_host": cfg.smtp_host,
        "dry_run": cfg.dry_run,
        "sent_today": engine.mail.mails.sent_today(),
        "daily_limit": engine.store.settings().mail_daily_limit,
    }


@router.get("/mail")
async def mail_list(job_id: str | None = None, state: str | None = None, direction: str | None = None):
    return engine.mail.mails.list(job_id=job_id, state=state, direction=direction)


@router.post("/mail/check")
async def mail_check():
    try:
        return await engine.mail.check(engine.store.settings())
    except (MailError, guard.Stopped) as exc:
        raise mail_failure(exc) from exc


@router.post("/jobs/{job_id}/mail")
async def mail_draft(job_id: str, request: MailDraft):
    existing(job_id)
    try:
        return engine.mail.draft(job_id, request.kind, engine.store.settings(), request.to)
    except (MailError, KeyError) as exc:
        raise mail_failure(exc) from exc


@router.get("/mail/{mail_id}")
async def mail_get(mail_id: str):
    try:
        return engine.mail.mails.get(mail_id)
    except KeyError as exc:
        raise mail_failure(exc) from exc


@router.post("/mail/{mail_id}/approve")
async def mail_approve(mail_id: str):
    try:
        return engine.mail.approve(mail_id)
    except (MailError, KeyError) as exc:
        raise mail_failure(exc) from exc


@router.post("/mail/{mail_id}/reject")
async def mail_reject(mail_id: str):
    try:
        return engine.mail.reject(mail_id)
    except (MailError, KeyError) as exc:
        raise mail_failure(exc) from exc


@router.post("/mail/{mail_id}/send")
async def mail_send(mail_id: str):
    settings = engine.store.settings()
    try:
        return await engine.mail.send(mail_id, settings, auto=settings.mail_auto_send)
    except (MailError, guard.Stopped, KeyError) as exc:
        raise mail_failure(exc) from exc


class MailHandled(BaseModel):
    outcome: Literal["interview", "offer", "hired", "rejected", "withdrawn"] | None = None


@router.post("/mail/{mail_id}/handled")
async def mail_handled(mail_id: str, request: MailHandled):
    """Mark a received message dealt with; optionally record the outcome it announced."""
    try:
        row = engine.mail.mails.get(mail_id)
    except KeyError as exc:
        raise mail_failure(exc) from exc
    engine.mail.mails.update(mail_id, state="handled")
    if request.outcome and row["job_id"]:
        job = existing(row["job_id"])
        engine.store.update(row["job_id"], state=request.outcome)
        engine.store.event(row["job_id"], "user_outcome", {"status": request.outcome, "note": f"from mail {mail_id}", "previous_state": job["state"]})
    return engine.mail.mails.get(mail_id)


class Company(BaseModel):
    name: str = Field(min_length=1, max_length=100)
    url: str = ""
    ats_platform: Literal["", "greenhouse", "lever", "ashby", "smartrecruiters", "workday"] = ""

    @field_validator("url")
    @classmethod
    def public(cls, value: str) -> str:
        return validate_public_url(value) if value else value


@router.get("/monitor")
async def monitor_state():
    """The watch list with the health of every company: when it was read, how many vacancies are open,
    how many visits failed in a row and why."""
    return {
        "watching": engine.monitor.watch.targets(enabled_only=False),
        "running": bool(engine.monitoring and not engine.monitoring.done()),
    }


@router.post("/monitor/companies")
async def monitor_add(company: Company):
    engine.monitor.watch.add(Target(company.name, company.url, company.ats_platform))
    return {"status": "watching", "company": company.name}


@router.delete("/monitor/companies/{name}")
async def monitor_remove(name: str):
    if not engine.monitor.watch.remove(name):
        raise HTTPException(404, "That company is not on the watch list")
    return {"status": "removed"}


@router.post("/monitor/run")
async def monitor_run(everything: bool = False):
    """Visit the companies that are due now (all of them with `everything=true`) in the background."""
    if engine.monitoring and not engine.monitoring.done():
        raise HTTPException(409, "The monitor is already running")
    try:
        guard.assert_running()
    except guard.Stopped as exc:
        raise HTTPException(423, str(exc)) from exc
    engine.monitoring = asyncio.create_task(engine._monitor_round_now(engine.store.settings(), everything))
    return {"status": "started"}


@router.get("/summary")
async def summary():
    """Counts for a glance: the funnel, the reply rate, today's submissions, mail and the monitor."""
    return pipeline_stats(engine.store)


@router.get("/attention")
async def attention():
    """Everything that waits for you: forms ready to send, drafts to approve, replies to read, questions."""
    return attention_items(engine.store)
