from __future__ import annotations

import hashlib
import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from candidate_context import read_context, studio_snapshot, write_context
from database import _db_conn

from .models import Settings, Vacancy, canonical_url


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


class Store:
    def initialize(self) -> None:
        with _db_conn() as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS automation_settings (
                    id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS automation_jobs (
                    id TEXT PRIMARY KEY, canonical_url TEXT UNIQUE NOT NULL,
                    vacancy TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'new',
                    evaluation TEXT, package TEXT, tab_id INTEGER,
                    error TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS automation_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, job_id TEXT,
                    stage TEXT NOT NULL, payload TEXT NOT NULL, created_at TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS automation_answers (
                    job_id TEXT NOT NULL DEFAULT '', question TEXT NOT NULL,
                    answer TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(job_id,question));
                CREATE TABLE IF NOT EXISTS automation_files (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, job_id TEXT NOT NULL,
                    kind TEXT NOT NULL, path TEXT NOT NULL, sha256 TEXT NOT NULL,
                    created_at TEXT NOT NULL);
            """)
        # Recovery never retries a potentially completed submission.
        for job in self.jobs():
            if job["state"] in {"ranking", "preparing", "filling", "submitting", "verifying"}:
                state = (
                    "submission_uncertain"
                    if job["state"] in {"submitting", "verifying"}
                    else "needs_review"
                )
                self.update(
                    job["id"],
                    state=state,
                    error="Interrupted. Review the last browser state before resuming.",
                )
                self.event(job["id"], "recovery", {"previous_state": job["state"]})

    def settings(self) -> Settings:
        with _db_conn() as db:
            row = db.execute("SELECT payload FROM automation_settings WHERE id=1").fetchone()
        settings = Settings.model_validate_json(row[0]) if row else Settings()
        context = read_context()
        if context:
            settings.context = context
        preferences = studio_snapshot(settings.context).get("preferences", {})
        if preferences.get("jobTitles"):
            settings.queries = preferences["jobTitles"][:10]
        if preferences:
            settings.preferences = json.dumps(preferences, ensure_ascii=False)
        return settings

    def save_settings(self, settings: Settings) -> None:
        # Studio owns structured facts. Automation settings cannot overwrite a newer snapshot.
        context = read_context()
        if studio_snapshot(context):
            settings.context = context
        elif settings.context:
            write_context(settings.context)
        with _db_conn() as db:
            db.execute(
                (
                    "INSERT INTO automation_settings VALUES(1, ?) ON CONFLICT(id)"
                    " DO UPDATE SET payload=excluded.payload"
                ),
                (settings.model_dump_json(),),
            )
            db.commit()

    def add(self, vacancy: Vacancy) -> dict[str, Any]:
        url = canonical_url(vacancy.url)
        identity = url
        if vacancy.external_id and vacancy.company:
            identity = f"{vacancy.source}:{vacancy.company.casefold()}:{vacancy.external_id}"
        if "linkedin.com/jobs/view/" in url:
            vacancy = vacancy.model_copy(update={"url": url})
        job_id = hashlib.sha256(identity.encode()).hexdigest()[:24]
        stamp = now()
        with _db_conn() as db:
            db.execute(
                (
                    "INSERT OR IGNORE INTO automation_jobs(id, canonical_url, vac"
                    "ancy, created_at, updated_at) VALUES(?,?,?,?,?)"
                ),
                (job_id, url, vacancy.model_dump_json(), stamp, stamp),
            )
            db.commit()
            row = db.execute(
                "SELECT id FROM automation_jobs WHERE id=? OR canonical_url=?", (job_id, url)
            ).fetchone()
        return self.job(row[0])

    def _decode(self, row: sqlite3.Row) -> dict[str, Any]:
        item = dict(row)
        for key in ("vacancy", "evaluation", "package"):
            item[key] = json.loads(item[key]) if item[key] else None
        return item

    def job(self, job_id: str) -> dict[str, Any]:
        with _db_conn() as db:
            db.row_factory = sqlite3.Row
            row = db.execute("SELECT * FROM automation_jobs WHERE id=?", (job_id,)).fetchone()
        if row is None:
            raise KeyError(job_id)
        return self._decode(row)

    def jobs(self) -> list[dict[str, Any]]:
        with _db_conn() as db:
            db.row_factory = sqlite3.Row
            rows = db.execute("SELECT * FROM automation_jobs ORDER BY created_at DESC").fetchall()
        return [self._decode(row) for row in rows]

    def update(self, job_id: str, **values: Any) -> None:
        if not values or set(values) - {"state", "evaluation", "package", "tab_id", "error"}:
            raise ValueError("Invalid job update")
        for key in ("evaluation", "package"):
            if key in values:
                values[key] = json.dumps(values[key], ensure_ascii=False)
        values["updated_at"] = now()
        with _db_conn() as db:
            db.execute(
                f"UPDATE automation_jobs SET {','.join(k + '=?' for k in values)} WHERE id=?",
                (*values.values(), job_id),
            )
            db.commit()

    def event(self, job_id: str | None, stage: str, payload: Any) -> None:
        with _db_conn() as db:
            db.execute(
                "INSERT INTO automation_events(job_id,stage,payload,created_at) VALUES(?,?,?,?)",
                (job_id, stage, json.dumps(payload, ensure_ascii=False), now()),
            )
            db.commit()

    def events(self, job_id: str | None = None) -> list[dict[str, Any]]:
        with _db_conn() as db:
            db.row_factory = sqlite3.Row
            rows = db.execute(
                "SELECT * FROM automation_events"
                + (" WHERE job_id=?" if job_id else "")
                + " ORDER BY id",
                (job_id,) if job_id else (),
            ).fetchall()
        return [{**dict(row), "payload": json.loads(row["payload"])} for row in rows]

    def answers(self, job_id: str = "") -> dict[str, str]:
        with _db_conn() as db:
            return dict(
                db.execute(
                    "SELECT question,answer FROM automation_answers WHERE job_id=?", (job_id,)
                ).fetchall()
            )

    def answer(self, question: str, answer: str, job_id: str = "") -> None:
        with _db_conn() as db:
            db.execute(
                (
                    "INSERT INTO automation_answers VALUES(?,?,?,?) ON CONFLICT(j"
                    "ob_id,question) DO UPDATE SET answer=excluded.answer,updated"
                    "_at=excluded.updated_at"
                ),
                (job_id, question, answer, now()),
            )
            db.commit()

    def file(self, job_id: str, kind: str, path: Path) -> None:
        with _db_conn() as db:
            db.execute(
                (
                    "INSERT INTO automation_files(job_id,kind,path,sha256,created"
                    "_at) VALUES(?,?,?,?,?)"
                ),
                (
                    job_id,
                    kind,
                    str(path.resolve()),
                    hashlib.sha256(path.read_bytes()).hexdigest(),
                    now(),
                ),
            )
            db.commit()

    def files(self, job_id: str | None = None) -> list[dict[str, Any]]:
        with _db_conn() as db:
            db.row_factory = sqlite3.Row
            rows = db.execute(
                "SELECT * FROM automation_files" + (" WHERE job_id=?" if job_id else ""),
                (job_id,) if job_id else (),
            ).fetchall()
        return [dict(row) for row in rows]

    def submissions_today(self) -> int:
        day = now()[:10]
        with _db_conn() as db:
            return db.execute(
                (
                    "SELECT COUNT(DISTINCT job_id) FROM automation_events WHERE s"
                    "tage='submit_attempt' AND created_at LIKE ?"
                ),
                (day + "%",),
            ).fetchone()[0]
