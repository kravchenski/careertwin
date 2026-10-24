"""The monitor watches company career sites: it files what is new, closes what vanished, and is polite."""

from __future__ import annotations

import asyncio
import json
from datetime import datetime, timedelta
from unittest.mock import AsyncMock

import pytest

from careertwin import db
from careertwin.automation import guard, monitor
from careertwin.automation.models import Settings
from careertwin.automation.monitor import CompanyMonitor, Listing, Robots, Target
from careertwin.automation.store import Store

PROFILE = "## Contact\n| Full name | Alex Demo |\n| Email | alex@example.com |\n## Experience\n- Built Python APIs.\n"


@pytest.fixture
def store(tmp_path, monkeypatch):
    monkeypatch.setattr(db, "DB_PATH", tmp_path / "test.db")
    monkeypatch.setattr(guard, "DATA_DIR", tmp_path / "guard")
    value = Store()
    value.initialize()
    value.save_settings(Settings(context=PROFILE))
    return value


def settings(**values):
    values.setdefault("monitor_delay_seconds", 0)
    values.setdefault("queries", ["Python Developer"])
    return Settings(context=PROFILE, **values)


def job(title, url=None, location="Remote"):
    return Listing(title=title, url=url or f"https://careers.acme.example/jobs/{title.lower().replace(' ', '-')}", location=location, description=f"<p>{title} at Acme. Build things.</p>")


class Site:
    """A career site whose list of vacancies the test changes between visits."""

    def __init__(self, *listings):
        self.listings, self.calls, self.fail = list(listings), 0, None

    async def __call__(self, target):
        self.calls += 1
        if self.fail:
            raise self.fail
        return list(self.listings)


@pytest.fixture
def acme(store):
    site = Site(job("Python Developer"), job("Senior Python Engineer"), job("Sales Manager"))
    watcher = CompanyMonitor(store, fetch=site)
    watcher.watch.add(Target("Acme", "https://careers.acme.example"))
    return watcher, site


async def visit(watcher, **values):
    return await watcher.run(settings(**values), everything=True)


def pipeline(store):
    return {item["vacancy"]["title"]: item["state"] for item in store.jobs()}


# ---- what is filed -------------------------------------------------------------------------------


async def test_the_first_visit_files_the_relevant_vacancies_already_open(store, acme):
    watcher, _ = acme
    summary = await visit(watcher)
    assert summary["checked"] == 1 and summary["failed"] == 0 and len(summary["new"]) == 2
    assert pipeline(store) == {"Python Developer": "new", "Senior Python Engineer": "new"}  # the sales role is not relevant
    found = [e for e in store.events() if e["stage"] == "monitor_found"]
    assert len(found) == 2 and all(e["payload"]["baseline"] for e in found)
    first = store.jobs()[0]["vacancy"]
    assert first["source"] == "company:Acme" and first["company"] == "Acme" and "<p>" not in first["description"]


async def test_the_first_visit_can_be_silent(store, acme):
    watcher, _ = acme
    summary = await visit(watcher, monitor_import_existing=False)
    assert summary["new"] == [] and store.jobs() == []
    assert len(watcher.watch.seen("Acme")) == 3  # but they are remembered, so only later ones are news


async def test_only_what_appeared_since_the_last_visit_is_filed(store, acme):
    watcher, site = acme
    await visit(watcher)
    assert (await visit(watcher))["new"] == []  # nothing changed, nothing filed again
    site.listings.append(job("Python Backend Developer"))
    summary = await visit(watcher)
    assert len(summary["new"]) == 1 and pipeline(store)["Python Backend Developer"] == "new"
    found = [e for e in store.events() if e["stage"] == "monitor_found"]
    assert found[-1]["payload"]["baseline"] is False


async def test_a_vacancy_already_in_the_pipeline_is_not_filed_twice(store, acme):
    watcher, site = acme
    await visit(watcher)
    watcher.watch.remove("Acme")
    watcher.watch.add(Target("Acme", "https://careers.acme.example"))  # a fresh watch of a known site
    await visit(watcher)
    assert len(store.jobs()) == 2
    assert len([e for e in store.events() if e["stage"] == "monitor_found"]) == 2  # and it was announced once


async def test_no_more_than_the_limit_is_filed_per_visit(store, acme):
    watcher, site = acme
    site.listings = [job(f"Python Developer {i}") for i in range(30)]
    summary = await visit(watcher, monitor_max_new=4)
    assert len(summary["new"]) == 4


@pytest.mark.parametrize(
    ("title", "queries", "expected"),
    [
        ("Senior Python Engineer", ["Python Developer"], True), ("Sales Manager", ["Python Developer"], False),
        ("Backend Developer", ["Python Developer"], False), ("Python Developer (m/f/d)", ["Python Developer"], True),
        ("Machine Learning Engineer", ["AI Engineer", "Machine Learning"], True), ("Anything", [], True),
        ("Developer", ["Developer"], True), ("Frontend Developer", ["Frontend Developer"], True),
    ],
)
def test_relevance_is_a_shared_word_with_a_search_query(title, queries, expected):
    assert monitor.relevant(Listing(title=title, url="https://x.example/1"), queries) is expected


async def test_a_link_that_does_not_point_to_a_public_address_is_not_filed(store, acme):
    watcher, site = acme
    site.listings = [job("Python Developer", url="http://127.0.0.1:8000/admin"), job("Python Engineer", url="https://careers.acme.example/jobs/2")]
    summary = await visit(watcher)
    assert len(summary["new"]) == 1 and list(pipeline(store)) == ["Python Engineer"]


# ---- what disappears and what comes back ---------------------------------------------------------


async def test_a_vacancy_that_leaves_the_site_is_closed_unless_it_was_applied_to(store, acme):
    watcher, site = acme
    await visit(watcher)
    developer = next(item for item in store.jobs() if item["vacancy"]["title"] == "Python Developer")
    engineer = next(item for item in store.jobs() if item["vacancy"]["title"] == "Senior Python Engineer")
    store.update(engineer["id"], state="submitted")
    site.listings = [job("Sales Manager")]
    site.listings += [job("Python Contract", url="https://careers.acme.example/jobs/x"), job("Other Python", url="https://careers.acme.example/jobs/y")]
    summary = await visit(watcher)
    assert summary["closed"] == 2
    assert store.job(developer["id"])["state"] == "closed" and "no longer" in store.job(developer["id"])["error"]
    assert store.job(engineer["id"])["state"] == "submitted"  # an application is history, not a draft
    assert any(e["stage"] == "monitor_closed" for e in store.events(developer["id"]))


async def test_a_closed_vacancy_cannot_be_worked_on(store, acme):
    from careertwin.automation.engine import Engine

    watcher, site = acme
    await visit(watcher)
    victim = store.jobs()[0]
    store.update(victim["id"], state="closed")
    with pytest.raises(ValueError, match="no longer"):
        Engine(store).enqueue(victim["id"], "rank")


async def test_a_vacancy_that_returns_is_not_filed_again(store, acme):
    watcher, site = acme
    await visit(watcher)
    removed = site.listings.pop(0)
    site.listings.append(job("Python Pad", url="https://careers.acme.example/jobs/pad"))
    await visit(watcher)
    site.listings.append(removed)
    summary = await visit(watcher)
    assert summary["new"] == [] and len(store.jobs()) == 3


async def test_an_empty_answer_from_a_company_with_vacancies_is_a_broken_page_not_a_mass_closing(store, acme):
    watcher, site = acme
    await visit(watcher)
    site.listings = []
    summary = await visit(watcher)
    assert summary["failed"] == 1 and summary["closed"] == 0
    assert all(item["state"] == "new" for item in store.jobs())
    assert "page may have changed" in watcher.watch.targets()[0]["last_error"]


# ---- health and politeness -----------------------------------------------------------------------


async def test_a_failing_site_is_visited_less_and_less_often_and_the_error_is_kept(store, acme, monkeypatch):
    watcher, site = acme
    monkeypatch.setenv("ACCOUNT_PASSWORD", "hunter2-hunter2")
    site.fail = ConnectionError("cannot connect, credentials hunter2-hunter2")
    waits = []
    for _ in range(4):
        await visit(watcher, monitor_interval_minutes=60)
        row = watcher.watch.targets()[0]
        waits.append(datetime.fromisoformat(row["next_check"]) - datetime.fromisoformat(row["last_checked"]))
    assert [row for row in waits] == sorted(waits) and waits[3] >= waits[0] * 4 - timedelta(seconds=1)
    assert row["failures"] == 4 and "ConnectionError" in row["last_error"] and "hunter2" not in row["last_error"]
    assert not any("hunter2" in json.dumps(e["payload"]) for e in store.events())
    site.fail = None
    await visit(watcher, monitor_interval_minutes=60)
    assert watcher.watch.targets()[0]["failures"] == 0 and watcher.watch.targets()[0]["last_error"] == ""


async def test_the_backoff_has_a_ceiling(store, acme):
    watcher, site = acme
    site.fail = ConnectionError("down")
    for _ in range(12):
        await visit(watcher, monitor_interval_minutes=600)
    row = watcher.watch.targets()[0]
    assert datetime.fromisoformat(row["next_check"]) - datetime.fromisoformat(row["last_checked"]) <= monitor.BACKOFF_CAP + timedelta(seconds=1)


async def test_only_the_companies_that_are_due_are_visited(store, acme):
    watcher, site = acme
    await watcher.run(settings(), everything=False)
    await watcher.run(settings(), everything=False)  # not due again yet
    assert site.calls == 1
    await watcher.run(settings(), everything=True)
    assert site.calls == 2


async def test_two_companies_on_one_host_are_never_visited_at_the_same_time(store):
    running, peak = 0, 0

    async def slow(target):
        nonlocal running, peak
        running += 1
        peak = max(peak, running)
        await asyncio.sleep(0.02)
        running -= 1
        return [job("Python Developer", url=f"https://careers.shared.example/{target.name}")]

    watcher = CompanyMonitor(store, fetch=slow)
    for name in ("A", "B", "C"):
        watcher.watch.add(Target(name, "https://careers.shared.example"))
    await visit(watcher)
    assert peak == 1


async def test_the_batch_size_limits_a_round(store):
    watcher = CompanyMonitor(store, fetch=AsyncMock(return_value=[]))
    for index in range(6):
        watcher.watch.add(Target(f"Co{index}", ""))
    summary = await watcher.run(settings(monitor_batch=2), everything=False)
    assert summary["checked"] == 2


async def test_the_kill_switch_stops_the_monitor(store, acme):
    watcher, site = acme
    guard.stop("test")
    with pytest.raises(guard.Stopped):
        await visit(watcher)
    assert site.calls == 0 and store.jobs() == []


# ---- robots.txt ----------------------------------------------------------------------------------


async def test_robots_txt_is_respected_and_fetched_once():
    calls = []

    async def fetch(url):
        calls.append(url)
        return "User-agent: *\nDisallow: /careers/private\nAllow: /"

    robots = Robots(fetch=fetch)
    assert await robots.allowed("https://acme.example/careers") is True
    assert await robots.allowed("https://acme.example/careers/private/x") is False
    assert calls == ["https://acme.example/robots.txt"]


async def test_a_site_without_robots_txt_is_open():
    async def missing(url):
        raise ConnectionError("404")

    assert await Robots(fetch=missing).allowed("https://acme.example/careers") is True


async def test_a_career_page_that_asks_not_to_be_read_is_skipped(store, monkeypatch):
    async def deny(url):
        return "User-agent: *\nDisallow: /"

    async def none(*args, **kwargs):
        return []

    import careertwin.jobs.boards as boards

    monkeypatch.setattr(boards, "search_company_jobs", none)
    called = []

    async def discover(name, url):
        called.append(url)
        return []

    monkeypatch.setattr(boards, "_search_via_discovered_api", discover)
    with pytest.raises(PermissionError, match="robots.txt"):
        await monitor.default_fetch(Target("Acme", "https://careers.acme.example"), Robots(fetch=deny))
    assert called == []


# ---- the watch list ------------------------------------------------------------------------------


def test_the_watch_list_follows_the_settings(store):
    watcher = CompanyMonitor(store, fetch=Site())
    cfg = settings(companies=[{"name": "Acme", "url": "https://careers.acme.example", "ats_platform": "greenhouse"}, {"name": "Globex", "careers": "https://globex.example/jobs"}])
    assert watcher.sync(cfg) == 2
    assert {row["name"]: (row["careers_url"], row["ats"]) for row in watcher.watch.targets()} == {
        "Acme": ("https://careers.acme.example", "greenhouse"), "Globex": ("https://globex.example/jobs", ""),
    }


def test_the_known_employers_are_watched_only_when_asked(store):
    watcher = CompanyMonitor(store, fetch=Site())
    watcher.sync(settings())
    assert watcher.watch.targets() == []
    assert watcher.sync(settings(monitor_known_companies=True)) >= 200 and len(watcher.watch.targets()) >= 200


def test_a_company_can_be_removed_with_what_was_remembered_about_it(store, acme):
    watcher, _ = acme
    watcher.watch.record("Acme", {"k": job("Python Developer")}, [], [])
    assert watcher.watch.remove("Acme") and not watcher.watch.remove("Acme")
    assert watcher.watch.seen("Acme") == {}


# ---- the engine and the API ----------------------------------------------------------------------


async def test_new_vacancies_go_through_ranking_and_preparing(store, acme, monkeypatch):
    from careertwin.automation.engine import Engine

    engine = Engine(store)
    engine._monitor = acme[0]
    process = AsyncMock()
    monkeypatch.setattr(engine, "_process", process)
    summary = await engine.run_monitor(settings(), everything=True)
    process.assert_awaited_once()
    assert process.await_args.args[0] == summary["new"] and len(summary["new"]) == 2


@pytest.fixture
def api(store, acme, monkeypatch):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from careertwin.api.routes import automation
    from careertwin.automation.engine import Engine

    engine = Engine(store)
    engine._monitor = acme[0]
    monkeypatch.setattr(automation, "engine", engine)
    app = FastAPI()
    app.include_router(automation.router)
    with TestClient(app) as client:
        yield client, engine


def test_the_api_lists_adds_and_removes_companies(api):
    client, _ = api
    base = "/v1/automation/monitor"
    assert [row["name"] for row in client.get(base).json()["watching"]] == ["Acme"]
    assert client.post(base + "/companies", json={"name": "Globex", "url": "https://globex.example/jobs", "ats_platform": "lever"}).json()["status"] == "watching"
    assert {row["name"] for row in client.get(base).json()["watching"]} == {"Acme", "Globex"}
    assert client.delete(base + "/companies/Globex").status_code == 200
    assert client.delete(base + "/companies/Globex").status_code == 404


@pytest.mark.parametrize("body", [{"name": "X", "url": "http://127.0.0.1/admin"}, {"name": "X", "url": "file:///etc/passwd"}, {"name": "X", "ats_platform": "myspace"}, {"name": ""}])
def test_the_api_refuses_a_company_that_points_inside_or_is_malformed(api, body):
    client, _ = api
    assert client.post("/v1/automation/monitor/companies", json=body).status_code == 422


def test_the_api_runs_the_monitor_in_the_background_unless_stopped(api, monkeypatch):
    client, engine = api
    ran = []

    async def fake(settings, everything):
        ran.append(everything)

    monkeypatch.setattr(engine, "_monitor_round_now", fake)
    assert client.post("/v1/automation/monitor/run?everything=true").json() == {"status": "started"}
    guard.stop("test")
    assert client.post("/v1/automation/monitor/run").status_code == 423
