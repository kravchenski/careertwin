"""A failure is readable in the window and kept in a log, and the skills table gives a clean stack."""

from __future__ import annotations

import os
import time

import pytest

from careertwin.interview import runlog
from careertwin.interview.brief import InterviewBrief, _skill_terms


# ---- the log ---------------------------------------------------------------


def test_the_log_gets_a_timestamped_line_per_message_on_one_line(tmp_path):
    runlog.log("error: the provider said\nsomething   long")
    runlog.log("screenshot: captured 119 KB in 15 ms")
    lines = runlog.path().read_text().splitlines()
    assert len(lines) == 2
    assert lines[0][:4].isdigit() and lines[0].endswith("error: the provider said something long")
    assert lines[1].endswith("captured 119 KB in 15 ms")


def test_a_big_log_is_rotated_and_an_unwritable_one_never_raises(tmp_path, monkeypatch):
    monkeypatch.setattr(runlog, "MAX_BYTES", 200)
    for number in range(40):
        runlog.log(f"line {number} " + "x" * 20)
    assert runlog.path().with_name("interview.log.1").exists() and runlog.path().stat().st_size < 400
    blocker = tmp_path / "file"
    blocker.write_text("not a folder")
    monkeypatch.setattr(runlog, "path", lambda: blocker / "interview.log")
    runlog.log("this goes nowhere")  # no exception: a log must not break the window


def test_the_tests_never_touch_the_real_log():
    assert "run-log" in str(runlog.path())


# ---- the window ----------------------------------------------------------------


@pytest.fixture
def window(monkeypatch):
    pytest.importorskip("PySide6")
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from careertwin.interview.overlay import Bridge, OverlayWindow, create_app

    app = create_app([])
    bridge = Bridge()
    win = OverlayWindow(bridge, on_ask=lambda _: None, on_hint=lambda: None, on_pause=lambda _: None)
    win.show()
    app.processEvents()
    yield win, bridge, app
    win.close()


LONG = "No AI gateway answers at http://127.0.0.1:3260. Pick a provider in Settings → AI, or start Switchyard."


def test_an_error_is_written_out_above_the_answer_not_only_in_the_narrow_pill(window):
    win, bridge, app = window
    bridge.started.emit("[screenshot]")
    bridge.error.emit(LONG)
    app.processEvents()
    text = win.answer.toPlainText()
    assert "No AI gateway answers at http://127.0.0.1:3260" in text and "Settings → AI" in text
    assert text.index("⚠") < text.index("Screenshot")  # above the empty entry the failure left
    assert win.last_state == ("error", LONG) and win.pill.toolTip().startswith(LONG[:40])
    assert "error: No AI gateway" in runlog.path().read_text()


def test_an_error_shows_even_when_there_is_no_answer_yet(window):
    win, bridge, app = window
    bridge.error.emit("Call audio is not available on macOS yet.")
    app.processEvents()
    assert "Call audio is not available on macOS yet." in win.answer.toPlainText()


def test_the_error_goes_away_when_work_starts_again(window):
    win, bridge, app = window
    bridge.error.emit(LONG)
    app.processEvents()
    assert "⚠" in win.answer.toPlainText()
    bridge.state.emit("thinking")
    app.processEvents()
    assert "⚠" not in win.answer.toPlainText()
    bridge.error.emit(LONG)
    bridge.started.emit("What is REST?")
    app.processEvents()
    assert "⚠" not in win.answer.toPlainText() and win.error_text == ""
    bridge.error.emit(LONG)
    app.processEvents()
    win.error_text = ""  # a new screenshot clears it too
    win.shot.setEnabled(False)  # (no tool in this window: only the clearing is under test)
    assert "⚠" in win.answer.toPlainText()


def test_a_failed_screenshot_names_the_step_that_failed(window, monkeypatch):
    from careertwin.interview import screenshot
    from careertwin.interview.coach import StreamingCoach
    from careertwin.interview.library import ContextController, Library
    from careertwin.interview.runtime import Runtime
    from careertwin.interview.session import InterviewSession
    from careertwin.interview.settings import InterviewSettings
    from careertwin.interview.transcript import TranscriptLog

    win, bridge, app = window
    monkeypatch.setattr(screenshot, "available_tools", lambda: ["grim"])
    brief = InterviewBrief(candidate="x")
    coach = StreamingCoach(brief)
    session = InterviewSession(brief, None, coach, bridge.callbacks(), log=TranscriptLog(runlog.path().parent))
    win.runtime = Runtime(InterviewSettings(), ContextController(brief, Library()), coach, session)
    win.shot.setEnabled(True)
    monkeypatch.setattr(screenshot, "capture", lambda tool="auto", output=None: (_ for _ in ()).throw(screenshot.ScreenshotError("grim failed: no output")))
    win.shot.clicked.emit()
    end = time.monotonic() + 5
    while not win.error_text and time.monotonic() < end:
        app.processEvents()
        time.sleep(0.01)
    assert win.error_text == "The screenshot failed: grim failed: no output"
    assert "The screenshot failed" in win.answer.toPlainText() and win.windowOpacity() == 1.0
    log = runlog.path().read_text()
    assert "screenshot: scope=all" in log and "The screenshot failed: grim failed: no output" in log


# ---- the skills table ---------------------------------------------------------------


TABLE = """## Skills

| Level | Skills |
| --- | --- |
| Strong | TypeScript, JavaScript, Python, Java, Spring Boot, SQL / PostgreSQL, Docker |
| Working | FastAPI, React (some), Vite |
| Self-ratings (1-5 scale in forms) | Python 4, Java 4 |
"""


def test_the_header_and_the_first_column_of_the_skills_table_are_not_skills():
    terms = _skill_terms(TABLE)
    assert terms[:5] == ["TypeScript", "JavaScript", "Python", "Java", "Spring Boot"]
    for noise in ("Level", "Skills", "Strong", "Working", "---"):
        assert noise not in terms
    assert "FastAPI" in terms and "React" in terms


def test_plain_skill_lists_still_work():
    text = "## Skills\n- Languages: Python, Java\n- Tools: Docker, Git\n"
    assert _skill_terms(text) == ["Python", "Java", "Docker", "Git"]


def test_the_stack_hint_is_clean(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text("## Contact\n| Full name | A |\n" + TABLE)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    hint = InterviewBrief.load().stack_hint()
    assert hint.startswith("TypeScript, JavaScript, Python, Java") and "Level" not in hint and "Strong" not in hint


# ---- a reply that is cut off ---------------------------------------------------------


def sse_reply(*parts, finish="stop"):
    import json

    import httpx

    events = [{"choices": [{"delta": {"content": part}}]} for part in parts]
    events.append({"choices": [{"delta": {}, "finish_reason": finish}]})
    body = "".join(f"data: {json.dumps(event)}\n\n" for event in events) + "data: [DONE]\n\n"
    return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=body)


def coach_replying(response):
    import httpx

    from careertwin.interview.coach import StreamingCoach

    brief = InterviewBrief(candidate="x")
    return StreamingCoach(brief, url="http://gateway", model="m", transport=httpx.MockTransport(lambda request: response))


def test_the_coach_knows_why_a_reply_ended():
    import threading

    coach = coach_replying(sse_reply("POINTS:\n- a", finish="length"))
    assert "".join(coach.stream("[screenshot]", [], threading.Event(), "en", "QUJD")) == "POINTS:\n- a"
    assert coach.last_finish == "length"
    coach = coach_replying(sse_reply("POINTS:\n- a"))
    list(coach.stream("What is REST?", [], threading.Event(), "en"))
    assert coach.last_finish == "stop"


def test_a_reply_that_is_not_streamed_reports_its_end_too():
    import json
    import threading

    import httpx

    body = {"choices": [{"message": {"content": "done"}, "finish_reason": "length"}]}
    coach = coach_replying(httpx.Response(200, content=json.dumps(body), headers={"content-type": "application/json"}))
    assert "".join(coach.stream("Q", [], threading.Event(), "en")) == "done" and coach.last_finish == "length"


def test_a_solution_gets_room_and_is_told_not_to_think_aloud():
    from careertwin.interview.coach import SCREENSHOT_MAX_TOKENS
    from careertwin.interview.prompts import SCREENSHOT_SYSTEM

    assert SCREENSHOT_MAX_TOKENS >= 3000
    assert "Never think aloud" in SCREENSHOT_SYSTEM and "no comments in the code except the one" in SCREENSHOT_SYSTEM


def test_the_session_says_so_when_the_model_ran_out_of_room(tmp_path):
    from careertwin.interview.session import Callbacks, InterviewSession
    from careertwin.interview.transcript import TranscriptLog

    class Cut:
        last_finish = "length"

        def stream(self, question, history, cancel, language=None, image=None):
            yield "POINTS:\n- a\nANSWER:\nint x ="

    statuses, done = [], []
    session = InterviewSession(
        InterviewBrief(candidate="x"), None, Cut(),
        Callbacks(on_status=statuses.append, on_suggestion_done=done.append, on_turn=lambda s, t: None),
        log=TranscriptLog(tmp_path),
    )
    session.ask_screenshot("QUJD")
    end = time.monotonic() + 3
    while not (done and statuses) and time.monotonic() < end:
        time.sleep(0.01)
    session.stop()
    assert any(status.startswith("Cut off: the model ran out of room") for status in statuses)


def test_the_window_shows_the_cut_off_notice_until_the_next_question(window):
    win, bridge, app = window
    bridge.started.emit("[screenshot]")
    bridge.done.emit("POINTS:\n- a\nANSWER:\n```java\nint x =")
    bridge.status.emit("Cut off: the model ran out of room before it finished. Ask again.")
    app.processEvents()
    text = win.answer.toPlainText()
    assert "✂" in text and "ran out of room before it finished" in text and "int x =" in text
    bridge.started.emit("Another question")
    app.processEvents()
    assert "✂" not in win.answer.toPlainText()
