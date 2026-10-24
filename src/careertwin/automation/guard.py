"""Limits around the agents that no model output can talk its way past.

* Where a form may be filled: the employer's own site (the registrable domain of the vacancy link) and
  the application systems employers use. A page that sends the tab somewhere else stops the agent
  before it types anything, so an instruction hidden in a posting cannot make it hand your CV or
  answers to another site.
* Where the account password may be typed: only on the domains you list in `CAREERTWIN_LOGIN_DOMAINS`,
  which is empty by default. A form on a shared application system is not a place for it: anyone can
  publish a page there.
* The kill switch: a file, `data/STOP`. While it exists nothing is submitted and nothing is sent, and
  it works from any shell (`touch data/STOP`) even when the API is down.
"""

from __future__ import annotations

import json
import os
from collections.abc import Iterable
from pathlib import Path
from urllib.parse import urlsplit

from careertwin.settings import DATA_DIR

# Application systems used by many employers. The employer's own domain is always allowed as well.
ATS_DOMAINS = (
    "greenhouse.io", "lever.co", "ashbyhq.com", "smartrecruiters.com", "myworkdayjobs.com", "workday.com",
    "workable.com", "bamboohr.com", "recruitee.com", "teamtailor.com", "personio.de", "personio.com",
    "join.com", "breezy.hr", "icims.com", "taleo.net", "jobvite.com", "pinpointhq.com", "comeet.com",
    "successfactors.com", "successfactors.eu", "oraclecloud.com", "eightfold.ai", "rippling.com",
    "hibob.com", "dover.com", "jazz.co", "applytojob.com", "recruiterbox.com", "hirehive.com",
)
TWO_PART_SUFFIXES = frozenset({
    "co.uk", "org.uk", "ac.uk", "com.pl", "com.au", "co.jp", "com.br", "co.in", "com.ua", "com.tr",
    "co.nz", "com.cn", "com.sg", "co.za", "com.mx", "com.ar", "co.il", "com.hk", "co.kr",
})


class Stopped(RuntimeError):
    """The kill switch is on."""


class Blocked(RuntimeError):
    """The agent was about to act where it may not."""


def registrable(host: str) -> str:
    """`jobs.example.co.uk` -> `example.co.uk`. A heuristic: no public-suffix list is shipped."""
    labels = host.lower().rstrip(".").split(".")
    if len(labels) <= 2:
        return ".".join(labels)
    tail = ".".join(labels[-2:])
    return ".".join(labels[-3:]) if tail in TWO_PART_SUFFIXES else tail


def host_of(url: str) -> str:
    try:
        return (urlsplit(url).hostname or "").lower().rstrip(".")
    except ValueError:
        return ""


def _listed(variable: str) -> set[str]:
    return {registrable(item.strip().lower()) for item in os.getenv(variable, "").split(",") if item.strip()}


def login_domains() -> set[str]:
    """Where the account password may be typed (CAREERTWIN_LOGIN_DOMAINS, comma-separated)."""
    return _listed("CAREERTWIN_LOGIN_DOMAINS")


def form_domains(vacancy_url: str) -> set[str]:
    """Where a form for this vacancy may be filled."""
    employer = registrable(host_of(vacancy_url))
    return {employer, *ATS_DOMAINS, *_listed("CAREERTWIN_FORM_DOMAINS"), *login_domains()} - {""}


def host_allowed(url: str, allowed: Iterable[str]) -> bool:
    host = host_of(url)
    return bool(host) and registrable(host) in set(allowed)


def password_allowed(url: str) -> bool:
    return host_allowed(url, login_domains())


def tab_url(listing: str, tab_id: int | str) -> str | None:
    """The address of one tab from the text a `tabs_list` call returned; None when it is not there."""
    try:
        data = json.loads(listing)
    except ValueError:
        return None
    tabs = data.get("tabs", []) if isinstance(data, dict) else data
    for tab in tabs if isinstance(tabs, list) else []:
        if isinstance(tab, dict) and str(tab.get("id", tab.get("tabId"))) == str(tab_id):
            url = tab.get("url")
            return url if isinstance(url, str) else None
    return None


def kill_file() -> Path:
    return DATA_DIR / "STOP"


def stopped() -> bool:
    return kill_file().exists()


def stop(reason: str = "stopped by the user") -> None:
    path = kill_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(reason + "\n")


def resume() -> None:
    kill_file().unlink(missing_ok=True)


def assert_running() -> None:
    if stopped():
        raise Stopped("The kill switch is on (data/STOP). Remove it, or call DELETE /v1/automation/kill, to continue.")
