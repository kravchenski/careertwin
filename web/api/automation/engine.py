from __future__ import annotations

import asyncio
import hashlib
import os
from typing import Any

from .browser import BrowserAgent
from .documents import DocumentAgent
from .excel import ExcelAgent
from .models import Vacancy
from .ranker import JevRanker
from .sources import SearchAgent
from .store import Store, now


class Engine:
    def __init__(self, store: Store | None = None):
        self.store = store or Store()
        self.search = SearchAgent()
        self.ranker = JevRanker()
        self.documents = DocumentAgent()
        self.browser = BrowserAgent()
        self.tracker = ExcelAgent(self.store)
        self.tracker_task: asyncio.Task | None = None
        self.tasks: dict[str, asyncio.Task] = {}
        self.scheduler: asyncio.Task | None = None
        self.cycle: asyncio.Task | None = None
        self.last_run: str | None = None

    def start(self) -> None:
        self.store.initialize()
        previous = [event for event in self.store.events() if event["stage"] == "cycle_started"]
        if previous:
            self.last_run = previous[-1]["created_at"]
        self.scheduler = asyncio.create_task(self._schedule())
        self.tracker_task = asyncio.create_task(self.tracker.run())

    async def close(self) -> None:
        tasks = [task for task in [self.scheduler, self.cycle, *self.tasks.values()] if task]
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        if self.tracker_task:
            self.tracker_task.cancel()
            await asyncio.gather(self.tracker_task, return_exceptions=True)
        await self.tracker.flush()

    def status(self) -> dict[str, Any]:
        return {
            "running": bool(self.cycle and not self.cycle.done()),
            "active_jobs": [key for key, task in self.tasks.items() if not task.done()],
            "last_run": self.last_run,
            "jev_configured": bool(os.getenv("TYPESAFE_API_KEY")),
            "decision_backend": self.store.settings().decision_backend,
            "decision_configured": self.store.settings().decision_backend == "switchyard"
            or bool(os.getenv("TYPESAFE_API_KEY")),
            "browser_url": os.getenv("SWITCHYARD_MCP_URL", "http://127.0.0.1:3260/mcp"),
            "submitted_today": self.store.submissions_today(),
            "excel_path": str(self.tracker.path.resolve()),
            "excel_error": self.tracker.error,
        }

    def run(self) -> None:
        if self.cycle and not self.cycle.done():
            raise ValueError("A search cycle is already running")
        if not self.store.settings().context.strip():
            raise ValueError("Save your candidate context first")
        if self.store.settings().decision_backend == "typesafe" and not os.getenv(
            "TYPESAFE_API_KEY"
        ):
            raise ValueError("Set TYPESAFE_API_KEY before starting TypeSafe ranking")
        self.cycle = asyncio.create_task(self._cycle())

    def enqueue(self, job_id: str, operation: str) -> None:
        job = self.store.job(job_id)
        if job_id in self.tasks and not self.tasks[job_id].done():
            raise ValueError("This job is already being processed")
        if job["state"] in {"submitted", "interview", "offer", "hired", "rejected", "withdrawn"}:
            raise ValueError("This application already has a recorded outcome")
        if (
            job["state"] in {"submission_uncertain", "submitting", "verifying"}
            and operation != "verify"
        ):
            raise ValueError("Verify the previous submission before any further actions")
        self.tasks[job_id] = asyncio.create_task(self._operation(job_id, operation))

    async def _operation(self, job_id: str, operation: str) -> None:
        try:
            settings, job = self.store.settings(), self.store.job(job_id)
            if operation == "rank":
                self.store.update(job_id, state="ranking", error="")
                vacancy = await self.search.hydrate(Vacancy.model_validate(job["vacancy"]))
                # Keep the full description used by the judge, not just a discovery snippet.
                from database import _db_conn

                with _db_conn() as db:
                    db.execute(
                        "UPDATE automation_jobs SET vacancy=? WHERE id=?",
                        (vacancy.model_dump_json(), job_id),
                    )
                    db.commit()
                evaluation = await self.ranker.evaluate(vacancy, settings)
                evaluation["context_sha256"] = hashlib.sha256(settings.context.encode()).hexdigest()
                self.store.update(job_id, state=evaluation["category"], evaluation=evaluation)
                self.store.event(job_id, "ranked", evaluation)
            elif operation == "prepare":
                if not job["evaluation"]:
                    raise ValueError("Rank this vacancy before preparing a CV")
                if (
                    job["evaluation"].get("context_sha256")
                    != hashlib.sha256(settings.context.encode()).hexdigest()
                ):
                    raise ValueError("Candidate context changed. Rank again before preparing a CV")
                self.store.update(job_id, state="preparing", error="")
                package = await self.documents.prepare(job, settings, self.store)
                self.store.update(job_id, state="prepared", package=package)
                self.store.event(
                    job_id,
                    "prepared",
                    {
                        "revision": package["revision"],
                        "files": package["files"],
                        "checks": package["checks"],
                    },
                )
            elif operation in {"fill", "submit", "resume", "verify"}:
                if not job["package"]:
                    raise ValueError("Prepare a verified CV before opening the form")
                if operation != "verify" and job["package"]["profile"] != settings.context:
                    raise ValueError("Candidate context changed. Rank and prepare a fresh CV first")
                if job["state"] == "submission_uncertain" and operation != "verify":
                    raise ValueError(
                        "Verify the previous submission before any further form actions"
                    )
                if operation == "verify" and job["state"] not in {
                    "submission_uncertain",
                    "verifying",
                    "submitting",
                    "needs_review",
                }:
                    raise ValueError("There is no interrupted submission to verify")
                if operation == "verify":
                    job = {**job, "state": "submission_uncertain"}
                allow = operation == "submit" or (operation == "resume" and settings.auto_submit)
                await self.browser.apply(job, settings, self.store, allow_submit=allow)
            else:
                raise ValueError("Unknown operation")
        except asyncio.CancelledError:
            job = self.store.job(job_id)
            if job["state"] not in {"submitted", "submission_uncertain"}:
                state = (
                    "submission_uncertain"
                    if job["state"] in {"submitting", "verifying"}
                    else "needs_review"
                )
                self.store.update(job_id, state=state, error="Processing interrupted")
            self.store.event(job_id, "interrupted", {"operation": operation})
            raise
        except Exception as exc:
            state = self.store.job(job_id)["state"]
            self.store.update(
                job_id,
                state="submission_uncertain"
                if state in {"submitting", "verifying", "submission_uncertain"}
                else "needs_review",
                error=str(exc),
            )
            self.store.event(job_id, "error", {"operation": operation, "message": str(exc)})

    async def _cycle(self) -> None:
        self.last_run = now()
        self.store.event(None, "cycle_started", {"at": self.last_run})
        try:
            settings = self.store.settings()
            vacancies, errors = await self.search.discover(settings)
            for error in errors:
                self.store.event(None, "source_error", error)
            ids = []
            for vacancy in vacancies:
                job = self.store.add(vacancy)
                if job["state"] == "new" and job["id"] not in ids:
                    ids.append(job["id"])
            for job_id in ids[: settings.limit]:
                if job_id in self.tasks and not self.tasks[job_id].done():
                    continue
                self.enqueue(job_id, "rank")
                await self.tasks[job_id]
                if self.store.job(job_id)["state"] != "priority":
                    continue
                if settings.auto_prepare:
                    self.enqueue(job_id, "prepare")
                    await self.tasks[job_id]
                if settings.auto_submit and self.store.job(job_id)["state"] == "prepared":
                    self.enqueue(job_id, "submit")
                    await self.tasks[job_id]
            self.store.event(
                None,
                "cycle_finished",
                {
                    "found": len(vacancies),
                    "processed": min(len(ids), settings.limit),
                    "source_errors": errors,
                },
            )
        except asyncio.CancelledError:
            self.store.event(None, "cycle_interrupted", {})
            raise
        except Exception as exc:
            self.store.event(None, "cycle_error", {"message": str(exc)})

    async def _schedule(self) -> None:
        while True:
            await asyncio.sleep(60)
            settings = self.store.settings()
            if not settings.scheduled:
                continue
            from datetime import datetime, timezone

            elapsed = (
                (datetime.now(timezone.utc) - datetime.fromisoformat(self.last_run)).total_seconds()
                if self.last_run
                else float("inf")
            )
            if elapsed >= settings.interval_hours * 3600 and not self.status()["running"]:
                try:
                    self.run()
                except ValueError:
                    pass
