//! Settings: four tabs, each about one thing. Interview (what the answers know), Practice (get ready for the
//! technical interview), AI (the provider, the keys and the three models) and General.

use std::path::PathBuf;

use egui::{ComboBox, RichText, ScrollArea, TextEdit, Ui};
use serde_json::json;

use super::theme::{accent, danger, good, muted, on_accent, semibold, strong, warn};
use super::{App, Event, text, theme};
use crate::accounts::Account;
use crate::coach::Request;
use crate::export::{self, Format};
use crate::library::{SavedDocument, read_document};
use crate::prepare::{self, Analysis, BankQuestion};
use crate::providers::{OTHER, PROVIDERS, provider_for};
use crate::settings::{ANSWER_LAYOUTS, ANSWER_STYLES, STT_ENGINES, WHISPER_MODELS};
use crate::transcript::read_all;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Interview,
    Practice,
    Ai,
    General,
}

pub struct State {
    pub tab: Tab,
    pub models: Vec<String>,
    pub models_error: Option<String>,
    pub connected: Option<Result<usize, String>>,
    pub new_key: String,
    pub checking_key: bool,
    pub rename: Option<String>,
    pub new_doc: String,
    pub bank: Option<Result<Vec<BankQuestion>, String>>,
    pub bank_busy: bool,
    pub bank_count: usize,
    pub focus: String,
    pub review: Option<Result<Analysis, String>>,
    pub review_busy: bool,
    pub exported: Option<Result<Vec<PathBuf>, String>>,
    pub formats: [bool; 3],
    pub icon_path: String,
    pub icon_error: Option<String>,
    pub confirm_reset: bool,
    /// The sound sources of the computer, looked up when the General tab first opens or on Refresh.
    pub sources: Option<Vec<String>>,
    /// The dossier as it is being edited, and the project folder it belongs to.
    pub dossier: String,
    pub dossier_of: String,
    /// The screenshot key as it is being typed.
    pub key_text: String,
    pub key_text_ready: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: Tab::Interview,
            models: Vec::new(),
            models_error: None,
            connected: None,
            new_key: String::new(),
            checking_key: false,
            rename: None,
            new_doc: String::new(),
            bank: None,
            bank_busy: false,
            bank_count: 10,
            focus: String::new(),
            review: None,
            review_busy: false,
            exported: None,
            formats: [true, true, false],
            icon_path: String::new(),
            icon_error: None,
            confirm_reset: false,
            sources: None,
            dossier: String::new(),
            dossier_of: String::new(),
            key_text: String::new(),
            key_text_ready: false,
        }
    }
}

impl State {
    pub fn models_loaded(&mut self, result: Result<Vec<String>, String>) {
        match result {
            Ok(models) => {
                self.models = models;
                self.models_error = None;
            }
            Err(error) => self.models_error = Some(error),
        }
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).font(semibold(16.0)).color(strong()));
    ui.add_space(2.0);
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(muted()).small());
}

thread_local! {
    /// The label of the row being drawn: what a screen reader calls the input in it.
    static NAME: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Gives an input the name a screen reader reads: the label of its row, or one given.
trait Named: Sized {
    fn named(self) -> Self;
    fn named_as(self, name: &str) -> Self;
}

impl Named for egui::Response {
    fn named(self) -> Self {
        let name = NAME.with(|name| name.borrow().clone());
        self.named_as(&name)
    }

    fn named_as(self, name: &str) -> Self {
        self.accessible_name(name)
    }
}

impl<R> Named for egui::InnerResponse<R> {
    fn named(self) -> Self {
        Self { inner: self.inner, response: self.response.named() }
    }

    fn named_as(self, name: &str) -> Self {
        Self { inner: self.inner, response: self.response.named_as(name) }
    }
}

fn row<R>(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(150.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText::new(label).color(muted()));
        });
        NAME.with(|name| label.clone_into(&mut name.borrow_mut()));
        let result = content(ui);
        NAME.with(|name| name.borrow_mut().clear());
        result
    })
    .inner
}

/// A line of text with a menu of ready choices next to it.
fn model_picker(ui: &mut Ui, value: &mut String, models: &[String], hint: &str, name: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let edit = TextEdit::singleline(value).hint_text(hint).desired_width((ui.available_width() - 72.0).max(120.0));
        changed |= ui.add(edit).named_as(name).changed();
        let list = ComboBox::from_id_salt(("models", hint)).selected_text("").width(24.0).show_ui(ui, |ui| {
            if models.is_empty() {
                ui.label(RichText::new("—").color(muted()));
            }
            ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for model in models {
                    if ui.selectable_label(value == model, model).clicked() {
                        *value = model.clone();
                        changed = true;
                    }
                }
            });
        });
        list.named_as(&format!("{name}: {}", models.len()));
    });
    changed
}

impl App {
    pub fn settings_window(&mut self, ctx: &egui::Context) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        if !self.settings_open {
            return;
        }
        let mut open = true;
        egui::Window::new(t("settings"))
            .id(egui::Id::unique("settings"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([720.0, 640.0])
            .min_size([520.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (tab, key) in [
                        (Tab::Interview, "tab_interview"),
                        (Tab::Practice, "tab_practice"),
                        (Tab::Ai, "tab_ai"),
                        (Tab::General, "tab_general"),
                    ] {
                        let selected = self.settings_ui.tab == tab;
                        let label =
                            RichText::new(t(key)).font(semibold(15.0)).color(if selected { accent() } else { muted() });
                        if ui.add(egui::Button::new(label).frame(false).selected(selected)).clicked() {
                            self.settings_ui.tab = tab;
                        }
                        ui.add_space(8.0);
                    }
                });
                ui.separator();
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.settings_ui.tab {
                    Tab::Interview => self.tab_interview(ui),
                    Tab::Practice => self.tab_practice(ui),
                    Tab::Ai => self.tab_ai(ui),
                    Tab::General => self.tab_general(ui),
                });
            });
        self.settings_open = open;
    }

    // ---- Interview -------------------------------------------------------------------------------------------

    /// The text of a vacancy or the notes changed: use it now, without reading the documents again.
    fn interview_text_changed(&mut self) {
        let current = self.library.current().clone();
        self.brief.job_title = current.vacancy.title.clone();
        self.brief.company = current.vacancy.company.clone();
        self.brief.job_description = current.vacancy.text.clone();
        self.brief.notes = current.notes.trim().to_owned();
        if let Err(error) = self.library.save(&self.paths.context_file()) {
            self.say(&format!("Could not save the interviews: {error}"), true);
        }
    }

    fn tab_interview(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        let (mut switch_to, mut new, mut delete, mut rename_done) = (None, false, false, false);
        ui.horizontal(|ui| {
            let current = self.library.current().name.clone();
            ComboBox::from_id_salt("interview")
                .selected_text(current)
                .width(300.0)
                .show_ui(ui, |ui| {
                    for item in &self.library.interviews {
                        if ui.selectable_label(item.id == self.library.active, &item.name).clicked() {
                            switch_to = Some(item.id.clone());
                        }
                    }
                })
                .named_as(&t("interview"));
            new = ui.button(t("new")).clicked();
            if ui.button(t("rename")).clicked() {
                self.settings_ui.rename = Some(self.library.current().name.clone());
            }
            delete = ui.button(RichText::new(t("delete")).color(danger())).clicked();
        });
        if let Some(name) = self.settings_ui.rename.as_mut() {
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(name).desired_width(300.0)).named_as(&t("rename"));
                rename_done = ui.button("OK").clicked();
            });
        }
        note(ui, &t("interview_note"));
        if let Some(id) = switch_to {
            self.library.active = id;
            self.library_changed();
        }
        if new {
            self.library.new_interview("");
            self.library_changed();
        }
        if delete {
            let id = self.library.active.clone();
            self.library.delete(&id);
            self.library_changed();
        }
        if rename_done && let Some(name) = self.settings_ui.rename.take().filter(|name| !name.trim().is_empty()) {
            self.library.current_mut().name = name.trim().to_owned();
            self.library_changed();
        }

        section(ui, &t("vacancy"));
        let mut changed = false;
        {
            let (title_label, company_label, text_hint) = (t("vacancy_title"), t("vacancy_company"), t("vacancy_text"));
            let vacancy = &mut self.library.current_mut().vacancy;
            row(ui, &title_label, |ui| {
                changed |=
                    ui.add(TextEdit::singleline(&mut vacancy.title).desired_width(f32::INFINITY)).named().changed()
            });
            row(ui, &company_label, |ui| {
                changed |=
                    ui.add(TextEdit::singleline(&mut vacancy.company).desired_width(f32::INFINITY)).named().changed()
            });
            changed |= ui
                .add(
                    TextEdit::multiline(&mut vacancy.text)
                        .hint_text(&text_hint)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY),
                )
                .named_as(&text_hint)
                .changed();
        }
        section(ui, &t("notes"));
        let hint = t("notes_hint");
        changed |= ui
            .add(
                TextEdit::multiline(&mut self.library.current_mut().notes)
                    .hint_text(&hint)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            )
            .named_as(&hint)
            .changed();
        if changed {
            self.interview_text_changed();
        }

        section(ui, &t("documents"));
        let mut documents_changed = false;
        let mut remove = None;
        for (index, doc) in self.library.current_mut().documents.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let name = std::path::Path::new(&doc.path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| doc.path.clone());
                documents_changed |= ui.checkbox(&mut doc.enabled, "").named_as(&name).changed();
                ui.label(&name).on_hover_text(&doc.path);
                ComboBox::from_id_salt(("kind", index))
                    .selected_text(doc.kind.clone())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for kind in ["cv", "about"] {
                            documents_changed |= ui.selectable_value(&mut doc.kind, kind.to_owned(), kind).changed();
                        }
                    })
                    .named_as(&name);
                if ui.add(egui::Button::new(RichText::new(t("remove")).color(danger()).small()).frame(false)).clicked()
                {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            self.library.current_mut().documents.remove(index);
            documents_changed = true;
        }
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 90.0).max(120.0);
            ui.add(TextEdit::singleline(&mut self.settings_ui.new_doc).hint_text(t("file_path")).desired_width(width))
                .named_as(&t("file_path"));
            if ui.add_enabled(!self.settings_ui.new_doc.trim().is_empty(), egui::Button::new(t("add_file"))).clicked() {
                let path = PathBuf::from(self.settings_ui.new_doc.trim().trim_matches('"'));
                match read_document(&path) {
                    Ok(_) => {
                        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                        let kind = crate::brief::guess_kind(&name).to_owned();
                        self.library.current_mut().documents.push(SavedDocument {
                            path: path.to_string_lossy().into_owned(),
                            kind,
                            enabled: true,
                        });
                        self.settings_ui.new_doc.clear();
                        documents_changed = true;
                        let message = tf("file_added", &[("name", &name)]);
                        self.say(&message, false);
                    }
                    Err(problem) => self.say(&problem, true),
                }
            }
        });
        if documents_changed {
            self.library_changed();
        }
        for problem in self.problems.clone() {
            ui.label(RichText::new(problem).color(danger()).small());
        }

        section(ui, &t("about_file"));
        let about = self.paths.about.clone();
        let size = std::fs::read_to_string(&about).map(|text| text.chars().count()).ok();
        ui.horizontal(|ui| {
            ui.label(RichText::new(about.display().to_string()).color(muted()).small());
            if let Some(size) = size {
                ui.label(RichText::new(format!("{:.1}k", size as f64 / 1000.0)).color(good()).small());
            }
            if ui.add(egui::Button::new(RichText::new(t("open_folder")).small()).frame(false)).clicked() {
                open_path(about.parent().unwrap_or(&about));
            }
        });
        section(ui, &t("project_folder"));
        let mut project_changed = false;
        project_changed |= ui
            .add(TextEdit::singleline(&mut self.library.current_mut().project).desired_width(f32::INFINITY))
            .named_as(&t("project_folder"))
            .changed();
        if project_changed {
            self.interview_text_changed();
        }
        self.project_panel(ui);
    }

    /// Reading the project, and the dossier the answers use: what it shows, and a place to correct it.
    fn project_panel(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let folder = self.library.current().project.trim().to_owned();
        if folder.is_empty() {
            note(ui, &t("project_note"));
            return;
        }
        ui.horizontal(|ui| {
            let busy = self.analyzing.is_some();
            let label = if self.project.is_some() { t("project_reread") } else { t("project_read") };
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                self.analyze_project();
            }
            if let Some(stage) = &self.analyzing {
                let text = match stage.split_once(' ') {
                    Some((_, files)) => tf("project_asking", &[("n", files)]),
                    None => t("project_reading"),
                };
                ui.add(egui::Spinner::new());
                ui.label(RichText::new(text).color(muted()).small());
            }
        });
        let Some(project) = self.project.clone() else {
            note(ui, &t("project_not_read"));
            return;
        };
        note(
            ui,
            &tf(
                "project_summary",
                &[
                    ("name", &project.name),
                    ("files", &project.files.to_string()),
                    ("size", &format!("{:.0}", project.characters as f64 / 1000.0)),
                    ("when", &project.analyzed_at),
                ],
            ),
        );
        if self.settings_ui.dossier_of != folder {
            self.settings_ui.dossier_of.clone_from(&folder);
            self.settings_ui.dossier.clone_from(&project.dossier);
        }
        ui.label(RichText::new(t("dossier")).color(muted()));
        ui.add(
            TextEdit::multiline(&mut self.settings_ui.dossier)
                .desired_rows(8)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        )
        .named_as(&t("dossier"));
        ui.horizontal(|ui| {
            let edited = self.settings_ui.dossier.trim() != project.dossier.trim();
            if ui.add_enabled(edited, egui::Button::new(t("dossier_save"))).clicked() {
                let mut edited_project = crate::project::ProjectContext::new(
                    &project.path,
                    &project.name,
                    &project.dossier,
                    project.files,
                    project.characters,
                    &project.analyzed_at,
                    project.chunks.clone(),
                );
                match crate::project::save_dossier(
                    &self.paths.interview_dir(),
                    &mut edited_project,
                    &self.settings_ui.dossier,
                ) {
                    Ok(()) => {
                        self.project = Some(std::sync::Arc::new(edited_project));
                        self.library_changed();
                    }
                    Err(error) => self.say(&error.to_string(), true),
                }
            }
            if ui.button(RichText::new(t("project_forget")).color(danger())).clicked() {
                crate::project::forget(&self.paths.interview_dir(), std::path::Path::new(&folder));
                self.settings_ui.dossier_of.clear();
                self.library_changed();
            }
        });
        note(ui, &t("dossier_note"));
    }

    // ---- Practice --------------------------------------------------------------------------------------------

    fn step(&self, ui: &mut Ui, number: usize, title: &str, done: bool, detail: &str, optional: bool) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        ui.horizontal(|ui| {
            super::theme::step_mark(ui, done);
            ui.label(RichText::new(format!("{number}. {title}")).font(semibold(15.0)).color(strong()));
            if optional {
                ui.label(RichText::new(t("optional")).color(muted()).small());
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(26.0);
            ui.label(RichText::new(detail).color(if done { muted() } else { warn() }).small());
        });
        ui.add_space(-4.0);
    }

    fn tab_practice(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        ui.label(RichText::new(t("practice_title")).font(semibold(18.0)).color(strong()));
        note(ui, &t("practice_intro"));
        ui.add_space(8.0);
        let interview = self.library.current().clone();
        let cv = self.brief.documents.iter().find(|doc| doc.kind == "cv" && doc.enabled).map(|doc| doc.name.clone());
        let about_chars = self.brief.candidate.chars().count();
        self.step(
            ui,
            1,
            &t("step_vacancy"),
            !interview.vacancy.is_empty(),
            &if interview.vacancy.is_empty() { t("missing_vacancy") } else { interview.vacancy.label() },
            false,
        );
        self.step(ui, 2, &t("step_cv"), cv.is_some(), &cv.clone().unwrap_or_else(|| t("missing_cv")), false);
        self.step(
            ui,
            3,
            &t("step_about"),
            about_chars > 0,
            &if about_chars > 0 {
                format!("ABOUT.md · {:.1}k", about_chars as f64 / 1000.0)
            } else {
                t("missing_about")
            },
            false,
        );
        self.step(
            ui,
            4,
            &t("step_project"),
            !interview.project.is_empty(),
            &if interview.project.is_empty() { t("missing_project") } else { interview.project.clone() },
            true,
        );

        let ready = !self.brief.is_empty();
        section(ui, &format!("5. {}", t("step_bank")));
        ui.horizontal(|ui| {
            let label = ui.label(RichText::new(t("how_many")).color(muted()));
            ui.add(egui::DragValue::new(&mut self.settings_ui.bank_count).range(3..=20)).labelled_by(label.id);
            ui.add(
                TextEdit::singleline(&mut self.settings_ui.focus)
                    .hint_text(t("focus_hint"))
                    .desired_width(f32::INFINITY),
            )
            .named_as(&t("focus"));
        });
        ui.horizontal(|ui| {
            let busy = self.settings_ui.bank_busy;
            let label = if busy { t("working") } else { t("build_bank") };
            if ui
                .add_enabled(ready && !busy, egui::Button::new(RichText::new(label).color(on_accent())).fill(accent()))
                .clicked()
            {
                self.build_bank();
            }
        });
        match &self.settings_ui.bank {
            None => note(ui, &t("nothing_yet")),
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(danger()));
            }
            Some(Ok(questions)) => {
                for (index, item) in questions.iter().enumerate() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{}. {}", index + 1, item.question)).font(semibold(15.0)).color(strong()),
                    );
                    if !item.kind.is_empty() {
                        ui.label(RichText::new(&item.kind).color(muted()).small());
                    }
                    for line in &item.outline {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("–").color(accent()));
                            super::answer::rich_line(ui, line, 14.5, super::theme::text());
                        });
                    }
                }
            }
        }

        section(ui, &format!("6. {}", t("step_review")));
        let busy = self.settings_ui.review_busy;
        let label = if busy { t("working") } else { t("review_cv") };
        if ui.add_enabled(ready && !busy && cv.is_some(), egui::Button::new(label)).clicked() {
            self.review_cv();
        }
        match &self.settings_ui.review {
            None => note(ui, &t("nothing_yet")),
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(danger()));
            }
            Some(Ok(analysis)) => {
                if !analysis.summary.is_empty() {
                    ui.label(&analysis.summary);
                }
                let list = |ui: &mut Ui, title: String, items: &[String]| {
                    if items.is_empty() {
                        return;
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new(title).font(semibold(14.0)).color(muted()));
                    for item in items {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("–").color(accent()));
                            ui.label(item);
                        });
                    }
                };
                list(ui, t("strengths"), &analysis.strengths);
                if !analysis.gaps.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t("gaps")).font(semibold(14.0)).color(muted()));
                    for gap in &analysis.gaps {
                        ui.label(RichText::new(&gap.gap).color(strong()));
                        ui.label(RichText::new(&gap.how_to_address).color(muted()));
                    }
                }
                list(ui, t("likely"), &analysis.likely_questions);
                list(ui, t("fixes"), &analysis.cv_fixes);
            }
        }

        section(ui, &t("past_questions"));
        note(ui, &t("past_questions_body"));
        let records = read_all(&self.paths.transcripts_dir());
        let rows = export::rows(&records);
        ui.label(
            RichText::new(tf("questions_found", &[("n", &rows.len().to_string()), ("m", &records.len().to_string())]))
                .color(muted()),
        );
        ui.horizontal(|ui| {
            for (index, format) in Format::ALL.iter().enumerate() {
                ui.checkbox(&mut self.settings_ui.formats[index], format.extension().to_uppercase());
            }
        });
        let chosen: Vec<Format> = Format::ALL
            .iter()
            .enumerate()
            .filter(|(index, _)| self.settings_ui.formats[*index])
            .map(|(_, format)| *format)
            .collect();
        if ui.add_enabled(!chosen.is_empty() && !rows.is_empty(), egui::Button::new(t("export_questions"))).clicked() {
            self.export_questions(chosen);
        }
        match &self.settings_ui.exported {
            Some(Ok(files)) => {
                let folder = self.paths.exports_dir();
                ui.label(
                    RichText::new(tf(
                        "export_done",
                        &[("n", &files.len().to_string()), ("folder", &folder.display().to_string())],
                    ))
                    .color(good())
                    .small(),
                );
                if ui.add(egui::Button::new(t("open_folder")).frame(false)).clicked() {
                    open_path(&folder);
                }
            }
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(danger()).small());
            }
            None => {}
        }
    }

    fn build_bank(&mut self) {
        self.settings_ui.bank_busy = true;
        self.settings_ui.bank = None;
        let (coach, tx, repaint) = (self.coach.clone(), self.tx.clone(), self.repaint.clone());
        let count = self.settings_ui.bank_count;
        let prompt = prepare::questions_prompt(&self.brief, count, &self.settings_ui.focus, self.language().as_deref());
        self.handle.spawn(async move {
            let mut result = Err("no answer".to_owned());
            for _ in 0..2 {
                // free models sometimes return broken JSON; try once more before giving up
                let request = Request {
                    system: prepare::QUESTIONS_SYSTEM.to_owned(),
                    prompt: prompt.clone(),
                    image: None,
                    max_tokens: 3000,
                    temperature: 0.4,
                };
                result = match coach.complete(&request).await {
                    Ok(text) => prepare::parse_questions(&text, count).map_err(|error| error.0),
                    Err(error) => Err(error.message),
                };
                if result.is_ok() {
                    break;
                }
            }
            let _ = tx.send(Event::Bank(result));
            repaint();
        });
    }

    fn review_cv(&mut self) {
        self.settings_ui.review_busy = true;
        self.settings_ui.review = None;
        let (coach, tx, repaint) = (self.coach.clone(), self.tx.clone(), self.repaint.clone());
        let prompt = prepare::analysis_prompt(&self.brief, self.language().as_deref());
        self.handle.spawn(async move {
            let mut result = Err("no answer".to_owned());
            for _ in 0..2 {
                let request = Request {
                    system: prepare::ANALYSIS_SYSTEM.to_owned(),
                    prompt: prompt.clone(),
                    image: None,
                    max_tokens: 3000,
                    temperature: 0.3,
                };
                result = match coach.complete(&request).await {
                    Ok(text) => prepare::parse_analysis(&text).map_err(|error| error.0),
                    Err(error) => Err(error.message),
                };
                if result.is_ok() {
                    break;
                }
            }
            let _ = tx.send(Event::Review(result));
            repaint();
        });
    }

    // ---- AI --------------------------------------------------------------------------------------------------

    fn tab_ai(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        let mut changed = false;
        let address = self.settings.gateway();
        let known = provider_for(&address);
        row(ui, &t("provider"), |ui| {
            let label = known.map_or("Other (own address)".to_owned(), |provider| provider.label.to_owned());
            let combo = ComboBox::from_id_salt("provider").selected_text(label).width(260.0).show_ui(ui, |ui| {
                for provider in PROVIDERS {
                    if ui.selectable_label(known == Some(provider), provider.label).clicked() && known != Some(provider)
                    {
                        // a model id belongs to one provider, and so does a key
                        self.settings.model.clear();
                        self.settings.vision_model.clear();
                        if self.settings.set_gateway(provider.url).is_ok() {
                            changed = true;
                        }
                    }
                }
                let _ = OTHER;
            });
            combo.named();
        });
        let mut url =
            if self.settings.gateway_url.is_empty() { address.clone() } else { self.settings.gateway_url.clone() };
        let url_changed = row(ui, &t("base_url"), |ui| {
            ui.add(TextEdit::singleline(&mut url).desired_width(f32::INFINITY)).named().lost_focus()
        });
        if url_changed && url.trim() != self.settings.gateway_url.trim() && url.trim() != address {
            match self.settings.set_gateway(url.trim()) {
                Ok(()) => changed = true,
                Err(error) => self.say(&tf("bad_value", &[("error", &error.to_string())]), true),
            }
        }
        if let Some(provider) = known.filter(|provider| provider.cloud) {
            note(ui, &tf("cloud_note", &[("name", provider.label)]));
        }

        if known.is_none_or(|provider| provider.cloud) {
            section(ui, &t("accounts"));
            let here: Vec<Account> = self.settings.accounts_here();
            if here.is_empty() {
                note(ui, &t("no_keys"));
            }
            let (mut use_name, mut remove_name) = (None, None);
            for account in &here {
                egui::Frame::new()
                    .fill(super::theme::field())
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&account.name).font(semibold(15.0)).color(strong()));
                            ui.label(RichText::new(account.tail()).color(muted()).small());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new(RichText::new(t("remove")).color(danger()))).clicked() {
                                    remove_name = Some(account.name.clone());
                                }
                                if account.key == self.settings.gateway_key {
                                    ui.label(RichText::new(t("in_use")).color(accent()));
                                } else if ui.button(t("use")).clicked() {
                                    use_name = Some(account.name.clone());
                                }
                            });
                        });
                    });
            }
            if let Some(name) = use_name {
                self.settings.use_account(&name);
                changed = true;
            }
            if let Some(name) = remove_name {
                self.settings.remove_account(&name);
                changed = true;
            }
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 120.0).max(120.0);
                let edit = ui
                    .add(
                        TextEdit::singleline(&mut self.settings_ui.new_key)
                            .password(true)
                            .hint_text(t("paste_key"))
                            .desired_width(width),
                    )
                    .named_as(&t("paste_key"));
                let enter = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let can = !self.settings_ui.new_key.trim().is_empty() && !self.settings_ui.checking_key;
                if (ui.add_enabled(can, egui::Button::new(t("save_key"))).clicked() || (enter && can)) && can {
                    self.settings_ui.checking_key = true;
                    let (coach, tx, repaint, key, url) = (
                        self.coach.clone(),
                        self.tx.clone(),
                        self.repaint.clone(),
                        self.settings_ui.new_key.trim().to_owned(),
                        address.clone(),
                    );
                    self.handle.spawn(async move {
                        let result = coach.check_key(&url, &key).await.map_err(|error| error.message);
                        let _ = tx.send(Event::KeyChecked { key, result });
                        repaint();
                    });
                }
            });
            note(ui, &t("keys_note"));
        }

        let mut app_changed = false;
        row(ui, &t("parallel"), |ui| {
            let label = t(&format!("parallel_{}", self.app.parallel_accounts));
            ComboBox::from_id_salt("parallel")
                .selected_text(label)
                .width(320.0)
                .show_ui(ui, |ui| {
                    for mode in crate::settings::PARALLEL_MODES {
                        if ui
                            .selectable_label(self.app.parallel_accounts == mode, t(&format!("parallel_{mode}")))
                            .clicked()
                        {
                            self.app.parallel_accounts = mode.to_owned();
                            app_changed = true;
                        }
                    }
                })
                .named();
        });
        row(ui, &t("first_word"), |ui| {
            app_changed |= ui
                .add(
                    egui::DragValue::new(&mut self.app.first_word_seconds)
                        .range(crate::settings::FIRST_WORD_RANGE)
                        .suffix(" s"),
                )
                .named()
                .changed();
        });
        note(ui, &t("parallel_note"));
        if app_changed {
            self.app_changed();
        }

        section(ui, &t("models_title"));
        let models = self.settings_ui.models.clone();
        ui.label(RichText::new(t("model_ask")).font(semibold(15.0)).color(strong()));
        changed |= model_picker(ui, &mut self.settings.model, &models, "auto", &t("model_ask"));
        note(ui, &t("model_ask_note"));
        row(ui, &t("fallbacks"), |ui| {
            changed |= ui
                .add(
                    TextEdit::singleline(&mut self.settings.fallback_models)
                        .hint_text(t("fallbacks_hint"))
                        .desired_width(f32::INFINITY),
                )
                .named()
                .changed();
        });
        ui.add_space(10.0);
        ui.label(RichText::new(t("model_screenshot")).font(semibold(15.0)).color(strong()));
        changed |= model_picker(ui, &mut self.settings.vision_model, &models, "", &t("model_screenshot"));
        note(ui, &t("model_shot_note"));
        ui.add_space(10.0);
        ui.label(RichText::new(t("model_audio")).font(semibold(15.0)).color(strong()));
        row(ui, &t("engine"), |ui| {
            let label = if self.settings.stt_engine == "deepgram" { t("engine_deepgram") } else { t("engine_whisper") };
            ComboBox::from_id_salt("engine")
                .selected_text(label)
                .width(260.0)
                .show_ui(ui, |ui| {
                    for engine in STT_ENGINES {
                        let text = if engine == "deepgram" { t("engine_deepgram") } else { t("engine_whisper") };
                        if ui.selectable_label(self.settings.stt_engine == engine, text).clicked() {
                            self.settings.stt_engine = engine.to_owned();
                            changed = true;
                        }
                    }
                })
                .named();
        });
        if self.settings.stt_engine == "deepgram" {
            row(ui, &t("deepgram_key"), |ui| {
                changed |= ui
                    .add(
                        TextEdit::singleline(&mut self.settings.deepgram_key)
                            .password(true)
                            .desired_width(f32::INFINITY),
                    )
                    .named()
                    .changed();
            });
            row(ui, &t("speech_model"), |ui| {
                changed |= ui
                    .add(TextEdit::singleline(&mut self.settings.deepgram_model).desired_width(f32::INFINITY))
                    .named()
                    .changed();
            });
        } else {
            row(ui, &t("speech_model"), |ui| {
                ComboBox::from_id_salt("whisper")
                    .selected_text(self.settings.whisper_model.clone())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for model in WHISPER_MODELS {
                            if ui.selectable_label(self.settings.whisper_model == model, model).clicked() {
                                self.settings.whisper_model = model.to_owned();
                                changed = true;
                            }
                        }
                    })
                    .named();
            });
        }
        note(ui, &t("audio_note"));

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button(t("load_models")).clicked() {
                self.settings_ui.models_error = None;
                let (coach, tx, repaint) = (self.coach.clone(), self.tx.clone(), self.repaint.clone());
                self.handle.spawn(async move {
                    let _ = tx.send(Event::Models(coach.models().await.map_err(|error| error.message)));
                    repaint();
                });
            }
            if ui.button(t("test_connection")).clicked() {
                self.settings_ui.connected = None;
                let (coach, tx, repaint) = (self.coach.clone(), self.tx.clone(), self.repaint.clone());
                self.handle.spawn(async move {
                    let _ = tx.send(Event::Connected(
                        coach.models().await.map(|models| models.len()).map_err(|error| error.message),
                    ));
                    repaint();
                });
            }
        });
        if let Some(error) = &self.settings_ui.models_error {
            ui.label(RichText::new(error).color(danger()).small());
        }
        match &self.settings_ui.connected {
            Some(Ok(count)) => {
                ui.label(RichText::new(tf("connected", &[("n", &count.to_string())])).color(good()).small());
            }
            Some(Err(error)) => {
                ui.label(RichText::new(error).color(danger()).small());
            }
            None => {}
        }
        if changed {
            self.settings_changed();
        }
    }

    // ---- General ---------------------------------------------------------------------------------------------

    fn tab_general(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        let mut changed = self.appearance(ui);
        section(ui, &t("language"));
        row(ui, &t("interview_language"), |ui| {
            let current = self.settings.language.clone();
            ComboBox::from_id_salt("language")
                .selected_text(if current == "auto" { t("auto") } else { crate::languages::name(Some(&current)) })
                .width(220.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current == "auto", t("auto")).clicked() {
                        self.settings.language = "auto".into();
                        changed = true;
                    }
                    for (code, name) in crate::languages::NAMES {
                        if ui.selectable_label(current == *code, *name).clicked() {
                            self.settings.language = (*code).to_owned();
                            changed = true;
                        }
                    }
                })
                .named();
        });
        row(ui, &t("window_language"), |ui| {
            let current = self.settings.ui_language.clone();
            let label = crate::languages::UI_LANGUAGES
                .iter()
                .find(|(code, _)| *code == current)
                .map_or(t("auto"), |(_, name)| (*name).to_owned());
            ComboBox::from_id_salt("ui_language")
                .selected_text(label)
                .width(220.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current == "auto", t("auto")).clicked() {
                        self.settings.ui_language = "auto".into();
                        changed = true;
                    }
                    for (code, name) in crate::languages::UI_LANGUAGES {
                        if ui.selectable_label(current == *code, *name).clicked() {
                            self.settings.ui_language = (*code).to_owned();
                            changed = true;
                        }
                    }
                })
                .named();
        });

        changed |= self.hearing_section(ui);

        section(ui, &t("answer_style"));
        row(ui, &t("style"), |ui| {
            for style in ANSWER_STYLES {
                changed |= ui
                    .selectable_value(&mut self.settings.answer_style, style.to_owned(), t(&format!("style_{style}")))
                    .changed();
            }
        });
        row(ui, &t("layout"), |ui| {
            for layout in ANSWER_LAYOUTS {
                changed |= ui
                    .selectable_value(
                        &mut self.settings.answer_layout,
                        layout.to_owned(),
                        t(&format!("layout_{layout}")),
                    )
                    .changed();
            }
        });

        section(ui, &t("screenshot_section"));
        changed |= ui.checkbox(&mut self.settings.screenshot_enabled, t("screenshot_enabled")).changed();
        let tools = crate::screenshot::available_tools();
        row(ui, &t("screenshot_tool"), |ui| {
            let current = self.settings.screenshot_tool.clone();
            ComboBox::from_id_salt("tool")
                .selected_text(if current.is_empty() || current == "auto" { t("auto") } else { current.clone() })
                .width(220.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current == "auto", t("auto")).clicked() {
                        self.settings.screenshot_tool = "auto".into();
                        changed = true;
                    }
                    for tool in &tools {
                        if ui.selectable_label(&current == tool, tool).clicked() {
                            self.settings.screenshot_tool = tool.clone();
                            changed = true;
                        }
                    }
                })
                .named();
        });
        note(ui, &t("screenshot_note"));
        if cfg!(windows) {
            ui.horizontal(|ui| {
                let key = self.screenshot_key.clone().unwrap_or_else(|| self.app.screenshot_key.clone());
                ui.label(RichText::new(tf("screenshot_key_now", &[("key", &key)])).color(muted()).small());
            });
            if !self.settings_ui.key_text_ready {
                self.settings_ui.key_text_ready = true;
                self.settings_ui.key_text.clone_from(&self.app.screenshot_key);
            }
            row(ui, &t("screenshot_key"), |ui| {
                let edit = ui.add(TextEdit::singleline(&mut self.settings_ui.key_text).desired_width(200.0)).named();
                let valid = crate::hotkey::parse(&self.settings_ui.key_text).is_some();
                if edit.lost_focus() && valid && self.settings_ui.key_text != self.app.screenshot_key {
                    self.app.screenshot_key = self.settings_ui.key_text.clone();
                    self.app_changed();
                    self.say(&tf("screenshot_key_restart", &[("key", &self.app.screenshot_key)]), false);
                }
                if !valid {
                    ui.label(RichText::new(t("screenshot_key_bad")).color(danger()).small());
                }
            });
        }
        if cfg!(windows) {
            let mut tray = self.app.tray_icon;
            if ui.checkbox(&mut tray, t("tray_icon")).changed() {
                self.app.tray_icon = tray;
                self.app_changed();
            }
            note(ui, &t("tray_note"));
        }
        if crate::capture_exclude::supported() {
            let mut hide = self.app.hide_from_sharing;
            if ui.checkbox(&mut hide, t("hide_from_sharing")).changed() {
                self.app.hide_from_sharing = hide;
                self.app_changed();
            }
            note(ui, &t("hide_note"));
        }

        section(ui, &t("updates"));
        let mut check = self.app.check_updates;
        if ui.checkbox(&mut check, t("check_updates")).changed() {
            self.app.check_updates = check;
            self.app_changed();
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.update_ui.checking, egui::Button::new(t("check_now"))).clicked() {
                self.check_for_update(true);
            }
            match &self.update_ui.last {
                Some(Ok(version)) => {
                    ui.label(RichText::new(tf("up_to_date", &[("version", version)])).color(good()).small());
                }
                Some(Err(why)) => {
                    ui.label(RichText::new(tf("update_failed", &[("why", why)])).color(danger()).small());
                }
                None => {}
            }
        });
        note(ui, &t("update_note"));

        section(ui, &t("data"));
        changed |= ui.checkbox(&mut self.settings.save_transcripts, t("save_transcripts")).changed();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tf("data_folder", &[("path", &self.paths.data.display().to_string())]))
                    .color(muted())
                    .small(),
            );
            if ui.add(egui::Button::new(RichText::new(t("open_folder")).small()).frame(false)).clicked() {
                open_path(&self.paths.data);
            }
        });
        ui.add_space(8.0);
        let reset = ui.button(RichText::new(t("reset")).color(danger()));
        if reset.clicked() {
            self.settings_ui.confirm_reset = true;
        }
        if self.settings_ui.confirm_reset {
            ui.horizontal(|ui| {
                if ui.button(RichText::new("OK").color(danger())).clicked() {
                    let keep = self.settings.clone();
                    self.settings = crate::settings::Settings::default();
                    let _ = keep;
                    self.settings_ui.confirm_reset = false;
                    changed = true;
                }
                if ui.button(t("close")).clicked() {
                    self.settings_ui.confirm_reset = false;
                }
            });
        }
        if changed {
            self.settings_changed();
        }
    }

    /// The call and the microphone: what the window listens to.
    fn hearing_section(&mut self, ui: &mut Ui) -> bool {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let mut changed = false;
        section(ui, &t("hearing"));
        changed |= ui.checkbox(&mut self.settings.listen, t("listen_call")).changed();
        let sources =
            self.settings_ui.sources.get_or_insert_with(|| crate::audio::list_sources().unwrap_or_default()).clone();
        let pick = |ui: &mut Ui, id: &str, value: &mut String, automatic: bool| -> bool {
            let mut changed = false;
            let shown = if value.trim().is_empty() || value == "auto" {
                t(if automatic { "auto" } else { "default_mic" })
            } else {
                value.clone()
            };
            ComboBox::from_id_salt(id)
                .selected_text(shown)
                .width(320.0)
                .show_ui(ui, |ui| {
                    let none = if automatic { "auto" } else { "" };
                    if ui.selectable_label(value == none, t(if automatic { "auto" } else { "default_mic" })).clicked() {
                        *value = none.to_owned();
                        changed = true;
                    }
                    for source in &sources {
                        if ui.selectable_label(value == source, source).clicked() {
                            value.clone_from(source);
                            changed = true;
                        }
                    }
                })
                .named();
            changed
        };
        if self.settings.listen {
            row(ui, &t("call_source"), |ui| {
                changed |= pick(ui, "call_source", &mut self.settings.call_source, true);
                if ui.button(t("refresh")).clicked() {
                    self.settings_ui.sources = None;
                }
            });
        }
        changed |= ui.checkbox(&mut self.settings.use_mic, t("use_mic")).changed();
        if self.settings.use_mic {
            row(ui, &t("mic_source"), |ui| {
                changed |= pick(ui, "mic_source", &mut self.settings.mic_source, false);
            });
        }
        note(ui, &t("hearing_note"));
        changed
    }

    /// Theme, equalizer, icon and the window itself: everything about how it looks.
    fn appearance(&mut self, ui: &mut Ui) -> bool {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let mut changed = false;
        section(ui, &t("appearance"));
        ui.label(RichText::new(t("theme")).color(muted()));
        let mut chosen: Option<String> = None;
        let current = self.app.theme.clone();
        ui.horizontal_wrapped(|ui| {
            for (name, panel, _, accent_color) in super::theme::PRESETS {
                if swatch(ui, &t(&format!("theme_{name}")), panel, accent_color, current == name) {
                    chosen = Some(name.to_owned());
                }
            }
            let (panel, _, accent_color) = crate::look::colors(&self.app);
            if swatch(ui, &t("theme_custom"), panel, accent_color, current == "custom") {
                chosen = Some("custom".to_owned());
            }
        });
        if let Some(name) = chosen {
            self.app.theme = name;
            self.app_changed();
        }
        if self.app.theme == "custom" {
            let mut edited = false;
            for (key, slot) in [("theme_background", 0), ("theme_text", 1), ("theme_accent", 2)] {
                row(ui, &t(key), |ui| {
                    let field = match slot {
                        0 => &mut self.app.custom_panel,
                        1 => &mut self.app.custom_text,
                        _ => &mut self.app.custom_accent,
                    };
                    let mut rgb = crate::look::parse_hex(field).unwrap_or([128, 128, 128]);
                    if ui.color_edit_button_srgb(&mut rgb).named().changed() {
                        *field = crate::look::hex(rgb);
                        edited = true;
                    }
                    ui.label(RichText::new(field.clone()).color(muted()).small());
                });
            }
            if edited {
                self.app_changed();
            }
        }

        ui.add_space(8.0);
        row(ui, &t("equalizer"), |ui| {
            let mut style = self.app.equalizer.clone();
            for choice in crate::settings::EQUALIZER_STYLES {
                ui.selectable_value(&mut style, choice.to_owned(), t(&format!("eq_{choice}")));
            }
            if style != self.app.equalizer {
                self.app.equalizer = style;
                self.app_changed();
            }
        });
        let mut animate = self.app.animate;
        if ui.checkbox(&mut animate, t("animate")).changed() {
            self.app.animate = animate;
            self.app_changed();
        }
        note(ui, &t("eq_note"));

        ui.add_space(8.0);
        ui.label(RichText::new(t("app_icon")).color(muted()));
        self.icon_picker(ui);

        ui.add_space(8.0);
        changed |= ui.checkbox(&mut self.settings.pinned, t("keep_on_top")).changed();
        changed |= ui.checkbox(&mut self.settings.frameless, t("frameless")).changed();
        row(ui, &t("opacity"), |ui| {
            changed |= ui.add(egui::Slider::new(&mut self.settings.opacity, 35..=100).suffix("%")).named().changed()
        });
        row(ui, &t("text_size"), |ui| {
            changed |= ui.add(egui::Slider::new(&mut self.settings.font_scale, 70..=180).suffix("%")).named().changed()
        });
        changed
    }

    fn icon_picker(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        let current = self.app.app_icon.clone();
        let mut chosen: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            let mut tile = |ui: &mut Ui, setting: &str, label: &str| {
                let selected = current == setting;
                let icon = super::icon::load(setting, accent()).unwrap_or_default();
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
                let texture = ui.ctx().load_texture(format!("icon-{setting}"), image, egui::TextureOptions::LINEAR);
                ui.vertical(|ui| {
                    let response = ui
                        .add(
                            egui::Button::image(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(52.0, 52.0)))
                                .selected(selected),
                        )
                        .named_as(label);
                    ui.label(RichText::new(label).small().color(if selected { accent() } else { muted() }));
                    if response.clicked() {
                        chosen = Some(setting.to_owned());
                    }
                });
            };
            tile(ui, "", &t("icon_default"));
            tile(ui, "theme", &t("icon_theme"));
            for (name, _) in super::icon::VARIANTS.iter().skip(1) {
                tile(ui, &format!("builtin:{name}"), name);
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(t("icon_custom")).color(muted()));
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 170.0).max(120.0);
            ui.add(
                TextEdit::singleline(&mut self.settings_ui.icon_path).hint_text(t("icon_path")).desired_width(width),
            )
            .named_as(&t("icon_path"));
            if ui
                .add_enabled(!self.settings_ui.icon_path.trim().is_empty(), egui::Button::new(t("icon_apply")))
                .clicked()
            {
                let path = self.settings_ui.icon_path.trim().trim_matches('"').to_owned();
                match super::icon::load(&path, accent()) {
                    Ok(_) => chosen = Some(path),
                    Err(why) => self.settings_ui.icon_error = Some(why),
                }
            }
        });
        if let Some(why) = &self.settings_ui.icon_error {
            ui.label(RichText::new(tf("icon_bad", &[("why", why)])).color(danger()).small());
        }
        note(ui, &t("icon_note"));
        if let Some(setting) = chosen {
            self.settings_ui.icon_error = None;
            self.app.app_icon = setting;
            self.app_changed();
        }
    }
}

/// A theme as a small card: its background with its accent on it, and its name. True when it was clicked.
fn swatch(ui: &mut Ui, name: &str, panel: [u8; 3], accent_color: [u8; 3], selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(74.0, 62.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    let painter = ui.painter();
    let fill = egui::Color32::from_rgb(panel[0], panel[1], panel[2]);
    painter.rect_filled(rect, 8.0, fill);
    let border = if selected {
        accent()
    } else if response.hovered() {
        muted()
    } else {
        super::theme::line()
    };
    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(
        rect.center() - egui::vec2(0.0, 8.0),
        10.0,
        egui::Color32::from_rgb(accent_color[0], accent_color[1], accent_color[2]),
    );
    let label =
        if super::theme::luminance(fill) < 0.5 { egui::Color32::from_gray(210) } else { egui::Color32::from_gray(50) };
    painter.text(
        rect.center_bottom() - egui::vec2(0.0, 9.0),
        egui::Align2::CENTER_CENTER,
        name,
        theme::sans(11.5),
        label,
    );
    response.clicked()
}

/// Open a folder or a file in the system's own program.
pub fn open_path(path: &std::path::Path) {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let _ = json!(null);
}
