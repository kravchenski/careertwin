"""Saved interview context: the documents, vacancy and notes you set up in the window."""

from __future__ import annotations

import json
import re
import shutil
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from careertwin.interview.brief import BriefError, InterviewBrief
from careertwin.interview.settings import interview_dir
from careertwin.profile.context import write_context


@dataclass
class SavedDocument:
    path: str
    kind: str = "cv"
    enabled: bool = True


@dataclass
class Library:
    """What survives between runs. Added files are copied into `data/interview/documents/`
    so moving or deleting the original does not break the saved context."""

    documents: list[SavedDocument] = field(default_factory=list)
    notes: str = ""
    vacancy: dict[str, str] = field(default_factory=dict)

    @staticmethod
    def path() -> Path:
        return interview_dir() / "context.json"

    @staticmethod
    def documents_dir() -> Path:
        return interview_dir() / "documents"

    @classmethod
    def load(cls) -> Library:
        try:
            data = json.loads(cls.path().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return cls()
        if not isinstance(data, dict):
            return cls()
        documents = [
            SavedDocument(str(item["path"]), str(item.get("kind", "cv")), bool(item.get("enabled", True)))
            for item in data.get("documents", [])
            if isinstance(item, dict) and item.get("path")
        ]
        vacancy = data.get("vacancy") if isinstance(data.get("vacancy"), dict) else {}
        return cls(documents, str(data.get("notes", "")), {str(k): str(v) for k, v in vacancy.items()})

    def save(self) -> None:
        self.path().parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "documents": [vars(doc) for doc in self.documents],
            "notes": self.notes,
            "vacancy": self.vacancy,
        }
        self.path().write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")

    def copy_in(self, source: str | Path) -> Path:
        """Copy a file into the library under a unique name and return the copy."""
        source = Path(source).expanduser()
        directory = self.documents_dir()
        directory.mkdir(parents=True, exist_ok=True)
        stem = re.sub(r"[^\w.\- ]+", "_", source.stem) or "document"
        target = directory / f"{stem}{source.suffix.lower()}"
        number = 2
        while target.exists() and target.read_bytes() != source.read_bytes():
            target = directory / f"{stem}-{number}{source.suffix.lower()}"
            number += 1
        if not target.exists():
            shutil.copyfile(source, target)
        return target


@dataclass
class ContextItem:
    path: str
    name: str
    kind: str
    enabled: bool
    chars: int


class ContextController:
    """Changes the brief and keeps the library in step. The window calls this; it has no Qt."""

    def __init__(self, brief: InterviewBrief, library: Library):
        self.brief = brief
        self.library = library

    def restore(self) -> list[str]:
        """Apply the saved context to the brief; return a message for each file that failed."""
        problems = []
        for saved in list(self.library.documents):
            try:
                document = self.brief.add_document(saved.path, saved.kind)
                document.kind, document.enabled = saved.kind, saved.enabled
            except BriefError as exc:
                problems.append(str(exc))
        if self.library.notes and not self.brief.notes:
            self.brief.notes = self.library.notes
        vacancy = self.library.vacancy
        if vacancy.get("type") == "text" and not self.brief.job_description:
            self.brief.set_vacancy(
                title=vacancy.get("title", ""),
                company=vacancy.get("company", ""),
                description=vacancy.get("text", ""),
            )
        elif vacancy.get("type") == "job" and not self.brief.job_description:
            try:
                self.brief._attach_job(vacancy["value"])
            except BriefError as exc:
                problems.append(str(exc))
        return problems

    def items(self) -> list[ContextItem]:
        return [
            ContextItem(doc.path, doc.name, doc.kind, doc.enabled, len(doc.text))
            for doc in self.brief.documents
        ]

    def sources(self) -> list[tuple[str, str]]:
        return self.brief.sources()

    def add_documents(self, paths: list[str]) -> list[str]:
        """Add files to the context and the library; return one error message per bad file."""
        errors = []
        for path in paths:
            try:
                saved = self.library.copy_in(path)
                document = self.brief.add_document(saved)
            except (BriefError, OSError) as exc:
                errors.append(str(exc))
                continue
            if not any(item.path == document.path for item in self.library.documents):
                self.library.documents.append(SavedDocument(document.path, document.kind, True))
        self.library.save()
        return errors

    def remove_document(self, path: str) -> None:
        self.brief.remove_document(path)
        self.library.documents = [doc for doc in self.library.documents if doc.path != path]
        file = Path(path)
        if file.parent == self.library.documents_dir() and file.exists():
            file.unlink()
        self.library.save()

    def update_document(self, path: str, *, kind: str | None = None, enabled: bool | None = None) -> None:
        for document in self.brief.documents:
            if document.path == path:
                document.kind = kind or document.kind
                document.enabled = document.enabled if enabled is None else enabled
        for saved in self.library.documents:
            if saved.path == path:
                saved.kind = kind or saved.kind
                saved.enabled = saved.enabled if enabled is None else enabled
        self.library.save()

    def set_notes(self, text: str) -> None:
        self.brief.notes = text.strip()
        self.library.notes = self.brief.notes
        self.library.save()

    def set_vacancy_text(self, text: str, title: str = "", company: str = "") -> None:
        text = text.strip()
        if not text:
            self.clear_vacancy()
            return
        self.brief.set_vacancy(title=title, company=company, description=text)
        self.library.vacancy = {"type": "text", "title": title, "company": company, "text": text}
        self.library.save()

    def set_vacancy_job(self, job_id: str) -> None:
        """Use a vacancy from the pipeline, together with the CV that was prepared for it."""
        self.brief._attach_job(job_id)
        self.library.vacancy = {"type": "job", "value": job_id}
        self.library.save()

    def set_vacancy_url(self, url: str) -> None:
        from careertwin.interview.brief import _fetch

        self.brief.set_vacancy(description=_fetch(url))
        self.library.vacancy = {"type": "text", "title": "", "company": "", "text": self.brief.job_description}
        self.library.save()

    def clear_vacancy(self) -> None:
        self.brief.clear_vacancy()
        self.library.vacancy = {}
        self.library.save()

    def vacancy_label(self) -> str:
        head = " at ".join(part for part in (self.brief.job_title, self.brief.company) if part)
        if head:
            return head
        return f"{len(self.brief.job_description) / 1000:.1f}k characters" if self.brief.job_description else ""

    def pipeline(self, limit: int = 40) -> list[tuple[str, str]]:
        """(vacancy id, "Title at Company") for the newest vacancies in the pipeline."""
        try:
            from careertwin.automation.store import Store

            jobs = Store().jobs()[:limit]
        except Exception:
            return []
        rows = []
        for job in jobs:
            vacancy = job.get("vacancy") or {}
            title = " at ".join(p for p in (vacancy.get("title"), vacancy.get("company")) if p)
            rows.append((job["id"], title or vacancy.get("url", job["id"])))
        return rows

    def about_text(self) -> str:
        return self.brief.candidate

    def save_about(self, text: str) -> None:
        """Write ABOUT.md (the single source of facts) and use it from now on."""
        write_context(text, self.brief.username)
        self.brief.set_candidate(text)

    def as_dict(self) -> dict[str, Any]:
        return {"documents": [vars(item) for item in self.items()], "vacancy": self.vacancy_label()}
