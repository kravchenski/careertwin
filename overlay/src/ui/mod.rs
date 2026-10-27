//! The window: the answer on top, a line to ask on, and Settings in a window of their own.

pub mod answer;
pub mod icon;
pub mod settings_ui;
pub mod text;
pub mod theme;
pub mod update_ui;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText, ScrollArea, Ui};
use tokio_util::sync::CancellationToken;

use crate::brief::InterviewBrief;
use crate::coach::{Coach, CoachConfig, Reply, Request};
use crate::export;
use crate::library::Library;
use crate::paths::Paths;
use crate::prepare::{Analysis, BankQuestion};
use crate::prompts::{self, SCREENSHOT_SYSTEM, SYSTEM, sections};
use crate::questions::{Kind, classify};
use crate::screenshot::{self, Shot};
use crate::settings::{AppSettings, Settings, load_all};
use crate::transcript::{INTERVIEWER, SCREENSHOT, TranscriptLog};
use crate::update::{self, Available, Release};

use theme::{ACCENT, DANGER, MUTED, ON_ACCENT, STRONG};

/// What the tasks that run off the window tell it.
pub enum Event {
    Chunk { id: u64, text: String },
    Finished { id: u64, result: Result<Reply, String> },
    Shot(Result<Shot, String>),
    Remote(String),
    Models(Result<Vec<String>, String>),
    KeyChecked { key: String, result: Result<bool, String> },
    Connected(Result<usize, String>),
    Bank(Result<Vec<BankQuestion>, String>),
    Review(Result<Analysis, String>),
    Latest { manual: bool, result: Result<Release, String> },
    Progress { done: u64, total: u64 },
    Downloaded(Result<PathBuf, String>),
    Exported(Result<Vec<PathBuf>, String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Typed,
    Screenshot,
}

pub struct Entry {
    pub id: u64,
    pub question: String,
    pub kind: Kind,
    pub source: Source,
    pub raw: String,
    pub done: bool,
    pub error: Option<String>,
    pub fell_back: Option<String>,
    pub first_ms: Option<u128>,
    pub started: Instant,
}

/// A screenshot being taken: the window is invisible for the moment of the capture.
struct Capturing {
    since: Instant,
    started: bool,
}

pub struct App {
    pub paths: Paths,
    pub settings: Settings,
    pub app: AppSettings,
    pub library: Library,
    pub brief: InterviewBrief,
    pub problems: Vec<String>,
    pub coach: Coach,
    pub handle: tokio::runtime::Handle,
    pub tx: Sender<Event>,
    rx: Receiver<Event>,
    pub repaint: Arc<dyn Fn() + Send + Sync>,
    pub entries: Vec<Entry>,
    pub shown: usize,
    pub input: String,
    pub details_open: bool,
    next_id: u64,
    running: Option<(u64, CancellationToken)>,
    pub log: TranscriptLog,
    pub notice: Option<(String, bool)>,
    capturing: Option<Capturing>,
    pub settings_open: bool,
    pub settings_ui: settings_ui::State,
    pub update_ui: update_ui::State,
    /// What the window was last told, so a change is applied once.
    applied: Applied,
}

#[derive(Default, PartialEq)]
struct Applied {
    icon: String,
    scale: i64,
    pinned: Option<bool>,
    frameless: Option<bool>,
}

impl App {
    pub fn new(paths: Paths, handle: tokio::runtime::Handle, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (settings, app) = load_all(&paths);
        let library = Library::load(&paths.context_file());
        let (brief, problems) = crate::library::build_brief(&paths.about, library.current());
        let (tx, rx) = channel();
        let log = TranscriptLog::new(paths.transcripts_dir(), &library.current().name);
        let mut this = Self {
            coach: Coach::new(CoachConfig::default()),
            paths,
            settings,
            app,
            library,
            brief,
            problems,
            handle,
            tx,
            rx,
            repaint,
            entries: Vec::new(),
            shown: 0,
            input: String::new(),
            details_open: false,
            next_id: 1,
            running: None,
            log,
            notice: None,
            capturing: None,
            settings_open: false,
            settings_ui: settings_ui::State::default(),
            update_ui: update_ui::State::default(),
            applied: Applied::default(),
        };
        this.apply_coach();
        this
    }

    pub fn t(&self, key: &str) -> String {
        text::t(&self.settings.ui_code(), key, &[])
    }

    pub fn tf(&self, key: &str, values: &[(&str, &str)]) -> String {
        text::t(&self.settings.ui_code(), key, values)
    }

    pub fn language(&self) -> Option<String> {
        crate::languages::normalize(Some(&self.settings.language))
    }

    /// Push the settings into the live coach.
    pub fn apply_coach(&mut self) {
        let url = self.settings.gateway_url.clone();
        let spare = crate::accounts::spares(&self.settings.accounts, &self.settings.gateway(), &self.settings.gateway_key);
        let non_empty = |text: &str| Some(text.to_owned()).filter(|text| !text.is_empty());
        self.coach.configure(CoachConfig {
            url,
            key: non_empty(&self.settings.gateway_key),
            spare_keys: spare,
            model: non_empty(&self.settings.model),
            vision_model: non_empty(&self.settings.vision_model),
            fallback_models: self.settings.fallbacks(),
        });
    }

    /// Save what changed and apply it.
    pub fn settings_changed(&mut self) {
        if let Err(error) = self.settings.save(&self.paths.settings_file()) {
            self.say(&format!("Could not save the settings: {error}"), true);
        }
        self.apply_coach();
    }

    pub fn app_changed(&mut self) {
        if let Err(error) = self.app.save(&self.paths.app_file()) {
            self.say(&format!("Could not save the settings: {error}"), true);
        }
    }

    pub fn library_changed(&mut self) {
        if let Err(error) = self.library.save(&self.paths.context_file()) {
            self.say(&format!("Could not save the interviews: {error}"), true);
        }
        let (brief, problems) = crate::library::build_brief(&self.paths.about, self.library.current());
        self.brief = brief;
        self.problems = problems;
        self.log.title = self.library.current().name.clone();
    }

    pub fn say(&mut self, text: &str, error: bool) {
        self.notice = Some((text.to_owned(), error));
    }

    pub fn busy(&self) -> bool {
        self.running.is_some() || self.capturing.is_some()
    }

    // ---- asking ----------------------------------------------------------------------------------------------

    pub fn ask(&mut self, question: &str) {
        let question = question.trim().to_owned();
        if question.is_empty() || self.busy() {
            return;
        }
        let kind = classify(&question);
        let history = self.log.history(6);
        let prompt = prompts::user_prompt(&self.brief, &question, kind, &history, self.language().as_deref(), &self.settings.answer_style, &self.settings.answer_layout, "", None);
        self.log.add(INTERVIEWER, &question);
        self.start(question, kind, Source::Typed, Request::text(SYSTEM, &prompt));
    }

    fn start(&mut self, question: String, kind: Kind, source: Source, request: Request) {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(Entry { id, question, kind, source, raw: String::new(), done: false, error: None, fell_back: None, first_ms: None, started: Instant::now() });
        self.shown = self.entries.len() - 1;
        self.details_open = self.settings.show_full_answer;
        let cancel = CancellationToken::new();
        self.running = Some((id, cancel.clone()));
        let (coach, tx, repaint) = (self.coach.clone(), self.tx.clone(), self.repaint.clone());
        self.handle.spawn(async move {
            let (chunks, wake) = (tx.clone(), repaint.clone());
            let result = coach
                .ask(&request, &cancel, |piece| {
                    let _ = chunks.send(Event::Chunk { id, text: piece.to_owned() });
                    wake();
                })
                .await
                .map_err(|error| error.message);
            let _ = tx.send(Event::Finished { id, result });
            repaint();
        });
    }

    pub fn stop(&mut self) {
        if let Some((_, cancel)) = &self.running {
            cancel.cancel();
        }
    }

    /// Answer what is on the screen: the window steps aside for the moment of the capture.
    pub fn start_screenshot(&mut self) {
        if self.busy() || !self.settings.screenshot_enabled {
            return;
        }
        self.capturing = Some(Capturing { since: Instant::now(), started: false });
        (self.repaint)();
    }

    fn capture_step(&mut self) {
        let Some(capturing) = self.capturing.as_mut() else { return };
        if capturing.started || capturing.since.elapsed() < Duration::from_millis(self.settings.screenshot_delay_ms.max(0) as u64) {
            return;
        }
        capturing.started = true;
        let (tool, tx, repaint) = (self.settings.screenshot_tool.clone(), self.tx.clone(), self.repaint.clone());
        let output = screenshot::focused_output();
        std::thread::spawn(move || {
            let result = screenshot::capture(&tool, output.as_deref()).map_err(|error| error.to_string());
            let _ = tx.send(Event::Shot(result));
            repaint();
        });
    }

    fn shot_taken(&mut self, result: Result<Shot, String>) {
        self.capturing = None;
        match result {
            Err(error) => self.say(&error, true),
            Ok(shot) => {
                let prompt = prompts::screenshot_prompt(&self.brief, self.language().as_deref(), &self.settings.answer_layout);
                self.log.add(INTERVIEWER, SCREENSHOT);
                self.start(SCREENSHOT.to_owned(), Kind::Technical, Source::Screenshot, Request::screenshot(SCREENSHOT_SYSTEM, &prompt, &shot.data_url()));
            }
        }
    }

    // ---- events ----------------------------------------------------------------------------------------------

    fn drain(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Chunk { id, text } => {
                    if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
                        if entry.raw.is_empty() {
                            entry.first_ms = Some(entry.started.elapsed().as_millis());
                        }
                        entry.raw.push_str(&text);
                    }
                }
                Event::Finished { id, result } => self.finished(id, result),
                Event::Shot(result) => self.shot_taken(result),
                Event::Remote(command) => match command.as_str() {
                    "screenshot" => self.start_screenshot(),
                    "ask" => {
                        let question = std::mem::take(&mut self.input);
                        self.ask(&question);
                    }
                    _ => {}
                },
                Event::Models(result) => self.settings_ui.models_loaded(result),
                Event::KeyChecked { key, result } => self.key_checked(&key, result),
                Event::Connected(result) => self.settings_ui.connected = Some(result),
                Event::Bank(result) => self.settings_ui.bank = Some(result),
                Event::Review(result) => self.settings_ui.review = Some(result),
                Event::Latest { manual, result } => self.latest(manual, result),
                Event::Progress { done, total } => self.update_ui.progress = Some((done, total)),
                Event::Downloaded(result) => {
                    self.update_ui.progress = None;
                    self.update_ui.downloaded = Some(result);
                }
                Event::Exported(result) => self.settings_ui.exported = Some(result),
            }
        }
    }

    fn finished(&mut self, id: u64, result: Result<Reply, String>) {
        if self.running.as_ref().is_some_and(|(running, _)| *running == id) {
            self.running = None;
        }
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else { return };
        entry.done = true;
        match result {
            Ok(reply) => {
                entry.fell_back = reply.fell_back;
                if entry.raw.trim().is_empty() {
                    entry.raw = reply.text;
                }
            }
            Err(error) if error == "cancelled" => {}
            Err(error) => entry.error = Some(error),
        }
        if entry.error.is_none() && !entry.raw.trim().is_empty() {
            let answer = prompts::parse_suggestion(&entry.raw).sections.answer;
            self.log.suggestions.push((entry.question.clone(), answer));
        }
        if self.settings.save_transcripts {
            let _ = self.log.save();
        }
    }

    fn key_checked(&mut self, key: &str, result: Result<bool, String>) {
        self.settings_ui.checking_key = false;
        match result {
            Err(error) => self.say(&error, true),
            Ok(verified) => match self.settings.add_account(key) {
                Ok(crate::settings::AccountChange::Added(account)) => {
                    self.settings_ui.new_key.clear();
                    self.settings_changed();
                    let message = self.tf(if verified { "saved_as" } else { "saved_unchecked" }, &[("name", &account.name)]);
                    self.say(&message, false);
                }
                Ok(_) => {}
                Err(error) => self.say(&error.to_string(), true),
            },
        }
    }

    // ---- updates ---------------------------------------------------------------------------------------------

    pub fn check_for_update(&mut self, manual: bool) {
        let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
        self.update_ui.checking = true;
        self.handle.spawn(async move {
            let result = update::latest(update::API).await.map_err(|error| error.to_string());
            let _ = tx.send(Event::Latest { manual, result });
            repaint();
        });
    }

    fn latest(&mut self, manual: bool, result: Result<Release, String>) {
        self.update_ui.checking = false;
        self.app.last_update_check = chrono::Utc::now().timestamp();
        self.app_changed();
        match result {
            Err(error) => {
                if manual {
                    self.update_ui.last = Some(Err(error));
                }
            }
            Ok(release) => {
                let current = env!("CARGO_PKG_VERSION");
                match update::decide(current, release, if manual { "" } else { &self.app.skipped_version }) {
                    Some(available) => {
                        self.update_ui.available = Some(available);
                        self.update_ui.dismissed = false;
                        self.update_ui.last = None;
                    }
                    None => self.update_ui.last = manual.then(|| Ok(current.to_owned())),
                }
            }
        }
    }

    pub fn download_update(&mut self, available: &Available) {
        let (release, tx, repaint) = (available.release.clone(), self.tx.clone(), self.repaint.clone());
        self.update_ui.downloaded = None;
        self.update_ui.progress = Some((0, 0));
        self.handle.spawn(async move {
            let progress = {
                let (tx, repaint) = (tx.clone(), repaint.clone());
                move |done, total| {
                    let _ = tx.send(Event::Progress { done, total });
                    repaint();
                }
            };
            let result = update::download(&release, update::System::current(), &update::downloads_dir(), false, progress).await.map_err(|error| error.to_string());
            let _ = tx.send(Event::Downloaded(result));
            repaint();
        });
    }

    // ---- export ----------------------------------------------------------------------------------------------

    pub fn export_questions(&mut self, formats: Vec<export::Format>) {
        let (from, into, tx, repaint) = (self.paths.transcripts_dir(), self.paths.exports_dir(), self.tx.clone(), self.repaint.clone());
        self.settings_ui.exported = None;
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M").to_string();
        self.handle.spawn_blocking(move || {
            let result = export::export_questions(&from, &into, &formats, &stamp).map_err(|error| error.to_string());
            let _ = tx.send(Event::Exported(result));
            repaint();
        });
    }

    // ---- the window itself -----------------------------------------------------------------------------------

    fn apply_window(&mut self, ctx: &egui::Context) {
        if self.applied.icon != self.app.app_icon || self.applied.icon.is_empty() && self.applied == Applied::default() {
            self.applied.icon = self.app.app_icon.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(icon::shared(&self.app.app_icon))));
        }
        if self.applied.scale != self.settings.font_scale {
            self.applied.scale = self.settings.font_scale;
            theme::apply(ctx, self.settings.font_scale as f32 / 100.0);
        }
        if self.applied.pinned != Some(self.settings.pinned) {
            self.applied.pinned = Some(self.settings.pinned);
            let level = if self.settings.pinned { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        }
        if self.applied.frameless != Some(self.settings.frameless) {
            self.applied.frameless = Some(self.settings.frameless);
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.settings.frameless));
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let busy = self.running.is_some();
        let bar = ui.horizontal(|ui| {
            let status = if self.capturing.is_some() {
                self.t("looking")
            } else if busy {
                self.t("thinking")
            } else {
                self.t("ready")
            };
            let dot = if busy || self.capturing.is_some() { ACCENT } else { theme::GOOD };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, dot);
            ui.label(RichText::new(status).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(self.t("settings")).color(MUTED)).frame(false)).clicked() {
                    self.settings_open = !self.settings_open;
                }
                let shot = egui::Button::new(self.t("screenshot")).min_size(egui::vec2(0.0, 34.0));
                if ui.add_enabled(self.settings.screenshot_enabled && !self.busy(), shot).on_hover_text("Ctrl+Shift+Enter").clicked() {
                    self.start_screenshot();
                }
            });
        });
        // the empty part of the bar drags the window, as the frame would
        let drag = ui.interact(bar.response.rect, ui.scope_id().with("drag"), egui::Sense::click_and_drag());
        if drag.drag_started() && self.settings.frameless {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    fn question_row(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let ask_width = 92.0;
            let hint = self.t("question_hint");
            let idle = !self.busy();
            let edit = egui::TextEdit::singleline(&mut self.input).hint_text(hint).desired_width(ui.available_width() - ask_width - 12.0).margin(egui::vec2(10.0, 8.0));
            let response = ui.add_enabled(idle, edit);
            let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if self.running.is_some() {
                if ui.add(egui::Button::new(self.t("stop")).min_size(egui::vec2(ask_width, 34.0))).clicked() {
                    self.stop();
                }
            } else {
                let ask = egui::Button::new(RichText::new(self.t("ask")).color(ON_ACCENT)).fill(ACCENT).min_size(egui::vec2(ask_width, 34.0));
                let can = !self.input.trim().is_empty() && !self.busy();
                if (ui.add_enabled(can, ask).clicked() || (enter && can)) && can {
                    let question = std::mem::take(&mut self.input);
                    self.ask(&question);
                    response.request_focus();
                }
            }
        });
    }

    fn answer_area(&mut self, ui: &mut Ui) {
        let size = 15.0 * self.settings.font_scale as f32 / 100.0;
        let main_only = self.settings.answer_layout == "main";
        if self.entries.is_empty() {
            ui.add_space(14.0);
            ui.label(RichText::new(self.t("empty_title")).font(theme::semibold(size + 3.0)).color(STRONG));
            ui.label(RichText::new(self.t("empty_body")).color(MUTED));
            if self.brief.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(self.t("no_context")).color(MUTED));
            }
            return;
        }
        let index = self.shown.min(self.entries.len() - 1);
        let translate = |key: &str| text::t(&self.settings.ui_code(), key, &[]);
        let mut details_open = self.details_open;
        ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(self.running.is_some()).show(ui, |ui| {
            let entry = &self.entries[index];
            let title = if entry.source == Source::Screenshot { translate("screenshot") } else { entry.question.clone() };
            ui.label(RichText::new(title).font(theme::semibold(size)).color(MUTED));
            ui.add_space(4.0);
            if let Some(error) = &entry.error {
                ui.label(RichText::new(error).color(DANGER));
            } else if entry.raw.is_empty() {
                ui.label(RichText::new(if entry.done { "—" } else { "…" }).color(MUTED));
            } else {
                answer::show(ui, &sections(&entry.raw), main_only, &mut details_open, size, &translate);
            }
        });
        self.details_open = details_open;
    }

    fn footer(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let sources = self.brief.sources().into_iter().map(|(_, label)| label).collect::<Vec<_>>().join(" · ");
            let name = self.library.current().name.clone();
            let line = if sources.is_empty() { name } else { format!("{name} — {sources}") };
            let last = self.entries.len().saturating_sub(1);
            let shown = self.shown.min(last);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.entries.len() > 1 {
                    if ui.add_enabled(shown < last, egui::Button::new("›").frame(false)).clicked() {
                        self.shown = shown + 1;
                    }
                    ui.label(RichText::new(self.tf("history", &[("n", &(shown + 1).to_string()), ("total", &self.entries.len().to_string())])).color(MUTED).small());
                    if ui.add_enabled(shown > 0, egui::Button::new("‹").frame(false)).clicked() {
                        self.shown = shown - 1;
                    }
                }
                if let Some(entry) = self.entries.get(shown) {
                    if !entry.raw.is_empty() && ui.add(egui::Button::new(RichText::new(self.t("copy")).color(MUTED).small()).frame(false)).clicked() {
                        let parts = sections(&entry.raw);
                        let text = if parts.answer.is_empty() { parts.points.join("\n").replace("**", "") } else { parts.answer };
                        ui.ctx().copy_text(text);
                    }
                    if let Some(model) = &entry.fell_back {
                        ui.label(RichText::new(self.tf("answered_by", &[("model", model)])).color(MUTED).small());
                    }
                }
                // what is left to the left of those: the interview and what the answers know, cut to fit
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(line).color(MUTED).small()).truncate());
                });
            });
        });
        if let Some((message, error)) = self.notice.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(message).color(if error { DANGER } else { ACCENT }).small());
                if ui.add(egui::Button::new(RichText::new("×").color(MUTED)).frame(false)).clicked() {
                    self.notice = None;
                }
            });
        }
    }

    /// Drawn every frame by the shell.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.drain();
        self.capture_step();
        self.apply_window(&ctx);
        if self.capturing.is_some() {
            ctx.request_repaint_after(Duration::from_millis(10));
        }
        let hidden = self.capturing.is_some();
        let opacity = if hidden { 0.0 } else { self.settings.opacity as f32 / 100.0 };
        if !hidden {
            egui::CentralPanel::default().frame(theme::panel_frame(opacity)).show(ui, |ui| {
                self.header(ui);
                ui.add_space(4.0);
                self.question_row(ui);
                ui.add_space(6.0);
                ui.separator();
                let footer_height = 28.0;
                let height = (ui.available_height() - footer_height).max(40.0);
                ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| self.answer_area(ui));
                self.footer(ui);
            });
            self.resize_grip(&ctx);
        }
        self.settings_window(&ctx);
        self.update_window(&ctx);
        if self.update_ui.available.is_none() && !self.update_ui.requested && update::due(&self.app, chrono::Utc::now().timestamp()) {
            self.update_ui.requested = true;
            self.check_for_update(false);
        }
        if self.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }

    /// A corner to pull when the window has no frame.
    fn resize_grip(&self, ctx: &egui::Context) {
        if !self.settings.frameless {
            return;
        }
        let rect = ctx.content_rect();
        let grip = egui::Rect::from_min_size(rect.right_bottom() - egui::vec2(18.0, 18.0), egui::vec2(18.0, 18.0));
        egui::Area::new(egui::Id::unique("grip")).fixed_pos(grip.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let response = ui.allocate_rect(grip, egui::Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            theme::grip(ui.painter(), grip.right_bottom());
            if response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(egui::viewport::ResizeDirection::SouthEast));
            }
        });
    }
}

/// The options the native window opens with.
pub fn window_options(settings: &Settings, app: &AppSettings) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("CareerTwin · Interview")
        .with_app_id("careertwin-interview")
        .with_icon(icon::shared(&app.app_icon))
        .with_inner_size([900.0, 420.0])
        .with_min_inner_size([560.0, 280.0])
        .with_transparent(true)
        .with_decorations(!settings.frameless);
    if settings.pinned {
        viewport = viewport.with_always_on_top();
    }
    eframe::NativeOptions { viewport, ..Default::default() }
}
