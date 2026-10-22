"""Settings, saved context, runtime wiring and the settings window of the interview app."""

from __future__ import annotations

import json
import os
import stat
import time

import httpx
import numpy as np
import pytest

from careertwin import db
from careertwin.interview import runtime as runtime_module
from careertwin.interview import settings as settings_module
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import StreamingCoach
from careertwin.interview.library import ContextController, Library
from careertwin.interview.runtime import Runtime
from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
from careertwin.interview.settings import InterviewSettings
from careertwin.interview.transcript import TranscriptLog

ORIGINAL_BUILD_SOURCES = Runtime.build_sources
CV_TEXT = (
    "Alex Demo, backend developer. Built FastAPI services on PostgreSQL at Northwind Logistics "
    "from 2022 to 2025 and deployed them with Docker. BSc Computer Science 2020."
)
ABOUT = "# Candidate\n\n## Experience\n- Built FastAPI services.\n\n## Skills\n- Python, Docker\n"


@pytest.fixture
def home(tmp_path, monkeypatch):
    """A private data directory and ABOUT.md for one test."""
    monkeypatch.setattr(settings_module, "DATA_DIR", tmp_path / "data")
    about = tmp_path / "ABOUT.md"
    about.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(about))
    monkeypatch.delenv("CAREERTWIN_RESUME", raising=False)
    return tmp_path


def cv(tmp_path, name="Alex_CV.txt", text=CV_TEXT):
    path = tmp_path / name
    path.write_text(text)
    return path


# ---- settings -------------------------------------------------------------------------


def test_settings_validate_and_persist(home):
    settings = InterviewSettings()
    settings.set("language", "ru")
    settings.set("opacity", 80)
    settings.set("whisper_model", "base")
    settings.set("pinned", False)
    saved = json.loads(InterviewSettings.path().read_text())
    assert saved["language"] == "ru" and saved["opacity"] == 80 and saved["pinned"] is False
    assert stat.S_IMODE(os.stat(InterviewSettings.path()).st_mode) == 0o600
    reloaded = InterviewSettings.load()
    assert (reloaded.language, reloaded.opacity, reloaded.whisper_model, reloaded.pinned) == (
        "ru", 80, "base", False,
    )


@pytest.mark.parametrize(
    ("name", "value"),
    [
        ("opacity", 10), ("opacity", "high"), ("whisper_model", "huge"), ("answer_style", "wild"),
        ("pinned", "yes"), ("gateway_url", "localhost:3260"), ("font_scale", 999), ("language", 5),
    ],
)
def test_settings_reject_bad_values_without_saving(home, name, value):
    settings = InterviewSettings()
    before = getattr(settings, name)
    with pytest.raises(ValueError):
        settings.set(name, value)
    assert getattr(settings, name) == before and not InterviewSettings.path().exists()
    with pytest.raises(KeyError):
        settings.set("nonsense", 1)


def test_settings_load_survives_a_damaged_file(home):
    InterviewSettings.path().parent.mkdir(parents=True)
    InterviewSettings.path().write_text('{"opacity": 5, "language": "pl", "bogus": 1, "pinned": "x"}')
    loaded = InterviewSettings.load()
    assert loaded.language == "pl" and loaded.opacity == 100 and loaded.pinned is True
    InterviewSettings.path().write_text("not json")
    assert InterviewSettings.load() == InterviewSettings()


def test_command_line_overrides_apply_to_one_run_only(home):
    from careertwin.interview.app import build_parser, settings_for_run

    InterviewSettings().set("language", "pl")
    args = build_parser().parse_args(["--language", "de", "--whisper", "tiny", "--text-only", "--framed", "--mic"])
    run = settings_for_run(args)
    assert (run.language, run.whisper_model, run.listen, run.frameless, run.use_mic) == (
        "de", "tiny", False, False, True,
    )
    assert InterviewSettings.load().language == "pl" and InterviewSettings.load().listen is True
    assert settings_for_run(build_parser().parse_args([])).language == "pl"


# ---- saved context --------------------------------------------------------------------


def make_controller(home):
    brief = InterviewBrief.load(allow_empty=True)
    return brief, ContextController(brief, Library.load())


def test_adding_documents_copies_them_and_survives_a_restart(home):
    source = cv(home)
    brief, controller = make_controller(home)
    assert controller.add_documents([str(source)]) == []
    stored = Library.documents_dir() / "Alex_CV.txt"
    assert stored.exists() and brief.documents[0].path == str(stored.resolve())
    source.unlink()
    controller.set_notes("Prefers backend roles")
    controller.set_vacancy_text("Senior Python role with FastAPI", title="Python Developer", company="Example")

    fresh_brief = InterviewBrief.load(allow_empty=True)
    problems = ContextController(fresh_brief, Library.load()).restore()
    assert problems == []
    assert [doc.name for doc in fresh_brief.documents] == ["Alex_CV.txt"]
    assert fresh_brief.notes == "Prefers backend roles" and fresh_brief.job_title == "Python Developer"
    assert "FastAPI" in fresh_brief.render() and "Northwind" in fresh_brief.render()


def test_identical_files_are_stored_once_and_different_ones_get_new_names(home):
    one = cv(home, "cv.txt")
    other_dir = home / "other"
    other_dir.mkdir()
    same = cv(other_dir, "cv.txt")
    (home / "third").mkdir()
    different = cv(home / "third", "cv.txt", CV_TEXT + " Another version, much longer than before.")
    library = Library()
    first, second = library.copy_in(one), library.copy_in(same)
    assert first == second
    assert library.copy_in(different).name == "cv-2.txt"


def test_documents_can_be_toggled_retyped_and_removed(home):
    _, controller = make_controller(home)
    controller.add_documents([str(cv(home, "notes.txt"))])
    item = controller.items()[0]
    assert item.kind == "about" and item.enabled
    controller.update_document(item.path, kind="cv", enabled=False)
    assert controller.brief.documents[0].kind == "cv" and "CV THE EMPLOYER" not in controller.brief.render()
    saved = Library.load().documents[0]
    assert (saved.kind, saved.enabled) == ("cv", False)
    controller.update_document(item.path, enabled=True)
    assert "CV THE EMPLOYER HAS READ" in controller.brief.render()
    controller.remove_document(item.path)
    assert controller.items() == [] and not (Library.documents_dir() / "notes.txt").exists()
    assert Library.load().documents == []


def test_bad_files_are_reported_not_attached(home):
    _, controller = make_controller(home)
    errors = controller.add_documents([str(cv(home, "tiny.txt", "hi")), str(home / "missing.pdf")])
    assert len(errors) == 2 and controller.items() == [] and Library.load().documents == []


def test_about_is_edited_and_saved_to_the_file(home):
    brief, controller = make_controller(home)
    controller.save_about("# Candidate\n\n## Skills\n- Rust, SQL\n\n## Honest limits\n- No Kubernetes.\n")
    assert "No Kubernetes" in (home / "ABOUT.md").read_text()
    assert "Rust" in brief.render() and "Built FastAPI" not in brief.render()


def test_an_empty_brief_is_allowed_and_reported_as_empty(home):
    (home / "ABOUT.md").write_text("")
    brief = InterviewBrief.load(allow_empty=True)
    assert brief.is_empty() and brief.sources() == []
    assert "(none provided)" in brief.render()
    assert not InterviewBrief.load(allow_empty=True, resumes=[str(cv(home))]).is_empty()


def test_vacancy_from_the_pipeline_brings_its_sent_cv(home, monkeypatch):
    from careertwin.automation.models import Vacancy
    from careertwin.automation.store import Store

    monkeypatch.setattr(db, "DB_PATH", home / "pipeline.db")
    store = Store()
    store.initialize()
    job = store.add(Vacancy(url="https://example.com/jobs/1", title="Python Developer", company="Example"))
    store.update(job["id"], package={"cv_path": str(cv(home, "sent_cv.txt"))})
    brief, controller = make_controller(home)
    assert controller.pipeline() == [(job["id"], "Python Developer at Example")]
    controller.set_vacancy_job(job["id"])
    assert controller.vacancy_label() == "Python Developer at Example"
    assert [doc.name for doc in brief.documents] == ["sent_cv.txt"]
    controller.clear_vacancy()
    assert controller.vacancy_label() == "" and Library.load().vacancy == {}


# ---- session: audio and speech model can change while it runs ---------------------------


class Source:
    def __init__(self, audio=None):
        self.audio = audio if audio is not None else np.zeros(0, dtype=np.int16)
        self.closed = False
        self.started = False

    def frames(self):
        self.started = True
        for start in range(0, len(self.audio) - 319, 320):
            yield self.audio[start : start + 320]

    def close(self):
        self.closed = True


class Recorder:
    def __init__(self):
        self.calls = []

    def stream(self, question, history, cancel, language=None, image=None):
        self.calls.append((question, language, image))
        yield "POINTS:\n- a\nANSWER:\nb"


class Whisper:
    def __init__(self, text="", language=None):
        self.text, self.language, self.loaded = text, language, False
        self.last_language, self.last_probability = None, 0.0

    def load(self):
        self.loaded = True

    def transcribe(self, pcm, prompt=""):
        return self.text


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def test_audio_sources_can_be_replaced_while_running(home):
    first, second = Source(), Source()
    session = InterviewSession(InterviewBrief.load(), Whisper(), Recorder(), log=TranscriptLog(home))
    session.start({INTERVIEWER: first})
    assert wait_for(lambda: first.started)
    session.set_sources({INTERVIEWER: second})
    assert first.closed and wait_for(lambda: second.started)
    session.set_sources({})
    assert second.closed
    session.stop()


def test_speech_model_is_swapped_after_the_new_one_loads(home):
    old, new = Whisper("old"), Whisper("new")
    states = []
    session = InterviewSession(
        InterviewBrief.load(), old, Recorder(), Callbacks(on_state=states.append), log=TranscriptLog(home)
    )
    session.start({INTERVIEWER: Source()})
    assert wait_for(lambda: old.loaded and session.transcriber is old)
    session.set_transcriber(new)
    assert wait_for(lambda: session.transcriber is new) and new.loaded
    session.stop()


def test_no_context_means_a_clear_error_instead_of_a_blind_answer(home):
    (home / "ABOUT.md").write_text("")
    errors, coach = [], Recorder()
    session = InterviewSession(
        InterviewBrief.load(allow_empty=True), None, coach, Callbacks(on_error=errors.append),
        log=TranscriptLog(home),
    )
    session.ask("Tell me about yourself")
    assert wait_for(lambda: errors) and "Settings" in errors[0] and coach.calls == []
    session.stop()


# ---- runtime ---------------------------------------------------------------------------


@pytest.fixture
def stack(home, monkeypatch):
    monkeypatch.setattr(
        runtime_module, "WhisperTranscriber", lambda model, language=None, device="auto": Whisper()
    )
    monkeypatch.setattr(Runtime, "build_sources", lambda self: {})
    brief = InterviewBrief.load(allow_empty=True)
    context = ContextController(brief, Library.load())
    coach = StreamingCoach(brief)
    session = InterviewSession(brief, None, coach, Callbacks(), log=TranscriptLog(home))
    runtime = Runtime(InterviewSettings(), context, coach, session)
    runtime.window_changes = []
    runtime.on_window = lambda name, value: runtime.window_changes.append((name, value))
    return runtime


def test_runtime_applies_language_style_and_gateway_at_once(stack):
    stack.update("language", "ru")
    assert stack.session.language.current == "ru"
    stack.update("language", "auto")
    assert stack.session.language.current is None
    stack.update("answer_style", "detailed")
    stack.update("gateway_url", "http://127.0.0.1:9999")
    stack.update("gateway_key", "secret")
    stack.update("model", "qwen-chat")
    assert (stack.coach.style, stack.coach.url, stack.coach.api_key, stack.coach.model) == (
        "detailed", "http://127.0.0.1:9999", "secret", "qwen-chat",
    )
    stack.update("model", "")
    assert stack.coach.model is None
    assert InterviewSettings.load().answer_style == "detailed"


def test_runtime_refuses_bad_values_and_keeps_the_old_one(stack):
    with pytest.raises(ValueError):
        stack.update("answer_style", "wild")
    assert stack.coach.style == "standard" and stack.settings.answer_style == "standard"


def test_runtime_reports_window_settings(stack):
    stack.update("pinned", False)
    stack.update("opacity", 70)
    stack.update("show_full_answer", True)
    assert stack.window_changes == [("pinned", False), ("opacity", 70), ("show_full_answer", True)]


def test_runtime_restarts_audio_and_reloads_speech(stack, monkeypatch):
    built, loaded = [], []
    monkeypatch.setattr(stack, "build_sources", lambda: built.append(1) or {INTERVIEWER: Source()})
    monkeypatch.setattr(
        runtime_module,
        "WhisperTranscriber",
        lambda model, language=None, device="auto": loaded.append((model, language, device)) or Whisper(),
    )
    stack.update("use_mic", True)
    assert built and loaded == [("small", None, "auto")]
    stack.update("whisper_model", "base")
    stack.update("whisper_device", "cpu")
    assert loaded[-1] == ("base", None, "cpu")
    stack.update("language", "pl")
    stack.update("whisper_model", "tiny")
    assert loaded[-1] == ("tiny", "pl", "cpu")
    stack.session.stop()


def test_runtime_builds_sources_from_settings(stack, monkeypatch):
    monkeypatch.setattr(
        runtime_module, "default_sources",
        lambda: type("S", (), {"interviewer": "out.monitor", "microphone": "mic"})(),
    )
    sources = ORIGINAL_BUILD_SOURCES(stack)
    assert [s.source for s in sources.values()] == ["out.monitor"]
    stack.settings.set("call_source", "other.monitor")
    stack.settings.set("use_mic", True)
    stack.settings.set("mic_source", "usb-mic")
    assert [s.source for s in ORIGINAL_BUILD_SOURCES(stack).values()] == ["other.monitor", "usb-mic"]
    stack.settings.set("listen", False)
    assert ORIGINAL_BUILD_SOURCES(stack) == {}


def test_runtime_lists_models_and_explains_failures(stack, monkeypatch):
    seen = {}

    def fake_get(url, headers=None, timeout=None):
        seen.update(url=url, headers=headers)
        return httpx.Response(200, json={"data": [{"id": "auto"}, {"id": "qwen-chat"}]}, request=httpx.Request("GET", url))

    monkeypatch.setattr(httpx, "get", fake_get)
    stack.settings.set("gateway_url", "http://gw:1/")
    stack.settings.set("gateway_key", "k")
    assert stack.models() == ["auto", "qwen-chat"]
    assert seen["url"] == "http://gw:1/v1/models" and seen["headers"] == {"Authorization": "Bearer k"}

    def refuse(url, headers=None, timeout=None):
        raise httpx.ConnectError("no")

    monkeypatch.setattr(httpx, "get", refuse)
    with pytest.raises(RuntimeError, match="Cannot reach http://gw:1"):
        stack.models()


def test_runtime_screenshot_follows_the_settings(stack, monkeypatch):
    from careertwin.interview import screenshot

    monkeypatch.setattr(screenshot, "available_tools", lambda: ["grim", "maim"])
    monkeypatch.setattr(screenshot, "capture_png", lambda tool="auto": f"png:{tool}".encode())
    assert stack.screenshot_ready()
    stack.update("screenshot_tool", "maim")
    assert stack.capture() == "cG5nOm1haW0="
    stack.update("screenshot_enabled", False)
    assert not stack.screenshot_ready()
    monkeypatch.setattr(screenshot, "available_tools", lambda: [])
    stack.update("screenshot_enabled", True)
    assert not stack.screenshot_ready()


def test_reset_restores_every_default(stack):
    stack.update("language", "ru")
    stack.update("answer_style", "concise")
    stack.update("opacity", 60)
    stack.reset()
    assert stack.settings == InterviewSettings() and stack.coach.style == "standard"
    assert stack.session.language.current is None
