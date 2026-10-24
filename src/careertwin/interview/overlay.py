"""Qt window that shows the live transcript and streaming answer suggestions."""

from __future__ import annotations

import html
from collections.abc import Callable

from PySide6.QtCore import QObject, Qt, Signal
from PySide6.QtGui import QGuiApplication, QKeySequence, QShortcut
from PySide6.QtWidgets import (
    QApplication,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMainWindow,
    QPushButton,
    QSlider,
    QSplitter,
    QTextBrowser,
    QVBoxLayout,
    QWidget,
)

from careertwin.interview.prompts import split_partial
from careertwin.interview.session import INTERVIEWER, Callbacks

STYLE = """
QTextBrowser, QLineEdit { border: 1px solid palette(mid); border-radius: 6px; padding: 8px; }
QPushButton { padding: 5px 12px; }
QPushButton:checked { background: palette(highlight); color: palette(highlighted-text); }
QLabel#heading { font-weight: 600; font-size: 11px; }
QLabel#status { font-weight: 600; }
"""


class Bridge(QObject):
    """Moves callbacks from worker threads onto the GUI thread."""

    turn = Signal(str, str)
    started = Signal(str)
    chunk = Signal(str)
    done = Signal(str)
    status = Signal(str)
    error = Signal(str)

    def callbacks(self) -> Callbacks:
        return Callbacks(
            on_turn=self.turn.emit,
            on_suggestion_start=self.started.emit,
            on_suggestion_chunk=self.chunk.emit,
            on_suggestion_done=self.done.emit,
            on_status=self.status.emit,
            on_error=self.error.emit,
        )


class OverlayWindow(QMainWindow):
    def __init__(
        self,
        bridge: Bridge,
        *,
        on_ask: Callable[[str], None],
        on_hint: Callable[[], None],
        on_pause: Callable[[bool], None],
        listening: bool = True,
    ):
        super().__init__()
        self.setWindowTitle("CareerTwin · Interview")
        self.resize(480, 760)
        self.setStyleSheet(STYLE)
        self.buffer = ""
        self.question = ""
        self.answer = ""
        self._on_ask, self._on_hint, self._on_pause = on_ask, on_hint, on_pause

        root = QWidget()
        layout = QVBoxLayout(root)
        layout.setContentsMargins(12, 12, 12, 12)
        layout.setSpacing(8)
        self.setCentralWidget(root)

        self.status = QLabel("Starting…" if listening else "Type a question below")
        self.status.setObjectName("status")
        self.pause = QPushButton("Pause")
        self.pause.setCheckable(True)
        self.pause.setEnabled(listening)
        self.hint = QPushButton("Hint now")
        self.pin = QPushButton("Pin")
        self.pin.setCheckable(True)
        self.pin.setChecked(True)
        header = QHBoxLayout()
        header.addWidget(self.status, 1)
        for button in (self.pause, self.hint, self.pin):
            header.addWidget(button)
        layout.addLayout(header)

        opacity_row = QHBoxLayout()
        opacity_row.addWidget(QLabel("Opacity"))
        self.opacity = QSlider(Qt.Orientation.Horizontal)
        self.opacity.setRange(35, 100)
        self.opacity.setValue(100)
        opacity_row.addWidget(self.opacity, 1)
        layout.addLayout(opacity_row)

        self.conversation = QTextBrowser()
        self.suggestion = QTextBrowser()
        self.suggestion.setPlaceholderText(
            "Answer suggestions appear here when the interviewer asks a question."
        )
        splitter = QSplitter(Qt.Orientation.Vertical)
        splitter.addWidget(self._section("Conversation", self.conversation))
        splitter.addWidget(self._section("Suggested answer", self.suggestion))
        splitter.setSizes([260, 400])
        layout.addWidget(splitter, 1)

        self.input = QLineEdit()
        self.input.setPlaceholderText("Type or paste a question, then press Enter")
        self.copy = QPushButton("Copy answer")
        entry = QHBoxLayout()
        entry.addWidget(self.input, 1)
        entry.addWidget(self.copy)
        layout.addLayout(entry)

        note = QLabel("Speech is transcribed on this computer. Only question text and your own "
                      "background are sent to the model.")
        note.setWordWrap(True)
        note.setEnabled(False)
        layout.addWidget(note)

        self.pause.toggled.connect(self._toggle_pause)
        self.hint.clicked.connect(self._on_hint)
        self.pin.toggled.connect(self.set_pinned)
        self.opacity.valueChanged.connect(lambda v: self.setWindowOpacity(v / 100))
        self.input.returnPressed.connect(self._submit)
        self.copy.clicked.connect(self._copy)
        QShortcut(QKeySequence("Ctrl+H"), self, activated=self._on_hint)
        QShortcut(QKeySequence("Ctrl+P"), self, activated=self.pause.toggle)

        bridge.turn.connect(self.add_turn)
        bridge.started.connect(self.begin_suggestion)
        bridge.chunk.connect(self.add_chunk)
        bridge.done.connect(self.finish_suggestion)
        bridge.status.connect(self.status.setText)
        bridge.error.connect(self.show_error)
        self.set_pinned(True)

    @staticmethod
    def _section(title: str, body: QWidget) -> QWidget:
        box = QWidget()
        column = QVBoxLayout(box)
        column.setContentsMargins(0, 0, 0, 0)
        heading = QLabel(title.upper())
        heading.setObjectName("heading")
        column.addWidget(heading)
        column.addWidget(body, 1)
        return box

    def set_pinned(self, pinned: bool) -> None:
        self.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint, pinned)
        self.show()

    def _toggle_pause(self, paused: bool) -> None:
        self.pause.setText("Resume" if paused else "Pause")
        self._on_pause(paused)

    def _submit(self) -> None:
        text = self.input.text().strip()
        if text:
            self.input.clear()
            self._on_ask(text)

    def _copy(self) -> None:
        QGuiApplication.clipboard().setText(self.answer or self.buffer)

    def add_turn(self, speaker: str, text: str) -> None:
        colour = "palette(link)" if speaker == INTERVIEWER else "palette(text)"
        self.conversation.append(
            f'<p style="margin:0 0 6px 0"><b style="color:{colour}">{html.escape(speaker)}</b><br>'
            f"{html.escape(text)}</p>"
        )

    def begin_suggestion(self, question: str) -> None:
        self.question, self.buffer, self.answer = question, "", ""
        self._render()

    def add_chunk(self, chunk: str) -> None:
        self.buffer += chunk
        self._render()

    def finish_suggestion(self, text: str) -> None:
        self.buffer = text
        _points, self.answer = split_partial(text)
        self._render()

    def show_error(self, message: str) -> None:
        self.status.setText(message)

    def _render(self) -> None:
        points, answer = split_partial(self.buffer)
        parts = [f'<p style="color:gray;margin:0 0 8px 0">{html.escape(self.question)}</p>']
        if points:
            items = "".join(f"<li>{html.escape(point)}</li>" for point in points)
            parts.append(f'<ul style="margin:0 0 10px 16px;font-size:15px">{items}</ul>')
        if answer:
            parts.append(f'<p style="font-size:16px;line-height:140%">{html.escape(answer)}</p>')
        self.suggestion.setHtml("".join(parts))
        self.suggestion.verticalScrollBar().setValue(0)


def create_app(argv: list[str] | None = None) -> QApplication:
    return QApplication.instance() or QApplication(argv or [])
