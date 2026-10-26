"""The newest vacancies, whichever site they came from: the order the candidate should look at them in."""

from __future__ import annotations

from datetime import date, datetime, timezone
from typing import Any
from urllib.parse import urlsplit

from .store import Store

# host fragment -> the name a person knows the site by (first match wins)
LABELS = (
    ("linkedin.com", "LinkedIn"),
    ("glassdoor.", "Glassdoor"),
    ("myworkdayjobs.com", "Workday"),
    ("pracuj.pl", "Pracuj.pl"),
    ("justjoin.it", "Just Join IT"),
    ("nofluffjobs.com", "No Fluff Jobs"),
    ("theprotocol.it", "theprotocol.it"),
    ("bulldogjob.", "Bulldog Job"),
    ("indeed.", "Indeed"),
    ("greenhouse.io", "Greenhouse"),
    ("lever.co", "Lever"),
    ("ashbyhq.com", "Ashby"),
    ("smartrecruiters.com", "SmartRecruiters"),
    ("remotive.", "Remotive"),
    ("remoteok.", "RemoteOK"),
    ("weworkremotely.", "We Work Remotely"),
    ("simplify.jobs", "Simplify"),
    ("talentdays.pl", "Talent Days"),
    ("olx.pl", "OLX"),
    ("infopraca.pl", "InfoPraca"),
)


HIDDEN = frozenset({"skip", "closed", "withdrawn"})  # not offered to the candidate any more


def source_label(url: str, source: str = "") -> str:
    """The site a posting is on, named for a person: `LinkedIn`, `Workday`, `Pracuj.pl`; its host otherwise."""
    host = (urlsplit(url).hostname or "").lower()
    for fragment, label in LABELS:
        if fragment in host:
            return label
    if host:
        return host.removeprefix("www.")
    return source.replace("_", " ").title() if source else ""


def _day(stamp: str) -> str:
    return stamp[:10] if stamp else ""


def newest(store: Store, *, days: int = 0, limit: int = 100, today: date | None = None) -> list[dict[str, Any]]:
    """Vacancies newest first. The day is the one the employer published when the source says it (`dated`),
    else the day the agent found the posting. With `days`, only those from that many days back."""
    today = today or datetime.now(timezone.utc).date()
    rows = []
    for job in store.jobs():
        vacancy, evaluation = job["vacancy"], job["evaluation"] or {}
        posted = _day(vacancy.get("posted_at", ""))
        day = posted or _day(job["created_at"])
        try:
            age = (today - date.fromisoformat(day)).days
        except ValueError:
            age = None
        if days and (age is None or age > days):
            continue
        rows.append(
            {
                "id": job["id"],
                "title": vacancy["title"],
                "company": vacancy.get("company", ""),
                "location": vacancy.get("location", ""),
                "url": vacancy["url"],
                "source": source_label(vacancy["url"], vacancy.get("source", "")),
                "day": day,
                "dated": bool(posted),
                "age_days": age,
                "state": job["state"],
                "score": evaluation.get("score"),
                "category": evaluation.get("category", ""),
                "error": job["error"],
            }
        )
    rows.sort(key=lambda row: (row["day"], row["score"] if row["score"] is not None else -1), reverse=True)
    return rows[:limit]
