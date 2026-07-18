"""Application tracking — tracks job applications and their status."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from pathlib import Path


class ApplicationStatus(str, Enum):
    DRAFT = "draft"
    APPLIED = "applied"
    PHONE_SCREEN = "phone_screen"
    TECHNICAL_INTERVIEW = "technical_interview"
    ONSITE = "onsite"
    OFFER = "offer"
    ACCEPTED = "accepted"
    REJECTED = "rejected"
    WITHDRAWN = "withdrawn"


@dataclass
class ApplicationNote:
    content: str
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())


@dataclass
class JobApplication:
    id: str
    job_id: str
    company: str
    position: str
    status: ApplicationStatus = ApplicationStatus.DRAFT
    applied_at: str = ""
    resume_version: str = ""
    cover_letter_version: str = ""
    notes: list[ApplicationNote] = field(default_factory=list)
    url: str = ""
    salary_expected: str = ""
    referral: str = ""


@dataclass
class ApplicationStats:
    total: int = 0
    by_status: dict[str, int] = field(default_factory=dict)
    response_rate: float = 0.0
    interview_rate: float = 0.0
    offer_rate: float = 0.0
    avg_response_days: float = 0.0


class ApplicationTracker:
    """Tracks job applications with persistence."""

    def __init__(self, data_path: str = ".applications.json"):
        self.data_path = Path(data_path)
        self._applications: list[JobApplication] | None = None

    def get_applications(self) -> list[JobApplication]:
        if self._applications is None:
            self._applications = self._load()
        return self._applications

    def add_application(self, application: JobApplication) -> JobApplication:
        apps = self.get_applications()
        apps.append(application)
        self._save(apps)
        return application

    def update_status(self, app_id: str, status: ApplicationStatus) -> JobApplication | None:
        apps = self.get_applications()
        for app in apps:
            if app.id == app_id:
                app.status = status
                self._save(apps)
                return app
        return None

    def add_note(self, app_id: str, content: str) -> JobApplication | None:
        apps = self.get_applications()
        for app in apps:
            if app.id == app_id:
                app.notes.append(ApplicationNote(content=content))
                self._save(apps)
                return app
        return None

    def get_stats(self) -> ApplicationStats:
        apps = self.get_applications()
        stats = ApplicationStats(total=len(apps))

        for app in apps:
            status = app.status.value
            stats.by_status[status] = stats.by_status.get(status, 0) + 1

        if stats.total > 0:
            responded = sum(
                1
                for a in apps
                if a.status
                not in (
                    ApplicationStatus.DRAFT,
                    ApplicationStatus.APPLIED,
                )
            )
            interviews = sum(
                1
                for a in apps
                if a.status
                in (
                    ApplicationStatus.PHONE_SCREEN,
                    ApplicationStatus.TECHNICAL_INTERVIEW,
                    ApplicationStatus.ONSITE,
                )
            )
            offers = sum(
                1
                for a in apps
                if a.status
                in (ApplicationStatus.OFFER, ApplicationStatus.ACCEPTED)
            )

            stats.response_rate = responded / stats.total
            stats.interview_rate = interviews / stats.total
            stats.offer_rate = offers / stats.total

        return stats

    def _load(self) -> list[JobApplication]:
        if not self.data_path.exists():
            return []
        try:
            data = json.loads(self.data_path.read_text())
            return [JobApplication(**item) for item in data]
        except (json.JSONDecodeError, KeyError):
            return []

    def _save(self, applications: list[JobApplication]) -> None:
        data = []
        for app in applications:
            data.append(
                {
                    "id": app.id,
                    "job_id": app.job_id,
                    "company": app.company,
                    "position": app.position,
                    "status": app.status.value,
                    "applied_at": app.applied_at,
                    "resume_version": app.resume_version,
                    "cover_letter_version": app.cover_letter_version,
                    "notes": [{"content": n.content, "created_at": n.created_at} for n in app.notes],
                    "url": app.url,
                    "salary_expected": app.salary_expected,
                    "referral": app.referral,
                }
            )
        self.data_path.write_text(json.dumps(data, indent=2))
