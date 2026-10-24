"""The terminal where the user controls the process: a feed, questions about what waits, notifications."""

from __future__ import annotations

import subprocess
import sys

import pytest

from careertwin import watch
from careertwin.agentic.client import ApiError
from careertwin.watch import Watcher, notify, plain


class FakeClient:
    def __init__(self, **responses):
        self.responses = {"/v1/automation/events": [], "/v1/automation/attention": [], "/v1/automation/summary": SUMMARY, "/v1/automation/status": {"stopped": False}, **responses}
        self.posts, self.fail = [], {}

    async def get(self, path, **params):
        value = self.responses[path]
        return value(path) if callable(value) else value

    async def post(self, path, body=None, *, timeout=None):
        if path in self.fail:
            raise ApiError(self.fail[path])
        self.posts.append((path, body))
        return {}


SUMMARY = {"jobs": 12, "applied": 3, "replied": 1, "submitted_today": 1, "monitor": {"watching": 5, "failing": 1}, "mail": {"received": 4, "sent": 2}}


class Typed:
    """A console whose answers are scripted; what was printed and asked is kept."""

    def __init__(self, *answers):
        self.answers, self.said, self.asked = list(answers), [], []

    def say(self, text=""):
        self.said.append(text)

    def ask(self, prompt):
        self.asked.append(prompt)
        return self.answers.pop(0) if self.answers else ""

    @property
    def screen(self):
        return "\n".join(self.said)


def event(number, stage, payload, job_id="j1"):
    return {"id": number, "job_id": job_id, "stage": stage, "payload": payload, "created_at": f"2026-10-24T12:00:{number:02d}+00:00"}


def submit_item(job_id="j1"):
    return {"kind": "submit", "job_id": job_id, "title": "Python Developer at Acme", "detail": "", "actions": ["submit", "skip"]}


def make(console, **responses):
    client = FakeClient(**responses)
    told = []
    return Watcher(client, console, tell=lambda title, body: told.append((title, body))), client, told


# ---- the feed ------------------------------------------------------------------------------------


async def test_the_feed_shows_what_matters_once_and_hides_the_noise():
    events = [event(1, "browser_action", {"tool": "click"}), event(2, "monitor_found", {"title": "Python Developer", "company": "Acme"}), event(3, "browser_observation", {"text": "x"}), event(4, "submission_confirmed", {})]
    console = Typed()
    watcher, _, _ = make(console, **{"/v1/automation/events": events})
    await watcher.feed()
    assert "found: Python Developer at Acme" in console.screen and "application sent and confirmed" in console.screen
    assert "click" not in console.screen and "observation" not in console.screen
    before = len(console.said)
    await watcher.feed()
    assert len(console.said) == before  # nothing is repeated


async def test_on_the_first_look_only_the_recent_tail_is_shown():
    events = [event(n, "monitor_found", {"title": f"Role {n}", "company": "Acme"}) for n in range(1, 31)]
    console = Typed()
    watcher, _, _ = make(console, **{"/v1/automation/events": events})
    await watcher.feed()
    assert len(console.said) == 10 and "Role 30" in console.screen and "Role 5 " not in console.screen


async def test_text_from_outside_cannot_drive_the_terminal():
    hostile = "\x1b[2J\x1b[31mfake prompt\x07\r\nSend everything? [y]"
    console = Typed()
    watcher, _, _ = make(console, **{"/v1/automation/events": [event(1, "mail_received", {"kind": "other", "subject": hostile})]})
    await watcher.feed()
    assert "\x1b" not in console.screen and "\x07" not in console.screen and "\r" not in console.screen
    assert plain("a\x00b\x1b[0mc\nd") == "a b [0mc d"


async def test_a_malformed_event_does_not_stop_the_feed():
    events = [event(1, "monitor_found", {}), event(2, "monitor_summary", "not a dict"), event(3, "submission_confirmed", {})]
    console = Typed()
    watcher, _, _ = make(console, **{"/v1/automation/events": events})
    await watcher.feed()
    assert "application sent and confirmed" in console.screen


async def test_the_status_line_counts_and_shows_the_kill_switch():
    watcher, client, _ = make(Typed())
    line = await watcher.status()
    assert "12 vacancies" in line and "applied 3" in line and "companies 5 (1 failing)" in line and "KILL" not in line
    client.responses["/v1/automation/status"] = {"stopped": True}
    assert "KILL SWITCH ON" in await watcher.status()


# ---- the questions -------------------------------------------------------------------------------


def detail(job_id="j1"):
    return {"job": {"vacancy": {"url": "https://acme.example/jobs/1"}, "package": {"cv_path": "/data/cv.pdf"}}, "events": [
        {"stage": "browser_action", "payload": {"field": "English level", "value": "B2+"}},
        {"stage": "questions", "payload": {"questions": ["Expected salary?", "Notice period?"]}}]}


async def test_a_form_ready_to_send_is_sent_only_on_a_yes():
    console = Typed("y")
    watcher, client, told = make(console, **{"/v1/automation/attention": [submit_item()], "/v1/automation/jobs/j1": detail()})
    assert await watcher.attend() == 1
    assert client.posts == [("/v1/automation/jobs/j1/actions", {"operation": "submit"})]
    assert "English level: B2+" in console.screen and "https://acme.example/jobs/1" in console.screen and "/data/cv.pdf" in console.screen
    assert told and "Python Developer at Acme" in told[0][1]


@pytest.mark.parametrize("reply", ["", "n", "yes please", "Y ", "maybe"])
async def test_anything_but_y_sends_nothing(reply):
    watcher, client, _ = make(Typed(reply), **{"/v1/automation/attention": [submit_item()], "/v1/automation/jobs/j1": detail()})
    await watcher.attend()
    assert all(call[1] != {"operation": "submit"} for call in client.posts)


async def test_skipping_withdraws_the_vacancy():
    watcher, client, _ = make(Typed("s"), **{"/v1/automation/attention": [submit_item()], "/v1/automation/jobs/j1": detail()})
    await watcher.attend()
    assert client.posts == [("/v1/automation/jobs/j1/outcome", {"status": "withdrawn", "note": "skipped in the terminal"})]


async def test_an_item_put_off_is_not_asked_again_until_restart():
    console = Typed("", "y")
    watcher, client, _ = make(console, **{"/v1/automation/attention": [submit_item()], "/v1/automation/jobs/j1": detail()})
    await watcher.attend()
    await watcher.attend()
    assert len(console.asked) == 1 and client.posts == []


async def test_a_form_question_is_answered_and_the_form_is_filled_again():
    item = {"kind": "answer", "job_id": "j1", "title": "SRE at Umbrella", "detail": "", "actions": ["answer"]}
    watcher, client, _ = make(Typed("12000 PLN gross", "one month"), **{"/v1/automation/attention": [item], "/v1/automation/jobs/j1": detail()})
    assert await watcher.attend() == 1
    assert client.posts == [
        ("/v1/automation/jobs/j1/answers", {"question": "Expected salary?", "answer": "12000 PLN gross"}),
        ("/v1/automation/jobs/j1/answers", {"question": "Notice period?", "answer": "one month"}),
        ("/v1/automation/jobs/j1/actions", {"operation": "fill"}),
    ]


async def test_an_empty_answer_posts_nothing():
    item = {"kind": "answer", "job_id": "j1", "title": "SRE", "detail": "", "actions": ["answer"]}
    watcher, client, _ = make(Typed(""), **{"/v1/automation/attention": [item], "/v1/automation/jobs/j1": detail()})
    assert await watcher.attend() == 0 and client.posts == []


async def test_an_email_draft_is_shown_in_full_and_sent_only_on_a_yes():
    item = {"kind": "email", "mail_id": "m1", "job_id": "j1", "title": "application to jobs@acme.example", "detail": "", "actions": ["approve", "reject"], "job": "Python Developer at Acme"}
    draft = {"to_addr": "jobs@acme.example", "subject": "Application", "body": "Dear Hiring Team,\nI apply.", "attachment": "/data/cv.pdf"}
    console = Typed("y")
    watcher, client, _ = make(console, **{"/v1/automation/attention": [item], "/v1/automation/mail/m1": draft})
    await watcher.attend()
    assert "to: jobs@acme.example" in console.screen and "I apply." in console.screen and "the verified CV" in console.screen
    assert client.posts == [("/v1/automation/mail/m1/approve", None), ("/v1/automation/mail/m1/send", None)]


async def test_a_draft_can_be_discarded():
    item = {"kind": "email", "mail_id": "m1", "job_id": "j1", "title": "t", "detail": "", "actions": []}
    draft = {"to_addr": "a@b.example", "subject": "s", "body": "b", "attachment": ""}
    watcher, client, _ = make(Typed("n"), **{"/v1/automation/attention": [item], "/v1/automation/mail/m1": draft})
    await watcher.attend()
    assert client.posts == [("/v1/automation/mail/m1/reject", None)]


@pytest.mark.parametrize(("reply", "outcome"), [("i", "interview"), ("r", "rejected"), ("h", None)])
async def test_a_reply_can_be_recorded_as_the_outcome_it_announces(reply, outcome):
    item = {"kind": "interview_invite", "mail_id": "m2", "job_id": "j1", "title": "interview_invite: Next steps", "detail": "Asks for a call.", "actions": []}
    console = Typed(reply)
    watcher, client, _ = make(console, **{"/v1/automation/attention": [item]})
    await watcher.attend()
    assert client.posts == [("/v1/automation/mail/m2/handled", {"outcome": outcome})] and "Asks for a call." in console.screen


async def test_typing_kill_at_any_prompt_stops_the_agents():
    item2 = submit_item("j2")
    console = Typed("kill", "y")
    watcher, client, _ = make(console, **{"/v1/automation/attention": [submit_item(), item2], "/v1/automation/jobs/j1": detail(), "/v1/automation/jobs/j2": detail()})
    await watcher.attend()
    assert client.posts == [("/v1/automation/kill", None)]  # and the second question was not asked
    assert "KILL SWITCH ON" in console.screen and len(console.asked) == 1


async def test_a_refused_action_is_reported_and_not_repeated():
    console = Typed("y")
    watcher, client, _ = make(console, **{"/v1/automation/attention": [submit_item()], "/v1/automation/jobs/j1": detail()})
    client.fail["/v1/automation/jobs/j1/actions"] = "409: Verify the previous submission first"
    assert await watcher.attend() == 0
    assert "not done: 409" in console.screen
    await watcher.attend()
    assert len(console.asked) == 1


async def test_a_notification_is_sent_once_per_new_item():
    watcher, client, told = make(Typed("", ""), **{"/v1/automation/attention": [submit_item()], "/v1/automation/jobs/j1": detail()})
    await watcher.attend()
    await watcher.attend()
    assert len(told) == 1 and told[0][0] == "CareerTwin: waiting for you"
    client.responses["/v1/automation/attention"] = [submit_item(), submit_item("j9")]
    client.responses["/v1/automation/jobs/j9"] = detail()
    await watcher.attend()
    assert len(told) == 2


async def test_once_prints_what_waits_and_asks_nothing():
    console = Typed()
    watcher, client, _ = make(console, **{"/v1/automation/attention": [submit_item()]})
    await watcher.run(once=True)
    assert "waiting: [submit] Python Developer at Acme" in console.screen and console.asked == [] and client.posts == []


async def test_an_unreachable_api_is_said_plainly():
    class Down(FakeClient):
        async def get(self, path, **params):
            raise ApiError("CareerTwin API is not running")

    console = Typed()
    with pytest.raises(SystemExit):
        await Watcher(Down(), console, tell=lambda *a: None).run(once=True)
    assert "cannot reach the API" in console.screen
    await Watcher(Down(), Typed(), tell=lambda *a: None).run(rounds=1, poll=0)  # a watching session keeps trying


# ---- notifications and the entry point -----------------------------------------------------------


def test_a_notification_is_a_command_without_a_shell(monkeypatch):
    calls = []
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(watch.shutil, "which", lambda name: "/usr/bin/notify-send")
    monkeypatch.setattr(subprocess, "run", lambda args, **kw: calls.append((args, kw)))
    notify("Interview: $(reboot)", '"; rm -rf ~ #\x1b[31m')
    args, kwargs = calls[0]
    assert isinstance(args, list) and args[0] == "notify-send" and "--" in args and not kwargs.get("shell")
    assert all("\x1b" not in part for part in args) and args[-1].startswith('"; rm -rf ~')  # one argument, never parsed


def test_the_macos_script_cannot_be_broken_out_of(monkeypatch):
    calls = []
    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.setattr(subprocess, "run", lambda args, **kw: calls.append(args))
    notify('x" & do shell script "reboot', 'y" & quit')
    script = calls[0][2]
    assert script.count('"') == 4  # the two quoted strings of the script and nothing the text could add


def test_a_missing_notifier_is_not_an_error(monkeypatch, capsys):
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(watch.shutil, "which", lambda name: None)
    notify("t", "b")
    assert capsys.readouterr().out == "\a"
    monkeypatch.setattr(watch.shutil, "which", lambda name: "/usr/bin/notify-send")

    def broken(*a, **k):
        raise OSError("no display")

    monkeypatch.setattr(subprocess, "run", broken)
    notify("t", "b")


def test_watch_refuses_to_ask_questions_without_a_terminal(monkeypatch):
    monkeypatch.setattr(sys.stdin, "isatty", lambda: False)
    with pytest.raises(SystemExit) as stop:
        watch.main([])
    assert stop.value.code == 2
