from __future__ import annotations

import os
from typing import Literal

import httpx
from fastapi import APIRouter, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel

from careertwin.automation import guard
from careertwin.automation.engine import Engine
from careertwin.automation.excel import workbook
from careertwin.automation.models import Answer, Outcome, Settings, Vacancy
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
    engine.store.answer(request.question, request.answer, job_id)
    engine.store.event(job_id, "user_answer", request.model_dump())
    return {"status": "saved"}


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
