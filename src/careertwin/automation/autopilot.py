"""Autopilot: one switch that lets the agents find, rank, prepare, fill and send applications without you.

It is the candidate's own standing decision, made with `careertwin autopilot on` or from the API with the
token, and never by an agent: no agent tool exposes it. It changes four saved settings and nothing else:

* `scheduled`: the search agent runs on its timer;
* `auto_prepare` and `auto_submit`: a vacancy above the threshold gets its CV and its form, and the form is sent;
* `consent_auto`: the terms and privacy boxes a form needs before it can be sent are ticked.

What it never does, with or without the switch: create an account, type a password outside the domains you
listed, solve a CAPTCHA, tick a marketing box, send mail, guess an answer your profile does not hold, go past
the daily limit or act while `data/STOP` exists. An application that needs one of those waits for you and the
next one carries on.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Literal

import httpx

from careertwin.automation import guard
from careertwin.automation.models import Settings
from careertwin.automation.store import Store

FLOOR = 80.0  # a fit below this is not worth an unattended application
DEFAULT_DAILY_LIMIT = 5


@dataclass(frozen=True)
class Check:
    status: Literal["ok", "warn", "blocker"]
    title: str
    detail: str = ""


def switchyard_reachable(timeout: float = 3.0) -> bool:
    url = os.getenv("SWITCHYARD_URL", "http://127.0.0.1:3260").rstrip("/") + "/v1/models"
    key = os.getenv("SWITCHYARD_API_KEY", "")
    try:
        return httpx.get(url, headers={"Authorization": f"Bearer {key}"} if key else {}, timeout=timeout).status_code < 500
    except httpx.HTTPError:
        return False


def readiness(store: Store, settings: Settings, *, gateway: bool | None = None) -> list[Check]:
    """What stands between the candidate and an unattended run. A blocker keeps the switch off."""
    checks: list[Check] = []
    if not settings.context.strip():
        checks.append(Check("blocker", "No ABOUT context", "The agents answer forms only from facts you wrote; there are none"))
    else:
        checks.append(Check("ok", "Your ABOUT context is loaded"))
    reachable = switchyard_reachable() if gateway is None else gateway
    checks.append(
        Check("ok", "Switchyard answers")
        if reachable
        else Check("blocker", "Switchyard does not answer", "Ranking, CVs and forms need the gateway and its Chrome extension")
    )
    if guard.stopped():
        checks.append(Check("blocker", "The kill switch is on", "Remove data/STOP (or DELETE /v1/automation/kill) first"))
    standing = store.standing()
    if standing:
        checks.append(Check("ok", f"{len(standing)} standing answers saved"))
    else:
        checks.append(
            Check(
                "warn",
                "No standing answers yet",
                "Every question your profile does not answer parks that application until you answer; save the usual ones with `careertwin answers add`",
            )
        )
    if guard.login_domains() and os.getenv("ACCOUNT_EMAIL") and os.getenv("ACCOUNT_PASSWORD"):
        checks.append(Check("ok", "The agents can sign in on " + ", ".join(sorted(guard.login_domains()))))
    else:
        checks.append(
            Check(
                "warn",
                "No login configured",
                "Sites that ask you to sign in (Workday, for one) wait for you; create the account yourself, then set ACCOUNT_EMAIL, ACCOUNT_PASSWORD and CAREERTWIN_LOGIN_DOMAINS",
            )
        )
    return checks


def blockers(checks: list[Check]) -> list[Check]:
    return [check for check in checks if check.status == "blocker"]


def enable(settings: Settings, *, daily_limit: int | None = None, threshold: float | None = None) -> Settings:
    """The settings with the switch on. The fit threshold never goes below FLOOR."""
    return settings.model_copy(
        update={
            "scheduled": True,
            "auto_prepare": True,
            "auto_submit": True,
            "consent_auto": True,
            "daily_limit": daily_limit if daily_limit is not None else min(settings.daily_limit, DEFAULT_DAILY_LIMIT),
            "threshold": max(FLOOR, threshold if threshold is not None else settings.threshold),
        }
    )


def disable(settings: Settings) -> Settings:
    return settings.model_copy(update={"scheduled": False, "auto_submit": False, "consent_auto": False})


def is_on(settings: Settings) -> bool:
    return settings.auto_submit and settings.auto_prepare


def report(store: Store, settings: Settings, *, gateway: bool | None = None) -> dict:
    """Everything the terminal and the dashboard show about the switch."""
    jobs = store.jobs()
    waiting = [
        {"job_id": job["id"], "title": job["vacancy"]["title"], "company": job["vacancy"].get("company", ""), "state": job["state"], "detail": job["error"]}
        for job in jobs
        if job["state"] in {"awaiting_answers", "needs_review", "submission_uncertain", "form_ready"}
    ]
    return {
        "on": is_on(settings),
        "consent_auto": settings.consent_auto,
        "scheduled": settings.scheduled,
        "threshold": settings.threshold,
        "daily_limit": settings.daily_limit,
        "submitted_today": store.submissions_today(),
        "submitted_total": sum(1 for job in jobs if job["state"] == "submitted"),
        "waiting_for_you": waiting,
        "checks": [check.__dict__ for check in readiness(store, settings, gateway=gateway)],
    }
