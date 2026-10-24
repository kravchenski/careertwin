"""The settings window: your documents, vacancy, language, audio, model, screenshots, window."""

from __future__ import annotations

import base64
import html
import itertools
import threading
from collections.abc import Callable
from pathlib import Path
from typing import Any

from PySide6.QtCore import QSignalBlocker, Qt, QTimer, Signal
from PySide6.QtGui import QImage
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMessageBox,
    QPlainTextEdit,
    QPushButton,
    QScrollArea,
    QSlider,
    QSpinBox,
    QTabWidget,
    QTextBrowser,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

from careertwin.interview import languages
from careertwin.interview.runtime import Runtime
from careertwin.interview.settings import ANSWER_STYLES, DEVICES, WHISPER_MODELS

STRINGS = {
    "en": {
        "title": "Settings", "docs": "Documents", "vacancy": "Vacancy", "about": "About me",
        "language": "Language", "audio": "Audio & speech", "model": "AI model",
        "screenshot": "Screenshot", "window": "Window",
        "docs_intro": "Files the coach reads before every answer: your CV, a LinkedIn export, a cover letter, a portfolio. PDF, DOCX and text work. You can also drop files on the interview window.",
        "add_files": "Add files…", "kind_cv": "CV (the employer has it)", "kind_about": "About me (extra facts)",
        "no_docs": "No documents yet.", "remove": "Remove", "chars": "{n} characters",
        "vacancy_intro": "The vacancy you are interviewing for. Pick one from the pipeline (its prepared CV is attached too) or paste the text or a link.",
        "pipeline": "From the pipeline", "none": "— none —", "use_selected": "Use this vacancy",
        "paste_vacancy": "Paste the vacancy text or a link…", "use_text": "Use text or link",
        "clear": "Clear", "current": "Current: {label}", "no_vacancy": "No vacancy selected.",
        "fetching": "Fetching the page…", "notes": "Notes for this interview",
        "notes_hint": "Anything the coach should know: salary expectations for this role, topics to avoid…",
        "about_intro": "Your ABOUT.md: the facts every answer is built from. Edit it here; it is saved to the file.",
        "save": "Save", "saved": "Saved", "about_stats": "{n} characters · {path}",
        "interview_language": "Interview language", "ui_language": "Window language",
        "auto": "Auto-detect", "follow": "Follow the interview", "answer_style": "Answer style",
        "answer_layout": "Answer layout", "layout_main": "Main answer only (read it aloud)", "layout_lines": "Lines to say, then the detailed answer", "style_concise": "Concise", "style_standard": "Standard", "style_detailed": "Detailed",
        "full_answer": "Show the detailed answer (main answer, key points, practical context)",
        "language_note": "Answers are written in the interview language. Speech recognition uses it too.",
        "listen": "Listen to the call", "call_source": "Call audio", "default_output": "Automatic: follows the output that plays the call", "listening_to": "Listening to: {name}",
        "refresh": "Refresh", "use_mic": "Also transcribe my microphone (use headphones)",
        "mic_source": "Microphone", "default_mic": "Default microphone",
        "whisper_model": "Speech model", "whisper_device": "Device",
        "speech_note": "Bigger models are more accurate and slower. A new model downloads on first use.",
        "gateway_url": "Gateway URL", "gateway_key": "API key", "model_id": "Model",
        "load_models": "Load models", "test": "Test connection", "models_loaded": "{n} models available",
        "connected": "Connected: {n} models", "model_note": "Leave the URL, key and model empty to use .env (SWITCHYARD_*) and auto routing.",
        "shot_enabled": "Enable the Screenshot button", "shot_tool": "Capture tool",
        "shot_delay": "Wait before capture", "shot_test": "Test capture",
        "shot_tools": "Installed: {tools}", "shot_none": "No capture tool found. Install grim (Wayland, e.g. niri or sway) or spectacle; maim or scrot on X11.",
        "shot_ok": "Captured {w}×{h}", "shot_note": "The window turns transparent while the screen is captured. The image goes to the vision model through the gateway.",
        "pinned": "Keep the window on top", "frameless": "Frameless window",
        "opacity": "Opacity", "font_scale": "Text size", "save_transcripts": "Save transcripts to data/interviews",
        "reset": "Reset all settings", "reset_ask": "Restore every setting to its default?",
        "bad_value": "Not saved: {error}", "close": "Close", "pick_files": "Choose documents",
        "prepare": "Prepare",
        "prepare_intro": "Get ready before the call. The question bank lists what this interviewer is likely to ask, with first-person lines to say. The CV review compares your CV with the vacancy.",
        "bank_count": "Questions", "bank_focus": "Focus", "bank_focus_hint": "optional: Python, system design, behavioral…",
        "bank_build": "Build question bank", "cv_review": "Review my CV for this vacancy",
        "working": "Working…", "prepare_empty": "Nothing yet. Build the bank or review your CV.",
        "bank_title": "Question bank", "review_title": "CV review", "review_summary": "Summary",
        "review_strengths": "Lead with", "review_gaps": "Gaps and how to address them",
        "review_questions": "Questions the gaps invite", "review_fixes": "Fixes to the CV",
        "need_context": "Add your CV or write about yourself first.",
        "interview": "Interview", "new_interview": "New", "rename_interview": "Rename",
        "delete_interview": "Delete", "interview_name": "Interview name",
        "interview_hint": "Each interview keeps its own vacancy, CV and notes. ABOUT.md is shared.",
        "delete_ask": "Delete this interview with its vacancy, notes and documents?",
    },
    "ru": {
        "title": "Настройки", "docs": "Документы", "vacancy": "Вакансия", "about": "Обо мне",
        "language": "Язык", "audio": "Звук и речь", "model": "Модель ИИ",
        "screenshot": "Скриншот", "window": "Окно",
        "docs_intro": "Файлы, которые ассистент читает перед каждым ответом: резюме, экспорт LinkedIn, сопроводительное письмо, портфолио. Подходят PDF, DOCX и текст. Файлы можно и просто перетащить на окно интервью.",
        "add_files": "Добавить файлы…", "kind_cv": "Резюме (его видел работодатель)", "kind_about": "Обо мне (дополнительные факты)",
        "no_docs": "Документов пока нет.", "remove": "Убрать", "chars": "{n} символов",
        "vacancy_intro": "Вакансия, на которую вы идёте. Выберите из пайплайна (резюме, подготовленное для неё, подключится тоже) или вставьте текст или ссылку.",
        "pipeline": "Из пайплайна", "none": "— нет —", "use_selected": "Использовать вакансию",
        "paste_vacancy": "Вставьте текст вакансии или ссылку…", "use_text": "Использовать текст или ссылку",
        "clear": "Очистить", "current": "Сейчас: {label}", "no_vacancy": "Вакансия не выбрана.",
        "fetching": "Загружаю страницу…", "notes": "Заметки к этому интервью",
        "notes_hint": "Всё, что нужно знать ассистенту: ожидания по зарплате на эту роль, темы, которых лучше избегать…",
        "about_intro": "Ваш ABOUT.md: факты, из которых строится каждый ответ. Правьте здесь, сохраняется в файл.",
        "save": "Сохранить", "saved": "Сохранено", "about_stats": "{n} символов · {path}",
        "interview_language": "Язык интервью", "ui_language": "Язык окна",
        "auto": "Определять сам", "follow": "Как язык интервью", "answer_style": "Стиль ответа",
        "answer_layout": "Вид ответа", "layout_main": "Только основной ответ (читать вслух)", "layout_lines": "Строки для речи и подробный ответ", "style_concise": "Кратко", "style_standard": "Обычный", "style_detailed": "Подробно",
        "full_answer": "Показывать подробный ответ (основной ответ, ключевые моменты, практический контекст)",
        "language_note": "Ответы пишутся на языке интервью. Распознавание речи тоже использует его.",
        "listen": "Слушать звонок", "call_source": "Звук звонка", "default_output": "Автоматически: следует за выходом, где играет звонок", "listening_to": "Сейчас слушаю: {name}",
        "refresh": "Обновить", "use_mic": "Расшифровывать и мой микрофон (нужны наушники)",
        "mic_source": "Микрофон", "default_mic": "Микрофон по умолчанию",
        "whisper_model": "Модель речи", "whisper_device": "Устройство",
        "speech_note": "Большие модели точнее и медленнее. Новая модель скачивается при первом использовании.",
        "gateway_url": "Адрес шлюза", "gateway_key": "API-ключ", "model_id": "Модель",
        "load_models": "Загрузить модели", "test": "Проверить связь", "models_loaded": "Доступно моделей: {n}",
        "connected": "Связь есть: моделей {n}", "model_note": "Оставьте адрес, ключ и модель пустыми, чтобы брать .env (SWITCHYARD_*) и автоподбор.",
        "shot_enabled": "Включить кнопку «Скриншот»", "shot_tool": "Программа снимка",
        "shot_delay": "Пауза перед снимком", "shot_test": "Проверить снимок",
        "shot_tools": "Установлено: {tools}", "shot_none": "Нет программы для снимков. Установите grim (Wayland, например niri или sway) или spectacle; на X11 — maim или scrot.",
        "shot_ok": "Снимок {w}×{h}", "shot_note": "Пока снимается экран, окно становится прозрачным. Изображение уходит в модель через шлюз.",
        "pinned": "Держать окно поверх", "frameless": "Окно без рамки",
        "opacity": "Прозрачность", "font_scale": "Размер текста", "save_transcripts": "Сохранять расшифровки в data/interviews",
        "reset": "Сбросить все настройки", "reset_ask": "Вернуть все настройки по умолчанию?",
        "bad_value": "Не сохранено: {error}", "close": "Закрыть", "pick_files": "Выберите документы",
        "prepare": "Подготовка",
        "prepare_intro": "Подготовьтесь до звонка. Банк вопросов — что вас, скорее всего, спросят, с готовыми строками от первого лица. Разбор резюме сравнивает ваше резюме с вакансией.",
        "bank_count": "Вопросов", "bank_focus": "Акцент", "bank_focus_hint": "по желанию: Python, архитектура, поведенческие…",
        "bank_build": "Собрать банк вопросов", "cv_review": "Разобрать моё резюме под эту вакансию",
        "working": "Работаю…", "prepare_empty": "Пока пусто. Соберите банк вопросов или разберите резюме.",
        "bank_title": "Банк вопросов", "review_title": "Разбор резюме", "review_summary": "Итог",
        "review_strengths": "С чего начать", "review_gaps": "Пробелы и как о них говорить",
        "review_questions": "Вопросы, которые вызовут пробелы", "review_fixes": "Правки в резюме",
        "need_context": "Сначала добавьте резюме или напишите о себе.",
        "interview": "Интервью", "new_interview": "Новое", "rename_interview": "Переименовать",
        "delete_interview": "Удалить", "interview_name": "Название интервью",
        "interview_hint": "У каждого интервью своя вакансия, резюме и заметки. ABOUT.md общий.",
        "delete_ask": "Удалить это интервью вместе с вакансией, заметками и документами?",
    },
    "pl": {
        "title": "Ustawienia", "docs": "Dokumenty", "vacancy": "Oferta", "about": "O mnie",
        "language": "Język", "audio": "Dźwięk i mowa", "model": "Model AI",
        "screenshot": "Zrzut ekranu", "window": "Okno",
        "docs_intro": "Pliki, które asystent czyta przed każdą odpowiedzią: CV, eksport z LinkedIn, list motywacyjny, portfolio. PDF, DOCX i tekst. Pliki możesz też upuścić na okno rozmowy.",
        "add_files": "Dodaj pliki…", "kind_cv": "CV (widział je pracodawca)", "kind_about": "O mnie (dodatkowe fakty)",
        "no_docs": "Brak dokumentów.", "remove": "Usuń", "chars": "{n} znaków",
        "vacancy_intro": "Oferta, na którą idziesz. Wybierz z pipeline'u (przygotowane dla niej CV też zostanie dołączone) albo wklej tekst lub link.",
        "pipeline": "Z pipeline'u", "none": "— brak —", "use_selected": "Użyj tej oferty",
        "paste_vacancy": "Wklej tekst oferty lub link…", "use_text": "Użyj tekstu lub linku",
        "clear": "Wyczyść", "current": "Teraz: {label}", "no_vacancy": "Nie wybrano oferty.",
        "fetching": "Pobieram stronę…", "notes": "Notatki do tej rozmowy",
        "notes_hint": "Wszystko, co asystent powinien wiedzieć: oczekiwania finansowe na tę rolę, tematy do ominięcia…",
        "about_intro": "Twój ABOUT.md: fakty, z których powstaje każda odpowiedź. Edytuj tutaj, zapisuje się do pliku.",
        "save": "Zapisz", "saved": "Zapisano", "about_stats": "{n} znaków · {path}",
        "interview_language": "Język rozmowy", "ui_language": "Język okna",
        "auto": "Wykryj automatycznie", "follow": "Jak język rozmowy", "answer_style": "Styl odpowiedzi",
        "answer_layout": "Wygląd odpowiedzi", "layout_main": "Tylko odpowiedź główna (czytaj na głos)", "layout_lines": "Zdania do powiedzenia i szczegóły", "style_concise": "Zwięzły", "style_standard": "Standardowy", "style_detailed": "Szczegółowy",
        "full_answer": "Pokazuj szczegółową odpowiedź (odpowiedź główna, kluczowe punkty, kontekst praktyczny)",
        "language_note": "Odpowiedzi powstają w języku rozmowy. Rozpoznawanie mowy też go używa.",
        "listen": "Słuchaj rozmowy", "call_source": "Dźwięk rozmowy", "default_output": "Automatycznie: podąża za wyjściem, na którym gra rozmowa", "listening_to": "Teraz słucham: {name}",
        "refresh": "Odśwież", "use_mic": "Transkrybuj też mój mikrofon (użyj słuchawek)",
        "mic_source": "Mikrofon", "default_mic": "Domyślny mikrofon",
        "whisper_model": "Model mowy", "whisper_device": "Urządzenie",
        "speech_note": "Większe modele są dokładniejsze i wolniejsze. Nowy model pobiera się przy pierwszym użyciu.",
        "gateway_url": "Adres bramki", "gateway_key": "Klucz API", "model_id": "Model",
        "load_models": "Wczytaj modele", "test": "Sprawdź połączenie", "models_loaded": "Dostępne modele: {n}",
        "connected": "Połączono: modele {n}", "model_note": "Zostaw adres, klucz i model puste, aby użyć .env (SWITCHYARD_*) i automatycznego wyboru.",
        "shot_enabled": "Włącz przycisk zrzutu ekranu", "shot_tool": "Narzędzie zrzutu",
        "shot_delay": "Opóźnienie przed zrzutem", "shot_test": "Testuj zrzut",
        "shot_tools": "Zainstalowane: {tools}", "shot_none": "Brak narzędzia do zrzutów. Zainstaluj grim (Wayland, np. niri lub sway) albo spectacle; na X11 maim lub scrot.",
        "shot_ok": "Zrzut {w}×{h}", "shot_note": "Podczas zrzutu okno staje się przezroczyste. Obraz trafia do modelu przez bramkę.",
        "pinned": "Trzymaj okno na wierzchu", "frameless": "Okno bez ramki",
        "opacity": "Przezroczystość", "font_scale": "Rozmiar tekstu", "save_transcripts": "Zapisuj transkrypcje w data/interviews",
        "reset": "Przywróć wszystkie ustawienia", "reset_ask": "Przywrócić domyślne wartości wszystkich ustawień?",
        "bad_value": "Nie zapisano: {error}", "close": "Zamknij", "pick_files": "Wybierz dokumenty",
        "prepare": "Przygotowanie",
        "prepare_intro": "Przygotuj się przed rozmową. Bank pytań to to, o co prawdopodobnie zapytają, z gotowymi zdaniami w pierwszej osobie. Przegląd CV porównuje je z ofertą.",
        "bank_count": "Pytań", "bank_focus": "Nacisk", "bank_focus_hint": "opcjonalnie: Python, architektura, behawioralne…",
        "bank_build": "Zbuduj bank pytań", "cv_review": "Przejrzyj moje CV pod tę ofertę",
        "working": "Pracuję…", "prepare_empty": "Na razie pusto. Zbuduj bank pytań albo przejrzyj CV.",
        "bank_title": "Bank pytań", "review_title": "Przegląd CV", "review_summary": "Podsumowanie",
        "review_strengths": "Zacznij od", "review_gaps": "Braki i jak o nich mówić",
        "review_questions": "Pytania, które wywołają braki", "review_fixes": "Poprawki w CV",
        "need_context": "Najpierw dodaj CV albo napisz o sobie.",
        "interview": "Rozmowa", "new_interview": "Nowa", "rename_interview": "Zmień nazwę",
        "delete_interview": "Usuń", "interview_name": "Nazwa rozmowy",
        "interview_hint": "Każda rozmowa ma własną ofertę, CV i notatki. ABOUT.md jest wspólny.",
        "delete_ask": "Usunąć tę rozmowę razem z ofertą, notatkami i dokumentami?",
    },
}

STYLE = """
QDialog, QWidget#page { background: #15161f; }
QWidget { color: #d7dae8; font-size: 14px; }
QTabWidget::pane { border: 1px solid rgba(255,255,255,0.10); border-radius: 8px; top: -1px; }
QTabBar::tab { background: transparent; padding: 8px 14px; color: rgba(215,218,232,0.55); }
QTabBar::tab:selected { color: #ffffff; border-bottom: 2px solid #bcb4ff; }
QLineEdit, QPlainTextEdit, QComboBox, QSpinBox { background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.12); border-radius: 6px; padding: 6px 8px;
    selection-background-color: #6f66d9; }
QComboBox QAbstractItemView { background: #1e1f2c; selection-background-color: rgba(188,180,255,0.25); }
QPushButton { background: rgba(188,180,255,0.16); border: 1px solid rgba(188,180,255,0.45);
    border-radius: 14px; padding: 6px 16px; color: #ffffff; }
QPushButton:hover { background: rgba(188,180,255,0.28); }
QPushButton:disabled { color: rgba(215,218,232,0.35); border-color: rgba(255,255,255,0.12); }
QToolButton { background: transparent; border: none; color: rgba(215,218,232,0.6); padding: 2px 8px; }
QToolButton:hover { color: #ff9a9a; }
QLabel#hint { color: rgba(215,218,232,0.55); font-size: 13px; }
QLabel#status { color: #bcb4ff; }
QLabel#error { color: #ff9a9a; }
QFrame#row { background: rgba(255,255,255,0.05); border-radius: 8px; }
"""


class DocRow(QWidget):
    """One attached document: include toggle, name, kind, size and remove."""

    def __init__(self, dialog: SettingsDialog, item) -> None:
        super().__init__()
        row = QHBoxLayout(self)
        row.setContentsMargins(8, 4, 8, 4)
        self.include = QCheckBox(item.name)
        self.include.setChecked(item.enabled)
        self.include.setToolTip(item.path)
        size = QLabel(dialog.t("chars", n=f"{item.chars:,}"))
        size.setObjectName("hint")
        self.kind = QComboBox()
        self.kind.addItem(dialog.t("kind_cv"), "cv")
        self.kind.addItem(dialog.t("kind_about"), "about")
        self.kind.setCurrentIndex(0 if item.kind == "cv" else 1)
        remove = QToolButton()
        remove.setText("✕")
        remove.setToolTip(dialog.t("remove"))
        row.addWidget(self.include, 1)
        row.addWidget(size)
        row.addWidget(self.kind)
        row.addWidget(remove)
        self.include.toggled.connect(lambda v: dialog.change_document(item.path, enabled=v))
        self.kind.currentIndexChanged.connect(
            lambda _i: dialog.change_document(item.path, kind=self.kind.currentData())
        )
        remove.clicked.connect(lambda: dialog.remove_document(item.path))


class SettingsDialog(QDialog):
    TABS = ("docs", "vacancy", "prepare", "about", "language", "audio", "model", "screenshot", "window")
    _finished = Signal(int, object, object)

    def __init__(self, window, runtime: Runtime) -> None:
        super().__init__(window)
        self.win, self.rt = window, runtime
        self.setWindowTitle("CareerTwin · Interview")
        self.resize(820, 660)
        self.setStyleSheet(STYLE)
        self._ready = False
        self.prepared: dict[str, dict] = {}
        self._tasks: dict[int, Callable[[Any, str | None], None]] = {}
        self._ids = itertools.count(1)
        self._finished.connect(self._on_finished)

        layout = QVBoxLayout(self)
        picker = QHBoxLayout()
        self.interview_label = QLabel()
        self.interview_combo = QComboBox()
        self.interview_combo.setMinimumWidth(260)
        self.new_interview = QPushButton()
        self.rename_interview = QPushButton()
        self.delete_interview = QPushButton()
        picker.addWidget(self.interview_label)
        picker.addWidget(self.interview_combo, 1)
        for button in (self.new_interview, self.rename_interview, self.delete_interview):
            picker.addWidget(button)
        layout.addLayout(picker)
        self.interview_hint = self._hint()
        layout.addWidget(self.interview_hint)
        self.interview_combo.activated.connect(self._switch_interview)
        self.new_interview.clicked.connect(self._new_interview)
        self.rename_interview.clicked.connect(self._rename_interview)
        self.delete_interview.clicked.connect(self._delete_interview)
        self.tabs = QTabWidget()
        layout.addWidget(self.tabs, 1)
        footer = QHBoxLayout()
        self.status = QLabel("")
        self.status.setObjectName("status")
        self.close_button = QPushButton()
        self.close_button.clicked.connect(self.hide)
        footer.addWidget(self.status, 1)
        footer.addWidget(self.close_button)
        layout.addLayout(footer)

        builders = {
            "docs": self._build_docs, "vacancy": self._build_vacancy, "prepare": self._build_prepare,
            "about": self._build_about,
            "language": self._build_language, "audio": self._build_audio, "model": self._build_model,
            "screenshot": self._build_screenshot, "window": self._build_window,
        }
        for key in self.TABS:
            page = QWidget()
            page.setObjectName("page")
            builders[key](page)
            self.tabs.addTab(page, "")
        self.retranslate()
        self.refresh()
        self._ready = True

    # ---- helpers -----------------------------------------------------------

    def t(self, key: str, **values: Any) -> str:
        text = STRINGS.get(self.win.ui, STRINGS["en"]).get(key) or STRINGS["en"][key]
        return text.format(**values) if values else text

    def show_tab(self, key: str) -> None:
        self.refresh()
        self.tabs.setCurrentIndex(self.TABS.index(key) if key in self.TABS else 0)
        self.show()
        self.raise_()
        self.activateWindow()

    def say(self, text: str, error: bool = False) -> None:
        self.status.setObjectName("error" if error else "status")
        self.status.setStyleSheet("")
        self.status.style().unpolish(self.status)
        self.status.style().polish(self.status)
        self.status.setText(text)

    def _set(self, name: str, value: Any) -> None:
        try:
            self.rt.update(name, value)
        except ValueError as exc:
            self.say(self.t("bad_value", error=exc), error=True)
        else:
            self.say("")

    def run(self, work: Callable[[], Any], done: Callable[[Any, str | None], None]) -> None:
        """Run `work` off the GUI thread; `done(result, error)` runs on it."""
        task_id = next(self._ids)
        self._tasks[task_id] = done

        def target() -> None:
            try:
                self._finished.emit(task_id, work(), None)
            except Exception as exc:
                self._finished.emit(task_id, None, str(exc))

        threading.Thread(target=target, daemon=True).start()

    def _on_finished(self, task_id: int, result: Any, error: Any) -> None:
        if callback := self._tasks.pop(task_id, None):
            callback(result, error)

    def _form(self, page: QWidget) -> tuple[QVBoxLayout, QFormLayout]:
        outer = QVBoxLayout(page)
        outer.setContentsMargins(18, 16, 18, 16)
        form = QFormLayout()
        form.setLabelAlignment(Qt.AlignmentFlag.AlignLeft)
        form.setHorizontalSpacing(18)
        form.setVerticalSpacing(12)
        outer.addLayout(form)
        return outer, form

    def _hint(self, text: str = "") -> QLabel:
        label = QLabel(text)
        label.setObjectName("hint")
        label.setWordWrap(True)
        return label

    def _bind_check(self, box: QCheckBox, name: str) -> None:
        box.toggled.connect(lambda value: self._set(name, value))

    def _bind_combo(self, combo: QComboBox, name: str) -> None:
        combo.activated.connect(lambda _i: self._set(name, combo.currentData()))

    @staticmethod
    def _select(combo: QComboBox, value: Any) -> None:
        index = combo.findData(value)
        if index < 0:
            combo.addItem(str(value) or "—", value)
            index = combo.count() - 1
        combo.setCurrentIndex(index)

    # ---- interviews --------------------------------------------------------

    def refresh_interviews(self) -> None:
        with QSignalBlocker(self.interview_combo):
            self.interview_combo.clear()
            for interview_id, name in self.rt.context.interviews():
                self.interview_combo.addItem(name, interview_id)
            self._select(self.interview_combo, self.rt.context.current.id)

    def interview_changed(self, problems: list[str] | None = None) -> None:
        """The active interview changed: show its context everywhere."""
        self.refresh()
        self.win.refresh_controls()
        if problems:
            self.say(problems[0], error=True)

    def _switch_interview(self, _index: int) -> None:
        self.interview_changed(self.rt.context.switch_interview(self.interview_combo.currentData()))

    def _new_interview(self) -> None:
        name, ok = QInputDialog.getText(self, self.t("new_interview"), self.t("interview_name"))
        if ok:
            self.interview_changed(self.rt.context.new_interview(name))
            self.tabs.setCurrentIndex(self.TABS.index("vacancy"))

    def _rename_interview(self) -> None:
        name, ok = QInputDialog.getText(
            self, self.t("rename_interview"), self.t("interview_name"),
            text=self.rt.context.interview_name(),
        )
        if ok and name.strip():
            self.rt.context.rename_interview(name)
            self.refresh_interviews()
            self.win.refresh_controls()

    def _delete_interview(self) -> None:
        answer = QMessageBox.question(self, self.t("delete_interview"), self.t("delete_ask"))
        if answer == QMessageBox.StandardButton.Yes:
            self.interview_changed(self.rt.context.delete_interview())

    # ---- documents ---------------------------------------------------------

    def _build_docs(self, page: QWidget) -> None:
        layout = QVBoxLayout(page)
        layout.setContentsMargins(18, 16, 18, 16)
        self.docs_intro = self._hint()
        layout.addWidget(self.docs_intro)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        holder = QWidget()
        self.docs_layout = QVBoxLayout(holder)
        self.docs_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        scroll.setWidget(holder)
        layout.addWidget(scroll, 1)
        self.add_button = QPushButton()
        self.add_button.clicked.connect(self._choose_files)
        row = QHBoxLayout()
        row.addWidget(self.add_button)
        row.addStretch(1)
        layout.addLayout(row)

    def _choose_files(self) -> None:
        paths, _ = QFileDialog.getOpenFileNames(
            self, self.t("pick_files"), str(Path.home()), "Documents (*.pdf *.docx *.txt *.md)"
        )
        if paths:
            self.add_files(paths)

    def add_files(self, paths: list[str]) -> None:
        errors = self.rt.context.add_documents(paths)
        self.after_context_change()
        self.say(errors[0], error=True) if errors else self.say(self.t("saved"))

    def change_document(self, path: str, **changes: Any) -> None:
        self.rt.context.update_document(path, **changes)
        self.win.refresh_controls()

    def remove_document(self, path: str) -> None:
        self.rt.context.remove_document(path)
        self.after_context_change()

    def after_context_change(self) -> None:
        self.refresh_docs()
        self.win.refresh_controls()

    def refresh_docs(self) -> None:
        while self.docs_layout.count():
            widget = self.docs_layout.takeAt(0).widget()
            if widget:
                widget.setParent(None)
                widget.deleteLater()
        items = self.rt.context.items()
        if not items:
            hint = self._hint(self.t("no_docs"))
            self.docs_layout.addWidget(hint)
            hint.show()
        for item in items:
            row = DocRow(self, item)
            self.docs_layout.addWidget(row)
            row.show()

    # ---- vacancy and notes -------------------------------------------------

    def _build_vacancy(self, page: QWidget) -> None:
        layout = QVBoxLayout(page)
        layout.setContentsMargins(18, 16, 18, 16)
        self.vacancy_intro = self._hint()
        layout.addWidget(self.vacancy_intro)
        pick = QHBoxLayout()
        self.pipeline = QComboBox()
        self.pipeline.setSizeAdjustPolicy(QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon)
        self.use_selected = QPushButton()
        self.use_selected.clicked.connect(self._use_pipeline)
        pick.addWidget(self.pipeline, 1)
        pick.addWidget(self.use_selected)
        layout.addLayout(pick)
        self.vacancy_text = QPlainTextEdit()
        self.vacancy_text.setMaximumHeight(130)
        layout.addWidget(self.vacancy_text)
        buttons = QHBoxLayout()
        self.use_text = QPushButton()
        self.use_text.clicked.connect(self._use_text)
        self.clear_vacancy = QPushButton()
        self.clear_vacancy.clicked.connect(self._clear_vacancy)
        buttons.addWidget(self.use_text)
        buttons.addWidget(self.clear_vacancy)
        buttons.addStretch(1)
        layout.addLayout(buttons)
        self.vacancy_current = self._hint()
        layout.addWidget(self.vacancy_current)
        self.notes_label = QLabel()
        layout.addWidget(self.notes_label)
        self.notes = QPlainTextEdit()
        layout.addWidget(self.notes, 1)
        self._notes_timer = QTimer(self, singleShot=True, interval=600)
        self._notes_timer.timeout.connect(lambda: self.rt.context.set_notes(self.notes.toPlainText()))
        self.notes.textChanged.connect(self._notes_timer.start)

    def _use_pipeline(self) -> None:
        job_id = self.pipeline.currentData()
        try:
            self.rt.context.set_vacancy_job(job_id) if job_id else self.rt.context.clear_vacancy()
        except Exception as exc:
            self.say(str(exc), error=True)
            return
        self.refresh_interviews()
        self.after_context_change()
        self.refresh_vacancy()
        self.say(self.t("saved"))

    def _use_text(self) -> None:
        text = self.vacancy_text.toPlainText().strip()
        if text.startswith(("http://", "https://")) and "\n" not in text:
            self.say(self.t("fetching"))
            self.run(
                lambda: self.rt.context.set_vacancy_url(text),
                lambda _result, error: self._vacancy_done(error),
            )
            return
        self.rt.context.set_vacancy_text(text)
        self._vacancy_done(None)

    def _vacancy_done(self, error: str | None) -> None:
        if error:
            self.say(error, error=True)
            return
        self.refresh_interviews()
        self.after_context_change()
        self.refresh_vacancy()
        self.say(self.t("saved"))

    def _clear_vacancy(self) -> None:
        self.rt.context.clear_vacancy()
        self.vacancy_text.clear()
        self.after_context_change()
        self.refresh_vacancy()

    def refresh_vacancy(self) -> None:
        label = self.rt.context.vacancy_label()
        self.vacancy_current.setText(self.t("current", label=label) if label else self.t("no_vacancy"))

    # ---- prepare -----------------------------------------------------------

    def _build_prepare(self, page: QWidget) -> None:
        layout = QVBoxLayout(page)
        layout.setContentsMargins(18, 16, 18, 16)
        self.prepare_intro = self._hint()
        layout.addWidget(self.prepare_intro)
        row = QHBoxLayout()
        self.bank_count_label = QLabel()
        self.bank_count = QSpinBox()
        self.bank_count.setRange(3, 15)
        self.bank_count.setValue(10)
        self.bank_count.setButtonSymbols(QSpinBox.ButtonSymbols.NoButtons)
        self.bank_focus_label = QLabel()
        self.bank_focus = QLineEdit()
        self.bank_build = QPushButton()
        for widget in (self.bank_count_label, self.bank_count, self.bank_focus_label):
            row.addWidget(widget)
        row.addWidget(self.bank_focus, 1)
        row.addWidget(self.bank_build)
        layout.addLayout(row)
        row = QHBoxLayout()
        self.cv_review = QPushButton()
        row.addWidget(self.cv_review)
        row.addStretch(1)
        layout.addLayout(row)
        self.prepare_view = QTextBrowser()
        layout.addWidget(self.prepare_view, 1)
        self.bank_build.clicked.connect(self._build_bank)
        self.cv_review.clicked.connect(self._review_cv)

    def _prepare_guard(self) -> bool:
        if self.rt.context.brief.is_empty():
            self.say(self.t("need_context"), error=True)
            return False
        self.say(self.t("working"))
        return True

    def _build_bank(self) -> None:
        if not self._prepare_guard():
            return
        count, focus = self.bank_count.value(), self.bank_focus.text().strip()
        key = self.rt.context.current.id
        self.run(
            lambda: self.rt.question_bank(count, focus),
            lambda result, error: self._prepared(key, "bank", result, error),
        )

    def _review_cv(self) -> None:
        if not self._prepare_guard():
            return
        key = self.rt.context.current.id
        self.run(
            self.rt.analyze_cv, lambda result, error: self._prepared(key, "review", result, error)
        )

    def _prepared(self, key: str, part: str, result: Any, error: str | None) -> None:
        if error or result is None:
            self.say(error or "", error=True)
            return
        self.prepared.setdefault(key, {})[part] = result
        self.say(self.t("saved"))
        self.refresh_prepare()

    def refresh_prepare(self) -> None:
        results = self.prepared.get(self.rt.context.current.id, {})
        esc = lambda text: html.escape(str(text)).replace("**", "")  # noqa: E731
        parts: list[str] = []
        if review := results.get("review"):
            parts.append(f"<h2>{esc(self.t('review_title'))}</h2><p>{esc(review['summary'])}</p>")
            for key, title in (("strengths", "review_strengths"), ("likely_questions", "review_questions"), ("cv_fixes", "review_fixes")):
                if review[key]:
                    items = "".join(f"<li>{esc(item)}</li>" for item in review[key])
                    parts.append(f"<h3>{esc(self.t(title))}</h3><ul>{items}</ul>")
            if review["gaps"]:
                items = "".join(
                    f"<li><b>{esc(gap['gap'])}</b><br>{esc(gap['how_to_address'])}</li>"
                    for gap in review["gaps"]
                )
                parts.append(f"<h3>{esc(self.t('review_gaps'))}</h3><ul>{items}</ul>")
        if bank := results.get("bank"):
            parts.append(f"<h2>{esc(self.t('bank_title'))}</h2>")
            for item in bank:
                kind = f" <i>({esc(item['kind'])})</i>" if item["kind"] else ""
                lines = "".join(f"<li>{esc(line)}</li>" for line in item["outline"])
                parts.append(f"<p><b>{esc(item['question'])}</b>{kind}</p><ul>{lines}</ul>")
        if parts:
            self.prepare_view.setHtml("".join(parts))
        else:
            self.prepare_view.setHtml(f"<p style='color:gray'>{esc(self.t('prepare_empty'))}</p>")

    # ---- about me ----------------------------------------------------------

    def _build_about(self, page: QWidget) -> None:
        layout = QVBoxLayout(page)
        layout.setContentsMargins(18, 16, 18, 16)
        self.about_intro = self._hint()
        layout.addWidget(self.about_intro)
        self.about = QPlainTextEdit()
        self.about.setTabChangesFocus(True)
        layout.addWidget(self.about, 1)
        row = QHBoxLayout()
        self.about_stats = self._hint()
        self.about_save = QPushButton()
        self.about_save.clicked.connect(self._save_about)
        row.addWidget(self.about_stats, 1)
        row.addWidget(self.about_save)
        layout.addLayout(row)

    def _save_about(self) -> None:
        try:
            self.rt.context.save_about(self.about.toPlainText())
        except OSError as exc:
            self.say(str(exc), error=True)
            return
        self.win.refresh_controls()
        self.refresh_about()
        self.say(self.t("saved"))

    def refresh_about(self) -> None:
        from careertwin.profile.context import context_path

        text = self.rt.context.about_text()
        if not self.about.hasFocus() and self.about.toPlainText() != text:
            self.about.setPlainText(text)
        path = context_path(self.rt.context.brief.username)
        self.about_stats.setText(self.t("about_stats", n=f"{len(text):,}", path=path))

    # ---- language ----------------------------------------------------------

    def _build_language(self, page: QWidget) -> None:
        outer, form = self._form(page)
        self.interview_language = QComboBox()
        self.ui_language = QComboBox()
        self.answer_style = QComboBox()
        self.answer_layout = QComboBox()
        self.full_answer = QCheckBox()
        self.language_labels = [QLabel() for _ in range(4)]
        form.addRow(self.language_labels[0], self.interview_language)
        form.addRow(self.language_labels[1], self.ui_language)
        form.addRow(self.language_labels[2], self.answer_style)
        form.addRow(self.language_labels[3], self.answer_layout)
        form.addRow(self.full_answer)
        self.language_note = self._hint()
        outer.addWidget(self.language_note)
        outer.addStretch(1)
        self._bind_combo(self.interview_language, "language")
        self._bind_combo(self.ui_language, "ui_language")
        self._bind_combo(self.answer_style, "answer_style")
        self._bind_combo(self.answer_layout, "answer_layout")
        self._bind_check(self.full_answer, "show_full_answer")

    # ---- audio and speech --------------------------------------------------

    def _build_audio(self, page: QWidget) -> None:
        outer, form = self._form(page)
        self.listen = QCheckBox()
        self.call_source = QComboBox()
        self.refresh_sources = QPushButton()
        self.use_mic = QCheckBox()
        self.mic_source = QComboBox()
        self.whisper_model = QComboBox()
        self.whisper_device = QComboBox()
        self.audio_labels = [QLabel() for _ in range(4)]
        source_row = QHBoxLayout()
        source_row.addWidget(self.call_source, 1)
        source_row.addWidget(self.refresh_sources)
        form.addRow(self.listen)
        form.addRow(self.audio_labels[0], source_row)
        self.audio_now = self._hint()
        form.addRow("", self.audio_now)
        form.addRow(self.use_mic)
        form.addRow(self.audio_labels[1], self.mic_source)
        form.addRow(self.audio_labels[2], self.whisper_model)
        form.addRow(self.audio_labels[3], self.whisper_device)
        self.speech_note = self._hint()
        outer.addWidget(self.speech_note)
        outer.addStretch(1)
        for name in WHISPER_MODELS:
            self.whisper_model.addItem(name, name)
        for name in DEVICES:
            self.whisper_device.addItem(name, name)
        self._bind_check(self.listen, "listen")
        self._bind_check(self.use_mic, "use_mic")
        self._bind_combo(self.call_source, "call_source")
        self._bind_combo(self.mic_source, "mic_source")
        self._bind_combo(self.whisper_model, "whisper_model")
        self._bind_combo(self.whisper_device, "whisper_device")
        self.refresh_sources.clicked.connect(self.refresh_audio)

    def refresh_audio(self) -> None:
        self.audio_now.setText(
            self.t("listening_to", name=self.rt.current_source) if self.rt.current_source else ""
        )
        sources = self.rt.audio_sources()
        monitors = sorted(name for name in sources if name.endswith(".monitor"))
        inputs = sorted(name for name in sources if not name.endswith(".monitor"))
        for combo, default, names, current in (
            (self.call_source, self.t("default_output"), monitors, self.rt.settings.call_source),
            (self.mic_source, self.t("default_mic"), inputs, self.rt.settings.mic_source),
        ):
            with QSignalBlocker(combo):
                combo.clear()
                combo.addItem(default, "")
                for name in names:
                    combo.addItem(name, name)
                self._select(combo, current)

    # ---- model -------------------------------------------------------------

    def _build_model(self, page: QWidget) -> None:
        outer, form = self._form(page)
        self.gateway_url = QLineEdit()
        self.gateway_key = QLineEdit()
        self.gateway_key.setEchoMode(QLineEdit.EchoMode.Password)
        self.model_id = QComboBox()
        self.model_id.setEditable(True)
        self.load_models = QPushButton()
        self.test_gateway = QPushButton()
        self.model_labels = [QLabel() for _ in range(3)]
        model_row = QHBoxLayout()
        model_row.addWidget(self.model_id, 1)
        model_row.addWidget(self.load_models)
        form.addRow(self.model_labels[0], self.gateway_url)
        form.addRow(self.model_labels[1], self.gateway_key)
        form.addRow(self.model_labels[2], model_row)
        row = QHBoxLayout()
        row.addWidget(self.test_gateway)
        row.addStretch(1)
        outer.addLayout(row)
        self.model_note = self._hint()
        outer.addWidget(self.model_note)
        outer.addStretch(1)
        self.gateway_url.editingFinished.connect(lambda: self._set("gateway_url", self.gateway_url.text()))
        self.gateway_key.editingFinished.connect(lambda: self._set("gateway_key", self.gateway_key.text()))
        self.model_id.lineEdit().editingFinished.connect(self._set_model)
        self.model_id.activated.connect(lambda _i: self._set_model())
        self.load_models.clicked.connect(self._load_models)
        self.test_gateway.clicked.connect(self._test_gateway)

    def _set_model(self) -> None:
        text = self.model_id.currentText().strip()
        self._set("model", "" if text in {"", "auto"} else text)

    def _load_models(self) -> None:
        self.run(self.rt.models, self._models_loaded)

    def _models_loaded(self, models: list[str] | None, error: str | None) -> None:
        if error or models is None:
            self.say(error or "", error=True)
            return
        current = self.rt.settings.model
        with QSignalBlocker(self.model_id):
            self.model_id.clear()
            self.model_id.addItems(models if "auto" in models else ["auto", *models])
            self.model_id.setCurrentText(current or "auto")
        self.say(self.t("models_loaded", n=len(models)))

    def _test_gateway(self) -> None:
        self.run(
            self.rt.models,
            lambda models, error: self.say(
                error, error=True
            ) if error else self.say(self.t("connected", n=len(models or []))),
        )

    # ---- screenshot --------------------------------------------------------

    def _build_screenshot(self, page: QWidget) -> None:
        outer, form = self._form(page)
        self.shot_enabled = QCheckBox()
        self.shot_tool = QComboBox()
        self.shot_delay = QSpinBox()
        self.shot_delay.setRange(0, 5000)
        self.shot_delay.setSingleStep(50)
        self.shot_delay.setButtonSymbols(QSpinBox.ButtonSymbols.NoButtons)
        self.shot_delay.setSuffix(" ms")
        self.shot_test = QPushButton()
        self.shot_labels = [QLabel() for _ in range(2)]
        form.addRow(self.shot_enabled)
        form.addRow(self.shot_labels[0], self.shot_tool)
        form.addRow(self.shot_labels[1], self.shot_delay)
        row = QHBoxLayout()
        row.addWidget(self.shot_test)
        row.addStretch(1)
        outer.addLayout(row)
        self.shot_tools = self._hint()
        self.shot_note = self._hint()
        outer.addWidget(self.shot_tools)
        outer.addWidget(self.shot_note)
        outer.addStretch(1)
        self._bind_check(self.shot_enabled, "screenshot_enabled")
        self._bind_combo(self.shot_tool, "screenshot_tool")
        self.shot_delay.valueChanged.connect(lambda value: self._set("screenshot_delay_ms", value))
        self.shot_test.clicked.connect(self._test_capture)
        self.shot_enabled.toggled.connect(lambda _v: self.win.refresh_controls())

    def refresh_screenshot(self) -> None:
        tools = self.rt.screenshot_tools()
        with QSignalBlocker(self.shot_tool):
            self.shot_tool.clear()
            self.shot_tool.addItem(self.t("auto"), "auto")
            for name in tools:
                self.shot_tool.addItem(name, name)
            self._select(self.shot_tool, self.rt.settings.screenshot_tool)
        self.shot_tools.setText(
            self.t("shot_tools", tools=", ".join(tools)) if tools else self.t("shot_none")
        )
        self.shot_test.setEnabled(bool(tools))

    def _test_capture(self) -> None:
        self.say("…")
        self.win.setWindowOpacity(0.0)
        QTimer.singleShot(
            self.rt.settings.screenshot_delay_ms,
            lambda: self.run(self.rt.capture, self._capture_done),
        )

    def _capture_done(self, data: str | None, error: str | None) -> None:
        self.win.setWindowOpacity(self.rt.settings.opacity / 100)
        if error or data is None:
            self.say(error or "", error=True)
            return
        image = QImage.fromData(base64.b64decode(data))
        self.say(self.t("shot_ok", w=image.width(), h=image.height()))

    # ---- window ------------------------------------------------------------

    def _build_window(self, page: QWidget) -> None:
        outer, form = self._form(page)
        self.pinned = QCheckBox()
        self.frameless = QCheckBox()
        self.save_transcripts = QCheckBox()
        self.opacity = QSlider(Qt.Orientation.Horizontal)
        self.opacity.setRange(35, 100)
        self.font_scale = QSlider(Qt.Orientation.Horizontal)
        self.font_scale.setRange(70, 180)
        self.window_labels = [QLabel() for _ in range(2)]
        self.reset_button = QPushButton()
        form.addRow(self.pinned)
        form.addRow(self.frameless)
        form.addRow(self.window_labels[0], self.opacity)
        form.addRow(self.window_labels[1], self.font_scale)
        form.addRow(self.save_transcripts)
        row = QHBoxLayout()
        row.addWidget(self.reset_button)
        row.addStretch(1)
        outer.addLayout(row)
        outer.addStretch(1)
        self._bind_check(self.pinned, "pinned")
        self._bind_check(self.frameless, "frameless")
        self._bind_check(self.save_transcripts, "save_transcripts")
        self.opacity.valueChanged.connect(lambda value: self._set("opacity", value))
        self.font_scale.valueChanged.connect(lambda value: self._set("font_scale", value))
        self.reset_button.clicked.connect(self._reset)

    def _reset(self) -> None:
        if QMessageBox.question(self, self.t("reset"), self.t("reset_ask")) == QMessageBox.StandardButton.Yes:
            self.rt.reset()
            self.refresh()

    # ---- refresh and translate ---------------------------------------------

    def refresh(self) -> None:
        """Show the current settings and context in every control."""
        settings = self.rt.settings
        self.refresh_interviews()
        self.refresh_docs()
        self.refresh_vacancy()
        self.refresh_prepare()
        self.refresh_about()
        self.refresh_audio()
        self.refresh_screenshot()
        with QSignalBlocker(self.notes):
            if not self.notes.hasFocus():
                self.notes.setPlainText(self.rt.context.brief.notes)
        pipeline = self.rt.context.pipeline()
        with QSignalBlocker(self.pipeline):
            self.pipeline.clear()
            self.pipeline.addItem(self.t("none"), "")
            for job_id, label in pipeline:
                self.pipeline.addItem(label, job_id)
        for box, name in (
            (self.full_answer, "show_full_answer"), (self.listen, "listen"), (self.use_mic, "use_mic"),
            (self.shot_enabled, "screenshot_enabled"), (self.pinned, "pinned"),
            (self.frameless, "frameless"), (self.save_transcripts, "save_transcripts"),
        ):
            with QSignalBlocker(box):
                box.setChecked(getattr(settings, name))
        for widget, value in (
            (self.opacity, settings.opacity), (self.font_scale, settings.font_scale),
            (self.shot_delay, settings.screenshot_delay_ms),
        ):
            with QSignalBlocker(widget):
                widget.setValue(value)
        for combo, value in (
            (self.interview_language, settings.language), (self.ui_language, settings.ui_language),
            (self.answer_style, settings.answer_style), (self.answer_layout, settings.answer_layout),
            (self.whisper_model, settings.whisper_model),
            (self.whisper_device, settings.whisper_device),
        ):
            with QSignalBlocker(combo):
                self._select(combo, value)
        for field, value in ((self.gateway_url, settings.gateway_url), (self.gateway_key, settings.gateway_key)):
            with QSignalBlocker(field):
                field.setText(value)
        with QSignalBlocker(self.model_id):
            self.model_id.setCurrentText(settings.model or "auto")

    def retranslate(self) -> None:
        self.setWindowTitle(f"{self.t('title')} · CareerTwin")
        for index, key in enumerate(self.TABS):
            self.tabs.setTabText(index, self.t(key).replace("&", "&&"))
        self.close_button.setText(self.t("close"))
        self.interview_label.setText(self.t("interview"))
        self.new_interview.setText(self.t("new_interview"))
        self.rename_interview.setText(self.t("rename_interview"))
        self.delete_interview.setText(self.t("delete_interview"))
        self.interview_hint.setText(self.t("interview_hint"))
        self.docs_intro.setText(self.t("docs_intro"))
        self.add_button.setText(self.t("add_files"))
        self.vacancy_intro.setText(self.t("vacancy_intro"))
        self.use_selected.setText(self.t("use_selected"))
        self.vacancy_text.setPlaceholderText(self.t("paste_vacancy"))
        self.use_text.setText(self.t("use_text"))
        self.clear_vacancy.setText(self.t("clear"))
        self.prepare_intro.setText(self.t("prepare_intro"))
        self.bank_count_label.setText(self.t("bank_count"))
        self.bank_focus_label.setText(self.t("bank_focus"))
        self.bank_focus.setPlaceholderText(self.t("bank_focus_hint"))
        self.bank_build.setText(self.t("bank_build"))
        self.cv_review.setText(self.t("cv_review"))
        self.notes_label.setText(self.t("notes"))
        self.notes.setPlaceholderText(self.t("notes_hint"))
        self.about_intro.setText(self.t("about_intro"))
        self.about_save.setText(self.t("save"))
        for label, key in zip(
            self.language_labels, ("interview_language", "ui_language", "answer_style", "answer_layout")
        ):
            label.setText(self.t(key))
        self.full_answer.setText(self.t("full_answer"))
        self.language_note.setText(self.t("language_note"))
        self._fill_language_combos()
        self.listen.setText(self.t("listen"))
        self.use_mic.setText(self.t("use_mic"))
        self.refresh_sources.setText(self.t("refresh"))
        for label, key in zip(self.audio_labels, ("call_source", "mic_source", "whisper_model", "whisper_device")):
            label.setText(self.t(key))
        self.speech_note.setText(self.t("speech_note"))
        for label, key in zip(self.model_labels, ("gateway_url", "gateway_key", "model_id")):
            label.setText(self.t(key))
        self.load_models.setText(self.t("load_models"))
        self.test_gateway.setText(self.t("test"))
        self.model_note.setText(self.t("model_note"))
        self.gateway_url.setPlaceholderText(self.rt.gateway())
        self.shot_enabled.setText(self.t("shot_enabled"))
        for label, key in zip(self.shot_labels, ("shot_tool", "shot_delay")):
            label.setText(self.t(key))
        self.shot_test.setText(self.t("shot_test"))
        self.shot_note.setText(self.t("shot_note"))
        self.pinned.setText(self.t("pinned"))
        self.frameless.setText(self.t("frameless"))
        self.save_transcripts.setText(self.t("save_transcripts"))
        for label, key in zip(self.window_labels, ("opacity", "font_scale")):
            label.setText(self.t(key))
        self.reset_button.setText(self.t("reset"))
        if self._ready:
            self.refresh()

    def _fill_language_combos(self) -> None:
        settings = self.rt.settings
        with QSignalBlocker(self.interview_language):
            self.interview_language.clear()
            self.interview_language.addItem(self.t("auto"), "auto")
            for code in languages.NAMES:
                if code in languages.NATIVE:
                    self.interview_language.addItem(f"{languages.NATIVE[code]} ({code})", code)
            self._select(self.interview_language, settings.language)
        with QSignalBlocker(self.ui_language):
            self.ui_language.clear()
            self.ui_language.addItem(self.t("follow"), "auto")
            for code in languages.UI_LANGUAGES:
                self.ui_language.addItem(languages.NATIVE[code], code)
            self._select(self.ui_language, settings.ui_language)
        with QSignalBlocker(self.answer_layout):
            self.answer_layout.clear()
            for layout in ("main", "lines"):
                self.answer_layout.addItem(self.t(f"layout_{layout}"), layout)
            self._select(self.answer_layout, settings.answer_layout)
        with QSignalBlocker(self.answer_style):
            self.answer_style.clear()
            for style in ANSWER_STYLES:
                self.answer_style.addItem(self.t(f"style_{style}"), style)
            self._select(self.answer_style, settings.answer_style)
