"""What the coach knows: your ABOUT.md, the CV the employer received, and the vacancy."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path

from careertwin.interview.documents import DocumentError, read_document
from careertwin.interview.project import ProjectContext
from careertwin.profile.context import read_context, salary_policy

CONTEXT_CHARS = 26_000
CV_CHARS = 9_000
JOB_CHARS = 6_000
JOB_ID = re.compile(r"[0-9a-f]{24}")
LOW_PRIORITY_SECTIONS = ("applications log", "cv files", "contact")
# Parts of ABOUT.md written for CV writing and bookkeeping, not for answering a question: the
# log of applications, CV file names, the job-ad matching rules and the project catalogue.
# Dropping them makes every request about 40% smaller, so the first words come sooner.
SKIP_FOR_ANSWERS = ("applications log", "cv files", "matching a job ad")


class BriefError(ValueError):
    """The candidate context or vacancy could not be loaded."""


@dataclass
class Document:
    """A file the coach reads. `cv` is what the employer received; `about` is extra facts."""

    name: str
    text: str
    path: str = ""
    kind: str = "cv"
    enabled: bool = True


def guess_kind(name: str) -> str:
    """CVs are recognised by name; anything else is treated as extra facts about you."""
    return "cv" if re.search(r"cv|resume|резюме|curriculum|životopis", name, re.I) else "about"


@dataclass
class InterviewBrief:
    candidate: str
    job_title: str = ""
    company: str = ""
    job_description: str = ""
    salary: str = "auto"
    notes: str = ""
    documents: list[Document] = field(default_factory=list)
    project: ProjectContext | None = None

    username: str = "default"

    @classmethod
    def load(
        cls,
        *,
        username: str = "default",
        job: str | None = None,
        notes: str = "",
        resumes: list[str] | None = None,
        allow_empty: bool = False,
    ) -> InterviewBrief:
        """Build a brief from ABOUT.md, the CVs you attach and an optional vacancy.

        `job` is a pipeline vacancy id, a path to a text file with the description,
        or an http(s) URL. With a vacancy id the CV prepared for it is attached too.
        `resumes` are PDF, DOCX or text files; CAREERTWIN_RESUME adds more
        (separated like PATH entries).
        """
        try:
            candidate = read_context(username)
        except ValueError as exc:
            raise BriefError(str(exc)) from exc
        if not candidate.strip() and not allow_empty:
            raise BriefError(
                "No candidate context. Fill ABOUT.md (see ABOUT.example.md) or complete the Studio."
            )
        brief = cls(
            candidate=candidate,
            salary=_describe_salary(salary_policy(candidate)),
            notes=notes,
            username=username,
        )
        if job:
            brief._attach_job(job)
        paths = [*(resumes or []), *filter(None, os.getenv("CAREERTWIN_RESUME", "").split(os.pathsep))]
        for path in paths:
            brief.add_document(path)
        return brief

    def add_document(self, path: str | Path, kind: str | None = None) -> Document:
        """Attach a CV or any file about you; the same file is attached once."""
        resolved = str(Path(path).expanduser().resolve())
        for existing in self.documents:
            if existing.path == resolved:
                return existing
        try:
            text = read_document(resolved)
        except DocumentError as exc:
            raise BriefError(str(exc)) from exc
        name = Path(resolved).name
        document = Document(name=name, text=text, path=resolved, kind=kind or guess_kind(name))
        self.documents.append(document)
        return document

    def remove_document(self, path: str) -> None:
        self.documents = [doc for doc in self.documents if doc.path != path]

    def set_candidate(self, text: str) -> None:
        """Replace the ABOUT text (the caller saves it to ABOUT.md)."""
        self.candidate = text
        self.salary = _describe_salary(salary_policy(text))

    def set_vacancy(self, *, title: str = "", company: str = "", description: str = "") -> None:
        self.job_title, self.company, self.job_description = title, company, description

    def clear_vacancy(self) -> None:
        self.set_vacancy()

    def is_empty(self) -> bool:
        """True when there is nothing to ground an answer in."""
        return not self.candidate.strip() and not any(doc.enabled for doc in self.documents)

    def _attach_job(self, job: str) -> None:
        if JOB_ID.fullmatch(job):
            from careertwin.automation.store import Store

            try:
                record = Store().job(job)
            except KeyError as exc:
                raise BriefError(f"No vacancy with id {job} in the pipeline") from exc
            vacancy = record["vacancy"]
            self.job_title = vacancy.get("title", "")
            self.company = vacancy.get("company", "")
            self.job_description = vacancy.get("description", "")
            sent = (record.get("package") or {}).get("cv_path")
            if sent and Path(sent).is_file():
                self.add_document(sent)
        elif job.startswith(("http://", "https://")):
            self.job_description = _fetch(job)
        elif Path(job).is_file():
            self.job_description = Path(job).read_text(encoding="utf-8", errors="replace")
        else:
            raise BriefError(f"--job must be a vacancy id, a file or a URL, got {job!r}")

    def sources(self) -> list[tuple[str, str]]:
        """What the coach is working from, as (kind, label) pairs for the window."""
        items = [("about", f"ABOUT.md · {len(self.candidate) / 1000:.1f}k")] if self.candidate.strip() else []
        items += [
            (doc.kind, f"{doc.name} · {len(doc.text) / 1000:.1f}k")
            for doc in self.documents
            if doc.enabled
        ]
        if self.job_title or self.job_description:
            head = " at ".join(part for part in (self.job_title, self.company) if part)
            items.append(("job", head or "vacancy"))
        if self.project and self.project.dossier.strip():
            items.append(("project", f"project {self.project.name}"))
        return items

    def render(self) -> str:
        parts = []
        if self.candidate.strip():
            parts.append(
                f"CANDIDATE CONTEXT (ABOUT.md, verified facts)\n{fit_context(slim_for_answers(self.candidate), CONTEXT_CHARS)}"
            )
        for doc in self.documents:
            if not doc.enabled:
                continue
            heading = (
                "CV THE EMPLOYER HAS READ"
                if doc.kind == "cv"
                else "MORE FACTS ABOUT THE CANDIDATE (the employer has not seen this)"
            )
            parts.append(f"{heading}: {doc.name}\n{doc.text[:CV_CHARS]}")
        if not parts:
            parts.append("CANDIDATE CONTEXT: (none provided)")
        if self.job_description or self.job_title:
            head = " at ".join(part for part in (self.job_title, self.company) if part)
            parts.append(f"VACANCY: {head}\n{self.job_description[:JOB_CHARS]}".strip())
        parts.append(f"SALARY POLICY: {self.salary}")
        if self.notes:
            parts.append(f"CANDIDATE NOTES FOR THIS INTERVIEW\n{self.notes}")
        if self.project and (dossier := self.project.for_prompt()):
            parts.append(dossier)
        return "\n\n".join(parts)

    def stack_hint(self, limit: int = 8) -> str:
        """The candidate's main technologies, so a coding task is solved in a language they know."""
        terms = _skill_terms(self.candidate)
        for doc in self.documents:
            if doc.enabled:
                terms += _skill_terms(doc.text)
        return ", ".join(list(dict.fromkeys(terms))[:limit])

    def vocabulary(self) -> str:
        """Names worth biasing speech recognition toward: the vacancy and your skills."""
        words = [self.company, self.job_title]
        words += re.findall(r"\b[A-Z][A-Za-z+#.]{2,}\b", self.job_description)[:25]
        words += _skill_terms(self.candidate)
        for doc in self.documents:
            if doc.enabled:
                words += _skill_terms(doc.text)
        seen: dict[str, None] = {}
        for word in words:
            if word:
                seen.setdefault(word, None)
        return ", ".join(list(seen)[:45])


def slim_for_answers(text: str) -> str:
    """ABOUT.md without the sections that never help answer a question."""
    parts = re.split(r"^(?=## )", text, flags=re.M)
    skip = tuple(f"## {name}" for name in SKIP_FOR_ANSWERS)
    kept = "".join(part for part in parts if not part.lower().startswith(skip))
    return re.sub(r"^### Project catalog[\s\S]*?(?=^## |\Z)", "", kept, flags=re.M)


def fit_context(text: str, budget: int) -> str:
    """Keep ABOUT.md whole when it fits; otherwise drop its least useful sections first.

    The tail of a long ABOUT.md holds the interview stories and honest limits, so
    cutting by length would remove exactly what the coach needs.
    """
    if len(text) <= budget:
        return text
    parts = re.split(r"^(?=## )", text, flags=re.M)
    kept = list(parts)
    for marker in LOW_PRIORITY_SECTIONS:
        if sum(map(len, kept)) <= budget:
            break
        kept = [part for part in kept if not part.lower().startswith(f"## {marker}")]
    joined = "".join(kept)
    return joined if len(joined) <= budget else joined[:budget]


def _skill_terms(candidate: str) -> list[str]:
    """Technology names from the `## Skills` section of the candidate context."""
    match = re.search(r"^##\s+Skills\s*$([\s\S]*?)(?=^##\s|\Z)", candidate, re.M | re.I)
    if not match:
        return []
    terms: list[str] = []
    for line in match[1].splitlines():
        line = line.split(":", 1)[-1]
        for part in re.split(r"[,;()|]", line):
            term = part.strip(" -*\t.")
            if 0 < len(term.split()) <= 3 and len(term) <= 30 and re.search(r"[A-Z0-9]", term):
                terms.append(term)
    return terms


def _describe_salary(policy: dict) -> str:
    if policy.get("mode") != "fixed":
        return "auto: do not invent a figure; give a researched market range or ask the range"
    return "fixed: " + ", ".join(f"{k}={v}" for k, v in policy.items() if k != "mode" and v)


def _fetch(url: str) -> str:
    import asyncio

    from careertwin.jobs.scanner import _fetch_job_description_crawl4ai

    try:
        result = asyncio.run(_fetch_job_description_crawl4ai(url, title=""))
    except Exception as exc:
        raise BriefError(f"Could not fetch {url}: {type(exc).__name__}: {exc}") from exc
    if result.get("unavailable") or not result.get("description"):
        raise BriefError(f"The vacancy page {url} has no readable description")
    return result["description"]
