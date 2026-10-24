"""What the pipeline looks like, and what is waiting for the user. Read-only, computed from the stores."""

from __future__ import annotations

import statistics
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone
from typing import Any

from .mail import MailStore
from .monitor import WatchStore
from .store import Store

APPLIED = frozenset({"submitted", "interview", "offer", "hired", "rejected", "withdrawn"})
FUNNEL = (
    "new", "ranking", "priority", "stretch", "needs_info", "skip", "preparing", "prepared", "filling",
    "form_ready", "awaiting_answers", "submitting", "verifying", "submitted", "submission_uncertain",
    "needs_review", "interview", "offer", "hired", "rejected", "withdrawn", "closed",
)
REPLY_KINDS = frozenset({"interview_invite", "rejection", "needs_info", "other"})
DAYS = 14


def _day(stamp: str) -> str:
    return stamp[:10]


def stats(store: Store, days: int = DAYS) -> dict[str, Any]:
    jobs, events = store.jobs(), store.events()
    mails, watch = MailStore().list(limit=10_000), WatchStore().targets(enabled_only=False)
    by_state = Counter(job["state"] for job in jobs)
    sources: dict[str, dict[str, int]] = defaultdict(lambda: {"found": 0, "applied": 0})
    for job in jobs:
        source = job["vacancy"].get("source", "import")
        sources[source]["found"] += 1
        sources[source]["applied"] += job["state"] in APPLIED

    first_applied: dict[str, str] = {}
    first_reply: dict[str, str] = {}
    submissions: dict[str, set[str]] = defaultdict(set)
    for event in events:
        job_id = event["job_id"]
        if event["stage"] in {"submission_confirmed", "mail_sent"} and job_id:
            first_applied.setdefault(job_id, event["created_at"])
        if event["stage"] == "submit_attempt" and job_id:
            submissions[_day(event["created_at"])].add(job_id)
    for mail in sorted(mails, key=lambda row: row["created_at"]):
        if mail["direction"] == "in" and mail["kind"] in REPLY_KINDS and mail["job_id"]:
            first_reply.setdefault(mail["job_id"], mail["created_at"])
    waits = [
        (datetime.fromisoformat(first_reply[job_id]) - datetime.fromisoformat(stamp)).total_seconds() / 86_400
        for job_id, stamp in first_applied.items()
        if job_id in first_reply and first_reply[job_id] >= stamp
    ]
    applied = sum(by_state[state] for state in APPLIED)
    replied = len({job_id for job_id in first_reply if store_state(jobs, job_id) in APPLIED})

    today = datetime.now(timezone.utc).date()
    timeline = []
    for offset in range(days - 1, -1, -1):
        date = (today - timedelta(days=offset)).isoformat()
        timeline.append(
            {
                "date": date,
                "found": sum(1 for job in jobs if _day(job["created_at"]) == date),
                "submitted": len(submissions.get(date, ())),
                "replies": sum(1 for mail in mails if mail["direction"] == "in" and _day(mail["created_at"]) == date),
            }
        )
    return {
        "jobs": len(jobs),
        "by_state": {state: by_state[state] for state in FUNNEL if by_state[state]} | {state: n for state, n in by_state.items() if state not in FUNNEL},
        "by_source": dict(sorted(sources.items())),
        "applied": applied,
        "interviews": by_state["interview"] + by_state["offer"] + by_state["hired"],
        "rejections": by_state["rejected"],
        "replied": replied,
        "reply_rate": round(replied / applied, 3) if applied else None,
        "median_days_to_reply": round(statistics.median(waits), 1) if waits else None,
        "timeline": timeline,
        "submitted_today": store.submissions_today(),
        "mail": {
            "received": sum(1 for mail in mails if mail["direction"] == "in"),
            "sent": sum(1 for mail in mails if mail["direction"] == "out" and mail["state"] in {"sent", "dry_run"}),
            "drafts": sum(1 for mail in mails if mail["direction"] == "out" and mail["state"] == "draft"),
            "to_read": sum(1 for mail in mails if mail["direction"] == "in" and mail["state"] == "new"),
        },
        "monitor": {
            "watching": len(watch),
            "failing": sum(1 for row in watch if row["failures"]),
            "open": sum(row["open_count"] for row in watch),
        },
    }


def store_state(jobs: list[dict[str, Any]], job_id: str) -> str:
    return next((job["state"] for job in jobs if job["id"] == job_id), "")


def attention(store: Store) -> list[dict[str, Any]]:
    """Everything that waits for the user, most urgent first. The terminal pane and the notifications read this."""
    items: list[dict[str, Any]] = []

    def title(job: dict[str, Any]) -> str:
        vacancy = job["vacancy"]
        return f'{vacancy["title"]} at {vacancy.get("company") or "?"}'

    for job in store.jobs():
        state = job["state"]
        kind = {
            "form_ready": "submit",  # the form is filled; sending it is the user's decision
            "awaiting_answers": "answer",
            "submission_uncertain": "verify",
            "needs_review": "review",
        }.get(state)
        if kind:
            items.append({"kind": kind, "job_id": job["id"], "title": title(job), "detail": job["error"], "since": job["updated_at"], "actions": {
                "submit": ["submit", "skip"], "answer": ["answer"], "verify": ["verify"], "review": ["resume", "skip"]}[kind]})
    mails = MailStore()
    jobs = {job["id"]: job for job in store.jobs()}
    for row in mails.list(direction="out", state="draft"):
        job = jobs.get(row["job_id"])
        items.append({"kind": "email", "mail_id": row["id"], "job_id": row["job_id"], "title": f'{row["kind"].replace("_", " ")} to {row["to_addr"]}: {row["subject"]}',
                      "detail": row["body"][:300], "since": row["updated_at"], "actions": ["approve", "reject"], "job": title(job) if job else ""})
    for row in mails.list(direction="in", state="new"):
        job = jobs.get(row["job_id"])
        items.append({"kind": row["kind"], "mail_id": row["id"], "job_id": row["job_id"], "title": f'{row["kind"].replace("_", " ")}: {row["subject"]}',
                      "detail": row["summary"], "since": row["created_at"], "actions": ["handled", "interview", "rejected"], "job": title(job) if job else ""})
    order = {"interview_invite": 0, "submit": 1, "email": 2, "needs_info": 3, "answer": 4, "rejection": 5, "verify": 6, "review": 7}
    return sorted(items, key=lambda item: (order.get(item["kind"], 9), item["since"]))
