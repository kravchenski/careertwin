"""CareerTwin API — studio routes."""

from __future__ import annotations

import datetime
import json
import sqlite3
from typing import Any
from uuid import uuid4

from fastapi import APIRouter

from database import StudioSubmit, _db_conn, _row_to_dict, resolve_profile_name

router = APIRouter()


@router.post("/v1/studio/submit")
async def studio_submit(payload: StudioSubmit) -> dict[str, Any]:
    """Persist the assembled Studio state (preferences + gathered profile)."""
    profile = dict(payload.profile)
    profile["fullName"] = resolve_profile_name(
        profile.get("fullName"),
        payload.first_name,
        payload.last_name,
    )
    screenshots_json = json.dumps(payload.screenshots or [])
    completed_at = payload.completed_at or datetime.datetime.now().isoformat()

    with _db_conn() as conn:
        existing = conn.execute(
            "SELECT id FROM studio_sessions WHERE username = ?", (payload.username,)
        ).fetchone()

        if existing:
            conn.execute("""
                UPDATE studio_sessions SET
                    first_name = ?, last_name = ?, linkedin_url = ?, linkedin_email = ?, github = ?, website = ?,
                    preferences = ?, profile = ?, screenshots = ?, completed_at = ?,
                    updated_at = CURRENT_TIMESTAMP
                WHERE username = ?
            """, (
                payload.first_name, payload.last_name, payload.linkedin_url, payload.linkedin_email, payload.github, payload.website,
                json.dumps(payload.preferences), json.dumps(profile),
                screenshots_json, completed_at, payload.username,
            ))
        else:
            conn.execute("""
                INSERT INTO studio_sessions (id, username, first_name, last_name, linkedin_url, linkedin_email, github, website, preferences, profile, screenshots, completed_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                uuid4().hex[:12], payload.username, payload.first_name, payload.last_name, payload.linkedin_url, payload.linkedin_email,
                payload.github, payload.website, json.dumps(payload.preferences),
                json.dumps(profile), screenshots_json, completed_at,
            ))
        conn.commit()
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            "SELECT * FROM studio_sessions WHERE username = ?", (payload.username,)
        ).fetchone()
    return {"success": True, "session": _row_to_dict(row)}


@router.get("/v1/studio/state")
async def studio_state(username: str) -> dict[str, Any]:
    with _db_conn() as conn:
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            "SELECT * FROM studio_sessions WHERE username = ?", (username,)
        ).fetchone()
    if row is None:
        return {"exists": False, "username": username}
    data = _row_to_dict(row)
    profile = dict(data.get("profile") or {})
    profile["fullName"] = resolve_profile_name(
        profile.get("fullName"),
        data.get("first_name"),
        data.get("last_name"),
    )
    data["profile"] = profile
    data["exists"] = True
    return data


@router.delete("/v1/studio/state")
async def delete_studio_state(username: str) -> dict[str, Any]:
    with _db_conn() as conn:
        conn.execute("DELETE FROM studio_sessions WHERE username = ?", (username,))
        conn.commit()
    return {"success": True}
