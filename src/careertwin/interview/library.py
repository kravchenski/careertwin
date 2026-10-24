"""Saved interview context: one set of documents, vacancy and notes per interview."""

from __future__ import annotations

import json
import re
import shutil
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from careertwin.interview import project as project_store
from careertwin.interview.brief import BriefError, InterviewBrief
from careertwin.interview.settings import interview_dir
from careertwin.profile.context import write_context

DEFAULT_NAME = "Interview"


@dataclass
class SavedDocument:
    path: str
    kind: str = "cv"
    enabled: bool = True


@dataclass
class Interview:
    """One interview: its vacancy, the CV you sent for it, and your notes.

    ABOUT.md is shared by all interviews; everything here belongs to this one."""

    id: str = field(default_factory=lambda: uuid.uuid4().hex[:8])
    name: str = DEFAULT_NAME
    documents: list[SavedDocument] = field(default_factory=list)
    notes: str = ""
    vacancy: dict[str, str] = field(default_factory=dict)
    project: str = ""


@dataclass
class Library:
    """What survives between runs. Added files are copied into `data/interview/documents/`
    so moving or deleting the original does not break the saved context."""

    interviews: list[Interview] = field(default_factory=list)
    active: str = ""

    def __post_init__(self) -> None:
        if not self.interviews:
            self.interviews.append(Interview(name=f"{DEFAULT_NAME} 1"))
        if self.active not in {item.id for item in self.interviews}:
            self.active = self.interviews[0].id

    @property
    def current(self) -> Interview:
        return next(item for item in self.interviews if item.id == self.active)

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
        if "interviews" not in data:
            data = {"interviews": [{"name": f"{DEFAULT_NAME} 1", **data}]}
        interviews = [
            _read_interview(item) for item in data["interviews"] if isinstance(item, dict)
        ]
        return cls(interviews, str(data.get("active", "")))

    def save(self) -> None:
        self.path().parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "active": self.active,
            "interviews": [
                {
                    "id": item.id,
                    "name": item.name,
                    "documents": [vars(doc) for doc in item.documents],
                    "notes": item.notes,
                    "vacancy": item.vacancy,
                    "project": item.project,
                }
                for item in self.interviews
            ],
        }
        self.path().write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")

    def new(self, name: str = "") -> Interview:
        """Add an empty interview and make it the active one."""
        interview = Interview(name=name.strip() or f"{DEFAULT_NAME} {len(self.interviews) + 1}")
        self.interviews.append(interview)
        self.active = interview.id
        return interview

    def delete(self, interview_id: str) -> list[Path]:
        """Remove an interview; return the copied files no other interview uses (now unused)."""
        gone = next((item for item in self.interviews if item.id == interview_id), None)
        if gone is None:
            return []
        self.interviews = [item for item in self.interviews if item.id != interview_id]
        self.__post_init__()
        used = {doc.path for item in self.interviews for doc in item.documents}
        return [Path(doc.path) for doc in gone.documents if doc.path not in used]

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


def _read_interview(item: dict[str, Any]) -> Interview:
    documents = [
        SavedDocument(str(doc["path"]), str(doc.get("kind", "cv")), bool(doc.get("enabled", True)))
        for doc in item.get("documents", [])
        if isinstance(doc, dict) and doc.get("path")
    ]
    vacancy = item.get("vacancy") if isinstance(item.get("vacancy"), dict) else {}
    return Interview(
        id=str(item.get("id") or uuid.uuid4().hex[:8]),
        name=str(item.get("name") or DEFAULT_NAME),
        documents=documents,
        notes=str(item.get("notes", "")),
        vacancy={str(k): str(v) for k, v in vacancy.items()},
        project=str(item.get("project", "")),
    )


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

    # ---- interviews --------------------------------------------------------

    @property
    def current(self) -> Interview:
        return self.library.current

    def interviews(self) -> list[tuple[str, str]]:
        return [(item.id, item.name) for item in self.library.interviews]

    def interview_name(self) -> str:
        return self.current.name

    def new_interview(self, name: str = "") -> list[str]:
        """Start an empty interview and switch to it."""
        self.library.new(name)
        self.library.save()
        return self.apply_current()

    def switch_interview(self, interview_id: str) -> list[str]:
        if interview_id not in {item.id for item in self.library.interviews}:
            raise KeyError(interview_id)
        self.library.active = interview_id
        self.library.save()
        return self.apply_current()

    def rename_interview(self, name: str) -> None:
        name = name.strip()
        if name:
            self.current.name = name
            self.library.save()

    def delete_interview(self) -> list[str]:
        for file in self.library.delete(self.current.id):
            if file.parent == self.library.documents_dir() and file.exists():
                file.unlink()
        self.library.save()
        return self.apply_current()

    def apply_current(self) -> list[str]:
        """Make the brief match the active interview; return a message per file that failed."""
        self.brief.documents = []
        self.brief.notes = ""
        self.brief.project = None
        self.brief.clear_vacancy()
        return self.restore()

    def _autoname(self) -> None:
        """Name a fresh interview after its vacancy once we know what it is."""
        if re.fullmatch(rf"{DEFAULT_NAME}( \d+)?", self.current.name) and self.vacancy_label():
            label = self.vacancy_label()
            if not label.endswith("characters"):
                self.current.name = label

    # ---- restore and read --------------------------------------------------

    def restore(self) -> list[str]:
        """Apply the active interview to the brief; return a message for each file that failed."""
        problems = []
        interview = self.current
        for saved in list(interview.documents):
            try:
                document = self.brief.add_document(saved.path, saved.kind)
                document.kind, document.enabled = saved.kind, saved.enabled
            except BriefError as exc:
                problems.append(str(exc))
        if interview.notes and not self.brief.notes:
            self.brief.notes = interview.notes
        if interview.project and self.brief.project is None:
            self.brief.project = project_store.load(interview.project)
        vacancy = interview.vacancy
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

    # ---- documents ---------------------------------------------------------

    def add_documents(self, paths: list[str]) -> list[str]:
        """Add files to this interview; return one error message per bad file."""
        errors = []
        for path in paths:
            try:
                saved = self.library.copy_in(path)
                document = self.brief.add_document(saved)
            except (BriefError, OSError) as exc:
                errors.append(str(exc))
                continue
            self._remember(document.path, document.kind)
        self.library.save()
        return errors

    def _remember(self, path: str, kind: str) -> None:
        documents = self.current.documents
        if not any(item.path == path for item in documents):
            documents.append(SavedDocument(path, kind, True))

    def remove_document(self, path: str) -> None:
        self.brief.remove_document(path)
        self.current.documents = [doc for doc in self.current.documents if doc.path != path]
        still_used = any(
            doc.path == path for item in self.library.interviews for doc in item.documents
        )
        file = Path(path)
        if not still_used and file.parent == self.library.documents_dir() and file.exists():
            file.unlink()
        self.library.save()

    def update_document(self, path: str, *, kind: str | None = None, enabled: bool | None = None) -> None:
        for document in self.brief.documents:
            if document.path == path:
                document.kind = kind or document.kind
                document.enabled = document.enabled if enabled is None else enabled
        for saved in self.current.documents:
            if saved.path == path:
                saved.kind = kind or saved.kind
                saved.enabled = saved.enabled if enabled is None else enabled
        self.library.save()

    # ---- the project to defend ---------------------------------------------

    def project(self) -> project_store.ProjectContext | None:
        return self.brief.project

    def project_path(self) -> str:
        return self.current.project

    def attach_project(self, project: project_store.ProjectContext) -> None:
        """Defend this project in the active interview."""
        self.brief.project = project
        self.current.project = project.path
        self.library.save()

    def set_project_path(self, path: str) -> None:
        """Remember the folder; the analysis arrives with `attach_project`."""
        self.current.project = path
        self.brief.project = project_store.load(path)
        self.library.save()

    def remove_project(self) -> None:
        path = self.current.project
        self.brief.project = None
        self.current.project = ""
        self.library.save()
        if path and not any(item.project == path for item in self.library.interviews):
            project_store.forget(path)

    # ---- notes and vacancy -------------------------------------------------

    def set_notes(self, text: str) -> None:
        self.brief.notes = text.strip()
        self.current.notes = self.brief.notes
        self.library.save()

    def set_vacancy_text(self, text: str, title: str = "", company: str = "") -> None:
        text = text.strip()
        if not text:
            self.clear_vacancy()
            return
        self.brief.set_vacancy(title=title, company=company, description=text)
        self.current.vacancy = {"type": "text", "title": title, "company": company, "text": text}
        self._autoname()
        self.library.save()

    def set_vacancy_job(self, job_id: str) -> None:
        """Use a vacancy from the pipeline, together with the CV that was prepared for it."""
        self.brief._attach_job(job_id)
        self.current.vacancy = {"type": "job", "value": job_id}
        for document in self.brief.documents:
            self._remember(document.path, document.kind)
        self._autoname()
        self.library.save()

    def set_vacancy_url(self, url: str) -> None:
        from careertwin.interview.brief import _fetch

        self.brief.set_vacancy(description=_fetch(url))
        self.current.vacancy = {
            "type": "text", "title": "", "company": "", "text": self.brief.job_description,
        }
        self.library.save()

    def clear_vacancy(self) -> None:
        self.brief.clear_vacancy()
        self.current.vacancy = {}
        self.library.save()

    def vacancy_label(self) -> str:
        head = " at ".join(part for part in (self.brief.job_title, self.brief.company) if part)
        if head:
            return head
        if self.brief.job_description:
            return f"{len(self.brief.job_description) / 1000:.1f}k characters"
        return ""

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

    # ---- ABOUT.md ----------------------------------------------------------

    def about_text(self) -> str:
        return self.brief.candidate

    def save_about(self, text: str) -> None:
        """Write ABOUT.md (the single source of facts) and use it from now on."""
        write_context(text, self.brief.username)
        self.brief.set_candidate(text)

    def as_dict(self) -> dict[str, Any]:
        return {
            "interview": self.interview_name(),
            "documents": [vars(item) for item in self.items()],
            "vacancy": self.vacancy_label(),
        }
