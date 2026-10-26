"""Autopilot: the candidate's switch, the answers kept for every form, and the one kind of box that may be ticked."""

from __future__ import annotations

import json
from unittest.mock import AsyncMock

import pytest

from careertwin import db
from careertwin.agentic import tools as agent_tools
from careertwin.agentic.registry import all_tools
from careertwin.automation import autopilot, autopilot_cli, browser, guard
from careertwin.automation.engine import Engine
from careertwin.automation.models import Settings, Vacancy
from careertwin.automation.store import Store

PROFILE = """## Contact
| Full name | Test Candidate |
| Location | Gdańsk, Poland |
## Experience
- Built Python APIs with FastAPI.
"""


@pytest.fixture
def store(tmp_path, monkeypatch):
    monkeypatch.setattr(db, "DB_PATH", tmp_path / "test.db")
    monkeypatch.setattr(guard, "kill_file", lambda: tmp_path / "STOP")
    value = Store()
    value.initialize()
    value.save_settings(Settings(context=PROFILE))
    return value


class FakeMCP:
    page = '[ref_1] textbox "Work permit"\n[ref_2] button "Submit application"'
    instances: list = []

    def __init__(self):
        self.calls = []
        self.definitions = {name: {"name": name, "inputSchema": {}} for name in [*browser.ALLOWED, "tab_create", "create_file", "tabs_list"]}
        self.__class__.instances.append(self)

    async def connect(self):
        pass

    async def close(self):
        pass

    async def tool(self, name, args):
        self.calls.append((name, args))
        if name == "tabs_list":
            value = json.dumps({"currentTabId": 123, "tabs": [{"id": 123, "title": "t", "url": "https://example.com/jobs/python"}]})
        elif name == "tab_create":
            value = json.dumps({"tabId": 123, "page": {"text": self.page}})
        else:
            value = "Thank you for applying" if name == "click" else self.page
        return {"content": [{"type": "text", "text": value}]}


@pytest.fixture
def form(store, monkeypatch):
    FakeMCP.instances = []
    monkeypatch.setattr(browser, "MCP", FakeMCP)
    job = store.add(Vacancy(url="https://example.com/jobs/python", title="Python Developer", company="Example", description="Python " * 50))
    store.update(job["id"], state="prepared", package={"profile": PROFILE})
    return store.job(job["id"])


def clicked(name="click"):
    return [call for call in FakeMCP.instances[0].calls if call[0] == name]


# -- the switch -----------------------------------------------------------------------------------------------


def test_on_sets_the_four_switches_and_never_lowers_the_fit_threshold_below_the_floor():
    new = autopilot.enable(Settings(context=PROFILE, threshold=50, daily_limit=40), threshold=10)
    assert (new.scheduled, new.auto_prepare, new.auto_submit, new.consent_auto) == (True, True, True, True)
    assert new.threshold == autopilot.FLOOR
    assert new.daily_limit == autopilot.DEFAULT_DAILY_LIMIT  # a cautious default, not the old 40


def test_on_keeps_a_higher_threshold_and_an_explicit_limit():
    new = autopilot.enable(Settings(context=PROFILE, threshold=92), daily_limit=12)
    assert (new.threshold, new.daily_limit) == (92, 12)


def test_off_stops_sending_and_the_timer_but_leaves_the_rest():
    on = autopilot.enable(Settings(context=PROFILE, queries=["Rust Developer"]))
    off = autopilot.disable(on)
    assert (off.scheduled, off.auto_submit, off.consent_auto) == (False, False, False)
    assert off.queries == ["Rust Developer"] and off.auto_prepare is True


def test_a_missing_context_a_dead_gateway_and_the_kill_switch_each_keep_it_off(store, tmp_path, monkeypatch):
    titles = lambda checks: {check.title for check in autopilot.blockers(checks)}  # noqa: E731
    assert titles(autopilot.readiness(store, Settings(), gateway=True)) == {"No ABOUT context"}
    assert titles(autopilot.readiness(store, store.settings(), gateway=False)) == {"Switchyard does not answer"}
    monkeypatch.setattr(guard, "stopped", lambda: True)
    assert titles(autopilot.readiness(store, store.settings(), gateway=True)) == {"The kill switch is on"}


def test_the_cli_refuses_to_turn_on_with_a_blocker_and_turns_on_with_yes(store, monkeypatch):
    lines: list[str] = []
    monkeypatch.setattr(autopilot, "switchyard_reachable", lambda timeout=3.0: False)
    assert autopilot_cli.main(["on", "--yes"], out=lines.append) == 1
    assert not store.settings().auto_submit
    monkeypatch.setattr(autopilot, "switchyard_reachable", lambda timeout=3.0: True)
    assert autopilot_cli.main(["on", "--yes", "--daily-limit", "3"], out=lines.append) == 0
    saved = store.settings()
    assert saved.auto_submit and saved.consent_auto and saved.daily_limit == 3
    assert autopilot_cli.main(["off"], out=lines.append) == 0
    assert not store.settings().auto_submit


def test_the_cli_does_not_turn_on_without_a_yes_when_nobody_can_answer(store, monkeypatch):
    monkeypatch.setattr(autopilot, "switchyard_reachable", lambda timeout=3.0: True)
    monkeypatch.setattr("sys.stdin.isatty", lambda: False)
    assert autopilot_cli.main(["on"], out=lambda _: None) == 1
    assert not store.settings().auto_submit


def test_consent_alone_can_be_switched_without_the_rest_of_the_autopilot(store):
    lines: list[str] = []
    assert autopilot_cli.consent_main(["on"], out=lines.append) == 0
    saved = store.settings()
    assert saved.consent_auto and not saved.auto_submit and not saved.scheduled
    assert any("never ticks advertising" in line for line in lines)
    assert autopilot_cli.consent_main(["off"], out=lines.append) == 0
    assert not store.settings().consent_auto


def test_no_agent_can_turn_it_on_or_see_the_switch():
    assert not [tool.name for tool in all_tools() if "autopilot" in tool.name or "consent" in tool.name]
    assert not {"consent_auto", "auto_submit", "scheduled"} & set(agent_tools.SETTINGS_FIELDS)


# -- answers kept for every form ----------------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_a_standing_answer_is_evidence_on_a_form_it_was_not_given_for(store, form, monkeypatch):
    store.answer("Do you need sponsorship?", "No. Permanent resident of Poland.")
    model = AsyncMock(
        return_value=json.dumps(
            {"tool": "type", "args": {"ref": "ref_1", "text": "Permanent resident"}, "field": "Work permit", "value": "Permanent resident", "evidence": "Permanent resident of Poland"}
        )
    )
    monkeypatch.setattr(browser, "call_ai", model)
    monkeypatch.setattr(browser.JevRanker, "supports_answer", AsyncMock(return_value=True))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=False)
    assert clicked("type")
    assert "Do you need sponsorship?" in json.loads(model.await_args_list[0].args[1])["saved_answers"]


@pytest.mark.asyncio
async def test_a_standing_salary_is_never_offered_to_the_model(store, form, monkeypatch):
    store.answer("Expected salary", "20 000 PLN")
    store.answer("Do you need sponsorship?", "No")
    model = AsyncMock(return_value='{"done": true}')
    monkeypatch.setattr(browser, "call_ai", model)
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=False)
    assert set(json.loads(model.await_args_list[0].args[1])["saved_answers"]) == {"Do you need sponsorship?"}


def test_answers_cli_adds_lists_imports_and_refuses_a_salary(store, tmp_path):
    lines: list[str] = []
    assert autopilot_cli.answers_main(["add", "Relatives at the company?", "No"], out=lines.append) == 0
    assert autopilot_cli.answers_main(["add", "Salary expectation", "9000"], out=lines.append) == 1
    path = tmp_path / "answers.json"
    path.write_text(json.dumps({"Notice period": "Immediately", "Expected salary": "1"}))
    assert autopilot_cli.answers_main(["import", str(path)], out=lines.append) == 0
    assert store.standing() == {"Relatives at the company?": "No", "Notice period": "Immediately"}
    assert autopilot_cli.answers_main(["remove", "Notice period"], out=lines.append) == 0
    assert "Notice period" not in store.standing()


def test_a_standing_answer_over_the_api_resumes_the_applications_that_waited_for_it(store, form, monkeypatch):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from careertwin.api.routes import automation

    engine = Engine(store)
    queued: list[tuple[str, str]] = []
    engine.enqueue = lambda job_id, operation: queued.append((job_id, operation))
    monkeypatch.setattr(automation, "engine", engine)
    store.update(form["id"], state="awaiting_answers")
    app = FastAPI()
    app.include_router(automation.router)
    with TestClient(app) as client:
        base = "/v1/automation"
        body = {"question": "Work permit", "answer": "Permanent resident", "standing": True}
        # without autopilot "resume" stays the candidate's click
        assert client.post(f"{base}/jobs/{form['id']}/answers", json=body).json()["resumed"] == []
        assert store.standing() == {"Work permit": "Permanent resident"} and store.answers(form["id"]) == {}
        store.save_settings(autopilot.enable(store.settings()))
        assert client.put(f"{base}/standing-answers", json={"question": "Notice period", "answer": "Immediately"}).json()["resumed"] == [form["id"]]
        assert queued == [(form["id"], "resume")]
        assert client.put(f"{base}/standing-answers", json={"question": "Salary expectation", "answer": "1"}).status_code == 422
        assert client.delete(f"{base}/standing-answers", params={"question": "Notice period"}).status_code == 200


# -- the boxes ----------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    "line,expected",
    [
        ('[ref_3] checkbox "I agree to the Candidate Acknowledgement"', True),
        ('[ref_3] checkbox "I agree to the Privacy Statement"', True),
        ('[ref_3] checkbox "I certify that the information is true"', True),
        ('[ref_3] checkbox "I agree to receive job alerts and the newsletter"', False),
        ('[ref_3] checkbox "I consent to my data being shared with partners"', False),
        ('[ref_3] checkbox "Keep me in your talent community for future opportunities"', False),
        ('[ref_3] checkbox "Send me marketing emails"', False),
        ('[ref_3] button "I agree"', False),
        ('[ref_3] radio "I agree"', False),
    ],
)
def test_only_a_box_that_confirms_this_application_counts_as_an_acknowledgement(line, expected):
    assert browser.acknowledgement(line) is expected


def tick(label: str):
    return AsyncMock(
        return_value=json.dumps({"tool": "click", "args": {"ref": "ref_3"}, "field": label, "value": "agree", "evidence": browser.STANDING_CONSENT})
    )


@pytest.mark.asyncio
async def test_the_acknowledgement_is_ticked_when_the_candidate_allowed_it_and_the_tick_is_on_record(store, form, monkeypatch):
    FakeMCP.page = '[ref_1] textbox "Work permit"\n[ref_3] checkbox "I agree to the Candidate Acknowledgement"\n[ref_2] button "Submit"'
    store.save_settings(Settings(context=PROFILE, consent_auto=True))
    monkeypatch.setattr(browser, "call_ai", tick("I agree to the Candidate Acknowledgement"))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=False)
    assert clicked()
    assert [e["payload"]["field"] for e in store.events(form["id"]) if e["stage"] == "standing_consent"] == ["I agree to the Candidate Acknowledgement"]


@pytest.mark.asyncio
async def test_without_the_permission_the_same_box_is_not_ticked(store, form, monkeypatch):
    FakeMCP.page = '[ref_3] checkbox "I agree to the Candidate Acknowledgement"'
    monkeypatch.setattr(browser, "call_ai", tick("I agree to the Candidate Acknowledgement"))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=False)
    assert not clicked()
    assert store.job(form["id"])["state"] == "needs_review"


@pytest.mark.asyncio
async def test_a_marketing_box_is_not_ticked_even_with_the_permission(store, form, monkeypatch):
    FakeMCP.page = '[ref_3] checkbox "I agree to receive the newsletter and job alerts"'
    store.save_settings(Settings(context=PROFILE, consent_auto=True))
    monkeypatch.setattr(browser, "call_ai", tick("I agree to receive the newsletter and job alerts"))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=False)
    assert not clicked()
    assert store.job(form["id"])["state"] == "needs_review"


@pytest.mark.asyncio
async def test_the_permission_does_not_make_an_answer_without_evidence_acceptable(store, form, monkeypatch):
    FakeMCP.page = '[ref_1] textbox "Work permit"'
    store.save_settings(Settings(context=PROFILE, consent_auto=True))
    monkeypatch.setattr(
        browser,
        "call_ai",
        AsyncMock(return_value=json.dumps({"tool": "type", "args": {"ref": "ref_1", "text": "Yes"}, "field": "Work permit", "value": "Yes", "evidence": browser.STANDING_CONSENT})),
    )
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=False)
    assert not clicked("type")


def teardown_module():
    FakeMCP.page = '[ref_1] textbox "Work permit"\n[ref_2] button "Submit application"'


# -- what the filler knows about each application system ----------------------------------------------------------


@pytest.mark.parametrize(
    "url,expected",
    [
        ("https://boeing.wd1.myworkdayjobs.com/en-US/EXTERNAL_CAREERS/job/x/apply", "Workday"),
        ("https://intel.wd1.myworkdayjobs.com/External/job/Poland-Gdansk/X_JR1", "Workday"),
        ("https://jobs.dayforcehcm.com/kiddeglobal/CANDIDATEPORTAL/jobs/4768", "Dayforce"),
        ("https://evilmyworkdayjobs.com/apply", ""),
        ("https://example.com/jobs/1", ""),
        ("not a url", ""),
    ],
)
def test_the_notes_belong_to_the_application_system_whose_host_it_is(url, expected):
    from careertwin.automation.playbooks import playbook

    notes = playbook(url)
    assert (expected in notes) if expected else notes == ""


@pytest.mark.asyncio
async def test_the_model_is_given_the_notes_of_the_site_it_is_on(store, monkeypatch):
    FakeMCP.instances = []
    FakeMCP.tab_url = "https://acme.wd1.myworkdayjobs.com/External/job/X/apply"
    monkeypatch.setattr(browser, "MCP", FakeMCP)
    job = store.add(Vacancy(url="https://acme.wd1.myworkdayjobs.com/External/job/X", title="Intern", company="Acme", description="Python " * 50))
    store.update(job["id"], state="prepared", package={"profile": PROFILE})
    model = AsyncMock(return_value='{"done": true}')
    monkeypatch.setattr(browser, "call_ai", model)
    await browser.BrowserAgent().apply(store.job(job["id"]), store.settings(), store, allow_submit=False)
    payload = json.loads(model.await_args_list[0].args[1])
    assert "Use My Last Application" in payload["site_notes"]
    FakeMCP.tab_url = "https://example.com/jobs/python"


# -- the last look before a form is sent -------------------------------------------------------------------------


ABOUT_LINES = """- In forms, answer "eligible through work authorization / residence permit". Never answer "by virtue of citizenship".
| Countries | Belarus (citizen); Poland (permanent resident). In Workday pick "Citizen (Belarus)", never "Citizen (Poland)" |
3. **Items without proof** are never claimed. A fast honest bridge works better, for example "async Python with asyncio, ready to learn Celery".
- Forks are course tasks. They show up as "RS School coursework" at most, never as separate projects.
"""


def test_a_sentence_that_says_never_and_quotes_a_phrase_becomes_a_rule():
    from careertwin.automation import redlines

    phrases = {rule.message for rule in redlines.from_context(ABOUT_LINES)}
    assert phrases == {'the profile says never "by virtue of citizenship"', 'the profile says never "Citizen (Poland)"'}


def test_the_review_page_is_scanned_for_the_phrases_the_profile_forbids(tmp_path, monkeypatch):
    from careertwin.automation import redlines

    monkeypatch.setattr(redlines, "DATA_DIR", tmp_path)
    (tmp_path / "redlines.txt").write_text("# mine\nre:English[\\s\\S]{0,60}\\bC1\\b => English is B2+, never C1\nbroken(:re\nre:[unclosed\n")
    page = "Are you legally entitled to work?\nYes, by virtue of citizenship\nWhat is your level of English?\nC1"
    assert redlines.violations(page, ABOUT_LINES) == ['the profile says never "by virtue of citizenship"', "English is B2+, never C1"]
    assert redlines.violations("Yes, I have a work authorization visa\nPermanent Resident (Poland)", ABOUT_LINES) == []


@pytest.mark.asyncio
async def test_a_review_page_with_a_forbidden_answer_is_not_sent(store, form, monkeypatch):
    FakeMCP.page = '[ref_2] button "Submit"\nAre you legally entitled to work? Yes, by virtue of citizenship'
    store.save_settings(Settings(context=PROFILE + '\nNever answer "by virtue of citizenship".\n'))
    monkeypatch.setattr(browser, "call_ai", AsyncMock(return_value='{"tool":"click","args":{"ref":"ref_2"},"submit":true}'))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=True)
    assert not clicked()
    job = store.job(form["id"])
    assert job["state"] == "needs_review" and "by virtue of citizenship" in job["error"]
    assert store.submissions_today() == 0
    assert [e for e in store.events(form["id"]) if e["stage"] == "redline_blocked"]


@pytest.mark.asyncio
async def test_a_clean_review_page_is_sent_as_before(store, form, monkeypatch):
    FakeMCP.page = '[ref_2] button "Submit"\nAre you legally entitled to work? Yes, I have a work authorization visa'
    store.save_settings(Settings(context=PROFILE + '\nNever answer "by virtue of citizenship".\n'))
    monkeypatch.setattr(browser, "call_ai", AsyncMock(return_value='{"tool":"click","args":{"ref":"ref_2"},"submit":true}'))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=True)
    assert store.job(form["id"])["state"] == "submitted"


# -- a vacancy the candidate pasted ----------------------------------------------------------------------------


POSTING = (
    "Join Intel's Datacenter Media Software team as a Software Engineering Undergraduate Intern and work where real-time "
    "media meets AI. Our team builds software that moves, processes and understands video, audio and voice on Intel Xeon "
    "platforms. This includes the open-source Media Transport Library for SMPTE ST 2110 broadcast media over IP."
)


def pasted_posting_api(monkeypatch, jobs):
    import httpx

    from careertwin.agentic.client import CareerTwinClient

    calls = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append((request.method, request.url.path, json.loads(request.content) if request.content else None))
        if request.method == "GET":
            return httpx.Response(200, json=jobs)
        return httpx.Response(200, json={"id": "new1", "state": "new", "vacancy": json.loads(request.content)})

    monkeypatch.setattr(agent_tools, "_client", lambda: CareerTwinClient(base_url="http://api.test", transport=httpx.MockTransport(handler)))
    return calls


@pytest.mark.asyncio
async def test_a_pasted_posting_is_matched_to_the_one_in_the_pipeline(monkeypatch):
    known = {"id": "j9", "state": "priority", "vacancy": {"title": "Media AI Software Intern", "company": "Intel", "url": "https://i/1", "description": "Job Details: " + POSTING + " Qualifications: C and Python."}}
    other = {"id": "j1", "state": "new", "vacancy": {"title": "Cook", "company": "X", "url": "https://x/1", "description": "Make soup every day for the whole team in a busy kitchen of a hotel by the sea."}}
    pasted_posting_api(monkeypatch, [other, known])
    answer = await agent_tools.import_posting_text(POSTING)
    assert answer["status"] == "matched" and answer["id"] == "j9"


@pytest.mark.asyncio
async def test_an_unknown_posting_needs_its_link_and_is_never_given_one(monkeypatch):
    calls = pasted_posting_api(monkeypatch, [])
    answer = await agent_tools.import_posting_text(POSTING)
    assert answer["status"] == "needs_url" and not [c for c in calls if c[0] == "POST"]
    filed = await agent_tools.import_posting_text(POSTING, url="https://intel.wd1.myworkdayjobs.com/External/job/X", title="Media AI Software Intern", company="Intel")
    assert filed["status"] == "filed"
    assert [c for c in calls if c[0] == "POST"][0][2]["description"] == POSTING


# -- a model that answers in prose ---------------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_a_model_that_answers_in_prose_is_asked_again_before_the_application_is_given_up(store, form, monkeypatch):
    model = AsyncMock(side_effect=["Sure! I will click the button now.", '{"tool":"click","args":{"ref":"ref_2"},"submit":true}'])
    monkeypatch.setattr(browser, "call_ai", model)
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=True)
    assert model.await_count == 2
    assert "not one valid JSON object" in model.await_args_list[1].args[0]
    assert store.job(form["id"])["state"] == "submitted"


@pytest.mark.asyncio
async def test_a_model_that_never_returns_json_stops_the_application_for_review(store, form, monkeypatch):
    model = AsyncMock(return_value="I cannot help with that.")
    monkeypatch.setattr(browser, "call_ai", model)
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=True)
    assert model.await_count == 3
    assert store.job(form["id"])["state"] == "needs_review" and "invalid JSON" in store.job(form["id"])["error"]


@pytest.mark.asyncio
async def test_a_plan_with_null_fields_is_a_plan_with_empty_ones(store, form, monkeypatch):
    nulls = '{"tool":"click","args":{"ref":"ref_2"},"field":null,"value":null,"evidence":null,"submit":true,"questions":null,"message":null}'
    monkeypatch.setattr(browser, "call_ai", AsyncMock(return_value=nulls))
    await browser.BrowserAgent().apply(form, store.settings(), store, allow_submit=True)
    assert store.job(form["id"])["state"] == "submitted"


@pytest.mark.asyncio
async def test_a_tab_the_candidate_closed_is_opened_again_for_a_form_still_being_filled(store, form, monkeypatch):
    class Closed(FakeMCP):
        async def tool(self, name, args):
            if name == "read_page" and args.get("tabId") == 999:
                raise RuntimeError("Tab 999 does not exist")
            return await super().tool(name, args)

    monkeypatch.setattr(browser, "MCP", Closed)
    store.update(form["id"], tab_id=999)
    monkeypatch.setattr(browser, "call_ai", AsyncMock(return_value='{"done": true}'))
    await browser.BrowserAgent().apply(store.job(form["id"]), store.settings(), store, allow_submit=False)
    assert store.job(form["id"])["state"] == "form_ready" and store.job(form["id"])["tab_id"] == 123
    assert [name for name, _ in FakeMCP.instances[0].calls].count("tab_create") == 1


@pytest.mark.asyncio
async def test_a_closed_tab_of_a_sent_application_is_not_replaced_by_a_new_one(store, form, monkeypatch):
    class Closed(FakeMCP):
        async def tool(self, name, args):
            if name == "read_page" and args.get("tabId") == 999:
                raise RuntimeError("Tab 999 does not exist")
            return await super().tool(name, args)

    monkeypatch.setattr(browser, "MCP", Closed)
    store.update(form["id"], tab_id=999, state="submission_uncertain")
    await browser.BrowserAgent().apply(store.job(form["id"]), store.settings(), store, allow_submit=True)
    assert store.job(form["id"])["state"] == "submission_uncertain" and "does not exist" in store.job(form["id"])["error"]
    assert not [1 for name, _ in FakeMCP.instances[0].calls if name == "tab_create"]
