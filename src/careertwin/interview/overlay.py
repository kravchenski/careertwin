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
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMenu,
    QSizePolicy,
    QTextBrowser,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

from careertwin.interview.prompts import sections
from careertwin.interview.runtime import Runtime
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
        "ask": "Ask", "screenshot": "Screenshot",
        "placeholder": "Type a question manually…", "details_title": "Detailed answer", "details_main": "Main answer", "details_key": "Key points", "details_context": "Practical context", "details_button": "Details",
        "transcript": "Transcript", "opacity": "Opacity",
        "add_cv": "Add CV…",
        "quit": "Quit", "interviewer": "Interviewer", "you": "You",
        "waiting": "Waiting for the interviewer…",
        "waiting_hint": "Press {ask} to answer what was just said, or type a question above.",
        "context": "Context", "loading": "Loading model…", "listening": "Listening",
        "paused": "Paused", "thinking": "Thinking…", "idle": "Type a question",
        "no_screenshot": "Screenshots need grim (Wayland), spectacle, or maim/scrot (X11)",
        "drop_cv": "drop a PDF here to add a CV", "cv_added": "CV added",
        "settings": "Settings…",
        "no_context": "No context yet",
        "no_context_hint": "Open Settings → Interview and add your CV, or write about yourself in About me.",
        "shot_off": "Screenshots are turned off in Settings",
        "interview_menu": "Interview", "new_interview": "New interview…", "listening_to": "Listening to",
        "no_vacancy_hint": "No vacancy yet: Settings → Interview to paste it and to add the CV you sent.",
    },
    "ru": {
        "ask": "Спросить", "screenshot": "Скриншот",
        "placeholder": "Введите вопрос вручную…", "details_title": "Подробный ответ", "details_main": "Основной ответ", "details_key": "Ключевые моменты", "details_context": "Практический контекст", "details_button": "Подробно",
        "transcript": "Расшифровка", "opacity": "Прозрачность",
        "add_cv": "Добавить резюме…",
        "quit": "Выход", "interviewer": "Интервьюер", "you": "Вы",
        "waiting": "Жду вопрос интервьюера…",
        "waiting_hint": "Нажмите {ask}, чтобы ответить на последнюю фразу, или введите вопрос выше.",
        "context": "Контекст", "loading": "Загружаю модель…", "listening": "Слушаю",
        "paused": "Пауза", "thinking": "Думаю…", "idle": "Введите вопрос",
        "no_screenshot": "Для скриншотов нужен grim (Wayland), spectacle или maim/scrot (X11)",
        "drop_cv": "перетащите PDF, чтобы добавить резюме", "cv_added": "Резюме добавлено",
        "settings": "Настройки…",
        "no_context": "Контекста пока нет",
        "no_context_hint": "Откройте Настройки → Интервью и добавьте резюме или напишите о себе во вкладке «Обо мне».",
        "shot_off": "Скриншоты выключены в настройках",
        "interview_menu": "Интервью", "new_interview": "Новое интервью…", "listening_to": "Слушаю",
        "no_vacancy_hint": "Вакансии пока нет: Настройки → Интервью, чтобы вставить её и добавить резюме, которое вы отправляли.",
    },
    "pl": {
        "ask": "Zapytaj", "screenshot": "Zrzut ekranu",
        "placeholder": "Wpisz pytanie ręcznie…", "details_title": "Szczegółowa odpowiedź", "details_main": "Odpowiedź główna", "details_key": "Kluczowe punkty", "details_context": "Kontekst praktyczny", "details_button": "Szczegóły",
        "transcript": "Transkrypcja", "opacity": "Przezroczystość",
        "add_cv": "Dodaj CV…",
        "quit": "Zamknij", "interviewer": "Rekruter", "you": "Ty",
        "waiting": "Czekam na pytanie rekrutera…",
        "waiting_hint": "Naciśnij {ask}, aby odpowiedzieć na ostatnie zdanie, albo wpisz pytanie.",
        "context": "Kontekst", "loading": "Ładuję model…", "listening": "Słucham",
        "paused": "Pauza", "thinking": "Myślę…", "idle": "Wpisz pytanie",
        "no_screenshot": "Zrzuty ekranu wymagają grim (Wayland), spectacle lub maim/scrot (X11)",
        "drop_cv": "upuść tu plik PDF, aby dodać CV", "cv_added": "Dodano CV",
        "settings": "Ustawienia…",
        "no_context": "Brak kontekstu",
        "no_context_hint": "Otwórz Ustawienia → Rozmowa i dodaj CV albo napisz o sobie w zakładce „O mnie”.",
        "shot_off": "Zrzuty ekranu są wyłączone w ustawieniach",
        "interview_menu": "Rozmowa", "new_interview": "Nowa rozmowa…", "listening_to": "Słucham",
        "no_vacancy_hint": "Brak oferty: Ustawienia → Rozmowa, aby ją wkleić i dodać wysłane CV.",
    },
}
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
    source = Signal(str)
    partial = Signal(str)
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
            on_source=self.source.emit,
            on_partial=self.partial.emit,
            on_error=self.error.emit,
        )


class CircleButton(QWidget):
    """Round pause/resume button."""

    toggled = Signal(bool)

    def __init__(self) -> None:
        super().__init__()
        self.setFixedSize(44, 44)
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
        self.source_line = ""
        self.timer = QTimer(self, interval=45)
        self.timer.timeout.connect(self._decay)
        self.timer.start()

    def push(self, level: float) -> None:
        self.level = max(self.level, level)
        self.update()

    def show_state(self, kind: str, text: str) -> None:
        self.kind, self.text = kind, "" if kind == "listening" else text
        self.setToolTip(f"{text}\n{self.source_line}" if self.source_line else text)
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

    clicked = Signal()

    def __init__(self) -> None:
        super().__init__()
        self.full = ""
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)

    def mouseReleaseEvent(self, event) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit()

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
        runtime: Runtime | None = None,
        on_ask: Callable[[str], None] | None = None,
        on_hint: Callable[[], None] | None = None,
        on_pause: Callable[[bool], None] | None = None,
        on_screenshot: Callable[[], None] | None = None,
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
        self.runtime = runtime
        session = runtime.session if runtime else None
        self._on_ask = on_ask or (session.ask if session else lambda _text: None)
        self._on_hint = on_hint or (session.hint_now if session else lambda: None)
        self._on_pause = on_pause or (session.pause if session else lambda _paused: None)
        self._on_screenshot = on_screenshot
        self.sources = sources or []
        self.pinned = pinned
        self.font_scale = 100
        self.ui_locked = False
        self.settings_dialog = None
        self.ui = ui_language if ui_language in STRINGS else "en"
        self.entries: list[Entry] = []
        self.index = -1
        self.full_answer = False
        self.layout_mode = "lines"
        self.last_state = ("idle" if not listening else "loading", "")
        self.setStyleSheet(
            f"""
            QWidget {{ color: {TEXT}; font-size: 15px; }}
            QLineEdit {{ background: transparent; border: none; padding: 8px 2px; font-size: 15px;
                        selection-background-color: #6f66d9; }}
            QTextBrowser {{ background: transparent; border: none; }}
            QToolButton {{ background: transparent; border: none; color: {MUTED}; padding: 4px 8px; }}
            QToolButton:hover {{ color: {STRONG}; }}
            QToolButton::menu-indicator {{ image: none; }}
            QMenu {{ background: rgb(30, 31, 44); border: 1px solid rgba(255,255,255,0.12);
                     border-radius: 8px; padding: 6px; }}
            QMenu::item {{ padding: 6px 18px; border-radius: 5px; }}
            QMenu::item:selected {{ background: rgba(188,180,255,0.22); }}
            QFrame#line {{ background: rgba(255,255,255,0.10); max-height: 1px; min-height: 1px; }}
            QLabel#dim {{ color: {MUTED}; font-size: 12px; }}
            QLabel#heard {{ color: #bcb4ff; font-size: 14px; padding: 2px 4px; }}
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
        self.shot.setEnabled(on_screenshot is not None or bool(runtime and runtime.screenshot_ready()))
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
        root.addWidget(self.input)
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
        self.details = QToolButton()
        self.details.setCheckable(True)
        self.details.setStyleSheet(
            f"QToolButton {{ border: 1px solid rgba(188,180,255,0.45); border-radius: 12px; "
            f"padding: 3px 14px; color: {TEXT}; }} "
            f"QToolButton:checked {{ background: rgba(188,180,255,0.30); color: {STRONG}; }}"
        )
        self.details.toggled.connect(lambda value: self._quick("show_full_answer", value))
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
        footer.addWidget(self.details)
        footer.addWidget(self.next)
        root.addLayout(footer)
        self.heard = ElidedLabel()
        self.heard.setObjectName("heard")
        self.heard.setCursor(Qt.CursorShape.ArrowCursor)
        self.heard.hide()
        root.insertWidget(root.indexOf(self.answer), self.heard)
        self.context = ElidedLabel()
        self.context.setObjectName("dim")
        self.context.clicked.connect(lambda: self.open_settings("interview"))
        root.addWidget(self.context)

        self._build_menu()
        self._translate()
        self.pill.show_state(*self.last_state[:1], self.tr_("idle") if not listening else self.tr_("loading"))

        self.ask.clicked.connect(self._on_hint)
        self.shot.clicked.connect(self._screenshot)
        self.input.returnPressed.connect(self._submit)
        self.previous.clicked.connect(lambda: self.show_entry(self.index - 1))
        self.next.clicked.connect(lambda: self.show_entry(self.index + 1))
        for sequence, slot in (
            ("Ctrl+Return", self._on_hint), ("Ctrl+Enter", self._on_hint),
            ("Ctrl+Shift+Return", self._screenshot), ("Ctrl+Shift+Enter", self._screenshot),
            ("Ctrl+Left", lambda: self.show_entry(self.index - 1)),
            ("Ctrl+Right", lambda: self.show_entry(self.index + 1)),
            ("Ctrl+P", self._key_pause),
            ("Ctrl+D", self.details.toggle),
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
        bridge.source.connect(self._source_changed)
        bridge.partial.connect(self._partial)
        bridge.error.connect(self.show_error)
        self.set_pinned(pinned)
        self._render()

    def tr_(self, key: str, **values: str) -> str:
        text = STRINGS[self.ui].get(key) or STRINGS["en"][key]
        return text.format(**values) if values else text

    def _build_menu(self) -> None:
        self.menu = QMenu(self)
        self.act_settings = QAction(self)
        self.act_transcript = QAction(self, checkable=True)
        self.act_quit = QAction(self)
        self.act_settings.triggered.connect(lambda: self.open_settings("general"))
        self.act_transcript.toggled.connect(self.transcript.setVisible)
        self.act_quit.triggered.connect(self.close)
        self.menu.addAction(self.act_settings)
        self.interview_menu = QMenu(self)
        self.interview_menu.aboutToShow.connect(self._fill_interview_menu)
        self.interview_menu_action = self.menu.addMenu(self.interview_menu)
        self.menu.addAction(self.act_transcript)
        self.menu.addSeparator()
        self.menu.addAction(self.act_quit)
        self.menu_button.setMenu(self.menu)

    def _fill_interview_menu(self) -> None:
        """List the saved interviews so one click switches vacancy, CV and notes together."""
        self.interview_menu.clear()
        if not self.runtime:
            return
        context = self.runtime.context
        group = QActionGroup(self.interview_menu)
        for interview_id, name in context.interviews():
            action = QAction(name, self.interview_menu, checkable=True)
            action.setChecked(interview_id == context.current.id)
            action.triggered.connect(lambda _c=False, key=interview_id: self.switch_interview(key))
            group.addAction(action)
            self.interview_menu.addAction(action)
        self.interview_menu.addSeparator()
        new = self.interview_menu.addAction(self.tr_("new_interview"))
        new.triggered.connect(self._new_interview)

    def switch_interview(self, interview_id: str) -> None:
        if not self.runtime:
            return
        problems = self.runtime.context.switch_interview(interview_id)
        self.refresh_controls()
        if self.settings_dialog is not None:
            self.settings_dialog.refresh()
        if problems:
            self.show_error(problems[0])

    def _new_interview(self) -> None:
        if not self.runtime:
            return
        self.runtime.context.new_interview()
        self.refresh_controls()
        self.open_settings("interview")

    def _quick(self, name: str, value: bool) -> None:
        """A menu toggle: saved through the runtime when there is one, else applied locally."""
        if self.runtime:
            try:
                self.runtime.update(name, value)
            except ValueError as exc:
                self.show_error(str(exc))
        else:
            self.apply_setting(name, value)

    def _translate(self) -> None:
        keys = KEYS
        self.ask.set_texts(self.tr_("ask"), keys["ask"])
        self.shot.set_texts(self.tr_("screenshot"), keys["screenshot"])
        self._refresh_screenshot()
        self.input.setPlaceholderText(self.tr_("placeholder"))
        self.previous.setText(keys["previous"])
        self.next.setText(keys["next"])
        self.act_transcript.setText(self.tr_("transcript"))
        self.details.setText(self.tr_("details_button"))
        self.interview_menu_action.setText(self.tr_("interview_menu"))
        self.act_settings.setText(self.tr_("settings"))
        self.act_quit.setText(self.tr_("quit"))
        self._update_context()
        self.transcript.setPlaceholderText("")

    def _refresh_screenshot(self) -> None:
        """Enable the Screenshot pill only when a tool exists and the setting is on."""
        if self.runtime:
            ready = self.runtime.screenshot_ready()
            reason = "" if ready else self.tr_(
                "no_screenshot" if self.runtime.settings.screenshot_enabled else "shot_off"
            )
        else:
            ready = self._on_screenshot is not None
            reason = "" if ready else self.tr_("no_screenshot")
        self.shot.setEnabled(ready)
        self.shot.setToolTip(reason)

    def refresh_controls(self) -> None:
        """Re-read everything that depends on the context and settings."""
        self._refresh_screenshot()
        self._update_context()
        self._render()

    def _update_context(self) -> None:
        sources = self.runtime.context.sources() if self.runtime else self.sources
        labels = " · ".join(label for _kind, label in sources)
        hint = f"  ·  {self.tr_('drop_cv')}"
        name = self.runtime.context.interview_name() if self.runtime else ""
        head = f"{name} — {labels}" if name and labels else name or labels
        self.context.set_full_text(f"{head}{hint}" if head else hint.lstrip(" ·"))

    def set_ui_language(self, code: str, manual: bool = False) -> None:
        """Switch the window language. Automatic switches follow the interview language and
        are ignored once the interface language is chosen in Settings."""
        if self.ui_locked and not manual:
            return
        new = code if code in STRINGS else "en"
        if new == self.ui:
            return
        self.ui = new
        self._translate()
        self.pill.show_state(self.last_state[0], self._state_text(self.last_state[0]))
        self._render()
        if self.settings_dialog is not None:
            self.settings_dialog.retranslate()

    def set_sources(self, sources: list[tuple[str, str]]) -> None:
        self.sources = sources
        self._update_context()

    def apply_setting(self, name: str, value) -> None:
        """Apply one saved setting to the window (called by the runtime and at start)."""
        if name == "pinned":
            self.pinned = bool(value)
            self.set_pinned(self.pinned)
        elif name == "frameless":
            self.frameless = bool(value)
            self.set_pinned(self.pinned)
            self.update()
        elif name == "opacity":
            self.setWindowOpacity(int(value) / 100)
        elif name == "font_scale":
            self.font_scale = int(value)
            self._render()
        elif name == "show_full_answer":
            self.full_answer = bool(value)
            self.details.blockSignals(True)
            self.details.setChecked(self.full_answer)
            self.details.blockSignals(False)
            self._render()
        elif name == "answer_layout":
            self.layout_mode = str(value)
            self.details.setVisible(self.layout_mode != "main")
            self._render()
        elif name == "ui_language":
            self.ui_locked = value != "auto"
            if self.ui_locked:
                self.set_ui_language(str(value), manual=True)
        elif name == "listen":
            self.pause.setEnabled(bool(value))

    def apply_settings(self, settings) -> None:
        for name in ("opacity", "font_scale", "show_full_answer", "answer_layout", "ui_language", "listen"):
            self.apply_setting(name, getattr(settings, name))

    def open_settings(self, tab: str = "interview") -> None:
        """Show the settings dialog (created on first use) on the given tab."""
        if self.runtime is None:
            return
        from careertwin.interview.settings_dialog import SettingsDialog

        if self.settings_dialog is None:
            self.settings_dialog = SettingsDialog(self, self.runtime)
        self.settings_dialog.show_tab(tab)

    def set_pinned(self, pinned: bool) -> None:
        self.pinned = pinned
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

    def _partial(self, text: str) -> None:
        """The words being spoken right now, before the question is complete."""
        self.heard.set_full_text(f"🎧 {text}" if text else "")
        self.heard.setVisible(bool(text))

    def _source_changed(self, name: str) -> None:
        self.pill.source_line = f"{self.tr_('listening_to')}: {name}"
        self.pill.show_state(*self.last_state[:1], self.last_state[1] or self._state_text(self.last_state[0]))

    def _level(self, speaker: str, level: float) -> None:
        if speaker == INTERVIEWER:
            self.pill.push(level)

    def _toggle_pause(self, paused: bool) -> None:
        self._on_pause(paused)

    def _key_pause(self) -> None:
        if self.pause.isEnabled():
            self.pause.set_paused(not self.pause.paused)
            self._on_pause(self.pause.paused)

    def add_files(self, paths: list[str]) -> None:
        """Add CVs or other documents about you to the context."""
        if not self.runtime:
            return
        errors = self.runtime.context.add_documents(paths)
        self.refresh_controls()
        if errors:
            self.show_error(errors[0])
        else:
            self.pill.show_state(self.last_state[0], self.tr_("cv_added"))

    def _screenshot(self) -> None:
        if not self.shot.isEnabled():
            return
        if not self.runtime:
            if self._on_screenshot:
                self._on_screenshot()
            return
        previous = self.windowOpacity()
        self.setWindowOpacity(0.0)
        QApplication.processEvents()

        def capture() -> None:
            try:
                data = self.runtime.capture()
            except Exception as exc:
                self.setWindowOpacity(previous)
                self.show_error(str(exc))
                return
            self.setWindowOpacity(previous)
            self.runtime.session.ask_screenshot(data)

        QTimer.singleShot(self.runtime.settings.screenshot_delay_ms, capture)

    def _submit(self) -> None:
        text = self.input.text().strip()
        if text:
            self.input.clear()
            self._on_ask(text)

    def dragEnterEvent(self, event) -> None:
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event) -> None:
        self.add_files([url.toLocalFile() for url in event.mimeData().urls() if url.isLocalFile()])

    RESIZE_EDGE = 6

    def _edges_at(self, point) -> Qt.Edge:
        """The window edges a press at `point` grabs; none away from the border."""
        edges = Qt.Edge(0)
        if point.x() <= self.RESIZE_EDGE:
            edges |= Qt.Edge.LeftEdge
        elif point.x() >= self.width() - self.RESIZE_EDGE:
            edges |= Qt.Edge.RightEdge
        if point.y() <= self.RESIZE_EDGE:
            edges |= Qt.Edge.TopEdge
        elif point.y() >= self.height() - self.RESIZE_EDGE:
            edges |= Qt.Edge.BottomEdge
        return edges

    def mousePressEvent(self, event) -> None:
        if not self.frameless or event.button() != Qt.MouseButton.LeftButton:
            return
        handle = self.windowHandle()
        if handle is None:
            return
        if edges := self._edges_at(event.position().toPoint()):
            handle.startSystemResize(edges)
            return
        child = self.childAt(event.position().toPoint())
        if child is None or isinstance(child, (StatusPill, QLabel, QFrame)):
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
        """The current answer as plain text, for copying (details only when they are shown)."""
        if not (0 <= self.index < len(self.entries)):
            return ""
        parts = sections(self.entries[self.index].raw)
        plain = lambda line: line.replace("**", "")  # noqa: E731
        if self.layout_mode == "main" and parts.answer:
            return plain(parts.answer)
        lines = [f"• {plain(point)}" for point in parts.points]
        if parts.answer and (self.full_answer or not parts.points):
            lines.append(parts.answer)
        if self.full_answer:
            lines += [f"• {plain(line)}" for line in parts.key]
            if parts.context:
                lines.append(parts.context)
        return "\n".join(lines)

    def _details_html(self, found, px) -> str:
        """The detailed answer: main answer, key points, practical context."""
        heading = lambda size, text: (  # noqa: E731
            f'<div style="font-size:{px(size)}px; font-weight:700; color:{STRONG}; '
            f'margin-top:14px">{html.escape(text)}</div>'
        )
        body = f"font-size:{px(15)}px; color:{TEXT}"
        parts = [heading(18, self.tr_("details_title"))]
        if found.answer:
            parts += [heading(16, self.tr_("details_main")), f'<p style="{body}">{render_bold(found.answer)}</p>']
        if found.key:
            items = "".join(f"<li>{render_bold(line)}</li>" for line in found.key)
            parts += [heading(16, self.tr_("details_key")), f'<ul style="{body}">{items}</ul>']
        if found.context:
            parts += [heading(16, self.tr_("details_context")), f'<p style="{body}">{render_bold(found.context)}</p>']
        return "".join(parts)

    def _render(self) -> None:
        total = len(self.entries)
        self.page.setText(f"{self.index + 1}/{total}" if total else "–")
        self.previous.setEnabled(self.index > 0)
        self.next.setEnabled(0 <= self.index < total - 1)
        scale = self.font_scale / 100
        px = lambda size: round(size * scale)  # noqa: E731
        if not total:
            if self.runtime and self.runtime.context.brief.is_empty():
                title, hint = self.tr_("no_context"), self.tr_("no_context_hint")
            else:
                title = self.tr_("waiting")
                hint = self.tr_("waiting_hint", ask=f"{self.tr_('ask')} ({KEYS['ask']})")
                if self.runtime and not self.runtime.context.brief.job_description:
                    hint += "  " + self.tr_("no_vacancy_hint")
            self.answer.setHtml(
                f'<p style="color:{MUTED}; font-size:{px(16)}px">{html.escape(title)}<br>'
                f'<span style="font-size:{px(13)}px">{html.escape(hint)}</span></p>'
            )
            return
        entry = self.entries[self.index]
        found = sections(entry.raw)
        points, spoken = found.points, found.answer
        parts = [
            f'<div style="font-size:{px(19)}px; font-weight:700; color:{STRONG}">'
            f"{html.escape(entry.question)}</div>"
        ]
        if self.layout_mode == "main" and spoken:
            parts.append(
                f'<p style="font-size:{px(18)}px; color:{TEXT}; line-height:155%; margin-top:8px">'
                f"{render_bold(spoken)}</p>"
            )
            points = []
        if points:
            items = "".join(f"<li>{render_bold(point)}</li>" for point in points)
            parts.append(f'<ul style="font-size:{px(16)}px">{items}</ul>')
        if self.layout_mode == "main":
            pass
        elif not points and spoken:
            parts.append(f'<p style="font-size:{px(16)}px; color:{TEXT}">{render_bold(spoken)}</p>')
        elif self.full_answer and found.has_details:
            parts.append(self._details_html(found, px))
        scroll = self.answer.verticalScrollBar().value()
        self.answer.setHtml("".join(parts))
        self.answer.verticalScrollBar().setValue(scroll)


def create_app(argv: list[str] | None = None) -> QApplication:
    app = QApplication.instance() or QApplication(argv or [])
    app.setApplicationName("careertwin-interview")
    app.setDesktopFileName("careertwin-interview")
    return app
