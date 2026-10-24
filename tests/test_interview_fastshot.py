"""The screenshot answer in as few seconds as the model allows, and a key that works without focus."""

from __future__ import annotations

import json
import os
import socket
import subprocess
import threading
import time
import types
from ctypes import wintypes

import httpx
import pytest

from careertwin.interview import control, hotkey_windows, screenshot
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import SCREENSHOT_MAX_TOKENS, StreamingCoach
from careertwin.interview.prompts import SCREENSHOT_SYSTEM, SYSTEM, screenshot_prompt
from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
from careertwin.interview.settings import InterviewSettings
from careertwin.interview.transcript import TranscriptLog

JPEG = b"\xff\xd8\xff\xe0jpeg-bytes"


def qt_app():
    """The one application object all window tests share: a bare core application would break them."""
    pytest.importorskip("PySide6")
    os.environ["QT_QPA_PLATFORM"] = "offscreen"  # a desktop session may export wayland
    from careertwin.interview.overlay import create_app

    return create_app([])


# ---- capture ---------------------------------------------------------------


@pytest.fixture
def wayland(monkeypatch):
    monkeypatch.setenv("WAYLAND_DISPLAY", "wayland-1")
    monkeypatch.setattr(screenshot.shutil, "which", lambda tool: f"/usr/bin/{tool}" if tool == "grim" else None)
    monkeypatch.setattr(screenshot, "_windows", lambda: False)


def test_a_shot_is_sent_as_a_data_url():
    assert screenshot.Shot(b"abc").data_url() == "data:image/jpeg;base64,YWJj"
    assert screenshot.Shot(b"abc", "image/png").data_url() == "data:image/png;base64,YWJj"


def test_grim_writes_a_jpeg_straight_to_the_pipe(wayland, monkeypatch):
    commands = []

    def run(command, **kwargs):
        commands.append(command)
        return subprocess.CompletedProcess(command, 0, stdout=JPEG, stderr=b"")

    monkeypatch.setattr(screenshot.subprocess, "run", run)
    assert screenshot.capture() == screenshot.Shot(JPEG)
    assert commands == [["grim", "-t", "jpeg", "-q", "82", "-"]]
    assert screenshot.capture("auto", "DP-1").data == JPEG
    assert commands[1] == ["grim", "-t", "jpeg", "-q", "82", "-o", "DP-1", "-"]


def test_a_grim_without_jpeg_falls_back_to_png(wayland, monkeypatch):
    seen = []

    def run(command, **kwargs):
        seen.append(command)
        if "-t" in command:
            return subprocess.CompletedProcess(command, 1, stdout=b"", stderr=b"no jpeg")
        with open(command[-1], "wb") as out:
            out.write(b"\x89PNG-bytes")
        return subprocess.CompletedProcess(command, 0, stdout="", stderr="")

    monkeypatch.setattr(screenshot.subprocess, "run", run)
    monkeypatch.setattr(screenshot, "_compress", lambda png: screenshot.Shot(png, "image/png"))
    shot = screenshot.capture("auto", "DP-1")
    assert shot == screenshot.Shot(b"\x89PNG-bytes", "image/png")
    assert seen[1][:3] == ["grim", "-o", "DP-1"]


def test_other_tools_are_compressed_through_qt():
    pytest.importorskip("PySide6")
    from PySide6.QtCore import QBuffer, QIODevice
    from PySide6.QtGui import QImage

    qt_app()
    image = QImage(64, 48, QImage.Format.Format_RGB32)
    image.fill(0x336699)
    buffer = QBuffer()
    buffer.open(QIODevice.OpenModeFlag.WriteOnly)
    image.save(buffer, "PNG")
    png = bytes(buffer.data())
    shot = screenshot._compress(png)
    assert shot.mime == "image/jpeg" and shot.data[:2] == b"\xff\xd8" and len(shot.data) < len(png) * 4
    assert screenshot._compress(b"not an image").mime == "image/png"


def test_windows_pixels_become_a_jpeg_without_a_png_detour():
    pytest.importorskip("PySide6")
    np = pytest.importorskip("numpy")
    from PySide6.QtGui import QImage

    qt_app()
    pixels = np.zeros((20, 30, 4), dtype=np.uint8)
    pixels[:, :, 0], pixels[:, :, 1], pixels[:, :, 2] = 10, 120, 240  # B, G, R as GDI delivers them
    shot = screenshot._jpeg_from_bgra(pixels)
    assert shot.mime == "image/jpeg"
    decoded = QImage.fromData(shot.data)
    colour = decoded.pixelColor(15, 10)
    assert (decoded.width(), decoded.height()) == (30, 20)
    assert abs(colour.red() - 240) < 12 and abs(colour.blue() - 10) < 12


def test_the_focused_monitor_comes_from_niri(monkeypatch):
    monkeypatch.setattr(screenshot, "_windows", lambda: False)
    monkeypatch.setattr(screenshot.shutil, "which", lambda tool: "/usr/bin/niri")
    monkeypatch.setenv("NIRI_SOCKET", "/run/user/1000/niri.sock")
    monkeypatch.setattr(
        screenshot.subprocess, "run",
        lambda command, **kw: subprocess.CompletedProcess(command, 0, stdout='{"name": "HDMI-A-1", "modes": []}'),
    )
    assert screenshot.focused_output() == "HDMI-A-1"
    monkeypatch.setattr(
        screenshot.subprocess, "run", lambda command, **kw: subprocess.CompletedProcess(command, 1, stdout="oops")
    )
    assert screenshot.focused_output() is None
    monkeypatch.delenv("NIRI_SOCKET")
    assert screenshot.focused_output() is None


# ---- the request -----------------------------------------------------------


ABOUT = "## Contact\n| Full name | Test Candidate |\n## Skills\n- Languages: Java, Python, TypeScript\n## Experience\n- Built FastAPI services at Acme.\n"


@pytest.fixture
def brief(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    return InterviewBrief.load()


def sse(*parts):
    body = "".join("data: " + json.dumps({"choices": [{"delta": {"content": part}}]}) + "\n\n" for part in parts)
    return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=body + "data: [DONE]\n\n")


def coach_with(brief, requests, **options):
    def handler(request):
        requests.append(request)
        if request.url.path.endswith("/models"):
            return httpx.Response(200, json={"data": []})
        return sse("POINTS:\n- two pointers\n", "ANSWER:\nreturn [1, 2]")

    return StreamingCoach(brief, url="http://gateway", api_key="k", model="main", transport=httpx.MockTransport(handler), **options)


def test_the_screenshot_request_is_lean_and_goes_to_the_vision_model(brief):
    requests = []
    coach = coach_with(brief, requests, vision_model="fast-vision")
    out = "".join(coach.stream("[screenshot]", ["Interviewer: earlier"], threading.Event(), "en", "data:image/jpeg;base64,QUJD"))
    assert "return [1, 2]" in out
    body = json.loads(requests[0].content)
    assert body["model"] == "fast-vision" and body["stream"] is True and body["max_tokens"] == SCREENSHOT_MAX_TOKENS >= 3000
    system, user = body["messages"]
    assert system["content"] == SCREENSHOT_SYSTEM and len(system["content"]) < len(SYSTEM) / 2
    text, image = user["content"]
    assert image["image_url"]["url"] == "data:image/jpeg;base64,QUJD"
    assert "Java, Python, TypeScript" in text["text"] and "CANDIDATE STACK" in text["text"]
    for private in ("Acme", "Test Candidate", "earlier", "FastAPI"):
        assert private not in text["text"] and private not in system["content"]


def test_a_bare_base64_png_is_still_accepted_and_the_main_model_is_the_default(brief):
    requests = []
    coach = coach_with(brief, requests)
    list(coach.stream("[screenshot]", [], threading.Event(), "en", "QUJD"))
    body = json.loads(requests[0].content)
    assert body["model"] == "main"
    assert body["messages"][1]["content"][1]["image_url"]["url"] == "data:image/png;base64,QUJD"
    list(coach.stream("What is REST?", [], threading.Event(), "en"))
    assert json.loads(requests[1].content)["messages"][0]["content"] == SYSTEM  # questions keep the full prompt


def test_one_connection_serves_every_request_and_can_be_opened_early(brief):
    requests = []
    coach = coach_with(brief, requests)
    coach.warm_up()
    assert requests[0].method == "GET" and str(requests[0].url) == "http://gateway/v1/models"
    assert requests[0].headers["authorization"] == "Bearer k"
    first = coach._client()
    list(coach.stream("[screenshot]", [], threading.Event(), "en", "QUJD"))
    assert coach._client() is first
    coach.close()
    assert coach._http is None


def test_warming_up_never_fails_when_the_gateway_is_down(brief):
    def refuse(request):
        raise httpx.ConnectError("refused")

    StreamingCoach(brief, url="http://gateway", transport=httpx.MockTransport(refuse)).warm_up()


def test_the_screenshot_prompt_names_the_stack_not_the_cv(brief):
    text = screenshot_prompt(brief, "pl", "main")
    assert "Java, Python, TypeScript" in text and "Polish" in text and "main answer only" in text
    assert "Acme" not in text


def test_the_vision_model_is_a_setting(tmp_path, monkeypatch):
    monkeypatch.setattr("careertwin.interview.settings.DATA_DIR", tmp_path)
    settings = InterviewSettings()
    assert settings.vision_model == "" and settings.screenshot_delay_ms == 60
    settings.set("vision_model", " fast-vision ")
    assert InterviewSettings.load().vision_model == "fast-vision"


def test_the_old_default_delay_is_replaced_by_the_new_one(tmp_path, monkeypatch):
    monkeypatch.setattr("careertwin.interview.settings.DATA_DIR", tmp_path)
    path = InterviewSettings.path()
    path.parent.mkdir(parents=True)
    path.write_text(json.dumps({"screenshot_delay_ms": 250}))
    assert InterviewSettings.load().screenshot_delay_ms == 60
    path.write_text(json.dumps({"screenshot_delay_ms": 400}))
    assert InterviewSettings.load().screenshot_delay_ms == 400


# ---- the session -----------------------------------------------------------


class ImageCoach:
    def __init__(self):
        self.calls = []

    def stream(self, question, history, cancel, language=None, image=None):
        self.calls.append(image)
        time.sleep(0.02)
        yield "POINTS:\n- a\nANSWER:\nb"


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def test_a_screenshot_is_answered_even_with_no_cv_and_reports_its_timing(tmp_path, monkeypatch):
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(tmp_path / "missing.md"))
    errors, statuses, done = [], [], []
    session = InterviewSession(
        InterviewBrief.load(allow_empty=True), None, ImageCoach(),
        Callbacks(on_error=errors.append, on_status=statuses.append, on_suggestion_done=done.append,
                  on_turn=lambda s, t: None),
        log=TranscriptLog(tmp_path),
    )
    assert session.brief.is_empty()
    session.ask_screenshot("data:image/jpeg;base64,QUJD", started_at=time.perf_counter())
    assert wait_for(lambda: done and statuses)
    assert errors == []
    assert statuses[-1].startswith("Screenshot: first words after ") and "whole answer after" in statuses[-1]
    session.ask("What is REST?")
    assert wait_for(lambda: errors)  # a spoken question still needs the context
    session.stop()


# ---- control from outside ---------------------------------------------------


@pytest.fixture
def runtime_dir(tmp_path, monkeypatch):
    short = tmp_path
    monkeypatch.setenv("XDG_RUNTIME_DIR", str(short))
    return short


def test_the_socket_lives_in_the_runtime_directory(runtime_dir):
    assert control.socket_path() == str(runtime_dir / "careertwin-interview.sock")


def test_the_command_says_when_no_window_runs(runtime_dir):
    with pytest.raises(control.ControlError, match="not running"):
        control.send("screenshot")
    with pytest.raises(control.ControlError, match="Unknown command"):
        control.send("explode")
    with pytest.raises(SystemExit, match="not running"):
        control.main(["screenshot"])


def serve_once(path, reply, received):
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(path)
    server.listen(1)

    def run():
        connection, _ = server.accept()
        received.append(connection.makefile("rb").readline())
        connection.sendall(reply)
        connection.close()
        server.close()

    threading.Thread(target=run, daemon=True).start()


def test_send_writes_one_line_and_returns_the_reply(runtime_dir):
    received = []
    serve_once(control.socket_path(), b"ok\n", received)
    assert control.send("screenshot") == "ok"
    assert received == [b"screenshot\n"]


def test_a_refusal_is_an_error_exit(runtime_dir):
    serve_once(control.socket_path(), b"unknown command 'x'\n", [])
    with pytest.raises(SystemExit, match="unknown command"):
        control.main(["ask"])


def test_the_window_hands_each_command_to_its_callback(runtime_dir):
    seen = []
    server = control.ControlServer(seen.append)
    assert server.start()
    try:
        assert control.send("screenshot") == "ok"
        assert control.send("ping") == "ok"  # asking whether a window runs does nothing else
        assert control.send("ask") == "ok"
        assert seen == ["screenshot", "ask"]
        assert oct(os.stat(control.socket_path()).st_mode & 0o777) == "0o600"
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as raw:
            raw.connect(control.socket_path())
            raw.sendall(b"explode\n")
            assert raw.makefile("rb").readline().startswith(b"unknown command")
        assert seen == ["screenshot", "ask"]
    finally:
        server.stop()
    assert not os.path.exists(control.socket_path())
    with pytest.raises(control.ControlError, match="not running"):
        control.send("ping")


def test_a_second_window_does_not_take_over_the_socket(runtime_dir):
    first, second = control.ControlServer(lambda name: None), control.ControlServer(lambda name: None)
    assert first.start()
    try:
        assert not second.start() and "already running" in second.error
        assert control.send("ping") == "ok"  # the first window still answers
    finally:
        first.stop()


def test_a_stale_socket_from_a_crashed_run_is_replaced(runtime_dir):
    leftover = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    leftover.bind(control.socket_path())
    leftover.close()  # the file stays, nobody listens
    server = control.ControlServer(lambda name: None)
    assert server.start()
    server.stop()


def test_the_command_does_nothing_on_windows_where_the_key_is_built_in(monkeypatch):
    monkeypatch.setattr(control.sys, "platform", "win32")
    with pytest.raises(control.ControlError, match="Ctrl\\+Shift\\+Enter"):
        control.send("screenshot")
    server = control.ControlServer(lambda name: None)
    assert server.start() is True and server._socket is None


# ---- the Windows hotkey -----------------------------------------------------


class FakeUser32:
    def __init__(self, accept=True, presses=2):
        self.accept, self.presses = accept, presses
        self.registered, self.unregistered, self.posted = [], [], []

    def RegisterHotKey(self, hwnd, ident, modifiers, key):
        self.registered.append((ident, modifiers, key))
        return 1 if self.accept else 0

    def GetMessageW(self, message_ref, hwnd, low, high):
        if self.presses:
            self.presses -= 1
            message_ref._obj.message = hotkey_windows.WM_HOTKEY
            return 1
        return 0

    def UnregisterHotKey(self, hwnd, ident):
        self.unregistered.append(ident)

    def PostThreadMessageW(self, thread, message, w, l):
        self.posted.append((thread, message))


class FakeKernel32:
    @staticmethod
    def GetCurrentThreadId():
        return 4242


def test_ctrl_shift_enter_runs_the_callback_for_each_press():
    user32, pressed = FakeUser32(presses=2), []
    hotkey = hotkey_windows.GlobalHotkey(lambda: pressed.append(1), libraries=(user32, FakeKernel32()))
    assert hotkey.start()
    hotkey._thread.join(2)
    assert pressed == [1, 1]
    ident, modifiers, key = user32.registered[0]
    assert modifiers == hotkey_windows.MOD_CONTROL | hotkey_windows.MOD_SHIFT | hotkey_windows.MOD_NOREPEAT
    assert key == 0x0D and user32.unregistered == [ident]
    assert wintypes.MSG is not None


def test_a_taken_key_is_reported_and_stop_is_safe():
    user32 = FakeUser32(accept=False)
    hotkey = hotkey_windows.GlobalHotkey(lambda: None, libraries=(user32, FakeKernel32()))
    assert hotkey.start() is False
    hotkey.stop()
    assert user32.unregistered == []


def test_the_app_starts_the_windows_hotkey_only_on_windows(monkeypatch):
    from careertwin.interview import app

    bridge = types.SimpleNamespace(remote=types.SimpleNamespace(emit=lambda name: None), error=types.SimpleNamespace(emit=lambda m: None))
    monkeypatch.setattr(app.sys, "platform", "linux")
    assert app.start_hotkey(bridge) is None


def test_a_screenshot_turn_is_still_logged_for_the_transcript(tmp_path):
    turns = []
    session = InterviewSession(
        InterviewBrief.load(allow_empty=True), None, ImageCoach(),
        Callbacks(on_turn=lambda speaker, text: turns.append((speaker, text))), log=TranscriptLog(tmp_path),
    )
    session.ask_screenshot("QUJD")
    assert turns == [(INTERVIEWER, "[screenshot]")]
    session.stop()


# ---- code in the answer -------------------------------------------------------


CODE = "def two_sum(nums, target):\n    left, right = 0, len(nums) - 1\n    while left < right:\n        return [left + 1, right + 1]"


def test_code_after_the_lines_is_not_turned_into_bullets():
    from careertwin.interview.prompts import sections

    text = f"POINTS:\n- use **two pointers**\n- move them inwards\n\n```python\n{CODE}\n```"
    parts = sections(text)
    assert parts.points == ["use **two pointers**", "move them inwards"]
    assert parts.answer.startswith("```python") and "    left, right = 0" in parts.answer
    proper = sections(f"POINTS:\n- a\nANSWER:\n```python\n{CODE}\n```")
    assert proper.points == ["a"] and "    while left < right" in proper.answer


def test_a_half_streamed_code_block_is_already_shown_as_code():
    from careertwin.interview.prompts import split_partial

    points, answer = split_partial("POINTS:\n- two pointers\nANSWER:\n```python\ndef two_sum(nums, target):\n    left =")
    assert points == ["two pointers"] and answer.endswith("    left =")


def test_the_window_tells_code_from_prose():
    pytest.importorskip("PySide6")
    from careertwin.interview.overlay import code_block, looks_like_code, render_answer

    assert looks_like_code(CODE)
    assert not looks_like_code("I would use two pointers because the array is sorted.\nIt takes linear time.\nIt needs no extra space.")
    html_text = render_answer(f"First the idea.\n```python\n{CODE}\n```\nThen the cost.", "color:red", 14)
    assert html_text.count("<pre") == 1 and "&lt;" in html_text and "    left, right = " in html_text
    assert "First the idea." in html_text and "Then the cost." in html_text and "monospace" in html_text
    assert "<pre" in render_answer(CODE, "color:red", 14)  # no fence: still code
    assert "<pre" not in render_answer("A plain **sentence**.", "color:red", 14)
    assert "&lt;b&gt;" in code_block("<b>", 14)
