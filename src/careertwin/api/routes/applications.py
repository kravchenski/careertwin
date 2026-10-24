"""CareerTwin API — submitted job-application tracking routes."""

from __future__ import annotations

import json
import logging
import sqlite3
from datetime import datetime, timezone
from typing import Any
from uuid import uuid4

from fastapi import APIRouter, HTTPException

from careertwin.api.schemas import JobApplicationCreate, JobApplicationUpdate
from careertwin.db import db_conn

logger = logging.getLogger("careertwin.api")

router = APIRouter()

_STAGE_COLUMN = {
    "applied": "applied_at",
    "phone_screen": "phone_screen_at",
    "technical": "technical_at",
    "offer": "offer_at",
}
_STAGE_ORDER = ["applied", "phone_screen", "technical", "offer"]
_STAGE_LABEL = {
    "applied": "Applied",
    "phone_screen": "Phone screen",
    "technical": "Technical",
    "offer": "Offer",
    "rejected": "Rejected",
}


def _now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _row_to_application(row: sqlite3.Row) -> dict[str, Any]:
    data = dict(row)
    try:
        notes = json.loads(data.get("notes") or "[]")
    except (json.JSONDecodeError, TypeError):
        notes = []

    status = data.get("status", "applied")
    stage_dates = {stage: data.get(_STAGE_COLUMN[stage]) for stage in _STAGE_ORDER}
    reached_index = _STAGE_ORDER.index(status) if status in _STAGE_ORDER else -1

    timeline = []
    for i, stage in enumerate(_STAGE_ORDER):
        timeline.append({
            "label": _STAGE_LABEL[stage],
            "date": stage_dates[stage] or "",
            "completed": stage_dates[stage] is not None and i < reached_index,
            "active": stage == status,
        })

    return {
        "id": data["id"],
        "company": data["company"],
        "position": data["position"],
        "domain": data.get("domain") or "",
        "job_url": data.get("job_url") or "",
        "status": status,
        "status_label": _STAGE_LABEL.get(status, status),
        "timeline": timeline,
        "notes": notes,
        "created_at": data.get("created_at"),
        "updated_at": data.get("updated_at"),
    }


@router.get("/v1/applications")
async def list_applications(username: str = "default") -> dict[str, Any]:
    with db_conn() as conn:
        conn.row_factory = sqlite3.Row
        rows = conn.execute(
            "SELECT * FROM job_applications WHERE username = ? ORDER BY created_at DESC",
            (username,),
        ).fetchall()

    applications = [_row_to_application(r) for r in rows]

    total_applied = len(applications)
    stats = []
    for stage in _STAGE_ORDER:
        count = sum(
            1 for a in applications
            if a["status"] in _STAGE_ORDER and _STAGE_ORDER.index(a["status"]) >= _STAGE_ORDER.index(stage)
        )
        stats.append({
            "name": _STAGE_LABEL[stage],
            "count": count,
            "percentage": round((count / total_applied) * 100) if total_applied else 0,
        })

    return {"success": True, "applications": applications, "pipeline": stats}


@router.post("/v1/applications")
async def create_application(req: JobApplicationCreate) -> dict[str, Any]:
    app_id = str(uuid4())
    now = _now()
    with db_conn() as conn:
        conn.execute(
            """
            INSERT INTO job_applications (id, username, company, position, domain, job_url, status, applied_at, notes, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'applied', ?, '[]', ?, ?)
            """,
            (app_id, req.username, req.company, req.position, req.domain, req.job_url, now, now, now),
        )
        conn.commit()
        conn.row_factory = sqlite3.Row
        row = conn.execute("SELECT * FROM job_applications WHERE id = ?", (app_id,)).fetchone()

    return {"success": True, "application": _row_to_application(row)}


@router.patch("/v1/applications/{application_id}")
async def update_application(application_id: str, req: JobApplicationUpdate) -> dict[str, Any]:
    with db_conn() as conn:
        conn.row_factory = sqlite3.Row
        row = conn.execute("SELECT * FROM job_applications WHERE id = ?", (application_id,)).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="Application not found")

        now = _now()
        if req.status:
            column = _STAGE_COLUMN.get(req.status)
            set_clauses = ["status = ?", "updated_at = ?"]
            params: list[Any] = [req.status, now]
            if column:
                set_clauses.append(f"{column} = COALESCE({column}, ?)")
                params.append(now)
            conn.execute(
                f"UPDATE job_applications SET {', '.join(set_clauses)} WHERE id = ?",
                (*params, application_id),
            )

        if req.note:
            try:
                notes = json.loads(row["notes"] or "[]")
            except (json.JSONDecodeError, TypeError):
                notes = []
            notes.insert(0, {"content": req.note, "date": now})
            conn.execute(
                "UPDATE job_applications SET notes = ?, updated_at = ? WHERE id = ?",
                (json.dumps(notes), now, application_id),
            )

        conn.commit()
        row = conn.execute("SELECT * FROM job_applications WHERE id = ?", (application_id,)).fetchone()

    return {"success": True, "application": _row_to_application(row)}


@router.delete("/v1/applications/{application_id}")
async def delete_application(application_id: str) -> dict[str, Any]:
    with db_conn() as conn:
        conn.execute("DELETE FROM job_applications WHERE id = ?", (application_id,))
        conn.commit()
    return {"success": True}
