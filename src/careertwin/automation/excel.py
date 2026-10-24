from __future__ import annotations

import asyncio
import hashlib
import io
import json
import os
from pathlib import Path
from typing import Any

from careertwin.db import db_conn
from careertwin.profile.context import context_path, read_context
from careertwin.settings import OUTPUT_DIR

from .store import Store


class ExcelAgent:
    """Keep a materialized workbook alongside the durable SQLite event journal."""

    def __init__(self, store: Store):
        self.store = store
        self.path = OUTPUT_DIR / "CareerTwin-applications.xlsx"
        self.error = ""
        self.revision = None

    async def flush(self) -> None:
        with db_conn() as db:
            revision = (
                db.execute("SELECT MAX(id) FROM automation_events").fetchone()[0],
                db.execute("SELECT MAX(updated_at) FROM automation_jobs").fetchone()[0],
                hashlib.sha256(read_context().encode()).hexdigest(),
            )
        if revision == self.revision and self.path.is_file():
            return

        def write():
            self.path.parent.mkdir(parents=True, exist_ok=True)
            temporary = Path(str(self.path) + ".tmp")
            temporary.write_bytes(workbook(self.store))
            os.replace(temporary, self.path)

        try:
            writing = asyncio.create_task(asyncio.to_thread(write))
            try:
                await asyncio.shield(writing)
            except asyncio.CancelledError:
                await writing
                raise
            self.revision, self.error = revision, ""
        except Exception as exc:
            self.error = str(exc)

    async def run(self) -> None:
        while True:
            await self.flush()
            await asyncio.sleep(2)


def safe(value: Any) -> str:
    value = str(value if value is not None else "")
    return "'" + value if value.startswith(("=", "+", "-", "@")) else value


def chunks(value: Any) -> list[str]:
    text = json.dumps(value, ensure_ascii=False) if isinstance(value, (dict, list)) else str(value)
    return [safe(text[i : i + 30000]) for i in range(0, len(text), 30000)] or [""]


def workbook(store: Store) -> bytes:
    from openpyxl import Workbook
    from openpyxl.styles import Font

    book = Workbook()
    jobs = book.active
    jobs.title = "Applications"
    jobs.append(
        [
            "ID",
            "Company",
            "Role",
            "URL",
            "Source",
            "Fit score",
            "Confidence",
            "Category",
            "Status",
            "CV path",
            "Created",
            "Updated",
            "Error",
        ]
    )
    for job in store.jobs():
        vacancy, evaluation, package = job["vacancy"], job["evaluation"] or {}, job["package"] or {}
        jobs.append(
            [
                safe(value)
                for value in [
                    job["id"],
                    vacancy["company"],
                    vacancy["title"],
                    vacancy["url"],
                    vacancy["source"],
                    evaluation.get("score", ""),
                    evaluation.get("confidence", ""),
                    evaluation.get("category", ""),
                    job["state"],
                    package.get("cv_path", ""),
                    job["created_at"],
                    job["updated_at"],
                    job["error"],
                ]
            ]
        )
    answers = book.create_sheet("Answers")
    answers.append(["Job ID", "Date", "Field", "Value part", "Exact value", "Evidence"])
    history = book.create_sheet("History")
    history.append(["Event ID", "Job ID", "Date", "Stage", "Part", "Full event JSON"])
    for event in store.events():
        payload = event["payload"]
        if event["stage"] == "browser_action" and payload.get("field"):
            values = chunks(payload.get("value", ""))
            evidence = chunks(payload.get("evidence", ""))
            for index in range(max(len(values), len(evidence))):
                answers.append(
                    [
                        event["job_id"],
                        event["created_at"],
                        safe(payload["field"]),
                        index + 1,
                        values[index] if index < len(values) else "",
                        evidence[index] if index < len(evidence) else "",
                    ]
                )
        for index, part in enumerate(chunks(payload), 1):
            history.append(
                [event["id"], event["job_id"], event["created_at"], event["stage"], index, part]
            )
    snapshots = book.create_sheet("Snapshots")
    snapshots.append(["Job ID", "Part", "Full vacancy, evaluation and document package JSON"])
    for job in store.jobs():
        for index, part in enumerate(
            chunks({key: job[key] for key in ("vacancy", "evaluation", "package")}), 1
        ):
            snapshots.append([job["id"], index, part])
    files = book.create_sheet("Files")
    files.append(["Job ID", "Kind", "Absolute path", "SHA256", "Date"])
    for item in store.files():
        files.append(
            [safe(item[key]) for key in ("job_id", "kind", "path", "sha256", "created_at")]
        )
    known = book.create_sheet("Saved answers")
    known.append(["Job ID", "Question", "Part", "User answer"])
    for job_id in ["", *[job["id"] for job in store.jobs()]]:
        for question, answer in store.answers(job_id).items():
            for index, part in enumerate(chunks(answer), 1):
                known.append([job_id, safe(question), index, part])
    shared = book.create_sheet("Candidate context")
    shared.append(["ABOUT.md absolute path", "SHA256", "Part", "Current shared context"])
    content = read_context()
    checksum = hashlib.sha256(content.encode()).hexdigest()
    for index, part in enumerate(chunks(content), 1):
        shared.append([safe(str(context_path().resolve())), checksum, index, part])
    for sheet in book:
        sheet.freeze_panes = "A2"
        sheet.auto_filter.ref = sheet.dimensions
        for cell in sheet[1]:
            cell.font = Font(bold=True)
        for column in sheet.columns:
            sheet.column_dimensions[column[0].column_letter].width = min(
                65, max(15, len(str(column[0].value)) + 5)
            )
    output = io.BytesIO()
    book.save(output)
    return output.getvalue()
