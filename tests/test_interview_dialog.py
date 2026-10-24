"""The interview window and its settings dialog, driven offscreen."""

from __future__ import annotations

import base64
import threading
import time
import types

import pytest

pytest.importorskip("PySide6")

from careertwin.interview import runtime as runtime_module  # noqa: E402
from careertwin.interview import screenshot  # noqa: E402
from careertwin.interview import settings as settings_module  # noqa: E402
from careertwin.interview.brief import InterviewBrief  # noqa: E402
from careertwin.interview.coach import StreamingCoach  # noqa: E402
from careertwin.interview.library import ContextController, Library  # noqa: E402
from careertwin.interview.overlay import Bridge, OverlayWindow, create_app  # noqa: E402
from careertwin.interview.runtime import Runtime  # noqa: E402
from careertwin.interview.session import InterviewSession  # noqa: E402
from careertwin.interview.settings import InterviewSettings  # noqa: E402
from careertwin.interview.transcript import TranscriptLog  # noqa: E402

CV_TEXT = (
    "Alex Demo, backend developer. Built FastAPI services on PostgreSQL at Northwind Logistics "
    "from 2022 to 2025 and deployed them with Docker. BSc Computer Science 2020."
)
ABOUT = "# Candidate\n\n## Experience\n- Built FastAPI services.\n\n## Skills\n- Python, Docker\n"


class FakeWhisper:
    """Stands in for faster-whisper: no model download, no audio."""

    def __init__(self, model="small", language=None, device="auto"):
        self.model, self.language, self.device = model, language, device
        self.last_language, self.last_probability = None, 0.0

    def load(self):
        pass

    def transcribe(self, pcm, prompt=""):
        return ""


class Gui:
    """The whole interview stack with a real window and dialog, but no audio or model."""

    def __init__(self, tmp_path, monkeypatch, about=ABOUT):
        monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
        monkeypatch.setattr(settings_module, "DATA_DIR", tmp_path / "data")
        monkeypatch.setattr(screenshot, "available_tools", lambda: ["grim", "maim"])
        monkeypatch.setattr(runtime_module, "WhisperTranscriber", FakeWhisper)
        monkeypatch.setattr(Runtime, "build_sources", lambda self: {})
        self.about_file = tmp_path / "ABOUT.md"
        self.about_file.write_text(about)
        monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(self.about_file))
        self.tmp = tmp_path
        self.app = create_app([])
        self.bridge = Bridge()
        brief = InterviewBrief.load(allow_empty=True)
        self.context = ContextController(brief, Library())
        coach = StreamingCoach(brief)
        self.session = InterviewSession(brief, None, coach, self.bridge.callbacks(), log=TranscriptLog(tmp_path))
        self.runtime = Runtime(InterviewSettings(), self.context, coach, self.session)
        self.window = OverlayWindow(self.bridge, runtime=self.runtime, listening=False)
        self.runtime.on_window = self.window.apply_setting
        self.window.apply_settings(self.runtime.settings)
        self.window.show()
        self.window.open_settings("interview")
        self.dialog = self.window.settings_dialog
        self.process()

    def process(self):
        self.app.processEvents()
        self.app.processEvents()

    def file(self, name="Alex_CV.txt", text=CV_TEXT):
        path = self.tmp / name
        path.write_text(text)
        return str(path)

    def choose(self, combo, data):
        combo.setCurrentIndex(combo.findData(data))
        combo.activated.emit(combo.currentIndex())


@pytest.fixture
def gui(tmp_path, monkeypatch):
    stack = Gui(tmp_path, monkeypatch)
    yield stack
    stack.window.close()


def test_dialog_opens_on_the_requested_tab(gui):
    assert gui.dialog.isVisible() and gui.dialog.tabs.currentIndex() == 0
    assert [gui.dialog.tabs.tabText(i) for i in range(gui.dialog.tabs.count())] == [
        "Interview", "About me", "Practice", "Project", "Audio", "AI", "General",
    ]
    for old_name, tab in (("screenshot", "General"), ("language", "General"), ("model", "AI"),
                          ("prepare", "Practice"), ("docs", "Interview"), ("vacancy", "Interview")):
        gui.window.open_settings(old_name)
        assert gui.dialog.tabs.tabText(gui.dialog.tabs.currentIndex()) == tab


def test_adding_a_document_updates_the_list_and_the_window(gui):
    gui.dialog.add_files([gui.file("Alex_CV.txt"), gui.file("linkedin.txt", CV_TEXT + " More facts.")])
    gui.process()
    rows = [gui.dialog.docs_layout.itemAt(i).widget() for i in range(gui.dialog.docs_layout.count())]
    assert [row.include.text() for row in rows] == ["Alex_CV.txt", "linkedin.txt"]
    assert [row.kind.currentData() for row in rows] == ["cv", "about"]
    assert "Alex_CV.txt" in gui.window.context.full and "linkedin.txt" in gui.window.context.full
    rows[0].include.setChecked(False)
    rows[1].kind.setCurrentIndex(0)
    assert "Alex_CV.txt" not in gui.window.context.full
    assert gui.context.brief.documents[1].kind == "cv"
    gui.dialog.add_files([gui.file("empty.txt", "x")])
    assert gui.dialog.status.objectName() == "error" and "almost no readable text" in gui.dialog.status.text()
    gui.dialog.remove_document(gui.context.brief.documents[0].path)
    gui.process()
    assert gui.dialog.docs_layout.count() == 1


def test_dropping_files_on_the_window_adds_them(gui):
    gui.window.add_files([gui.file("dropped_cv.txt")])
    assert [doc.name for doc in gui.context.brief.documents] == ["dropped_cv.txt"]
    gui.window.add_files([str(gui.tmp / "missing.pdf")])
    assert gui.window.last_state[0] == "error"


def test_language_tab_changes_the_interview_and_the_window(gui):
    gui.choose(gui.dialog.interview_language, "ru")
    assert gui.session.language.current == "ru" and InterviewSettings.load().language == "ru"
    gui.choose(gui.dialog.answer_style, "detailed")
    assert gui.runtime.coach.style == "detailed"
    assert gui.window.full_answer and gui.dialog.full_answer.isChecked()
    gui.dialog.full_answer.setChecked(False)
    assert not gui.window.full_answer and not gui.window.details.isChecked()
    gui.dialog.full_answer.setChecked(True)
    assert gui.window.full_answer and gui.window.details.isChecked()
    gui.choose(gui.dialog.ui_language, "pl")
    assert gui.window.ui == "pl" and gui.window.ask.title.text() == "Zapytaj" and gui.window.ui_locked
    gui.bridge.language.emit("en")
    assert gui.window.ui == "pl"
    assert gui.dialog.tabs.tabText(0) == "Rozmowa"
    gui.choose(gui.dialog.ui_language, "auto")
    gui.bridge.language.emit("ru")
    assert gui.window.ui == "ru"


def test_details_button_is_saved_and_the_menu_opens_settings(gui):
    gui.window.details.setChecked(True)
    assert InterviewSettings.load().show_full_answer is True
    gui.window.act_settings.trigger()
    assert gui.dialog is gui.window.settings_dialog and gui.dialog.isVisible()


def test_the_menu_keeps_only_what_is_needed_in_a_call(gui):
    names = [action.text() for action in gui.window.menu.actions() if not action.isSeparator()]
    assert names == ["Settings…", "Interview", "Transcript", "Quit"]


def test_buttons_show_the_ordinary_cursor_and_nothing_resizes_from_a_button(gui):
    from PySide6.QtCore import Qt
    from PySide6.QtWidgets import QSizeGrip, QWidget

    for widget in gui.window.findChildren(QWidget):
        assert widget.cursor().shape() != Qt.CursorShape.PointingHandCursor
    assert not gui.window.findChildren(QSizeGrip)
    assert not hasattr(gui.window, "get_answer")


def test_audio_tab_lists_sources_and_applies_choices(gui, monkeypatch):
    monkeypatch.setattr(Runtime, "audio_sources", staticmethod(lambda: ["out.monitor", "usb-mic", "alsa.monitor"]))
    built, loaded = [], []
    monkeypatch.setattr(gui.runtime, "build_sources", lambda: built.append(1) or {})
    monkeypatch.setattr(
        runtime_module, "WhisperTranscriber",
        lambda model, language=None, device="auto": loaded.append(model) or FakeWhisper(model, language, device),
    )
    gui.dialog.refresh_audio()
    assert [gui.dialog.call_source.itemData(i) for i in range(gui.dialog.call_source.count())] == [
        "", "alsa.monitor", "out.monitor",
    ]
    assert [gui.dialog.mic_source.itemData(i) for i in range(gui.dialog.mic_source.count())] == ["", "usb-mic"]
    gui.choose(gui.dialog.call_source, "out.monitor")
    assert gui.runtime.settings.call_source == "out.monitor" and built
    gui.dialog.use_mic.setChecked(True)
    gui.choose(gui.dialog.whisper_model, "base")
    assert gui.runtime.settings.use_mic and gui.runtime.settings.whisper_model == "base" and loaded[-1] == "base"
    gui.dialog.listen.setChecked(False)
    assert gui.window.pause.isEnabled() is False


def test_speech_engine_switches_the_active_controls_and_the_transcriber(gui, monkeypatch):
    dialog, settings = gui.dialog, gui.runtime.settings
    made = []

    class FakeDeepgram(FakeWhisper):
        def __init__(self, key, model, language=None):
            super().__init__(model, language)
            made.append((key, model, language))

    monkeypatch.setattr(runtime_module, "DeepgramTranscriber", FakeDeepgram)
    assert dialog.whisper_model.isEnabled() and not dialog.deepgram_key.isEnabled()
    gui.choose(dialog.stt_engine, "deepgram")
    assert settings.stt_engine == "deepgram"
    assert not dialog.whisper_model.isEnabled() and dialog.deepgram_key.isEnabled() and dialog.deepgram_test.isEnabled()
    assert "leaves this computer" in dialog.speech_note.text()
    dialog.deepgram_key.setText("dg-key")
    dialog.deepgram_key.editingFinished.emit()
    assert (settings.deepgram_key, settings.deepgram_model) == ("dg-key", "nova-3")
    assert made[-1][:2] == ("dg-key", "nova-3")
    assert InterviewSettings.load().stt_engine == "deepgram"
    gui.choose(dialog.stt_engine, "whisper")
    assert dialog.whisper_model.isEnabled() and not dialog.deepgram_key.isEnabled()
    assert "leaves this computer" not in dialog.speech_note.text()


def test_the_deepgram_key_can_be_tested_from_the_window(gui, monkeypatch):
    dialog = gui.dialog
    asked = []
    monkeypatch.setattr("careertwin.interview.settings_dialog.check_key", lambda key: asked.append(key))
    monkeypatch.setattr(dialog, "run", lambda work, done: done(work(), None))
    dialog.deepgram_key.setText("typed-key")
    dialog._test_deepgram()
    assert asked == ["typed-key"] and dialog.status.objectName() != "error" and dialog.status.text()

    def reject(key):
        from careertwin.interview.stt_deepgram import DeepgramError

        raise DeepgramError("Deepgram rejected the API key")

    monkeypatch.setattr("careertwin.interview.settings_dialog.check_key", reject)
    monkeypatch.setattr(dialog, "run", lambda work, done: done(None, "Deepgram rejected the API key"))
    dialog._test_deepgram()
    assert dialog.status.objectName() == "error" and "rejected" in dialog.status.text()


def test_model_tab_saves_the_gateway_and_lists_models(gui):
    gui.dialog.gateway_url.setText("http://127.0.0.1:9999/")
    gui.dialog.gateway_url.editingFinished.emit()
    gui.dialog.gateway_key.setText("secret")
    gui.dialog.gateway_key.editingFinished.emit()
    assert gui.runtime.coach.url == "http://127.0.0.1:9999/" and gui.runtime.coach.api_key == "secret"
    gui.dialog.gateway_url.setText("not a url")
    gui.dialog.gateway_url.editingFinished.emit()
    assert gui.dialog.status.objectName() == "error" and gui.runtime.coach.url == "http://127.0.0.1:9999/"
    gui.dialog._models_loaded(["qwen-chat", "auto"], None)
    assert gui.dialog.model_id.count() == 2 and "2" in gui.dialog.status.text()
    gui.dialog.model_id.setCurrentText("qwen-chat")
    gui.dialog.model_id.activated.emit(0)
    assert gui.runtime.coach.model == "qwen-chat"
    gui.dialog.model_id.setCurrentText("auto")
    gui.dialog.model_id.activated.emit(0)
    assert gui.runtime.coach.model is None
    gui.dialog._models_loaded(None, "Cannot reach http://x (ConnectError)")
    assert "Cannot reach" in gui.dialog.status.text()


def test_provider_choice_fills_the_address_and_never_carries_a_key_to_another_provider(gui):
    settings = gui.runtime.settings
    gui.dialog.gateway_key.setText("groq-key")
    gui.dialog.gateway_key.editingFinished.emit()
    settings.set("model", "llama-x")
    gui.choose(gui.dialog.provider, "openai")
    assert settings.gateway_url == "https://api.openai.com"
    assert settings.gateway_key == "" and settings.model == "" and gui.dialog.gateway_key.text() == ""
    assert "OpenAI" in gui.dialog.model_note.text() and "CV" in gui.dialog.model_note.text()
    gui.choose(gui.dialog.provider, "switchyard")
    assert settings.gateway_url == "http://127.0.0.1:3260"
    assert "OpenAI" not in gui.dialog.model_note.text()


def test_the_provider_follows_the_address_that_is_typed(gui):
    for text, expected in (
        ("https://api.groq.com/openai/v1", "groq"),
        ("http://localhost:11434/", "ollama"),
        ("http://10.0.0.5:3260", "other"),
    ):
        gui.dialog.gateway_url.setText(text)
        gui.dialog.gateway_url.editingFinished.emit()
        assert gui.dialog.provider.currentData() == expected
    gui.dialog.gateway_url.setText("https://api.groq.com/openai/v1")
    gui.dialog.gateway_url.editingFinished.emit()
    assert "Groq" in gui.dialog.model_note.text()


def test_the_screenshot_switch_controls_the_button(gui, monkeypatch):
    assert gui.window.shot.isEnabled()
    gui.dialog.shot_enabled.setChecked(False)
    assert not gui.window.shot.isEnabled() and "Settings" in gui.window.shot.toolTip()
    gui.dialog.shot_enabled.setChecked(True)
    assert gui.window.shot.isEnabled()
    monkeypatch.setattr(screenshot, "available_tools", lambda: [])
    gui.dialog.refresh_screenshot()
    gui.window.refresh_controls()
    assert not gui.window.shot.isEnabled() and "grim" in gui.dialog.shot_note.text()


def wait_for(gui, condition, rounds=100):
    for _ in range(rounds):
        gui.process()
        if condition():
            return True
        time.sleep(0.01)
    return False


def test_screenshot_button_sends_the_capture_to_the_session(gui, monkeypatch):
    sent, asked = [], []
    monkeypatch.setattr(
        screenshot, "capture", lambda tool="auto", output=None: asked.append(output) or screenshot.Shot(b"jpeg-bytes")
    )
    monkeypatch.setattr(gui.session, "ask_screenshot", lambda data, started_at=None: sent.append((data, started_at)))
    gui.window.shot.clicked.emit()
    assert wait_for(gui, lambda: sent)
    data, started_at = sent[0]
    assert data == "data:image/jpeg;base64," + base64.b64encode(b"jpeg-bytes").decode()
    assert started_at and asked == [None] and gui.window.windowOpacity() == 1.0
    monkeypatch.setattr(screenshot, "capture", lambda tool="auto", output=None: (_ for _ in ()).throw(screenshot.ScreenshotError("grim failed")))
    gui.window.shot.clicked.emit()
    assert wait_for(gui, lambda: gui.window.last_state == ("error", "The screenshot failed: grim failed"))
    assert gui.window.windowOpacity() == 1.0


def test_the_screenshot_shows_that_it_is_working_at_once(gui, monkeypatch):
    gate = threading.Event()
    monkeypatch.setattr(screenshot, "capture", lambda tool="auto", output=None: gate.wait(2) and screenshot.Shot(b"x"))
    monkeypatch.setattr(gui.session, "ask_screenshot", lambda data, started_at=None: None)
    gui.window.shot.clicked.emit()
    assert gui.window.pill.kind == "thinking" and "screen" in gui.window.pill.text
    gate.set()
    assert wait_for(gui, lambda: gui.window.windowOpacity() == 1.0)


def test_a_command_from_outside_captures_only_the_focused_monitor(gui, monkeypatch):
    asked, sent = [], []
    monkeypatch.setattr(screenshot, "focused_output", lambda: "HDMI-A-1")
    monkeypatch.setattr(
        screenshot, "capture", lambda tool="auto", output=None: asked.append(output) or screenshot.Shot(b"jpeg")
    )
    monkeypatch.setattr(gui.session, "ask_screenshot", lambda data, started_at=None: sent.append(data))
    monkeypatch.setattr(type(gui.window), "isActiveWindow", lambda self: False)
    monkeypatch.setattr(type(gui.window), "screen", lambda self: types.SimpleNamespace(name=lambda: "eDP-1"))
    opacities = []
    real = type(gui.window).setWindowOpacity
    monkeypatch.setattr(type(gui.window), "setWindowOpacity", lambda self, value: (opacities.append(value), real(self, value))[1])
    gui.bridge.remote.emit("screenshot")
    assert wait_for(gui, lambda: sent)
    assert asked == ["HDMI-A-1"]
    assert opacities == []  # the window is on another monitor: nothing to hide


def test_the_window_hides_for_a_capture_of_its_own_monitor(gui, monkeypatch):
    sent, opacities = [], []
    monkeypatch.setattr(screenshot, "focused_output", lambda: "eDP-1")
    monkeypatch.setattr(screenshot, "capture", lambda tool="auto", output=None: screenshot.Shot(b"jpeg"))
    monkeypatch.setattr(gui.session, "ask_screenshot", lambda data, started_at=None: sent.append(data))
    monkeypatch.setattr(type(gui.window), "isActiveWindow", lambda self: False)
    monkeypatch.setattr(type(gui.window), "screen", lambda self: types.SimpleNamespace(name=lambda: "eDP-1"))
    real = type(gui.window).setWindowOpacity
    monkeypatch.setattr(type(gui.window), "setWindowOpacity", lambda self, value: (opacities.append(value), real(self, value))[1])
    gui.bridge.remote.emit("screenshot")
    assert wait_for(gui, lambda: sent)
    assert opacities == [0.0, 1.0]


def test_a_command_from_outside_can_ask_for_the_last_question(gui, monkeypatch):
    hinted = []
    monkeypatch.setattr(gui.session, "hint_now", lambda: hinted.append(1))
    gui.window._on_hint = gui.session.hint_now
    gui.bridge.remote.emit("ask")
    gui.process()
    assert hinted == [1]


def test_vacancy_notes_and_about_tabs_write_through(gui):
    gui.dialog.vacancy_text.setPlainText("Senior Python role using FastAPI and PostgreSQL")
    gui.dialog._use_text()
    assert "FastAPI" in gui.context.brief.job_description and "characters" in gui.dialog.vacancy_current.text()
    gui.dialog._clear_vacancy()
    assert gui.context.brief.job_description == "" and "No vacancy" in gui.dialog.vacancy_current.text()
    gui.dialog.notes.setPlainText("Prefers backend roles")
    gui.dialog._notes_timer.timeout.emit()
    assert gui.context.brief.notes == "Prefers backend roles" and Library.load().current.notes == "Prefers backend roles"
    gui.dialog.about.setPlainText("# Candidate\n\n## Honest limits\n- No Kubernetes.\n")
    gui.dialog._save_about()
    assert "No Kubernetes" in gui.about_file.read_text() and "ABOUT.md" in gui.window.context.full
    assert gui.dialog.status.text() == "Saved"


def test_window_tab_and_reset(gui, monkeypatch):
    from PySide6.QtWidgets import QMessageBox

    gui.dialog.opacity.setValue(60)
    gui.dialog.font_scale.setValue(150)
    gui.dialog.pinned.setChecked(False)
    gui.dialog.save_transcripts.setChecked(False)
    assert round(gui.window.windowOpacity(), 2) == 0.6 and gui.window.font_scale == 150 and not gui.window.pinned
    saved = InterviewSettings.load()
    assert (saved.opacity, saved.font_scale, saved.pinned, saved.save_transcripts) == (60, 150, False, False)
    monkeypatch.setattr(QMessageBox, "question", lambda *a, **k: QMessageBox.StandardButton.Yes)
    gui.dialog._reset()
    assert gui.runtime.settings == InterviewSettings() and gui.dialog.opacity.value() == 100
    assert gui.window.windowOpacity() == 1.0 and gui.window.font_scale == 100


def test_font_scale_changes_the_answer_text(gui):
    gui.bridge.started.emit("Why us?")
    gui.bridge.done.emit("POINTS:\n- **Real** product\nANSWER:\nBecause.")
    gui.process()
    before = gui.window.answer.toHtml()
    gui.runtime.update("font_scale", 160)
    assert gui.window.answer.toHtml() != before


def test_an_empty_context_shows_where_to_add_one(tmp_path, monkeypatch):
    stack = Gui(tmp_path, monkeypatch, about="")
    try:
        text = stack.window.answer.toPlainText()
        assert "No context yet" in text and "Settings" in text
        stack.window.add_files([stack.file()])
        assert "No context yet" not in stack.window.answer.toPlainText()
    finally:
        stack.window.close()


def test_interviews_are_listed_switched_renamed_and_deleted(gui, monkeypatch):
    from PySide6.QtWidgets import QInputDialog, QMessageBox

    gui.dialog.add_files([gui.file("CV_python.txt")])
    gui.dialog.vacancy_text.setPlainText("Python role with FastAPI")
    gui.dialog._use_text()
    first = gui.context.current.id
    assert gui.dialog.interview_combo.count() == 1
    monkeypatch.setattr(QInputDialog, "getText", lambda *a, **k: ("Java at Beta", True))
    gui.dialog._new_interview()
    gui.process()
    assert gui.dialog.interview_combo.count() == 2 and gui.dialog.interview_combo.currentText() == "Java at Beta"
    assert gui.context.brief.documents == [] and gui.dialog.tabs.currentIndex() == gui.dialog.TABS.index("interview")
    assert gui.window.context.full.startswith("Java at Beta")
    assert "No vacancy yet" in gui.window.answer.toPlainText()
    gui.choose(gui.dialog.interview_combo, first)
    gui.process()
    assert [d.name for d in gui.context.brief.documents] == ["CV_python.txt"]
    assert gui.window.context.full.startswith("Python role") or "CV_python.txt" in gui.window.context.full
    monkeypatch.setattr(QInputDialog, "getText", lambda *a, **k: ("EPAM Python", True))
    gui.dialog._rename_interview()
    assert gui.dialog.interview_combo.currentText() == "EPAM Python"
    monkeypatch.setattr(QMessageBox, "question", lambda *a, **k: QMessageBox.StandardButton.Yes)
    gui.dialog._delete_interview()
    gui.process()
    assert gui.dialog.interview_combo.count() == 1 and gui.dialog.interview_combo.currentText() == "Java at Beta"


def test_the_window_menu_switches_interviews(gui):
    gui.context.rename_interview("First")
    first = gui.context.current.id
    gui.context.new_interview("Second")
    gui.window.refresh_controls()
    gui.window._fill_interview_menu()
    actions = [a for a in gui.window.interview_menu.actions() if a.isCheckable()]
    assert [(a.text(), a.isChecked()) for a in actions] == [("First", False), ("Second", True)]
    actions[0].trigger()
    assert gui.context.current.id == first and gui.window.context.full.startswith("First")
    gui.window._new_interview()
    assert gui.context.interview_name().startswith("Interview") and gui.dialog.tabs.currentIndex() == 0


BANK = (
    '{"questions": [{"question": "What is FastAPI?", "kind": "definition", '
    '"outline": ["It is a **Python** web framework.", "I built the order API with it."]}, '
    '{"question": "Do you know Kubernetes?", "kind": "technical", '
    '"outline": ["I have not used it in production."]}]}'
)
REVIEW = (
    '{"summary": "Strong backend fit.", "strengths": ["FastAPI at Northwind"], '
    '"gaps": [{"gap": "Kubernetes", "how_to_address": "Say it was coursework only."}], '
    '"likely_questions": ["Have you run Kubernetes?"], "cv_fixes": ["Add the p95 latency story."]}'
)


def wait_until(gui, condition, rounds=60):
    import time

    for _ in range(rounds):
        gui.process()
        if condition():
            return True
        time.sleep(0.02)
    return False


def test_prepare_tab_builds_a_question_bank_and_reviews_the_cv(gui, monkeypatch):
    calls = []

    def fake_complete(system, prompt, **kwargs):
        calls.append((system, prompt))
        return BANK if "questions" in system.split("Reply with JSON only:")[-1][:40] else REVIEW

    monkeypatch.setattr(gui.runtime.coach, "complete", fake_complete)
    gui.dialog.show_tab("prepare")
    gui.dialog.add_files([gui.file("Alex_CV.txt")])
    gui.dialog.vacancy_text.setPlainText("Senior Python role using FastAPI")
    gui.dialog._use_text()
    gui.runtime.update("language", "ru")
    gui.dialog.bank_count.setValue(5)
    gui.dialog.bank_focus.setText("system design")
    gui.dialog.bank_build.click()
    assert wait_until(gui, lambda: "What is FastAPI?" in gui.dialog.prepare_view.toPlainText())
    system, prompt = calls[0]
    assert "Write 5 questions in Russian" in prompt and "Focus on: system design" in prompt
    assert "Senior Python role" in prompt and "Northwind" in prompt
    text = gui.dialog.prepare_view.toPlainText()
    assert "It is a Python web framework." in text and "**" not in text

    gui.dialog.cv_review.click()
    assert wait_until(gui, lambda: "Strong backend fit." in gui.dialog.prepare_view.toPlainText())
    text = gui.dialog.prepare_view.toPlainText()
    assert "Kubernetes" in text and "Say it was coursework only." in text and "Add the p95 latency story." in text
    assert "What is FastAPI?" in text
    assert "Review the CV against the vacancy. Write in Russian." in calls[1][1]


def test_prepare_results_belong_to_their_interview(gui, monkeypatch):
    monkeypatch.setattr(gui.runtime.coach, "complete", lambda system, prompt, **k: BANK)
    gui.dialog.add_files([gui.file("Alex_CV.txt")])
    gui.dialog.bank_build.click()
    assert wait_until(gui, lambda: "What is FastAPI?" in gui.dialog.prepare_view.toPlainText())
    gui.context.new_interview("Other")
    gui.dialog.refresh()
    assert "What is FastAPI?" not in gui.dialog.prepare_view.toPlainText()
    gui.context.switch_interview(gui.context.interviews()[0][0])
    gui.dialog.refresh()
    assert "What is FastAPI?" in gui.dialog.prepare_view.toPlainText()


def test_prepare_errors_are_shown(gui, monkeypatch):
    from careertwin.interview.coach import CoachError

    def boom(system, prompt, **kwargs):
        raise CoachError("Switchyard is not reachable")

    monkeypatch.setattr(gui.runtime.coach, "complete", boom)
    gui.dialog.add_files([gui.file("Alex_CV.txt")])
    gui.dialog.cv_review.click()
    assert wait_until(gui, lambda: gui.dialog.status.objectName() == "error")
    assert "not reachable" in gui.dialog.status.text()


def test_prepare_needs_some_context_first(tmp_path, monkeypatch):
    stack = Gui(tmp_path, monkeypatch, about="")
    try:
        calls = []
        monkeypatch.setattr(stack.runtime.coach, "complete", lambda *a, **k: calls.append(1) or BANK)
        stack.dialog.bank_build.click()
        stack.dialog.cv_review.click()
        assert stack.dialog.status.objectName() == "error" and "Add your CV" in stack.dialog.status.text()
        assert calls == []
    finally:
        stack.window.close()


REPLY = (
    "POINTS:\n- **React** is a library for building **UIs**.\n- It is built on **components**.\n"
    "ANSWER:\nReact is a JavaScript library maintained by Meta.\n"
    "KEY:\n- **Components**: reusable pieces of UI.\n- **Virtual DOM**: batches updates.\n"
    "CONTEXT:\nUsed for single-page apps and, with React Native, for mobile apps."
)


def test_detailed_answer_shows_main_answer_key_points_and_context(gui):
    gui.bridge.started.emit("What is React?")
    gui.bridge.done.emit(REPLY)
    gui.process()
    text = gui.window.answer.toPlainText()
    for heading in ("Detailed answer", "Main answer", "Key points", "Practical context"):
        assert heading in text
    assert text.index("It is built on components") < text.index("Detailed answer") < text.index("Main answer")
    assert text.index("Main answer") < text.index("Key points") < text.index("Practical context")
    assert "Components: reusable pieces of UI." in text and "single-page apps" in text and "**" not in text
    assert gui.window.answer_text.splitlines()[0] == "• React is a library for building UIs."
    assert "Virtual DOM: batches updates." in gui.window.answer_text

    gui.window.details.setChecked(False)
    short = gui.window.answer.toPlainText()
    assert "Detailed answer" not in short and "Main answer" not in short and "Key points" not in short
    assert "Virtual DOM" not in gui.window.answer_text
    assert InterviewSettings.load().show_full_answer is False
    gui.window.details.setChecked(True)
    assert "Practical context" in gui.window.answer.toPlainText()


def test_details_are_translated_and_streamed_in_order(gui):
    gui.runtime.update("ui_language", "ru")
    gui.bridge.started.emit("Что такое React?")
    gui.bridge.chunk.emit("POINTS:\n- **React** — библиотека\n")
    gui.process()
    assert "Подробный ответ" not in gui.window.answer.toPlainText()
    gui.bridge.chunk.emit("ANSWER:\nReact — это библиотека.\nKEY:\n- **Компоненты**: блоки UI.\nCONTEXT:\nИспользуют в SPA.")
    gui.process()
    text = gui.window.answer.toPlainText()
    for heading in ("Подробный ответ", "Основной ответ", "Ключевые моменты", "Практический контекст"):
        assert heading in text
    assert gui.window.details.text() == "Подробно"


def test_main_answer_layout_shows_only_the_paragraph_as_it_streams(gui):
    gui.runtime.update("answer_layout", "main")
    assert gui.window.layout_mode == "main" and not gui.window.details.isVisible()
    assert InterviewSettings.load().answer_layout == "main"
    gui.bridge.started.emit("How would you manage 30 hours with your studies?")
    gui.bridge.chunk.emit("ANSWER:\nMy studies leave weekdays free, so ")
    gui.process()
    text = gui.window.answer.toPlainText()
    assert "How would you manage" in text and "My studies leave weekdays free, so" in text
    gui.bridge.chunk.emit("30 hours a week works for me.")
    gui.bridge.done.emit("ANSWER:\nMy studies leave weekdays free, so 30 hours a week works for me.")
    gui.process()
    text = gui.window.answer.toPlainText()
    for hidden in ("Main answer", "Detailed answer", "Key points", "Practical context", "•"):
        assert hidden not in text
    assert gui.window.answer_text == "My studies leave weekdays free, so 30 hours a week works for me."


def test_main_layout_survives_a_model_that_ignores_it(gui):
    gui.runtime.update("answer_layout", "main")
    gui.bridge.started.emit("What is React?")
    gui.bridge.done.emit("POINTS:\n- **React** is a UI library.\n- It uses components.")
    gui.process()
    assert "React is a UI library." in gui.window.answer.toPlainText()
    gui.bridge.started.emit("Plain?")
    gui.bridge.done.emit("Just a sentence without markers.")
    gui.process()
    assert "Just a sentence without markers." in gui.window.answer.toPlainText()


def test_layout_is_switched_in_settings(gui):
    gui.choose(gui.dialog.answer_layout, "main")
    assert gui.runtime.settings.answer_layout == "main" and gui.runtime.coach.layout == "main"
    assert gui.window.layout_mode == "main" and not gui.window.details.isVisible()
    gui.choose(gui.dialog.answer_layout, "lines")
    assert gui.window.layout_mode == "lines" and gui.window.details.isVisible() is True
    assert gui.runtime.coach.layout == "lines"


def test_the_live_caption_shows_what_is_being_said_and_then_goes_away(gui):
    assert not gui.window.heard.isVisible()
    gui.bridge.partial.emit("Thank you for joining us today, could you tell")
    gui.process()
    assert gui.window.heard.isVisible() and "Thank you for joining us today" in gui.window.heard.full
    gui.bridge.partial.emit("")
    gui.process()
    assert not gui.window.heard.isVisible() and gui.window.heard.full == ""


def test_only_a_press_on_the_border_resizes_the_window(gui):
    from PySide6.QtCore import QPoint, Qt

    window = gui.window
    width, height = window.width(), window.height()
    assert window._edges_at(QPoint(width // 2, height // 2)) == Qt.Edge(0)
    assert window._edges_at(QPoint(2, height // 2)) == Qt.Edge.LeftEdge
    assert window._edges_at(QPoint(width - 1, height // 2)) == Qt.Edge.RightEdge
    assert window._edges_at(QPoint(width - 1, height - 1)) == Qt.Edge.RightEdge | Qt.Edge.BottomEdge
    assert window._edges_at(QPoint(3, 3)) == Qt.Edge.LeftEdge | Qt.Edge.TopEdge


def test_the_ai_tab_asks_only_for_what_the_provider_needs(gui):
    dialog = gui.dialog
    form = dialog.model_form
    assert not form.isRowVisible(dialog.gateway_url) and not form.isRowVisible(dialog.gateway_key)
    assert not dialog.model_id.isHidden()
    gui.choose(dialog.provider, "groq")
    assert not form.isRowVisible(dialog.gateway_url) and form.isRowVisible(dialog.gateway_key)
    gui.choose(dialog.provider, "ollama")
    assert not form.isRowVisible(dialog.gateway_key)
    gui.choose(dialog.provider, "other")
    dialog.gateway_url.setText("https://llm.example.com")
    dialog.gateway_url.editingFinished.emit()
    assert form.isRowVisible(dialog.gateway_url) and form.isRowVisible(dialog.gateway_key)


def test_the_audio_tab_shows_the_controls_of_the_chosen_engine_only(gui):
    dialog = gui.dialog
    assert dialog.audio_form.isRowVisible(dialog.whisper_model)
    assert not dialog.audio_form.isRowVisible(dialog.deepgram_row)
    gui.choose(dialog.stt_engine, "deepgram")
    assert not dialog.audio_form.isRowVisible(dialog.whisper_model)
    assert dialog.audio_form.isRowVisible(dialog.deepgram_row)
    assert not hasattr(dialog, "whisper_device") and not hasattr(dialog, "deepgram_model")


def test_the_general_tab_holds_answers_window_and_the_screenshot_switch(gui):
    dialog = gui.dialog
    dialog.show_tab("general")
    page = dialog.tabs.currentWidget()
    for widget in (dialog.interview_language, dialog.answer_layout, dialog.ui_language,
                   dialog.pinned, dialog.shot_enabled, dialog.opacity, dialog.reset_button):
        assert page.isAncestorOf(widget)
    assert not hasattr(dialog, "shot_tool") and not hasattr(dialog, "shot_delay")
    dialog.show_tab("interview")
    page = dialog.tabs.currentWidget()
    assert page.isAncestorOf(dialog.docs_layout.parentWidget()) and page.isAncestorOf(dialog.vacancy_text)
    assert page.isAncestorOf(dialog.notes)


def test_the_screenshot_model_is_chosen_next_to_the_model(gui):
    dialog = gui.dialog
    dialog.vision_id.setCurrentText("meta/llama-4-scout")
    dialog.vision_id.lineEdit().editingFinished.emit()
    assert gui.runtime.settings.vision_model == "meta/llama-4-scout"
    assert gui.runtime.coach.vision_model == "meta/llama-4-scout"
    assert InterviewSettings.load().vision_model == "meta/llama-4-scout"
    dialog._models_loaded(["auto", "a", "b"], None)
    assert dialog.vision_id.currentText() == "meta/llama-4-scout" and dialog.vision_id.count() == 4
    dialog.vision_id.setCurrentText("auto")
    dialog.vision_id.lineEdit().editingFinished.emit()
    assert gui.runtime.settings.vision_model == "" and gui.runtime.coach.vision_model is None


def test_another_provider_forgets_the_screenshot_model_too(gui):
    dialog = gui.dialog
    gui.runtime.update("vision_model", "fast-vision")
    dialog.refresh()
    assert dialog.vision_id.currentText() == "fast-vision"
    gui.choose(dialog.provider, "groq")
    assert gui.runtime.settings.vision_model == "" and dialog.vision_id.currentText() == ""


def test_a_screenshot_solution_is_shown_as_code_under_the_lines(gui):
    gui.bridge.started.emit("[screenshot]")
    gui.bridge.done.emit(
        "POINTS:\n- use **two pointers**\nANSWER:\n```python\ndef two_sum(nums, target):\n    return [1, 2]\n```"
    )
    gui.process()
    text = gui.window.answer.toPlainText()
    assert text.startswith("Screenshot") and "[screenshot]" not in text
    assert "use two pointers" in text and "def two_sum(nums, target):" in text and "    return [1, 2]" in text
    assert "Detailed answer" not in text  # the code is the answer, not an extra behind a button
    gui.window.details.setChecked(False)
    assert "def two_sum" in gui.window.answer.toPlainText()
    assert gui.window.answer_text == "• use two pointers\ndef two_sum(nums, target):\n    return [1, 2]"


def test_the_status_dots_tell_how_long_the_screenshot_answer_took(gui):
    gui.bridge.status.emit("Screenshot: first words after 1.3 s, whole answer after 6.0 s")
    gui.process()
    assert "first words after 1.3 s" in gui.window.pill.toolTip()
    gui.bridge.status.emit("Listening")
    gui.process()
    assert "first words after 1.3 s" in gui.window.pill.toolTip()  # other status lines leave it alone
    gui.bridge.state.emit("listening")
    gui.process()
    assert "first words after 1.3 s" in gui.window.pill.toolTip()  # and so does a new state


# ---- the Project tab ----------------------------------------------------------


@pytest.fixture
def demo_project(tmp_path):
    root = tmp_path / "demo"
    (root / "app").mkdir(parents=True)
    (root / "README.md").write_text("# Demo\nA tiny service that greets people.\n")
    (root / "app" / "main.py").write_text("def greet(name):\n    return f'Hello, {name}'\n")
    return root


def analyse_with(gui, monkeypatch, root, dossier="## Overview\nA greeting service."):
    from PySide6.QtWidgets import QFileDialog

    monkeypatch.setattr(QFileDialog, "getExistingDirectory", lambda *a, **k: str(root))
    calls = []
    monkeypatch.setattr(
        gui.runtime.coach, "complete",
        lambda system, prompt, max_tokens=0, temperature=0.3: calls.append(prompt) or dossier,
    )
    gui.dialog.show_tab("project")
    gui.dialog.project_choose.click()
    return calls


def test_choosing_a_folder_analyses_it_and_shows_the_dossier(gui, monkeypatch, demo_project):
    calls = analyse_with(gui, monkeypatch, demo_project)
    assert wait_for(gui, lambda: gui.context.project() is not None)
    dialog = gui.dialog
    assert len(calls) == 1 and "def greet" in calls[0]
    assert dialog.project_text.toPlainText().startswith("## Overview") and dialog.project_text.isEnabled()
    assert "2 files" in dialog.project_stats.text() and dialog.project_path.text() == str(demo_project)
    assert dialog.project_again.isEnabled() and dialog.project_remove.isEnabled()
    assert "The project is ready: 2 files" in dialog.status.text()
    assert "project demo" in gui.window.context.full  # the dim line shows what the answers read
    assert Library.load().current.project == str(demo_project)


def test_a_cloud_provider_is_asked_before_the_code_is_sent(gui, monkeypatch, demo_project):
    from PySide6.QtWidgets import QMessageBox

    asked = []
    monkeypatch.setattr(QMessageBox, "question", lambda parent, title, text, *a: asked.append(text) or QMessageBox.StandardButton.No)
    gui.choose(gui.dialog.provider, "groq")
    calls = analyse_with(gui, monkeypatch, demo_project)
    gui.process()
    assert len(asked) == 1 and "demo" in asked[0] and "Groq" in asked[0]
    assert calls == [] and gui.context.project_path() == ""  # declined: nothing was read or sent
    monkeypatch.setattr(QMessageBox, "question", lambda *a, **k: QMessageBox.StandardButton.Yes)
    gui.dialog.project_choose.click()
    assert wait_for(gui, lambda: gui.context.project() is not None) and len(calls) == 1


def test_a_local_provider_is_not_asked(gui, monkeypatch, demo_project):
    from PySide6.QtWidgets import QMessageBox

    monkeypatch.setattr(QMessageBox, "question", lambda *a, **k: pytest.fail("asked about a local provider"))
    analyse_with(gui, monkeypatch, demo_project)
    assert wait_for(gui, lambda: gui.context.project() is not None)


def test_the_note_says_where_the_code_goes(gui):
    dialog = gui.dialog
    assert "stays on this computer" in dialog.project_note.text()
    gui.choose(dialog.provider, "groq")
    assert "Groq receives the files" in dialog.project_note.text() and ".env" in dialog.project_note.text()


def test_a_failed_analysis_is_reported_and_the_folder_is_kept_for_another_try(gui, monkeypatch, demo_project):
    from careertwin.interview.coach import CoachError

    def fail(system, prompt, max_tokens=0, temperature=0.3):
        raise CoachError("The provider is down")

    from PySide6.QtWidgets import QFileDialog

    monkeypatch.setattr(QFileDialog, "getExistingDirectory", lambda *a, **k: str(demo_project))
    monkeypatch.setattr(gui.runtime.coach, "complete", fail)
    gui.dialog.project_choose.click()
    assert wait_for(gui, lambda: gui.dialog.status.objectName() == "error")
    assert "provider is down" in gui.dialog.status.text()
    assert gui.context.project() is None and gui.context.project_path() == str(demo_project)
    assert gui.dialog.project_again.isEnabled()
    monkeypatch.setattr(gui.runtime.coach, "complete", lambda *a, **k: "## Overview\nOK now.")
    gui.dialog.project_again.click()
    assert wait_for(gui, lambda: gui.context.project() is not None)


def test_a_corrected_dossier_is_saved_and_used(gui, monkeypatch, demo_project):
    from careertwin.interview import project as project_store

    analyse_with(gui, monkeypatch, demo_project)
    assert wait_for(gui, lambda: gui.context.project() is not None)
    gui.dialog.project_text.setPlainText("## Overview\nI built it for my uncle's shop.")
    gui.dialog.project_save.click()
    assert gui.context.project().dossier == "## Overview\nI built it for my uncle's shop."
    assert project_store.load(demo_project).dossier.endswith("uncle's shop.")
    assert "uncle's shop" in gui.context.brief.render()


def test_removing_the_project_clears_the_tab_and_the_window(gui, monkeypatch, demo_project):
    analyse_with(gui, monkeypatch, demo_project)
    assert wait_for(gui, lambda: gui.context.project() is not None)
    gui.dialog.project_remove.click()
    dialog = gui.dialog
    assert gui.context.project() is None and gui.context.project_path() == ""
    assert dialog.project_stats.text() == "No project yet." and dialog.project_text.toPlainText() == ""
    assert not dialog.project_remove.isEnabled() and not dialog.project_save.isEnabled()
    assert "project demo" not in gui.window.context.full


def test_the_project_tab_follows_the_interview(gui, monkeypatch, demo_project):
    analyse_with(gui, monkeypatch, demo_project)
    assert wait_for(gui, lambda: gui.context.project() is not None)
    first = gui.context.current.id
    gui.window._new_interview()
    gui.dialog.refresh()
    assert gui.dialog.project_stats.text() == "No project yet."
    gui.window.switch_interview(first)
    assert "2 files" in gui.dialog.project_stats.text() and "A greeting service" in gui.dialog.project_text.toPlainText()


def test_the_project_tab_speaks_the_window_language(gui):
    gui.choose(gui.dialog.ui_language, "ru")
    assert gui.dialog.tabs.tabText(gui.dialog.TABS.index("project")) == "Проект"
    assert gui.dialog.project_choose.text() == "Выбрать папку…" and "досье" in gui.dialog.project_intro.text()
    gui.choose(gui.dialog.ui_language, "pl")
    assert gui.dialog.tabs.tabText(gui.dialog.TABS.index("project")) == "Projekt"


JAVA_ANSWER = (
    "POINTS:\n- use a `BufferedReader` in try-with-resources\nANSWER:\nClose it with try-with-resources:\n"
    "```java\ntry (BufferedReader reader = Files.newBufferedReader(path)) {\n    String line;\n}\n```\n"
    "KEY:\n- **Files.lines**: gives a `Stream<String>`\nCONTEXT:\nFine for small files."
)


def test_code_in_an_answer_is_shown_as_code_whatever_the_details_switch_says(gui):
    gui.window.details.setChecked(False)
    gui.bridge.started.emit("How do I read a file in Java?")
    gui.bridge.done.emit(JAVA_ANSWER)
    gui.process()
    text = gui.window.answer.toPlainText()
    assert "try (BufferedReader reader = Files.newBufferedReader(path)) {" in text and "    String line;" in text
    assert "Detailed answer" not in text and "Files.lines" not in text
    assert "```" not in text
    gui.window.details.setChecked(True)
    text = gui.window.answer.toPlainText()
    assert "    String line;" in text and text.count("try (BufferedReader") == 1  # the code is not shown twice
    assert "Key points" in text and "Files.lines" in text and "Fine for small files." in text
    assert "`" not in text  # backticks become monospace, they are not printed
    assert gui.window.answer_text.count("try (BufferedReader") == 1 and "```" not in gui.window.answer_text


def test_code_inside_a_list_is_shown_in_a_block_not_as_bullets(gui):
    gui.bridge.started.emit("Show me a loop")
    gui.bridge.done.emit("POINTS:\n- a plain loop\n```python\nfor n in range(3):\n    print(n)\n```\n- done")
    gui.process()
    text = gui.window.answer.toPlainText()
    assert "for n in range(3):\n    print(n)" in text.replace(" ", "\n") or "    print(n)" in text
    assert "a plain loop" in text and "done" in text and "```" not in text


def test_settings_offer_every_window_language_and_switch_to_it(gui):
    from careertwin.interview import i18n, languages

    dialog = gui.dialog
    offered = [dialog.ui_language.itemData(i) for i in range(dialog.ui_language.count())]
    assert offered == ["auto", *i18n.available()]
    assert [dialog.ui_language.itemText(i) for i in range(1, dialog.ui_language.count())] == [languages.NATIVE[c] for c in i18n.available()]
    for code, tab_title, label in (
        ("uk", "Інтерв’ю", "Мова вікна"), ("de", "Interview", "Fenstersprache"),
        ("fr", "Entretien", "Langue de la fenêtre"), ("es", "Entrevista", "Idioma de la ventana"),
    ):
        gui.choose(dialog.ui_language, code)
        assert gui.window.ui == code and dialog.tabs.tabText(0) == tab_title
        assert dialog.window_labels[0].text() == label
        assert dialog.close_button.text() == i18n.text(code, "dialog", "close")
    gui.choose(dialog.ui_language, "auto")


def test_the_project_and_ai_tabs_are_translated_too(gui):
    gui.choose(gui.dialog.ui_language, "uk")
    dialog = gui.dialog
    assert dialog.tabs.tabText(dialog.TABS.index("project")) == "Проєкт"
    assert dialog.project_choose.text() == "Вибрати папку…" and "досьє" in dialog.project_intro.text()
    assert dialog.model_labels[3].text() == "Модель для скріншотів"
    gui.choose(dialog.ui_language, "auto")


def test_the_size_of_a_small_project_is_not_shown_as_zero(gui, monkeypatch, demo_project):
    analyse_with(gui, monkeypatch, demo_project)
    assert wait_for(gui, lambda: gui.context.project() is not None)
    assert " · 0.1k · " in gui.dialog.project_stats.text()
    project = gui.context.project()
    for characters, shown in ((9_999, "10.0k"), (10_000, "10k"), (250_000, "250k"), (1_500_000, "1.5M")):
        project.characters = characters
        gui.dialog.refresh_project()
        assert f" · {shown} · " in gui.dialog.project_stats.text(), (characters, gui.dialog.project_stats.text())
