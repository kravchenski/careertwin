//! Settings: four tabs, each about one thing. Interview (what the answers know), Practice (get ready for the
//! technical interview), AI (the provider, the keys and the three models) and General.

use std::path::PathBuf;

use egui::{ComboBox, RichText, ScrollArea, Stroke, TextEdit, Ui};
use serde_json::json;

use super::theme::{accent, danger, good, muted, on_accent, semibold, strong, text as text_color, warn};
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
    /// The address and the key the list above was asked for: a list belongs to both.
    pub models_for: Option<(String, String)>,
    pub models_loading: bool,
    /// The AI tab asks for the list itself when it has none for this address and key. The tests turn it off.
    pub auto_models: bool,
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
    /// The pictures of the icon tiles, made once: keyed by the setting and the accent colour.
    pub icons: std::collections::HashMap<String, egui::TextureHandle>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: Tab::Interview,
            models: Vec::new(),
            models_error: None,
            models_for: None,
            models_loading: false,
            auto_models: true,
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
            icons: std::collections::HashMap::new(),
        }
    }
}

impl State {
    pub fn models_loaded(&mut self, result: Result<Vec<String>, String>) {
        self.models_loading = false;
        match result {
            Ok(models) => {
                self.models = models;
                self.models_error = None;
            }
            Err(error) => self.models_error = Some(error),
        }
    }
}

/// A heading for a group. Space goes before it, not after: the heading belongs to what follows, and a group is
/// further from the one before than its own lines are from each other. The first of a page starts at the top.
fn section(ui: &mut Ui, title: &str) {
    if ui.min_rect().height() > 1.0 {
        ui.add_space(24.0);
    }
    ui.label(RichText::new(title).font(semibold(19.0)).color(strong()));
    ui.add_space(2.0);
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(muted()).small());
}

thread_local! {
    /// The label of the row being drawn: what a screen reader calls the input in it.
    static NAME: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// The width of the label column of the page being drawn: as wide as its longest label, so a field is never far
    /// from the words that name it.
    static COLUMN: std::cell::Cell<f32> = const { std::cell::Cell::new(190.0) };
    /// A hint for the label of the next row: shown when the pointer rests on it, not as a line of its own.
    static HINT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The next row's label explains itself in a tooltip.
fn hint_next(text: &str) {
    HINT.with(|hint| text.clone_into(&mut hint.borrow_mut()));
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
        // a column of one width, so every field of a page starts on the same line
        let width = COLUMN.with(std::cell::Cell::get);
        ui.allocate_ui_with_layout(egui::vec2(width, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(width);
            let shown = ui.label(RichText::new(label).color(muted()));
            let hint = HINT.with(|hint| std::mem::take(&mut *hint.borrow_mut()));
            if !hint.is_empty() {
                shown.on_hover_text(hint);
            }
        });
        NAME.with(|name| label.clone_into(&mut name.borrow_mut()));
        let result = content(ui);
        NAME.with(|name| name.borrow_mut().clear());
        result
    })
    .inner
}

/// The models that have every word of the query in their id, in the order the provider listed them, whatever the
/// case: "llama 70b" finds "meta/llama-3.3-70b-instruct". An empty query finds them all.
fn matching<'a>(models: &'a [String], query: &str) -> Vec<&'a String> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    models
        .iter()
        .filter(|model| {
            let id = model.to_lowercase();
            words.iter().all(|word| id.contains(word.as_str()))
        })
        .collect()
}

/// A line of text with a menu of ready choices next to it. The menu has a search line on top: some providers list
/// hundreds of models.
fn model_picker(
    ui: &mut Ui,
    lang: &str,
    value: &mut String,
    (models, loading): (&[String], bool),
    hint: &str,
    name: &str,
) -> bool {
    let mut changed = false;
    let t = |key: &str| text::t(lang, key, &[]);
    ui.horizontal(|ui| {
        let edit = TextEdit::singleline(value).hint_text(hint).desired_width((ui.available_width() - 72.0).max(120.0));
        changed |= ui.add(edit).named_as(name).changed();
        let query_id = ui.scope_id().with(("models-search", hint));
        let seen_id = query_id.with("seen");
        let list = ComboBox::from_id_salt(("models", hint))
            .selected_text("")
            .width(24.0)
            // a click in the search line must not close the menu: choosing a model does, below
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                ui.set_min_width(320.0);
                if models.is_empty() {
                    ui.label(RichText::new(t(if loading { "models_loading" } else { "models_empty" })).color(muted()));
                    return;
                }
                let mut query: String = ui.data_mut(|data| data.get_temp(query_id)).unwrap_or_default();
                let first = ui.data_mut(|data| data.get_temp::<bool>(seen_id)).is_none();
                let search = ui
                    .add(TextEdit::singleline(&mut query).hint_text(t("search_models")).desired_width(f32::INFINITY))
                    .named_as(&t("search_models"));
                if first {
                    // the cursor is already in the search line when the menu opens
                    search.request_focus();
                    ui.data_mut(|data| data.insert_temp(seen_id, true));
                }
                let found = matching(models, &query);
                ui.label(RichText::new(format!("{} / {}", found.len(), models.len())).color(muted()).small());
                if found.is_empty() {
                    ui.label(RichText::new(t("models_none")).color(muted()));
                }
                ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for model in found {
                        if ui.selectable_label(value == model, model).clicked() {
                            *value = model.clone();
                            changed = true;
                            ui.close();
                        }
                    }
                });
                ui.data_mut(|data| data.insert_temp(query_id, query));
            });
        if list.inner.is_none() {
            // closed: the next time it opens it starts with a clear search
            ui.data_mut(|data| {
                data.remove::<String>(query_id);
                data.remove::<bool>(seen_id);
            });
        }
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
        // 720 x 640, or the window less a margin when that is smaller
        let room = ctx.content_rect().size();
        let size = egui::vec2(720.0_f32.min((room.x - 40.0).max(320.0)), 640.0_f32.min((room.y - 40.0).max(260.0)));
        egui::Window::new(t("settings"))
            .id(egui::Id::unique("settings"))
            .open(&mut open)
            .collapsible(false)
            // a fixed size, so its edges and corners are not handles: the pointer there does not turn into a resize arrow
            .resizable(false)
            .fixed_size(size)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (tab, key) in [
                        (Tab::Interview, "tab_interview"),
                        (Tab::Practice, "tab_practice"),
                        (Tab::Ai, "tab_ai"),
                        (Tab::General, "tab_general"),
                    ] {
                        let selected = self.settings_ui.tab == tab;
                        let name = t(key);
                        // a tab with room around its word: 16 on each side, 10 above and below
                        let galley = ui.painter().layout_no_wrap(
                            name.clone(),
                            semibold(14.0),
                            if selected { strong() } else { muted() },
                        );
                        let (rect, response) =
                            ui.allocate_exact_size(galley.size() + egui::vec2(32.0, 20.0), egui::Sense::click());
                        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, &name));
                        let color = if selected {
                            strong()
                        } else if response.hovered() {
                            text_color()
                        } else {
                            muted()
                        };
                        ui.painter().galley_with_override_text_color(
                            rect.center() - galley.size() / 2.0,
                            galley.clone(),
                            color,
                        );
                        if selected {
                            let y = rect.bottom() - 1.0;
                            ui.painter().line_segment(
                                [egui::pos2(rect.left() + 16.0, y), egui::pos2(rect.right() - 16.0, y)],
                                Stroke::new(2.0, accent()),
                            );
                        }
                        if response.clicked() {
                            self.settings_ui.tab = tab;
                        }
                    }
                });
                ui.add_space(4.0);
                ui.separator();
                COLUMN.with(|column| {
                    column.set(match self.settings_ui.tab {
                        Tab::Interview => 110.0,
                        Tab::Practice => 130.0,
                        Tab::Ai => 255.0,
                        Tab::General => 190.0,
                    });
                });
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
                .named_as(&t("interview"))
                .response
                .on_hover_text(t("interview_note"));
            new = ui.button(t("new")).clicked();
            if ui.button(t("rename")).clicked() {
                self.settings_ui.rename = Some(self.library.current().name.clone());
            }
            delete = ui.button(t("delete")).clicked();
        });
        if let Some(name) = self.settings_ui.rename.as_mut() {
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(name).desired_width(300.0)).named_as(&t("rename"));
                rename_done = ui.button("OK").clicked();
            });
        }
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
                if ui.add(egui::Button::new(RichText::new(t("remove")).color(muted()).small()).frame(false)).clicked() {
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
        ui.label(RichText::new(t("dossier")).color(muted())).on_hover_text(t("dossier_note"));
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
            if ui.button(t("project_forget")).clicked() {
                crate::project::forget(&self.paths.interview_dir(), std::path::Path::new(&folder));
                self.settings_ui.dossier_of.clear();
                self.library_changed();
            }
        });
    }

    // ---- Practice --------------------------------------------------------------------------------------------

    fn step(&self, ui: &mut Ui, number: usize, title: &str, done: bool, detail: &str, optional: bool) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        ui.add_space(10.0);
        // the line under a step sits right under it, and the tighter spacing stays inside the step
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 20.0;
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.horizontal(|ui| {
                super::theme::step_mark(ui, done);
                ui.label(RichText::new(format!("{number}. {title}")).font(semibold(14.0)).color(strong()));
                if optional {
                    ui.label(RichText::new(t("optional")).color(muted()).small());
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                ui.label(RichText::new(detail).color(if done { muted() } else { warn() }).small());
            });
        });
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
            // tall enough for every preset at once: the menu scrolls only past that
            let combo =
                ComboBox::from_id_salt("provider").selected_text(label).width(260.0).height(360.0).show_ui(ui, |ui| {
                    for provider in PROVIDERS {
                        if ui.selectable_label(known == Some(provider), provider.label).clicked()
                            && known != Some(provider)
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
                egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 4)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&account.name).font(semibold(14.0)).color(strong()));
                        ui.label(RichText::new(account.tail()).color(muted()).small());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t("remove")).clicked() {
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

        // a local gateway (Switchyard, Ollama, LM Studio) spreads the load over its accounts itself: these are for the
        // accounts saved here
        if known.is_none_or(|provider| provider.cloud) {
            let mut app_changed = false;
            hint_next(&t("parallel_note"));
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
            if app_changed {
                self.app_changed();
            }
        }

        section(ui, &t("models_title"));
        let models = self.settings_ui.models.clone();
        ui.label(RichText::new(t("model_ask")).font(semibold(14.0)).color(strong())).on_hover_text(t("model_ask_note"));
        let loading = self.settings_ui.models_loading;
        changed |= model_picker(ui, &lang, &mut self.settings.model, (&models, loading), "auto", &t("model_ask"));
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
        ui.label(RichText::new(t("model_screenshot")).font(semibold(14.0)).color(strong()))
            .on_hover_text(t("model_shot_note"));
        changed |=
            model_picker(ui, &lang, &mut self.settings.vision_model, (&models, loading), "", &t("model_screenshot"));
        ui.add_space(10.0);
        ui.label(RichText::new(t("model_audio")).font(semibold(14.0)).color(strong())).on_hover_text(t("audio_note"));
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

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button(t("load_models")).clicked() {
                self.load_models();
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
        self.models_follow_the_provider();
    }

    /// Asks the provider in use for its models, for the menus next to the model fields.
    fn load_models(&mut self) {
        self.settings_ui.models_error = None;
        self.settings_ui.models_loading = true;
        self.settings_ui.models_for = Some((self.settings.gateway(), self.settings.gateway_key.clone()));
        let (coach, tx, repaint) = (self.coach.clone(), self.tx.clone(), self.repaint.clone());
        self.handle.spawn(async move {
            let _ = tx.send(Event::Models(coach.models().await.map_err(|error| error.message)));
            repaint();
        });
    }

    /// A list of models belongs to one address and one key: when either changes (a provider was chosen, a key was
    /// saved or picked), the old list goes and the new one is asked for, so nobody has to press *Load models*. A cloud
    /// provider with no key yet is not asked: it would only answer "rejected". A failed ask is not repeated by itself,
    /// the button asks again.
    fn models_follow_the_provider(&mut self) {
        let state = &self.settings_ui;
        if !state.auto_models || state.models_loading {
            return;
        }
        let current = (self.settings.gateway(), self.settings.gateway_key.clone());
        if state.models_for.as_ref() == Some(&current) {
            return;
        }
        self.settings_ui.models.clear();
        self.settings_ui.models_error = None;
        let waits_for_a_key = current.1.trim().is_empty() && provider_for(&current.0).is_some_and(|p| p.cloud);
        if waits_for_a_key {
            self.settings_ui.models_for = None;
        } else {
            self.load_models();
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
        changed |= ui
            .checkbox(&mut self.settings.screenshot_enabled, t("screenshot_enabled"))
            .on_hover_text(t("screenshot_note"))
            .changed();
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
            if ui.checkbox(&mut tray, t("tray_icon")).on_hover_text(t("tray_note")).changed() {
                self.app.tray_icon = tray;
                self.app_changed();
            }
            ui.add_enabled_ui(self.app.tray_icon, |ui| {
                let mut hide = self.app.hide_from_taskbar;
                if ui.checkbox(&mut hide, t("hide_taskbar")).on_hover_text(t("taskbar_note")).changed() {
                    self.app.hide_from_taskbar = hide;
                    self.app_changed();
                }
            });
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
        if ui.checkbox(&mut check, t("check_updates")).on_hover_text(t("update_note")).changed() {
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
        let reset = ui.button(t("reset"));
        if reset.clicked() {
            self.settings_ui.confirm_reset = true;
        }
        if self.settings_ui.confirm_reset {
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
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
        changed |= ui.checkbox(&mut self.settings.listen, t("listen_call")).on_hover_text(t("hearing_note")).changed();
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
        changed
    }

    /// Theme, equalizer, icon and the window itself: everything about how it looks.
    fn appearance(&mut self, ui: &mut Ui) -> bool {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let mut changed = false;
        section(ui, &t("appearance"));

        // the language of the window comes first: whoever cannot read it needs it before anything else
        ui.label(RichText::new(t("window_language")).color(muted()));
        ui.horizontal_wrapped(|ui| {
            let current = self.settings.ui_language.clone();
            let mut pick = |ui: &mut Ui, code: &str, name: &str| {
                if ui.selectable_label(current == code, name).clicked() && current != code {
                    self.settings.ui_language = code.to_owned();
                    changed = true;
                }
            };
            pick(ui, "auto", &t("auto"));
            for (code, name) in crate::languages::UI_LANGUAGES {
                pick(ui, code, name);
            }
        });

        ui.add_space(10.0);
        ui.label(RichText::new(t("theme")).color(muted()));
        let mut chosen: Option<String> = None;
        let current = self.app.theme.clone();
        let system_dark = !matches!(ui.ctx().system_theme(), Some(egui::Theme::Light));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            for (name, panel, text_rgb, accent_rgb) in super::theme::PRESETS {
                if swatch(ui, &t(&format!("theme_{name}")), (panel, text_rgb, accent_rgb), None, current == name) {
                    chosen = Some(name.to_owned());
                }
            }
            let (dark, light) = (super::theme::PRESETS[0], super::theme::PRESETS[6]);
            if swatch(
                ui,
                &t("theme_system"),
                (dark.1, dark.2, dark.3),
                Some((light.1, light.2, light.3)),
                current == crate::look::SYSTEM_THEME,
            ) {
                chosen = Some(crate::look::SYSTEM_THEME.to_owned());
            }
            let own = crate::look::colors_for(&self.app, system_dark);
            let (custom_panel, custom_text) = (
                crate::look::parse_hex(&self.app.custom_panel).unwrap_or(own.0),
                crate::look::parse_hex(&self.app.custom_text).unwrap_or(own.1),
            );
            let custom_accent = crate::look::parse_hex(&self.app.custom_accent).unwrap_or(own.2);
            if swatch(ui, &t("theme_custom"), (custom_panel, custom_text, custom_accent), None, current == "custom") {
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

        // the accent, apart from the theme: any theme, in the colour of your choice
        ui.add_space(10.0);
        ui.label(RichText::new(t("accent_colour")).color(muted()));
        let mut accent_choice: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            let theme_own = crate::look::parse_hex(&self.app.accent).is_none();
            if ui.selectable_label(theme_own, t("accent_own")).clicked() {
                accent_choice = Some(String::new());
            }
            for (_, _, _, rgb) in super::theme::PRESETS {
                let hex = crate::look::hex(rgb);
                if accent_chip(ui, rgb, self.app.accent.eq_ignore_ascii_case(&hex)) {
                    accent_choice = Some(hex);
                }
            }
            let shown = crate::look::colors_for(&self.app, system_dark).2;
            let mut rgb = crate::look::parse_hex(&self.app.accent).unwrap_or(shown);
            if ui.color_edit_button_srgb(&mut rgb).named_as(&t("accent_colour")).changed() {
                accent_choice = Some(crate::look::hex(rgb));
            }
        });
        if let Some(choice) = accent_choice {
            self.app.accent = choice;
            self.app_changed();
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
        if ui.checkbox(&mut animate, t("animate")).on_hover_text(t("eq_note")).changed() {
            self.app.animate = animate;
            self.app_changed();
        }

        ui.add_space(8.0);
        ui.label(RichText::new(t("app_icon")).color(muted())).on_hover_text(t("icon_note"));
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

    /// Use the image at `path` as the icon, when it is a PNG, JPEG or ICO the window can read.
    fn use_icon(&mut self, path: &str) {
        match super::icon::load(path, accent()) {
            Ok(_) => {
                self.settings_ui.icon_error = None;
                self.settings_ui.icons.clear();
                self.settings_ui.icon_path = path.to_owned();
                self.app.app_icon = path.to_owned();
                self.app_changed();
            }
            Err(why) => self.settings_ui.icon_error = Some(why),
        }
    }

    fn icon_picker(&mut self, ui: &mut Ui) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        // a file dropped on the window is the icon
        let dropped = ui.ctx().input(|input| input.raw.dropped_files.first().map(|file| file.path().to_path_buf()));
        if let Some(path) = dropped {
            self.use_icon(&path.to_string_lossy());
        }
        let hovering = ui.ctx().input(|input| !input.raw.hovered_files.is_empty());
        let area_top = ui.cursor().top();

        let current = self.app.app_icon.clone();
        // what the tiles show: the icon that ships, its colours, and the person's own picture when there is one
        let mut tiles: Vec<(String, String)> =
            vec![(String::new(), t("icon_default")), ("theme".to_owned(), t("icon_theme"))];
        for (name, _) in super::icon::VARIANTS.iter().skip(1) {
            tiles.push((format!("builtin:{name}"), (*name).to_owned()));
        }
        let own = !(current.is_empty() || current == "theme" || super::icon::is_builtin(&current));
        if own {
            let name = std::path::Path::new(&current)
                .file_name()
                .map_or_else(|| t("icon_yours"), |name| name.to_string_lossy().into_owned());
            tiles.push((current.clone(), name));
        }
        // each picture is decoded and sent to the graphics card once, not every frame
        let accent_key = super::theme::hex_of(accent());
        for (setting, _) in &tiles {
            let key = format!("{setting}|{accent_key}");
            if let std::collections::hash_map::Entry::Vacant(slot) = self.settings_ui.icons.entry(key.clone()) {
                let icon = super::icon::load(setting, accent()).unwrap_or_default();
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
                slot.insert(ui.ctx().load_texture(format!("icon-{key}"), image, egui::TextureOptions::LINEAR));
            }
        }
        let mut chosen: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            for (setting, label) in &tiles {
                let selected = current == *setting;
                let texture = &self.settings_ui.icons[&format!("{setting}|{accent_key}")];
                ui.vertical(|ui| {
                    ui.set_max_width(82.0);
                    let response = ui
                        .add(
                            egui::Button::image(egui::Image::new(texture).fit_to_exact_size(egui::vec2(52.0, 52.0)))
                                .selected(selected),
                        )
                        .named_as(label);
                    ui.add(
                        egui::Label::new(RichText::new(label).small().color(if selected { accent() } else { muted() }))
                            .truncate(),
                    );
                    if response.clicked() {
                        chosen = Some(setting.clone());
                    }
                });
            }
        });
        if let Some(setting) = chosen {
            self.settings_ui.icon_error = None;
            self.app.app_icon = setting;
            self.app_changed();
        }
        ui.add_space(6.0);
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
                self.use_icon(&path);
            }
        });
        if let Some(why) = self.settings_ui.icon_error.clone() {
            ui.label(RichText::new(tf("icon_bad", &[("why", &why)])).color(danger()).small());
        }
        if hovering {
            // a file is being dragged over the window: say where it goes
            let area = egui::Rect::from_min_max(
                egui::pos2(ui.min_rect().left(), area_top),
                egui::pos2(ui.min_rect().right(), ui.cursor().top()),
            );
            ui.painter().rect_stroke(area.expand(4.0), 4.0, Stroke::new(2.0, accent()), egui::StrokeKind::Outside);
            ui.painter().text(
                area.center_top() + egui::vec2(0.0, 4.0),
                egui::Align2::CENTER_TOP,
                t("icon_drop"),
                super::theme::sans(13.0),
                accent(),
            );
        }
    }
}

/// A colour set to look at: (panel, text, accent).
type Look3 = ([u8; 3], [u8; 3], [u8; 3]);

fn rgb(value: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(value[0], value[1], value[2])
}

/// A theme as a small window of its own colours: a panel with two lines of text and a button in the accent, and the name
/// under it. `right` is a second look for the other half (the system theme is dark and light). True when it was clicked.
fn swatch(ui: &mut Ui, name: &str, left: Look3, right: Option<Look3>, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(92.0, 80.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    let card = egui::Rect::from_min_size(rect.min, egui::vec2(92.0, 56.0));
    let painter = ui.painter();
    let round = 6u8;
    let half = card.center().x;
    let draw = |painter: &egui::Painter, area: egui::Rect, look: Look3, corners: egui::CornerRadius| {
        painter.rect_filled(area, corners, rgb(look.0));
        let text = rgb(look.1);
        let x = area.left() + 10.0;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, area.top() + 11.0), egui::vec2(34.0, 5.0)),
            2.0,
            text.gamma_multiply(0.75),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, area.top() + 22.0), egui::vec2(22.0, 5.0)),
            2.0,
            text.gamma_multiply(0.38),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, area.bottom() - 17.0), egui::vec2(26.0, 9.0)),
            3.0,
            rgb(look.2),
        );
    };
    match right {
        None => draw(painter, card, left, egui::CornerRadius::same(round)),
        Some(other) => {
            let (l, r) = (
                egui::Rect::from_min_max(card.min, egui::pos2(half, card.max.y)),
                egui::Rect::from_min_max(egui::pos2(half, card.min.y), card.max),
            );
            let corners = |nw, ne, se, sw| egui::CornerRadius { nw, ne, se, sw };
            draw(painter, l, left, corners(round, 0, 0, round));
            draw(painter, r, other, corners(0, round, round, 0));
        }
    }
    let border = if selected {
        accent()
    } else if response.hovered() {
        muted()
    } else {
        super::theme::line()
    };
    painter.rect_stroke(
        card,
        round,
        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 9.0),
        egui::Align2::CENTER_CENTER,
        name,
        theme::sans(12.0),
        if selected { strong() } else { muted() },
    );
    response.clicked()
}

/// A colour to pick as the accent, as a dot; a ring says which one is in use. True when it was clicked.
fn accent_chip(ui: &mut Ui, color: [u8; 3], selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    let name = crate::look::hex(color);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &name));
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 10.0, rgb(color));
    if selected {
        painter.circle_stroke(rect.center(), 12.5, egui::Stroke::new(2.0, strong()));
    } else if response.hovered() {
        painter.circle_stroke(rect.center(), 12.5, egui::Stroke::new(1.0, muted()));
    }
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

#[cfg(test)]
mod tests {
    use super::matching;

    fn ids() -> Vec<String> {
        ["meta/llama-3.3-70b-instruct", "meta/llama-3.1-8b-instruct", "nvidia/nemotron-70b", "mistralai/Mixtral-8x7B"]
            .map(str::to_owned)
            .to_vec()
    }

    #[test]
    fn an_empty_search_finds_every_model_in_the_order_listed() {
        let models = ids();
        assert_eq!(matching(&models, "").len(), 4);
        assert_eq!(matching(&models, "   ").len(), 4);
        assert_eq!(matching(&models, "")[0], &models[0]);
    }

    #[test]
    fn every_word_must_be_in_the_id_whatever_the_case() {
        let models = ids();
        assert_eq!(matching(&models, "LLAMA 70b"), [&models[0]]);
        assert_eq!(matching(&models, "70b"), [&models[0], &models[2]]);
        assert_eq!(matching(&models, "mixtral"), [&models[3]]);
        assert!(matching(&models, "llama gemma").is_empty());
        assert_eq!(matching(&models, "instruct meta").len(), 2);
    }
}
