"""CareerTwin API — studio routes."""

from __future__ import annotations

import datetime
import json
import math
import re
import sqlite3
from typing import Any
from uuid import uuid4

from fastapi import APIRouter, HTTPException
from fastapi.responses import Response

from careertwin.api.schemas import StudioSubmit
from careertwin.db import db_conn, row_to_dict
from careertwin.profile.context import LOCK, build_context, read_context, write_context
from careertwin.profile.names import resolve_profile_name

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
    preferences = dict(payload.preferences)
    salary = dict(preferences.get("salary") or {})
    salary["mode"] = "fixed" if salary.get("mode") == "fixed" else "auto"
    if salary["mode"] == "auto":
        salary = {"mode": "auto"}
    else:
        if (
            not isinstance(salary.get("amount"), (int, float))
            or isinstance(salary["amount"], bool)
            or not math.isfinite(salary["amount"])
            or salary["amount"] <= 0
        ):
            raise HTTPException(422, "A fixed salary requires a positive amount")
        if salary.get("currency") not in {"PLN", "USD", "EUR", "GBP"}:
            raise HTTPException(422, "A fixed salary requires a supported currency")
        if salary.get("period") not in {"hour", "month", "year"} or salary.get("basis") not in {
            "gross",
            "net_b2b",
        }:
            raise HTTPException(422, "A fixed salary requires pay period and gross/net B2B basis")
    preferences["salary"] = salary
    for key, value in {
        "linkedin": payload.linkedin_url,
        "github": payload.github,
        "website": payload.website,
        "email": preferences.get("contactEmail"),
    }.items():
        if value and not profile.get(key):
            profile[key] = value
    screenshots_json = json.dumps(payload.screenshots or [])
    completed_at = payload.completed_at or datetime.datetime.now().isoformat()

    with LOCK, db_conn() as conn:
        existing = conn.execute(
            "SELECT id FROM studio_sessions WHERE username = ?", (payload.username,)
        ).fetchone()

        if existing:
            conn.execute(
                """
                UPDATE studio_sessions SET
                    first_name = ?, last_name = ?, linkedin_url = ?, linkedin_email = ?,
                    github = ?, website = ?,
                    preferences = ?, profile = ?, screenshots = ?, completed_at = ?,
                    updated_at = CURRENT_TIMESTAMP
                WHERE username = ?
            """,
                (
                    payload.first_name,
                    payload.last_name,
                    payload.linkedin_url,
                    payload.linkedin_email,
                    payload.github,
                    payload.website,
                    json.dumps(preferences),
                    json.dumps(profile),
                    screenshots_json,
                    completed_at,
                    payload.username,
                ),
            )
        else:
            conn.execute(
                """
                INSERT INTO studio_sessions (id, username, first_name, last_name, linkedin_url,
                    linkedin_email, github, website, preferences, profile, screenshots,
                    completed_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
                (
                    uuid4().hex[:12],
                    payload.username,
                    payload.first_name,
                    payload.last_name,
                    payload.linkedin_url,
                    payload.linkedin_email,
                    payload.github,
                    payload.website,
                    json.dumps(preferences),
                    json.dumps(profile),
                    screenshots_json,
                    completed_at,
                ),
            )
        previous = read_context(payload.username)
        context = build_context(profile, preferences, previous)
        contacts = dict(re.findall(r"^\| ([^|]+) \| ([^|]+)\|", context, re.M))
        # Keep older profile/resume endpoints synchronized with the same completed Studio data.
        columns = [
            "full_name",
            "email",
            "phone",
            "location",
            "github",
            "linkedin",
            "website",
            "summary",
            "skills",
        ]
        values = [
            profile["fullName"],
            *[
                profile.get(key) or contacts.get(label, "").strip()
                for key, label in [
                    ("email", "Email"),
                    ("phone", "Phone"),
                    ("location", "Location"),
                    ("github", "GitHub"),
                    ("linkedin", "LinkedIn"),
                    ("website", "Website"),
                ]
            ],
            profile.get("summary", ""),
            json.dumps(profile.get("skills", [])),
        ]
        conn.execute(
            "INSERT INTO user_profiles (id, username, " + ",".join(columns) + ") "
            "VALUES (" + ",".join("?" for _ in range(len(columns) + 2)) + ") "
            "ON CONFLICT(username) DO UPDATE SET "
            + ",".join(f"{column}=excluded.{column}" for column in columns)
            + ", updated_at=CURRENT_TIMESTAMP",
            [uuid4().hex[:12], payload.username, *values],
        )
        try:
            write_context(context, payload.username)
        except OSError as error:
            conn.rollback()
            raise HTTPException(
                503, "Could not write ABOUT.md; Studio was not completed. Check file permissions."
            ) from error
        try:
            conn.commit()
        except Exception:
            write_context(previous, payload.username)
            raise
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            "SELECT * FROM studio_sessions WHERE username = ?", (payload.username,)
        ).fetchone()
    return {
        "success": True,
        "session": row_to_dict(row),
        "context_url": "/v1/studio/about",
        "context_saved": True,
    }


@router.get("/v1/studio/state")
async def studio_state(username: str) -> dict[str, Any]:
    with db_conn() as conn:
        conn.row_factory = sqlite3.Row
        row = conn.execute(
            "SELECT * FROM studio_sessions WHERE username = ?", (username,)
        ).fetchone()
    if row is None:
        return {"exists": False, "username": username}
    data = row_to_dict(row)
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
    with db_conn() as conn:
        conn.execute("DELETE FROM studio_sessions WHERE username = ?", (username,))
        conn.commit()
    return {"success": True}


@router.get("/v1/studio/about")
async def studio_about(username: str = "default") -> Response:
    try:
        content = read_context(username)
    except ValueError as error:
        raise HTTPException(422, str(error)) from error
    if not content:
        raise HTTPException(404, "Complete Studio to create ABOUT.md")
    return Response(
        content,
        media_type="text/markdown",
        headers={"Content-Disposition": 'attachment; filename="ABOUT.md"'},
    )
