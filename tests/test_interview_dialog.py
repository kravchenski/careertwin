"""The interview window and its settings dialog, driven offscreen."""

from __future__ import annotations

import base64

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
        self.window.show()
        self.window.open_settings("docs")
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
    gui.window.open_settings("screenshot")
    assert gui.dialog.tabs.tabText(gui.dialog.tabs.currentIndex()) == "Screenshot"
    assert [gui.dialog.tabs.tabText(i) for i in range(gui.dialog.tabs.count())] == [
        "Documents", "Vacancy", "About me", "Language", "Audio && speech",
        "AI model", "Screenshot", "Window",
    ]


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
    gui.dialog.full_answer.setChecked(True)
    assert gui.window.full_answer and gui.window.act_full.isChecked()
    gui.choose(gui.dialog.ui_language, "pl")
    assert gui.window.ui == "pl" and gui.window.ask.title.text() == "Zapytaj" and gui.window.ui_locked
    gui.bridge.language.emit("en")
    assert gui.window.ui == "pl"
    assert gui.dialog.tabs.tabText(0) == "Dokumenty"
    gui.choose(gui.dialog.ui_language, "auto")
    gui.bridge.language.emit("ru")
    assert gui.window.ui == "ru"


def test_menu_quick_toggles_are_saved_too(gui):
    gui.window.act_pin.setChecked(False)
    gui.window.act_full.setChecked(True)
    gui.window.language_actions["de"].trigger()
    saved = InterviewSettings.load()
    assert (saved.pinned, saved.show_full_answer, saved.language) == (False, True, "de")
    assert gui.dialog is gui.window.settings_dialog


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


def test_screenshot_tab_controls_the_button(gui, monkeypatch):
    assert gui.window.shot.isEnabled()
    gui.dialog.shot_enabled.setChecked(False)
    assert not gui.window.shot.isEnabled() and "Settings" in gui.window.shot.toolTip()
    gui.dialog.shot_enabled.setChecked(True)
    assert gui.window.shot.isEnabled()
    assert [gui.dialog.shot_tool.itemData(i) for i in range(gui.dialog.shot_tool.count())] == ["auto", "grim", "maim"]
    gui.choose(gui.dialog.shot_tool, "maim")
    gui.dialog.shot_delay.setValue(0)
    assert gui.runtime.settings.screenshot_tool == "maim" and gui.runtime.settings.screenshot_delay_ms == 0
    monkeypatch.setattr(screenshot, "available_tools", lambda: [])
    gui.dialog.refresh_screenshot()
    gui.window.refresh_controls()
    assert not gui.window.shot.isEnabled() and "grim" in gui.dialog.shot_tools.text()


def test_screenshot_button_sends_the_capture_to_the_session(gui, monkeypatch):
    sent = []
    monkeypatch.setattr(screenshot, "capture_png", lambda tool="auto": b"png-bytes")
    monkeypatch.setattr(gui.session, "ask_screenshot", sent.append)
    gui.runtime.update("screenshot_delay_ms", 0)
    gui.window.shot.clicked.emit()
    for _ in range(20):
        gui.process()
        if sent:
            break
    assert sent == [base64.b64encode(b"png-bytes").decode()] and gui.window.windowOpacity() == 1.0
    monkeypatch.setattr(screenshot, "capture_png", lambda tool="auto": (_ for _ in ()).throw(screenshot.ScreenshotError("grim failed")))
    gui.window.shot.clicked.emit()
    for _ in range(20):
        gui.process()
    assert gui.window.last_state == ("error", "grim failed") and gui.window.windowOpacity() == 1.0


def test_capture_test_reports_the_image_size(gui):
    from PySide6.QtCore import QBuffer, QByteArray
    from PySide6.QtGui import QImage

    image = QImage(32, 20, QImage.Format.Format_RGB32)
    image.fill(0)
    data = QByteArray()
    buffer = QBuffer(data)
    buffer.open(QBuffer.OpenModeFlag.WriteOnly)
    image.save(buffer, "PNG")
    buffer.close()
    gui.dialog._capture_done(base64.b64encode(bytes(data)).decode(), None)
    assert "32×20" in gui.dialog.status.text()
    gui.dialog._capture_done(None, "grim failed")
    assert gui.dialog.status.objectName() == "error"


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
    assert gui.context.brief.documents == [] and gui.dialog.tabs.currentIndex() == gui.dialog.TABS.index("vacancy")
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
    assert gui.context.interview_name().startswith("Interview") and gui.dialog.tabs.currentIndex() == 1
