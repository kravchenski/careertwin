"""The settings window: the interview, you, practice, audio, the AI and the window."""

from __future__ import annotations

import html
import itertools
import os
import threading
from collections.abc import Callable
from pathlib import Path
from typing import Any

from PySide6.QtCore import QSignalBlocker, Qt, QTimer, Signal
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

from careertwin.interview import i18n, languages
from careertwin.interview import project as project_store
from careertwin.interview.providers import OTHER, PROVIDERS, provider_for
from careertwin.interview.runtime import Runtime
from careertwin.interview.settings import ANSWER_STYLES, WHISPER_MODELS
from careertwin.interview.stt_deepgram import check_key

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
QLabel#section { color: #ffffff; font-size: 15px; font-weight: 600; padding-top: 6px; }
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
    TABS = ("interview", "about", "practice", "project", "audio", "ai", "general")
    LEGACY_TABS = {
        "docs": "interview", "vacancy": "interview", "prepare": "practice", "language": "general",
        "model": "ai", "screenshot": "general", "window": "general",
    }
    _finished = Signal(int, object, object)
    _project_progress = Signal(str, int)

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
        self._project_progress.connect(self._show_project_progress)

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
            "interview": self._build_interview, "about": self._build_about,
            "practice": self._build_prepare, "project": self._build_project, "audio": self._build_audio,
            "ai": self._build_model, "general": self._build_general,
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
        return i18n.text(self.win.ui, "dialog", key, **values)

    def show_tab(self, key: str) -> None:
        key = self.LEGACY_TABS.get(key, key)
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

    def _section(self) -> QLabel:
        label = QLabel()
        label.setObjectName("section")
        return label

    def _scrolled(self, page: QWidget) -> QWidget:
        """A page that scrolls instead of squeezing when the window is small."""
        outer = QVBoxLayout(page)
        outer.setContentsMargins(0, 0, 0, 0)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        body = QWidget()
        body.setObjectName("page")
        scroll.setWidget(body)
        outer.addWidget(scroll)
        return body

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
            self.tabs.setCurrentIndex(self.TABS.index("interview"))

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

    def _build_interview(self, page: QWidget) -> None:
        column = QVBoxLayout(self._scrolled(page))
        column.setContentsMargins(18, 10, 18, 16)
        self.docs_title, self.vacancy_title = self._section(), self._section()
        for title, build in ((self.docs_title, self._build_docs), (self.vacancy_title, self._build_vacancy)):
            section = QWidget()
            build(section)
            section.layout().setContentsMargins(0, 0, 0, 8)
            column.addWidget(title)
            column.addWidget(section)

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
        scroll.setMinimumHeight(120)
        scroll.setMaximumHeight(210)
        layout.addWidget(scroll)
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
        self.notes.setMinimumHeight(110)
        layout.addWidget(self.notes)
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

    # ---- the project to defend -----------------------------------------------

    def _build_project(self, page: QWidget) -> None:
        layout = QVBoxLayout(page)
        layout.setContentsMargins(18, 16, 18, 16)
        self.project_intro = self._hint()
        layout.addWidget(self.project_intro)
        row = QHBoxLayout()
        self.project_choose = QPushButton()
        self.project_again = QPushButton()
        self.project_remove = QPushButton()
        self.project_path = QLabel()
        self.project_path.setObjectName("hint")
        self.project_path.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        for button in (self.project_choose, self.project_again, self.project_remove):
            row.addWidget(button)
        row.addWidget(self.project_path, 1)
        layout.addLayout(row)
        self.project_stats = self._hint()
        layout.addWidget(self.project_stats)
        self.project_dossier_label = QLabel()
        layout.addWidget(self.project_dossier_label)
        self.project_text = QPlainTextEdit()
        self.project_text.setTabChangesFocus(True)
        layout.addWidget(self.project_text, 1)
        row = QHBoxLayout()
        self.project_save = QPushButton()
        row.addWidget(self.project_save)
        row.addStretch(1)
        layout.addLayout(row)
        self.project_note = self._hint()
        layout.addWidget(self.project_note)
        self.project_choose.clicked.connect(self._choose_project)
        self.project_again.clicked.connect(lambda: self._analyze_project(self.rt.context.project_path()))
        self.project_remove.clicked.connect(self._remove_project)
        self.project_save.clicked.connect(self._save_project_text)

    def _choose_project(self) -> None:
        start = self.rt.context.project_path() or str(Path.home())
        folder = QFileDialog.getExistingDirectory(self, self.t("project_choose"), start)
        if folder:
            self._analyze_project(folder)

    def _analyze_project(self, folder: str) -> None:
        if not folder:
            return
        provider = provider_for(self.rt.gateway())
        if provider is None or provider.cloud:
            name = provider.label if provider else self.rt.gateway()
            ask = self.t("project_confirm", folder=Path(folder).name, name=name)
            if QMessageBox.question(self, self.t("tab_project"), ask) != QMessageBox.StandardButton.Yes:
                return
        self.rt.context.set_project_path(folder)
        self.refresh_project()
        self._project_progress.emit("read", 0)
        self.run(
            lambda: self.rt.analyze_project(folder, self._project_progress.emit),
            self._project_analyzed,
        )

    def _show_project_progress(self, stage: str, files: int) -> None:
        self.say(self.t("project_reading") if stage == "read" else self.t("project_asking", n=files))

    def _project_analyzed(self, project: Any, error: str | None) -> None:
        if error or project is None:
            self.say(error or "", error=True)
            return
        self.rt.context.attach_project(project)
        self.win.refresh_controls()
        self.refresh_project()
        self.say(self.t("project_done", n=project.files))

    def _save_project_text(self) -> None:
        if project := self.rt.context.project():
            project_store.save_dossier(project, self.project_text.toPlainText())
            self.say(self.t("saved"))

    def _remove_project(self) -> None:
        self.rt.context.remove_project()
        self.win.refresh_controls()
        self.refresh_project()

    def refresh_project(self) -> None:
        project, path = self.rt.context.project(), self.rt.context.project_path()
        self.project_path.setText(path)
        for button in (self.project_again, self.project_remove):
            button.setEnabled(bool(path))
        self.project_save.setEnabled(project is not None)
        self.project_text.setEnabled(project is not None)
        if project is None:
            stats = self.t("project_none")
        else:
            characters = project.characters
            size = (
                f"{characters / 1_000_000:.1f}M" if characters >= 1_000_000
                else f"{characters / 1000:.0f}k" if characters >= 10_000
                else f"{characters / 1000:.1f}k"
            )
            stats = self.t("project_stats", files=project.files, size=size, when=project.analyzed_at)
            if path and not Path(path).is_dir():
                stats += "  ·  " + self.t("project_missing")
        self.project_stats.setText(stats)
        with QSignalBlocker(self.project_text):
            if not self.project_text.hasFocus():
                self.project_text.setPlainText(project.dossier if project else "")

    # ---- audio and speech --------------------------------------------------

    def _build_audio(self, page: QWidget) -> None:
        outer, form = self._form(page)
        self.listen = QCheckBox()
        self.call_source = QComboBox()
        self.refresh_sources = QPushButton()
        self.use_mic = QCheckBox()
        self.mic_source = QComboBox()
        self.stt_engine = QComboBox()
        self.stt_engine.addItem("", "whisper")
        self.stt_engine.addItem("", "deepgram")
        self.whisper_model = QComboBox()
        self.deepgram_key = QLineEdit()
        self.deepgram_key.setEchoMode(QLineEdit.EchoMode.Password)
        self.deepgram_test = QPushButton()
        self.engine_label, self.dg_key_label = QLabel(), QLabel()
        self.audio_labels = [QLabel() for _ in range(3)]
        source_row = QHBoxLayout()
        source_row.addWidget(self.call_source, 1)
        source_row.addWidget(self.refresh_sources)
        form.addRow(self.listen)
        form.addRow(self.audio_labels[0], source_row)
        self.audio_now = self._hint()
        form.addRow("", self.audio_now)
        form.addRow(self.use_mic)
        form.addRow(self.audio_labels[1], self.mic_source)
        form.addRow(self.engine_label, self.stt_engine)
        form.addRow(self.audio_labels[2], self.whisper_model)
        key_row = QHBoxLayout()
        key_row.addWidget(self.deepgram_key, 1)
        key_row.addWidget(self.deepgram_test)
        form.addRow(self.dg_key_label, key_row)
        self.audio_form, self.deepgram_row = form, key_row
        self.speech_note = self._hint()
        outer.addWidget(self.speech_note)
        outer.addStretch(1)
        for name in WHISPER_MODELS:
            self.whisper_model.addItem(name, name)
        self._bind_check(self.listen, "listen")
        self._bind_check(self.use_mic, "use_mic")
        self._bind_combo(self.call_source, "call_source")
        self._bind_combo(self.mic_source, "mic_source")
        self._bind_combo(self.whisper_model, "whisper_model")
        self.stt_engine.activated.connect(lambda _i: self._set_engine())
        self.deepgram_key.editingFinished.connect(lambda: self._set("deepgram_key", self.deepgram_key.text()))
        self.deepgram_test.clicked.connect(self._test_deepgram)
        self.refresh_sources.clicked.connect(self.refresh_audio)

    def _set_engine(self) -> None:
        self._set("stt_engine", self.stt_engine.currentData())
        self._sync_engine()

    def _sync_engine(self) -> None:
        """Only the controls of the chosen engine are active, and the note describes its trade-off."""
        deepgram = self.rt.settings.stt_engine == "deepgram"
        for widget in (self.whisper_model, self.audio_labels[2]):
            widget.setEnabled(not deepgram)
        for widget in (self.deepgram_key, self.deepgram_test, self.dg_key_label):
            widget.setEnabled(deepgram)
        self.audio_form.setRowVisible(self.whisper_model, not deepgram)
        self.audio_form.setRowVisible(self.deepgram_row, deepgram)
        self.speech_note.setText(self.t("deepgram_note" if deepgram else "speech_note"))

    def _test_deepgram(self) -> None:
        key = self.deepgram_key.text().strip() or self.rt.settings.deepgram_key or os.getenv("DEEPGRAM_API_KEY", "")
        self.say("…")
        self.run(
            lambda: check_key(key),
            lambda _result, error: self.say(error, error=True) if error else self.say(self.t("deepgram_ok")),
        )

    def refresh_audio(self) -> None:
        self.audio_now.setText(
            self.t("listening_to", name=self.rt.current_source) if self.rt.current_source else ""
        )
        self.audio_form.setRowVisible(self.audio_now, bool(self.rt.current_source))
        monitors, inputs = self.rt.audio_choices()
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
        self.provider = QComboBox()
        for provider in PROVIDERS:
            self.provider.addItem(provider.label, provider.id)
        self.provider.addItem("", OTHER)
        self.provider_label = QLabel()
        self.model_id = QComboBox()
        self.model_id.setEditable(True)
        self.vision_id = QComboBox()
        self.vision_id.setEditable(True)
        self.vision_hint = self._hint()
        self.load_models = QPushButton()
        self.test_gateway = QPushButton()
        self.model_labels = [QLabel() for _ in range(4)]
        model_row = QHBoxLayout()
        model_row.addWidget(self.model_id, 1)
        model_row.addWidget(self.load_models)
        form.addRow(self.provider_label, self.provider)
        form.addRow(self.model_labels[0], self.gateway_url)
        form.addRow(self.model_labels[1], self.gateway_key)
        form.addRow(self.model_labels[2], model_row)
        form.addRow(self.model_labels[3], self.vision_id)
        form.addRow("", self.vision_hint)
        self.model_form = form
        row = QHBoxLayout()
        row.addWidget(self.test_gateway)
        row.addStretch(1)
        outer.addLayout(row)
        self.model_note = self._hint()
        outer.addWidget(self.model_note)
        outer.addStretch(1)
        self.gateway_url.editingFinished.connect(self._url_edited)
        self.provider.activated.connect(lambda _i: self._choose_provider())
        self.gateway_key.editingFinished.connect(lambda: self._set("gateway_key", self.gateway_key.text()))
        self.model_id.lineEdit().editingFinished.connect(self._set_model)
        self.model_id.activated.connect(lambda _i: self._set_model())
        self.vision_id.lineEdit().editingFinished.connect(self._set_vision_model)
        self.vision_id.activated.connect(lambda _i: self._set_vision_model())
        self.load_models.clicked.connect(self._load_models)
        self.test_gateway.clicked.connect(self._test_gateway)

    def _url_edited(self) -> None:
        self._set("gateway_url", self.gateway_url.text())
        self._sync_provider()

    def _choose_provider(self) -> None:
        provider = next((p for p in PROVIDERS if p.id == self.provider.currentData()), None)
        if provider is None:  # "other": type the address
            self.gateway_url.setFocus()
            self._show_provider_note()
            return
        if provider_for(self.rt.gateway()) != provider:
            # A key and a model id belong to one provider: never send them to another.
            with QSignalBlocker(self.gateway_key):
                self.gateway_key.setText("")
            self._set("gateway_key", "")
            self._set("model", "")
            self._set("vision_model", "")
            with QSignalBlocker(self.model_id):
                self.model_id.setCurrentText("auto")
            with QSignalBlocker(self.vision_id):
                self.vision_id.setCurrentText("")
        with QSignalBlocker(self.gateway_url):
            self.gateway_url.setText(provider.url)
        self._set("gateway_url", provider.url)
        self._sync_provider()

    def _sync_provider(self) -> None:
        """Show the preset the saved address belongs to, or "other"."""
        provider = provider_for(self.rt.gateway())
        with QSignalBlocker(self.provider):
            self._select(self.provider, provider.id if provider else OTHER)
        self.model_form.setRowVisible(self.gateway_url, provider is None)
        self.model_form.setRowVisible(self.gateway_key, provider is None or provider.cloud)
        self._show_provider_note()

    def _show_project_note(self) -> None:
        provider = provider_for(self.rt.gateway())
        if provider is not None and not provider.cloud:
            self.project_note.setText(self.t("project_local"))
        else:
            self.project_note.setText(self.t("project_cloud", name=provider.label if provider else self.rt.gateway()))

    def _show_provider_note(self) -> None:
        self._show_project_note()
        provider = provider_for(self.rt.gateway())
        note = self.t("cloud_note", name=provider.label) if provider and provider.cloud else self.t("model_note")
        self.model_note.setText(note)

    def _set_model(self) -> None:
        text = self.model_id.currentText().strip()
        self._set("model", "" if text in {"", "auto"} else text)

    def _set_vision_model(self) -> None:
        text = self.vision_id.currentText().strip()
        self._set("vision_model", "" if text in {"", "auto"} else text)

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
        with QSignalBlocker(self.vision_id):
            self.vision_id.clear()
            self.vision_id.addItems(["", *models])
            self.vision_id.setCurrentText(self.rt.settings.vision_model)
        self.say(self.t("models_loaded", n=len(models)))

    def _test_gateway(self) -> None:
        self.run(
            self.rt.models,
            lambda models, error: self.say(
                error, error=True
            ) if error else self.say(self.t("connected", n=len(models or []))),
        )

    # ---- general -----------------------------------------------------------

    def _build_general(self, page: QWidget) -> None:
        outer, form = self._form(self._scrolled(page))
        self.answers_title, self.window_title = self._section(), self._section()
        self.interview_language = QComboBox()
        self.answer_style = QComboBox()
        self.answer_layout = QComboBox()
        self.full_answer = QCheckBox()
        self.language_labels = [QLabel() for _ in range(3)]
        self.ui_language = QComboBox()
        self.pinned = QCheckBox()
        self.frameless = QCheckBox()
        self.shot_enabled = QCheckBox()
        self.save_transcripts = QCheckBox()
        self.opacity = QSlider(Qt.Orientation.Horizontal)
        self.opacity.setRange(35, 100)
        self.font_scale = QSlider(Qt.Orientation.Horizontal)
        self.font_scale.setRange(70, 180)
        self.window_labels = [QLabel() for _ in range(3)]
        self.language_note = self._hint()
        self.shot_note = self._hint()
        self.reset_button = QPushButton()
        form.addRow(self.answers_title)
        form.addRow(self.language_labels[0], self.interview_language)
        form.addRow(self.language_labels[1], self.answer_style)
        form.addRow(self.language_labels[2], self.answer_layout)
        form.addRow(self.full_answer)
        form.addRow(self.language_note)
        form.addRow(self.window_title)
        form.addRow(self.window_labels[0], self.ui_language)
        form.addRow(self.pinned)
        form.addRow(self.frameless)
        form.addRow(self.window_labels[1], self.opacity)
        form.addRow(self.window_labels[2], self.font_scale)
        form.addRow(self.shot_enabled)
        form.addRow(self.shot_note)
        form.addRow(self.save_transcripts)
        row = QHBoxLayout()
        row.addWidget(self.reset_button)
        row.addStretch(1)
        outer.addLayout(row)
        outer.addStretch(1)
        self._bind_combo(self.interview_language, "language")
        self._bind_combo(self.ui_language, "ui_language")
        self._bind_combo(self.answer_style, "answer_style")
        self._bind_combo(self.answer_layout, "answer_layout")
        self._bind_check(self.full_answer, "show_full_answer")
        self._bind_check(self.pinned, "pinned")
        self._bind_check(self.frameless, "frameless")
        self._bind_check(self.shot_enabled, "screenshot_enabled")
        self._bind_check(self.save_transcripts, "save_transcripts")
        self.opacity.valueChanged.connect(lambda value: self._set("opacity", value))
        self.font_scale.valueChanged.connect(lambda value: self._set("font_scale", value))
        self.reset_button.clicked.connect(self._reset)
        self.shot_enabled.toggled.connect(lambda _v: self.win.refresh_controls())

    def refresh_screenshot(self) -> None:
        tools = self.rt.screenshot_tools()
        self.shot_note.setText(self.t("shot_note") if tools else self.t("shot_none"))

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
        self.refresh_project()
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
        ):
            with QSignalBlocker(widget):
                widget.setValue(value)
        for combo, value in (
            (self.interview_language, settings.language), (self.ui_language, settings.ui_language),
            (self.answer_style, settings.answer_style), (self.answer_layout, settings.answer_layout),
            (self.whisper_model, settings.whisper_model),
            (self.stt_engine, settings.stt_engine),
        ):
            with QSignalBlocker(combo):
                self._select(combo, value)
        for field, value in (
            (self.gateway_url, settings.gateway_url), (self.gateway_key, settings.gateway_key),
            (self.deepgram_key, settings.deepgram_key),
        ):
            with QSignalBlocker(field):
                field.setText(value)
        self._sync_engine()
        with QSignalBlocker(self.model_id):
            self.model_id.setCurrentText(settings.model or "auto")
        with QSignalBlocker(self.vision_id):
            self.vision_id.setCurrentText(settings.vision_model)
        self._sync_provider()

    def retranslate(self) -> None:
        self.setWindowTitle(f"{self.t('title')} · CareerTwin")
        for index, key in enumerate(self.TABS):
            self.tabs.setTabText(index, self.t(f"tab_{key}").replace("&", "&&"))
        self.close_button.setText(self.t("close"))
        self.interview_label.setText(self.t("interview"))
        self.new_interview.setText(self.t("new_interview"))
        self.rename_interview.setText(self.t("rename_interview"))
        self.delete_interview.setText(self.t("delete_interview"))
        self.interview_hint.setText(self.t("interview_hint"))
        self.docs_title.setText(self.t("sec_docs"))
        self.vacancy_title.setText(self.t("sec_vacancy"))
        self.answers_title.setText(self.t("sec_answers"))
        self.window_title.setText(self.t("sec_window"))
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
        self.project_intro.setText(self.t("project_intro"))
        self.project_choose.setText(self.t("project_choose"))
        self.project_again.setText(self.t("project_again"))
        self.project_remove.setText(self.t("project_remove"))
        self.project_save.setText(self.t("project_save"))
        self.project_dossier_label.setText(self.t("project_dossier"))
        self._show_project_note()
        self.about_intro.setText(self.t("about_intro"))
        self.about_save.setText(self.t("save"))
        for label, key in zip(self.language_labels, ("interview_language", "answer_style", "answer_layout")):
            label.setText(self.t(key))
        self.full_answer.setText(self.t("full_answer"))
        self.language_note.setText(self.t("language_note"))
        self._fill_language_combos()
        self.listen.setText(self.t("listen"))
        self.use_mic.setText(self.t("use_mic"))
        self.refresh_sources.setText(self.t("refresh"))
        for label, key in zip(self.audio_labels, ("call_source", "mic_source", "whisper_model")):
            label.setText(self.t(key))
        self.engine_label.setText(self.t("engine"))
        self.dg_key_label.setText(self.t("deepgram_key"))
        self.deepgram_test.setText(self.t("deepgram_test"))
        self.stt_engine.setItemText(0, self.t("engine_whisper"))
        self.stt_engine.setItemText(1, self.t("engine_deepgram"))
        self._sync_engine()
        for label, key in zip(self.model_labels, ("gateway_url", "gateway_key", "model_id", "vision_id")):
            label.setText(self.t(key))
        self.vision_hint.setText(self.t("vision_hint"))
        self.load_models.setText(self.t("load_models"))
        self.test_gateway.setText(self.t("test"))
        self.provider_label.setText(self.t("provider"))
        self.provider.setItemText(self.provider.findData(OTHER), self.t("provider_other"))
        self._show_provider_note()
        self.gateway_url.setPlaceholderText(self.rt.gateway())
        self.shot_enabled.setText(self.t("shot_enabled"))
        self.refresh_screenshot()
        self.pinned.setText(self.t("pinned"))
        self.frameless.setText(self.t("frameless"))
        self.save_transcripts.setText(self.t("save_transcripts"))
        for label, key in zip(self.window_labels, ("ui_language", "opacity", "font_scale")):
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
