"""The monitor agent: watches company career sites and files every new vacancy.

A watch list of companies (from the settings, optionally the bundled list of known employers) is
checked on a timer. For each company the agent reads its open vacancies from the application system it
uses (Greenhouse, Lever, Ashby, SmartRecruiters, Workday) or, failing that, from the career page,
compares them with what it saw last time, and files what is new and relevant into the pipeline.

It is a polite visitor: one request at a time per host, robots.txt respected for career pages, failures
back off exponentially instead of hammering a site that is down, and an empty answer from a company that
had vacancies is treated as a broken page, not as "everything was closed".
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import re
import sqlite3
import time
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Any
from urllib.parse import urlsplit
from urllib.robotparser import RobotFileParser

from careertwin import vault
from careertwin.db import db_conn
from careertwin.net import fetch_text

from . import guard
from .models import Settings, Vacancy, canonical_url
from .sources import text
from .store import Store, now

USER_AGENT = "CareerTwin"
MAX_PER_COMPANY = 200
BACKOFF_CAP = timedelta(hours=24)
EMPTY_IS_SUSPICIOUS_AFTER = 3  # a company that had this many vacancies and now has none looks broken


@dataclass(frozen=True)
class Target:
    name: str
    url: str = ""
    ats: str = ""


@dataclass
class Listing:
    title: str
    url: str
    location: str = ""
    description: str = ""
    salary: str = ""
    external_id: str = ""


Fetcher = Callable[[Target], Awaitable[list[Listing]]]


def identity(listing: Listing) -> str:
    """What makes a vacancy the same vacancy on the next visit."""
    if listing.url:
        return canonical_url(listing.url)
    return hashlib.sha256(f"{listing.title}|{listing.location}".casefold().encode()).hexdigest()[:24]


def relevant(listing: Listing, queries: list[str]) -> bool:
    """A vacancy is filed only when its title shares a meaningful word with one of the search queries."""
    words = {word.casefold() for query in queries for word in re.findall(r"\w+", query) if len(word) > 2}
    words -= {"developer", "engineer", "senior", "junior", "the", "and", "for"}
    if not words:
        return True
    title = listing.title.casefold()
    return any(word in title for word in words) or any(
        re.search(rf"\b{re.escape(query.casefold())}\b", title) for query in queries if query.strip()
    )


class Robots:
    """robots.txt of a host, fetched once a day through the safe fetch."""

    def __init__(self, fetch: Callable[[str], Awaitable[str]] | None = None):
        self.fetch = fetch or (lambda url: fetch_text(url, max_bytes=200_000, timeout=15))
        self.cache: dict[str, tuple[float, RobotFileParser | None]] = {}

    async def allowed(self, url: str) -> bool:
        parts = urlsplit(url)
        origin = f"{parts.scheme}://{parts.netloc}"
        entry = self.cache.get(origin)
        if entry is None or time.monotonic() - entry[0] > 86_400:
            parser = RobotFileParser()
            try:
                parser.parse((await self.fetch(origin + "/robots.txt")).splitlines())
            except Exception:  # no robots.txt, or it cannot be read: nothing forbids a visit
                parser = None
            entry = self.cache[origin] = (time.monotonic(), parser)
        parser = entry[1]
        return True if parser is None else parser.can_fetch(USER_AGENT, url)


async def default_fetch(target: Target, robots: Robots | None = None) -> list[Listing]:
    from careertwin.jobs.boards import _search_via_discovered_api, detect_ats_from_url, search_company_jobs

    platform = target.ats or (detect_ats_from_url(target.url) if target.url else None)
    found = await search_company_jobs(target.name, platform, "", MAX_PER_COMPANY)
    if not found and target.url:
        if robots is not None and not await robots.allowed(target.url):
            raise PermissionError("robots.txt asks robots not to read this career page")
        found = await _search_via_discovered_api(target.name, target.url)
    return [
        Listing(
            title=item.title, url=item.url, location=item.location, description=item.description,
            salary=getattr(item, "salary", ""), external_id=str(getattr(item, "raw", {}).get("id", "")),
        )
        for item in found
        if item.title
    ]


class WatchStore:
    def __init__(self) -> None:
        with db_conn() as db:
            db.executescript(
                """
                CREATE TABLE IF NOT EXISTS automation_watch (
                    name TEXT PRIMARY KEY, careers_url TEXT NOT NULL DEFAULT '', ats TEXT NOT NULL DEFAULT '',
                    enabled INTEGER NOT NULL DEFAULT 1, baseline_done INTEGER NOT NULL DEFAULT 0,
                    last_checked TEXT, last_ok TEXT, failures INTEGER NOT NULL DEFAULT 0, next_check TEXT,
                    last_error TEXT NOT NULL DEFAULT '', open_count INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE IF NOT EXISTS automation_seen (
                    company TEXT NOT NULL, key TEXT NOT NULL, title TEXT NOT NULL, url TEXT NOT NULL,
                    first_seen TEXT NOT NULL, last_seen TEXT NOT NULL, gone_at TEXT,
                    PRIMARY KEY(company, key));
                """
            )

    def targets(self, *, enabled_only: bool = True) -> list[dict[str, Any]]:
        with db_conn() as db:
            db.row_factory = sqlite3.Row
            rows = db.execute("SELECT * FROM automation_watch" + (" WHERE enabled=1" if enabled_only else "") + " ORDER BY name").fetchall()
        return [dict(row) for row in rows]

    def add(self, target: Target) -> None:
        with db_conn() as db:
            db.execute(
                "INSERT INTO automation_watch(name, careers_url, ats) VALUES(?,?,?) "
                "ON CONFLICT(name) DO UPDATE SET careers_url=excluded.careers_url, ats=excluded.ats, enabled=1",
                (target.name, target.url, target.ats),
            )
            db.commit()

    def remove(self, name: str) -> bool:
        with db_conn() as db:
            db.execute("DELETE FROM automation_seen WHERE company=?", (name,))
            cursor = db.execute("DELETE FROM automation_watch WHERE name=?", (name,))
            db.commit()
            return cursor.rowcount > 0

    def due(self, limit: int) -> list[dict[str, Any]]:
        return [row for row in self.targets() if not row["next_check"] or row["next_check"] <= now()][:limit]

    def seen(self, company: str) -> dict[str, dict[str, Any]]:
        with db_conn() as db:
            db.row_factory = sqlite3.Row
            rows = db.execute("SELECT * FROM automation_seen WHERE company=?", (company,)).fetchall()
        return {row["key"]: dict(row) for row in rows}

    def record(self, company: str, listings: dict[str, Listing], gone: list[str], reopened: list[str]) -> None:
        stamp = now()
        with db_conn() as db:
            for key, listing in listings.items():
                db.execute(
                    "INSERT INTO automation_seen(company, key, title, url, first_seen, last_seen) VALUES(?,?,?,?,?,?) "
                    "ON CONFLICT(company, key) DO UPDATE SET last_seen=excluded.last_seen, title=excluded.title, url=excluded.url, gone_at=NULL",
                    (company, key, listing.title[:300], listing.url, stamp, stamp),
                )
            for key in gone:
                db.execute("UPDATE automation_seen SET gone_at=? WHERE company=? AND key=?", (stamp, company, key))
            db.commit()

    def health(self, name: str, *, ok: bool, interval: timedelta, error: str = "", open_count: int | None = None, baseline: bool = False) -> None:
        stamp = datetime.now(timezone.utc)
        with db_conn() as db:
            row = db.execute("SELECT failures FROM automation_watch WHERE name=?", (name,)).fetchone()
            failures = 0 if ok else (row[0] if row else 0) + 1
            wait = interval if ok else min(BACKOFF_CAP, interval * (2 ** min(failures, 8)))
            db.execute(
                "UPDATE automation_watch SET last_checked=?, last_ok=CASE WHEN ? THEN ? ELSE last_ok END, failures=?, next_check=?, "
                "last_error=?, open_count=COALESCE(?, open_count), baseline_done=CASE WHEN ? THEN 1 ELSE baseline_done END WHERE name=?",
                (stamp.isoformat(), ok, stamp.isoformat(), failures, (stamp + wait).isoformat(), error[:300], open_count, baseline and ok, name),
            )
            db.commit()


class CompanyMonitor:
    def __init__(self, store: Store, fetch: Fetcher | None = None, robots: Robots | None = None):
        self.store = store
        self.watch = WatchStore()
        self.robots = robots or Robots()
        self.fetch = fetch or (lambda target: default_fetch(target, self.robots))
        self._hosts: dict[str, asyncio.Semaphore] = {}

    # -- the watch list ---------------------------------------------------------------------

    def sync(self, settings: Settings) -> int:
        """Make the watch list contain the companies of the settings (and the known employers, if asked)."""
        targets = [Target(item["name"], item.get("url") or item.get("careers") or "", item.get("ats_platform") or "") for item in settings.companies if item.get("name")]
        if settings.monitor_known_companies:
            from careertwin.settings import COMPANIES_FILE

            known = json.loads(COMPANIES_FILE.read_text())
            targets += [Target(item["name"], item.get("careers", ""), item.get("ats_platform") or "") for item in known if item.get("name")]
        existing = {row["name"] for row in self.watch.targets(enabled_only=False)}
        for target in targets:
            if target.name not in existing or target.name in {item["name"] for item in settings.companies}:
                self.watch.add(target)
        return len(targets)

    # -- one company ------------------------------------------------------------------------

    async def check(self, row: dict[str, Any], settings: Settings) -> dict[str, Any]:
        guard.assert_running()
        target = Target(row["name"], row["careers_url"], row["ats"])
        interval = timedelta(minutes=settings.monitor_interval_minutes)
        host = urlsplit(target.url).netloc or target.name
        semaphore = self._hosts.setdefault(host, asyncio.Semaphore(1))
        try:
            async with semaphore:
                listings = await asyncio.wait_for(self.fetch(target), timeout=120)
                await asyncio.sleep(settings.monitor_delay_seconds)  # one request at a time per host, with a pause
        except Exception as exc:
            error = vault.redact(f"{type(exc).__name__}: {exc}")
            self.watch.health(target.name, ok=False, interval=interval, error=error)
            self.store.event(None, "monitor_error", {"company": target.name, "message": error})
            return {"company": target.name, "ok": False, "error": error, "new": []}

        seen = self.watch.seen(target.name)
        current = {identity(item): item for item in listings[:MAX_PER_COMPANY]}
        live_before = [key for key, entry in seen.items() if not entry["gone_at"]]
        if not current and len(live_before) >= EMPTY_IS_SUSPICIOUS_AFTER:
            error = f"No vacancies returned although {len(live_before)} were open: the page may have changed"
            self.watch.health(target.name, ok=False, interval=interval, error=error)
            self.store.event(None, "monitor_error", {"company": target.name, "message": error})
            return {"company": target.name, "ok": False, "error": error, "new": []}

        fresh = [key for key in current if key not in seen]
        reopened = [key for key, entry in seen.items() if entry["gone_at"] and key in current]
        gone = [key for key in live_before if key not in current]
        baseline = not row["baseline_done"]
        wanted = [key for key in fresh if relevant(current[key], settings.queries)]
        if baseline and not settings.monitor_import_existing:
            wanted = []
        filed = []
        known_urls = {job["canonical_url"] for job in self.store.jobs()}
        for key in wanted[: settings.monitor_max_new]:
            listing = current[key]
            try:
                vacancy = Vacancy(
                    url=listing.url or target.url, title=listing.title[:300], company=target.name, source=f"company:{target.name}",
                    location=listing.location, description=text(listing.description)[:100_000], salary=listing.salary, external_id=listing.external_id,
                )
            except ValueError:
                continue  # a link that is not a public address is not filed
            if canonical_url(vacancy.url) in known_urls:
                continue
            job = self.store.add(vacancy)
            self.store.event(job["id"], "monitor_found", {"company": target.name, "title": vacancy.title, "url": vacancy.url, "baseline": baseline})
            filed.append(job["id"])
        self.watch.record(target.name, current, gone, reopened)
        self._close(target.name, current, gone, seen)
        self.watch.health(target.name, ok=True, interval=interval, open_count=len(current), baseline=True)
        return {"company": target.name, "ok": True, "open": len(current), "fresh": len(fresh), "new": filed, "closed": len(gone), "reopened": len(reopened)}

    def _close(self, company: str, current: dict[str, Listing], gone: list[str], seen: dict[str, dict[str, Any]]) -> None:
        """A vacancy that disappeared from its career site is closed in the pipeline, unless it was already applied to."""
        urls = {canonical_url(seen[key]["url"]) for key in gone if seen[key]["url"]}
        for job in self.store.jobs():
            if job["canonical_url"] in urls and job["state"] in {"new", "priority", "stretch", "needs_info", "prepared", "form_ready", "awaiting_answers"}:
                self.store.update(job["id"], state="closed", error="The vacancy is no longer on the company's career site")
                self.store.event(job["id"], "monitor_closed", {"company": company, "title": job["vacancy"]["title"]})

    # -- a round ------------------------------------------------------------------------------

    async def run(self, settings: Settings, *, everything: bool = False) -> dict[str, Any]:
        """Check the companies that are due (all of them with `everything`); at most `monitor_batch` of them."""
        self.sync(settings)
        rows = self.watch.targets() if everything else self.watch.due(settings.monitor_batch)
        gate = asyncio.Semaphore(4)

        async def one(row: dict[str, Any]) -> dict[str, Any]:
            async with gate:
                return await self.check(row, settings)

        results = await asyncio.gather(*(one(row) for row in rows[: settings.monitor_batch if not everything else None]), return_exceptions=True)
        reports = [item for item in results if isinstance(item, dict)]
        for item in results:
            if isinstance(item, guard.Stopped):
                raise item
        new = [job_id for report in reports for job_id in report["new"]]
        summary = {"checked": len(reports), "failed": sum(1 for r in reports if not r["ok"]), "new": new, "closed": sum(r.get("closed", 0) for r in reports)}
        if reports:
            self.store.event(None, "monitor_summary", {**summary, "new": len(new)})
        return summary
