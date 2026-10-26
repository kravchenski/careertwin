from __future__ import annotations

import asyncio
import hashlib
import os
from typing import Any

from . import guard
from .browser import BrowserAgent
from .documents import DocumentAgent
from .excel import ExcelAgent
from .mail import MailAgent, MailError, configured as mail_configured
from .monitor import CompanyMonitor
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
        self._mail: MailAgent | None = None
        self.last_mail_check: float | None = None
        self.mail_error = ""
        self._monitor: CompanyMonitor | None = None
        self.last_monitor_tick: float | None = None
        self.monitoring: asyncio.Task | None = None

    @property
    def mail(self) -> MailAgent:
        """Created on first use: it opens the mail table, which a bare Engine does not need."""
        if self._mail is None:
            self._mail = MailAgent(self.store)
        return self._mail

    @property
    def monitor(self) -> CompanyMonitor:
        if self._monitor is None:
            self._monitor = CompanyMonitor(self.store)
        return self._monitor

    def start(self) -> None:
        self.store.initialize()
        previous = [event for event in self.store.events() if event["stage"] == "cycle_started"]
        if previous:
            self.last_run = previous[-1]["created_at"]
        self.scheduler = asyncio.create_task(self._schedule())
        self.tracker_task = asyncio.create_task(self.tracker.run())

    async def close(self) -> None:
        tasks = [task for task in [self.scheduler, self.cycle, self.monitoring, *self.tasks.values()] if task]
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
            "stopped": guard.stopped(),
            "active_jobs": [key for key, task in self.tasks.items() if not task.done()],
            "last_run": self.last_run,
            "jev_configured": bool(os.getenv("TYPESAFE_API_KEY")),
            "decision_backend": self.store.settings().decision_backend,
            "decision_configured": self.store.settings().decision_backend == "switchyard"
            or bool(os.getenv("TYPESAFE_API_KEY")),
            "browser_url": os.getenv("SWITCHYARD_MCP_URL", "http://127.0.0.1:3260/mcp"),
            "submitted_today": self.store.submissions_today(),
            "monitoring": bool(self.monitoring and not self.monitoring.done()),
            "mail_configured": mail_configured(),
            "mail_error": self.mail_error,
            "excel_path": str(self.tracker.path.resolve()),
            "excel_error": self.tracker.error,
        }

    def run(self) -> None:
        guard.assert_running()
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
        if job["state"] == "closed":
            raise ValueError("This vacancy is no longer on the company's career site")
        if (
            job["state"] in {"submission_uncertain", "submitting", "verifying"}
            and operation != "verify"
        ):
            raise ValueError("Verify the previous submission before any further actions")
        self.tasks[job_id] = asyncio.create_task(self._operation(job_id, operation))

    def resume_waiting(self, only: str = "") -> list[str]:
        """Carry on the applications that stopped on a question, once there is an answer. Only while the
        candidate has autopilot on: otherwise "resume" stays their click."""
        if not self.store.settings().auto_submit:
            return []
        resumed = []
        for job in self.store.jobs():
            if job["state"] == "awaiting_answers" and (not only or job["id"] == only):
                try:
                    self.enqueue(job["id"], "resume")
                    resumed.append(job["id"])
                except ValueError:  # already running, or finished meanwhile
                    pass
        return resumed

    async def _operation(self, job_id: str, operation: str) -> None:
        try:
            settings, job = self.store.settings(), self.store.job(job_id)
            if operation == "rank":
                self.store.update(job_id, state="ranking", error="")
                vacancy = await self.search.hydrate(Vacancy.model_validate(job["vacancy"]))
                # Keep the full description used by the judge, not just a discovery snippet.
                from careertwin.db import db_conn

                with db_conn() as db:
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

    async def _process(self, ids: list[str], settings) -> None:
        """Rank each new vacancy, prepare the CV of the strong ones, and submit when the settings allow."""
        for job_id in ids:
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

    async def run_monitor(self, settings, *, everything: bool = False) -> dict:
        summary = await self.monitor.run(settings, everything=everything)
        await self._process(summary["new"][: settings.limit], settings)
        return summary

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
            await self._process(ids[: settings.limit], settings)
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

    async def _monitor_round(self, settings) -> None:
        await self._monitor_round_now(settings, False)

    async def _monitor_round_now(self, settings, everything: bool) -> None:
        try:
            await self.run_monitor(settings, everything=everything)
        except guard.Stopped:
            pass
        except Exception as exc:
            self.store.event(None, "monitor_error", {"company": "", "message": str(exc)[:300]})

    async def _check_mail(self, settings) -> None:
        try:
            summary = await self.mail.check(settings)
            self.mail_error = ""
            if summary["new"]:
                self.store.event(None, "mail_checked", summary)
        except (MailError, guard.Stopped) as exc:
            if str(exc) != self.mail_error:  # say it once, not every half hour
                self.store.event(None, "mail_error", {"message": str(exc)})
            self.mail_error = str(exc)

    async def _schedule(self) -> None:
        import time

        while True:
            await asyncio.sleep(60)
            settings = self.store.settings()
            if settings.mail_scheduled and mail_configured() and not guard.stopped():
                due = self.last_mail_check is None or time.monotonic() - self.last_mail_check >= settings.mail_check_minutes * 60
                if due:
                    self.last_mail_check = time.monotonic()
                    await self._check_mail(settings)
            if settings.monitor_scheduled and not guard.stopped() and not (self.monitoring and not self.monitoring.done()):
                if self.last_monitor_tick is None or time.monotonic() - self.last_monitor_tick >= 300:
                    self.last_monitor_tick = time.monotonic()
                    self.monitoring = asyncio.create_task(self._monitor_round(settings))
            if not settings.scheduled or guard.stopped():
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
