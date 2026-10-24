"""Qt window: a compact translucent strip showing the live answer to the interviewer's question."""

from __future__ import annotations

import html
import re
from collections.abc import Callable
from dataclasses import dataclass

from PySide6.QtCore import QObject, QPointF, QRectF, Qt, QTimer, Signal
from PySide6.QtGui import (
    QAction,
    QActionGroup,
    QColor,
    QFontMetrics,
    QKeySequence,
    QPainter,
    QPen,
    QShortcut,
)
from PySide6.QtWidgets import (
    QApplication,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMenu,
    QPushButton,
    QSizeGrip,
    QSizePolicy,
    QSlider,
    QTextBrowser,
    QToolButton,
    QVBoxLayout,
    QWidget,
    QWidgetAction,
)

from careertwin.interview import languages
from careertwin.interview.prompts import split_partial
from careertwin.interview.session import INTERVIEWER, YOU, Callbacks

BACKGROUND = QColor(19, 20, 30, 232)
RING = QColor(188, 180, 255, 150)
ACCENT = QColor(188, 180, 255)
TEXT = "#d7dae8"
STRONG = "#ffffff"
MUTED = "rgba(215, 218, 232, 0.50)"
STATE_COLOURS = {
    "listening": QColor(188, 180, 255),
    "thinking": QColor(120, 200, 255),
    "error": QColor(255, 120, 120),
}

STRINGS = {
    "en": {
        "ask": "Ask", "screenshot": "Screenshot", "get_answer": "Get answer",
        "placeholder": "Type a question manually…", "full_answer": "Full answer",
        "transcript": "Transcript", "pin": "Pin on top", "opacity": "Opacity",
        "add_cv": "Add CV…", "language": "Interview language", "auto": "Auto-detect",
        "quit": "Quit", "interviewer": "Interviewer", "you": "You",
        "waiting": "Waiting for the interviewer…",
        "waiting_hint": "Press {ask} to answer what was just said, or type a question above.",
        "context": "Context", "loading": "Loading model…", "listening": "Listening",
        "paused": "Paused", "thinking": "Thinking…", "idle": "Type a question",
        "no_screenshot": "Screenshots need grim (Wayland), spectacle, or maim/scrot (X11)",
        "drop_cv": "drop a PDF here to add a CV", "cv_added": "CV added",
    },
    "ru": {
        "ask": "Спросить", "screenshot": "Скриншот", "get_answer": "Получить ответ",
        "placeholder": "Введите вопрос вручную…", "full_answer": "Полный ответ",
        "transcript": "Расшифровка", "pin": "Поверх всех окон", "opacity": "Прозрачность",
        "add_cv": "Добавить резюме…", "language": "Язык интервью", "auto": "Определять сам",
        "quit": "Выход", "interviewer": "Интервьюер", "you": "Вы",
        "waiting": "Жду вопрос интервьюера…",
        "waiting_hint": "Нажмите {ask}, чтобы ответить на последнюю фразу, или введите вопрос выше.",
        "context": "Контекст", "loading": "Загружаю модель…", "listening": "Слушаю",
        "paused": "Пауза", "thinking": "Думаю…", "idle": "Введите вопрос",
        "no_screenshot": "Для скриншотов нужен grim (Wayland), spectacle или maim/scrot (X11)",
        "drop_cv": "перетащите PDF, чтобы добавить резюме", "cv_added": "Резюме добавлено",
    },
    "pl": {
        "ask": "Zapytaj", "screenshot": "Zrzut ekranu", "get_answer": "Odpowiedz",
        "placeholder": "Wpisz pytanie ręcznie…", "full_answer": "Pełna odpowiedź",
        "transcript": "Transkrypcja", "pin": "Zawsze na wierzchu", "opacity": "Przezroczystość",
        "add_cv": "Dodaj CV…", "language": "Język rozmowy", "auto": "Wykryj automatycznie",
        "quit": "Zamknij", "interviewer": "Rekruter", "you": "Ty",
        "waiting": "Czekam na pytanie rekrutera…",
        "waiting_hint": "Naciśnij {ask}, aby odpowiedzieć na ostatnie zdanie, albo wpisz pytanie.",
        "context": "Kontekst", "loading": "Ładuję model…", "listening": "Słucham",
        "paused": "Pauza", "thinking": "Myślę…", "idle": "Wpisz pytanie",
        "no_screenshot": "Zrzuty ekranu wymagają grim (Wayland), spectacle lub maim/scrot (X11)",
        "drop_cv": "upuść tu plik PDF, aby dodać CV", "cv_added": "Dodano CV",
    },
}
MENU_LANGUAGES = ["en", "ru", "pl", "uk", "de", "fr", "es", "it", "pt", "cs"]
KEYS = {"ask": "Ctrl ↵", "screenshot": "Ctrl ⇧ ↵", "previous": "Ctrl ←", "next": "Ctrl →"}


class Bridge(QObject):
    """Moves callbacks from worker threads onto the GUI thread."""

    turn = Signal(str, str)
    started = Signal(str)
    chunk = Signal(str)
    done = Signal(str)
    status = Signal(str)
    state = Signal(str)
    level = Signal(str, float)
    language = Signal(str)
    error = Signal(str)

    def callbacks(self) -> Callbacks:
        return Callbacks(
            on_turn=self.turn.emit,
            on_suggestion_start=self.started.emit,
            on_suggestion_chunk=self.chunk.emit,
            on_suggestion_done=self.done.emit,
            on_status=self.status.emit,
            on_state=self.state.emit,
            on_level=self.level.emit,
            on_language=self.language.emit,
            on_error=self.error.emit,
        )


class CircleButton(QWidget):
    """Round pause/resume button."""

    toggled = Signal(bool)

    def __init__(self) -> None:
        super().__init__()
        self.setFixedSize(44, 44)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.paused = False
        self.setEnabled(True)

    def mouseReleaseEvent(self, event) -> None:
        if self.isEnabled() and event.button() == Qt.MouseButton.LeftButton:
            self.paused = not self.paused
            self.update()
            self.toggled.emit(self.paused)

    def set_paused(self, paused: bool) -> None:
        self.paused = paused
        self.update()

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        colour = QColor(ACCENT if self.isEnabled() else QColor(120, 120, 140))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(colour)
        painter.drawEllipse(self.rect().adjusted(1, 1, -1, -1))
        painter.setBrush(QColor(19, 20, 30))
        centre = QPointF(self.width() / 2, self.height() / 2)
        if self.paused:
            painter.drawPolygon([centre + QPointF(-5, -8), centre + QPointF(-5, 8), centre + QPointF(8, 0)])
        else:
            painter.drawRoundedRect(QRectF(centre.x() - 8, centre.y() - 8, 5, 16), 1.5, 1.5)
            painter.drawRoundedRect(QRectF(centre.x() + 3, centre.y() - 8, 5, 16), 1.5, 1.5)


class StatusPill(QWidget):
    """Seven dots that follow the call audio level, or a short status text."""

    DOTS = 7

    def __init__(self) -> None:
        super().__init__()
        self.setFixedSize(150, 44)
        self.level = 0.0
        self.text = ""
        self.kind = "idle"
        self.timer = QTimer(self, interval=45)
        self.timer.timeout.connect(self._decay)
        self.timer.start()

    def push(self, level: float) -> None:
        self.level = max(self.level, level)
        self.update()

    def show_state(self, kind: str, text: str) -> None:
        self.kind, self.text = kind, "" if kind == "listening" else text
        self.setToolTip(text)
        self.update()

    def _decay(self) -> None:
        if self.level > 0:
            self.level = max(0.0, self.level - 0.09)
            self.update()

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(255, 255, 255, 22))
        painter.drawRoundedRect(self.rect(), 22, 22)
        colour = STATE_COLOURS.get(self.kind, QColor(215, 218, 232, 140))
        if self.text:
            painter.setPen(colour)
            metrics = QFontMetrics(self.font())
            painter.drawText(
                self.rect().adjusted(14, 0, -14, 0),
                Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                metrics.elidedText(self.text, Qt.TextElideMode.ElideRight, self.width() - 28),
            )
            return
        lit = round(self.level * self.DOTS)
        spacing = (self.width() - 36) / (self.DOTS - 1)
        for index in range(self.DOTS):
            active = index < lit
            painter.setBrush(colour if active else QColor(215, 218, 232, 70))
            radius = 3.6 if active else 2.8
            painter.drawEllipse(QPointF(18 + index * spacing, self.height() / 2), radius, radius)


class Pill(QWidget):
    """Outlined button with a title and a dim shortcut hint, like `Ask  Ctrl ↵`."""

    clicked = Signal()

    def __init__(self, title: str, hint: str) -> None:
        super().__init__()
        self.setFixedHeight(44)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        row = QHBoxLayout(self)
        row.setContentsMargins(20, 0, 20, 0)
        row.setSpacing(10)
        self.title = QLabel(title)
        self.hint = QLabel(hint)
        self.title.setStyleSheet(f"color:{STRONG}; font-size:16px; background:transparent;")
        self.hint.setStyleSheet(f"color:{MUTED}; font-size:13px; background:transparent;")
        row.addWidget(self.title)
        row.addWidget(self.hint)
        self.hover = False

    def set_texts(self, title: str, hint: str) -> None:
        self.title.setText(title)
        self.hint.setText(hint)

    def enterEvent(self, _event) -> None:
        self.hover = True
        self.update()

    def leaveEvent(self, _event) -> None:
        self.hover = False
        self.update()

    def mouseReleaseEvent(self, event) -> None:
        if self.isEnabled() and event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit()

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        colour = QColor(RING)
        if not self.isEnabled():
            colour.setAlpha(50)
        elif self.hover:
            colour.setAlpha(230)
        painter.setPen(QPen(colour, 1.6))
        painter.setBrush(QColor(255, 255, 255, 18 if self.hover and self.isEnabled() else 0))
        painter.drawRoundedRect(QRectF(self.rect()).adjusted(1, 1, -1, -1), 22, 22)


class ElidedLabel(QLabel):
    """A one-line label that shortens its text with an ellipsis instead of widening the window."""

    def __init__(self) -> None:
        super().__init__()
        self.full = ""
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)

    def set_full_text(self, text: str) -> None:
        self.full = text
        self._elide()

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        self._elide()

    def _elide(self) -> None:
        shown = QFontMetrics(self.font()).elidedText(
            self.full, Qt.TextElideMode.ElideRight, max(self.width(), 40)
        )
        self.setText(shown)
        self.setToolTip(self.full if shown != self.full else "")


@dataclass
class Entry:
    question: str
    raw: str = ""


def render_bold(text: str) -> str:
    """Escape, then turn **key terms** into bold; a half-streamed `**` is dropped."""
    escaped = html.escape(text)
    escaped = re.sub(r"\*\*(.+?)\*\*", rf'<b style="color:{STRONG}">\1</b>', escaped)
    return escaped.replace("**", "")


class OverlayWindow(QWidget):
    def __init__(
        self,
        bridge: Bridge,
        *,
        on_ask: Callable[[str], None],
        on_hint: Callable[[], None],
        on_pause: Callable[[bool], None],
        on_screenshot: Callable[[], None] | None = None,
        on_add_document: Callable[[str], list[tuple[str, str]]] | None = None,
        on_language: Callable[[str | None], None] | None = None,
        sources: list[tuple[str, str]] | None = None,
        listening: bool = True,
        ui_language: str = "en",
        frameless: bool = True,
        pinned: bool = True,
    ):
        super().__init__()
        self.setWindowTitle("CareerTwin · Interview")
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.frameless = frameless
        self.setAcceptDrops(True)
        self.resize(900, 400)
        self.setMinimumSize(560, 280)
        self._on_ask, self._on_hint, self._on_pause = on_ask, on_hint, on_pause
        self._on_screenshot, self._on_add_document = on_screenshot, on_add_document
        self._on_language = on_language
        self.sources = sources or []
        self.ui = ui_language if ui_language in STRINGS else "en"
        self.entries: list[Entry] = []
        self.index = -1
        self.full_answer = False
        self.last_state = ("idle" if not listening else "loading", "")
        self.setStyleSheet(
            f"""
            QWidget {{ color: {TEXT}; font-size: 15px; }}
            QLineEdit {{ background: transparent; border: none; padding: 8px 2px; font-size: 15px;
                        selection-background-color: #6f66d9; }}
            QTextBrowser {{ background: transparent; border: none; }}
            QPushButton#flat {{ background: transparent; border: none; color: {STRONG};
                               font-weight: 700; padding: 6px 4px; }}
            QPushButton#flat:hover {{ color: #bcb4ff; }}
            QToolButton {{ background: transparent; border: none; color: {MUTED}; padding: 4px 8px; }}
            QToolButton:hover {{ color: {STRONG}; }}
            QToolButton::menu-indicator {{ image: none; }}
            QMenu {{ background: rgb(30, 31, 44); border: 1px solid rgba(255,255,255,0.12);
                     border-radius: 8px; padding: 6px; }}
            QMenu::item {{ padding: 6px 18px; border-radius: 5px; }}
            QMenu::item:selected {{ background: rgba(188,180,255,0.22); }}
            QFrame#line {{ background: rgba(255,255,255,0.10); max-height: 1px; min-height: 1px; }}
            QLabel#dim {{ color: {MUTED}; font-size: 12px; }}
            QLabel#page {{ background: rgba(255,255,255,0.08); border-radius: 15px; padding: 4px 16px; min-height: 22px;
                          color: {MUTED}; font-size: 13px; }}
            """
        )

        root = QVBoxLayout(self)
        root.setContentsMargins(22, 16, 22, 10)
        root.setSpacing(8)

        self.pause = CircleButton()
        self.pause.setEnabled(listening)
        self.pause.toggled.connect(self._toggle_pause)
        self.pill = StatusPill()
        self.ask = Pill("", "")
        self.shot = Pill("", "")
        self.shot.setEnabled(on_screenshot is not None)
        self.menu_button = QToolButton()
        self.menu_button.setText("☰")
        self.menu_button.setStyleSheet(f"font-size: 22px; color: {TEXT};")
        self.menu_button.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        header = QHBoxLayout()
        header.setSpacing(12)
        for widget in (self.pause, self.pill, self.ask, self.shot):
            header.addWidget(widget)
        header.addStretch(1)
        header.addWidget(self.menu_button)
        root.addLayout(header)

        self.input = QLineEdit()
        self.get_answer = QPushButton()
        self.get_answer.setObjectName("flat")
        self.get_answer.setCursor(Qt.CursorShape.PointingHandCursor)
        entry = QHBoxLayout()
        entry.addWidget(self.input, 1)
        entry.addWidget(self.get_answer)
        root.addLayout(entry)
        rule = QFrame()
        rule.setObjectName("line")
        root.addWidget(rule)

        self.answer = QTextBrowser()
        self.answer.setOpenLinks(False)
        self.answer.setLineWrapMode(QTextBrowser.LineWrapMode.WidgetWidth)
        self.answer.document().setDefaultStyleSheet(
            "ul { margin-left: 6px; margin-top: 4px; } li { margin-bottom: 6px; } "
            "p { margin-top: 10px; line-height: 150%; }"
        )
        self.transcript = QTextBrowser()
        self.transcript.setMaximumHeight(130)
        self.transcript.hide()
        root.addWidget(self.answer, 1)
        root.addWidget(self.transcript)

        self.previous = QToolButton()
        self.next = QToolButton()
        self.page = QLabel("–")
        self.page.setObjectName("page")
        self.page.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.page.setMinimumWidth(84)
        self.page.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        footer = QHBoxLayout()
        footer.addWidget(self.previous)
        footer.addStretch(1)
        footer.addWidget(self.page)
        footer.addStretch(1)
        footer.addWidget(self.next)
        footer.addWidget(QSizeGrip(self), 0, Qt.AlignmentFlag.AlignBottom)
        root.addLayout(footer)
        self.context = ElidedLabel()
        self.context.setObjectName("dim")
        root.addWidget(self.context)

        self._build_menu(pinned)
        self._translate()
        self.pill.show_state(*self.last_state[:1], self.tr_("idle") if not listening else self.tr_("loading"))

        self.ask.clicked.connect(self._on_hint)
        self.shot.clicked.connect(self._screenshot)
        self.get_answer.clicked.connect(self._submit)
        self.input.returnPressed.connect(self._submit)
        self.previous.clicked.connect(lambda: self.show_entry(self.index - 1))
        self.next.clicked.connect(lambda: self.show_entry(self.index + 1))
        for sequence, slot in (
            ("Ctrl+Return", self._on_hint), ("Ctrl+Enter", self._on_hint),
            ("Ctrl+Shift+Return", self._screenshot), ("Ctrl+Shift+Enter", self._screenshot),
            ("Ctrl+Left", lambda: self.show_entry(self.index - 1)),
            ("Ctrl+Right", lambda: self.show_entry(self.index + 1)),
            ("Ctrl+P", self._key_pause),
        ):
            QShortcut(QKeySequence(sequence), self, activated=slot)

        bridge.turn.connect(self.add_turn)
        bridge.started.connect(self.begin_suggestion)
        bridge.chunk.connect(self.add_chunk)
        bridge.done.connect(self.finish_suggestion)
        bridge.status.connect(self._status_text)
        bridge.state.connect(self.set_state)
        bridge.level.connect(self._level)
        bridge.language.connect(self.set_ui_language)
        bridge.error.connect(self.show_error)
        self.set_pinned(pinned)
        self._render()

    def tr_(self, key: str, **values: str) -> str:
        text = STRINGS[self.ui].get(key) or STRINGS["en"][key]
        return text.format(**values) if values else text

    def _build_menu(self, pinned: bool) -> None:
        self.menu = QMenu(self)
        self.act_transcript = QAction(self, checkable=True)
        self.act_full = QAction(self, checkable=True)
        self.act_pin = QAction(self, checkable=True, checked=pinned)
        self.act_cv = QAction(self)
        self.act_quit = QAction(self)
        self.act_transcript.toggled.connect(self.transcript.setVisible)
        self.act_full.toggled.connect(self._toggle_full)
        self.act_pin.toggled.connect(self.set_pinned)
        self.act_cv.triggered.connect(self._choose_document)
        self.act_quit.triggered.connect(self.close)
        slider = QSlider(Qt.Orientation.Horizontal)
        slider.setRange(35, 100)
        slider.setValue(100)
        slider.setFixedWidth(150)
        slider.valueChanged.connect(lambda value: self.setWindowOpacity(value / 100))
        self.opacity_label = QLabel()
        opacity = QWidget()
        row = QHBoxLayout(opacity)
        row.setContentsMargins(18, 4, 18, 4)
        row.addWidget(self.opacity_label)
        row.addWidget(slider)
        self.opacity_action = QWidgetAction(self)
        self.opacity_action.setDefaultWidget(opacity)
        self.language_menu = QMenu(self)
        self.language_group = QActionGroup(self)
        self.language_actions: dict[str | None, QAction] = {}
        for code in [None, *MENU_LANGUAGES]:
            action = QAction(self, checkable=True, checked=code is None)
            action.triggered.connect(lambda _checked=False, value=code: self._choose_language(value))
            self.language_group.addAction(action)
            self.language_menu.addAction(action)
            self.language_actions[code] = action
        for item in (self.act_transcript, self.act_full, self.act_pin):
            self.menu.addAction(item)
        self.menu.addAction(self.opacity_action)
        self.menu.addSeparator()
        self.menu.addAction(self.act_cv)
        self.language_menu_action = self.menu.addMenu(self.language_menu)
        self.menu.addSeparator()
        self.menu.addAction(self.act_quit)
        self.menu_button.setMenu(self.menu)

    def _translate(self) -> None:
        keys = KEYS
        self.ask.set_texts(self.tr_("ask"), keys["ask"])
        self.shot.set_texts(self.tr_("screenshot"), keys["screenshot"])
        self.shot.setToolTip("" if self._on_screenshot else self.tr_("no_screenshot"))
        self.input.setPlaceholderText(self.tr_("placeholder"))
        self.get_answer.setText(self.tr_("get_answer"))
        self.previous.setText(keys["previous"])
        self.next.setText(keys["next"])
        self.act_transcript.setText(self.tr_("transcript"))
        self.act_full.setText(self.tr_("full_answer"))
        self.act_pin.setText(self.tr_("pin"))
        self.act_cv.setText(self.tr_("add_cv"))
        self.act_quit.setText(self.tr_("quit"))
        self.opacity_label.setText(self.tr_("opacity"))
        self.language_menu_action.setText(self.tr_("language"))
        for code, action in self.language_actions.items():
            action.setText(self.tr_("auto") if code is None else languages.NATIVE.get(code, languages.name(code)))
        self._update_context()
        self.transcript.setPlaceholderText("")

    def _update_context(self) -> None:
        labels = " · ".join(label for _kind, label in self.sources)
        hint = f"  ·  {self.tr_('drop_cv')}" if self._on_add_document else ""
        self.context.set_full_text(f"{self.tr_('context')}: {labels}{hint}" if labels else hint.lstrip(" ·"))

    def set_ui_language(self, code: str) -> None:
        """Switch the window to the interview language when we have a translation."""
        new = code if code in STRINGS else "en"
        if new == self.ui:
            return
        self.ui = new
        self._translate()
        self.pill.show_state(self.last_state[0], self._state_text(self.last_state[0]))
        self._render()

    def set_sources(self, sources: list[tuple[str, str]]) -> None:
        self.sources = sources
        self._update_context()

    def set_pinned(self, pinned: bool) -> None:
        flags = Qt.WindowType.Window
        if self.frameless:
            flags |= Qt.WindowType.FramelessWindowHint
        if pinned:
            flags |= Qt.WindowType.WindowStaysOnTopHint
        visible = self.isVisible()
        self.setWindowFlags(flags)
        if visible:
            self.show()

    def _state_text(self, kind: str) -> str:
        return self.tr_({"listening": "listening", "thinking": "thinking", "loading": "loading",
                         "paused": "paused", "idle": "idle"}.get(kind, "idle"))

    def set_state(self, kind: str) -> None:
        text = self._state_text(kind) if kind != "error" else self.pill.toolTip()
        self.last_state = (kind, text)
        self.pill.show_state(kind, text)
        self.pause.set_paused(kind == "paused")

    def _status_text(self, text: str) -> None:
        if self.last_state[0] in {"loading", "idle"}:
            self.pill.show_state(self.last_state[0], self._state_text(self.last_state[0]))

    def show_error(self, message: str) -> None:
        self.last_state = ("error", message)
        self.pill.show_state("error", message)

    def _level(self, speaker: str, level: float) -> None:
        if speaker == INTERVIEWER:
            self.pill.push(level)

    def _toggle_pause(self, paused: bool) -> None:
        self._on_pause(paused)

    def _key_pause(self) -> None:
        if self.pause.isEnabled():
            self.pause.set_paused(not self.pause.paused)
            self._on_pause(self.pause.paused)

    def _toggle_full(self, value: bool) -> None:
        self.full_answer = value
        self._render()

    def _choose_language(self, code: str | None) -> None:
        if self._on_language:
            self._on_language(code)
        if code:
            self.set_ui_language(code)

    def _choose_document(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, self.tr_("add_cv"), "", "CV (*.pdf *.docx *.txt *.md)"
        )
        if path:
            self._add_document(path)

    def _add_document(self, path: str) -> None:
        if not self._on_add_document:
            return
        try:
            self.set_sources(self._on_add_document(path))
        except Exception as exc:
            self.show_error(str(exc))
        else:
            self.pill.show_state(self.last_state[0], self.tr_("cv_added"))

    def _screenshot(self) -> None:
        if self._on_screenshot and self.shot.isEnabled():
            self._on_screenshot()

    def _submit(self) -> None:
        text = self.input.text().strip()
        if text:
            self.input.clear()
            self._on_ask(text)

    def dragEnterEvent(self, event) -> None:
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event) -> None:
        for url in event.mimeData().urls():
            if url.isLocalFile():
                self._add_document(url.toLocalFile())

    def mousePressEvent(self, event) -> None:
        child = self.childAt(event.position().toPoint())
        movable = child is None or isinstance(child, (StatusPill, QLabel, QFrame))
        if self.frameless and movable and event.button() == Qt.MouseButton.LeftButton:
            if handle := self.windowHandle():
                handle.startSystemMove()

    def paintEvent(self, _event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(QPen(QColor(255, 255, 255, 28), 1))
        painter.setBrush(BACKGROUND if self.frameless else QColor(19, 20, 30))
        painter.drawRoundedRect(QRectF(self.rect()).adjusted(0.5, 0.5, -0.5, -0.5), 20, 20)

    def speaker_label(self, speaker: str) -> str:
        return self.tr_("interviewer") if speaker == INTERVIEWER else self.tr_("you") if speaker == YOU else speaker

    def add_turn(self, speaker: str, text: str) -> None:
        colour = "#bcb4ff" if speaker == INTERVIEWER else MUTED
        self.transcript.append(
            f'<p style="margin:0 0 6px 0"><b style="color:{colour}">{html.escape(self.speaker_label(speaker))}</b> '
            f"{html.escape(text)}</p>"
        )

    def begin_suggestion(self, question: str) -> None:
        self.entries.append(Entry(question))
        self.show_entry(len(self.entries) - 1)

    def add_chunk(self, chunk: str) -> None:
        if self.entries:
            self.entries[-1].raw += chunk
            if self.index == len(self.entries) - 1:
                self._render()

    def finish_suggestion(self, text: str) -> None:
        if self.entries:
            self.entries[-1].raw = text
            if self.index == len(self.entries) - 1:
                self._render()

    def show_entry(self, index: int) -> None:
        if not self.entries:
            return
        self.index = max(0, min(index, len(self.entries) - 1))
        self._render()

    @property
    def answer_text(self) -> str:
        """The current answer as plain text, for copying."""
        if not (0 <= self.index < len(self.entries)):
            return ""
        points, spoken = split_partial(self.entries[self.index].raw)
        lines = [point.replace("**", "") for point in points]
        return "\n".join([*(f"• {line}" for line in lines), *([spoken] if spoken else [])])

    def _render(self) -> None:
        total = len(self.entries)
        self.page.setText(f"{self.index + 1}/{total}" if total else "–")
        self.previous.setEnabled(self.index > 0)
        self.next.setEnabled(0 <= self.index < total - 1)
        if not total:
            hint = self.tr_("waiting_hint", ask=f"{self.tr_('ask')} ({KEYS['ask']})")
            self.answer.setHtml(
                f'<p style="color:{MUTED}; font-size:16px">{html.escape(self.tr_("waiting"))}<br>'
                f'<span style="font-size:13px">{html.escape(hint)}</span></p>'
            )
            return
        entry = self.entries[self.index]
        points, spoken = split_partial(entry.raw)
        parts = [
            f'<div style="font-size:19px; font-weight:700; color:{STRONG}">'
            f"{html.escape(entry.question)}</div>"
        ]
        if points:
            items = "".join(f"<li>{render_bold(point)}</li>" for point in points)
            parts.append(f'<ul style="font-size:16px">{items}</ul>')
        if spoken and (self.full_answer or not points):
            parts.append(f'<p style="font-size:16px; color:{TEXT}">{render_bold(spoken)}</p>')
        scroll = self.answer.verticalScrollBar().value()
        self.answer.setHtml("".join(parts))
        self.answer.verticalScrollBar().setValue(scroll)


def create_app(argv: list[str] | None = None) -> QApplication:
    app = QApplication.instance() or QApplication(argv or [])
    app.setApplicationName("careertwin-interview")
    app.setDesktopFileName("careertwin-interview")
    return app
