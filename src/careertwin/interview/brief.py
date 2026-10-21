"""What the coach knows: the candidate's context and the vacancy being discussed."""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

from careertwin.profile.context import read_context, salary_policy

CONTEXT_CHARS = 12_000
JOB_CHARS = 6_000
JOB_ID = re.compile(r"[0-9a-f]{24}")


class BriefError(ValueError):
    """The candidate context or vacancy could not be loaded."""


@dataclass
class InterviewBrief:
    candidate: str
    job_title: str = ""
    company: str = ""
    job_description: str = ""
    salary: str = "auto"
    notes: str = ""

    @classmethod
    def load(cls, *, username: str = "default", job: str | None = None, notes: str = "") -> InterviewBrief:
        """Build a brief from ABOUT.md and an optional vacancy.

        `job` is a pipeline vacancy id, a path to a text file with the description,
        or an http(s) URL of the vacancy page.
        """
        try:
            candidate = read_context(username)
        except ValueError as exc:
            raise BriefError(str(exc)) from exc
        if not candidate.strip():
            raise BriefError(
                "No candidate context. Fill ABOUT.md (see ABOUT.example.md) or complete the Studio."
            )
        policy = salary_policy(candidate)
        brief = cls(
            candidate=candidate,
            salary=_describe_salary(policy),
            notes=notes,
        )
        if job:
            brief._attach_job(job)
        return brief

    def _attach_job(self, job: str) -> None:
        if JOB_ID.fullmatch(job):
            from careertwin.automation.store import Store

            try:
                vacancy = Store().job(job)["vacancy"]
            except KeyError as exc:
                raise BriefError(f"No vacancy with id {job} in the pipeline") from exc
            self.job_title = vacancy.get("title", "")
            self.company = vacancy.get("company", "")
            self.job_description = vacancy.get("description", "")
        elif job.startswith(("http://", "https://")):
            self.job_description = _fetch(job)
        elif Path(job).is_file():
            self.job_description = Path(job).read_text(encoding="utf-8", errors="replace")
        else:
            raise BriefError(f"--job must be a vacancy id, a file or a URL, got {job!r}")

    def render(self) -> str:
        parts = [f"CANDIDATE CONTEXT\n{self.candidate[:CONTEXT_CHARS]}"]
        if self.job_description or self.job_title:
            head = " at ".join(part for part in (self.job_title, self.company) if part)
            parts.append(f"VACANCY: {head}\n{self.job_description[:JOB_CHARS]}".strip())
        parts.append(f"SALARY POLICY: {self.salary}")
        if self.notes:
            parts.append(f"CANDIDATE NOTES FOR THIS INTERVIEW\n{self.notes}")
        return "\n\n".join(parts)

    def vocabulary(self) -> str:
        """Names worth biasing speech recognition toward: the vacancy and your skills."""
        words = [self.company, self.job_title]
        words += re.findall(r"\b[A-Z][A-Za-z+#.]{2,}\b", self.job_description)[:25]
        words += _skill_terms(self.candidate)
        seen: dict[str, None] = {}
        for word in words:
            if word:
                seen.setdefault(word, None)
        return ", ".join(list(seen)[:45])


def _skill_terms(candidate: str) -> list[str]:
    """Technology names from the `## Skills` section of the candidate context."""
    match = re.search(r"^##\s+Skills\s*$([\s\S]*?)(?=^##\s|\Z)", candidate, re.M | re.I)
    if not match:
        return []
    terms: list[str] = []
    for line in match[1].splitlines():
        line = line.split(":", 1)[-1]
        for part in re.split(r"[,;()]", line):
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
