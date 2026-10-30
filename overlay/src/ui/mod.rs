//! The window: the answer on top, a line to ask on, and Settings in a window of their own.

pub mod answer;
pub mod equalizer;
pub mod icon;
pub mod settings_ui;
pub mod text;
pub mod theme;
pub mod update_ui;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText, ScrollArea, Ui};

use crate::brief::{InterviewBrief, ProjectDossier};
use crate::coach::{Coach, CoachConfig, Request};
use crate::export;
use crate::hearing;
use crate::library::Library;
use crate::paths::Paths;
use crate::prepare::{Analysis, BankQuestion};
use crate::project::{self, ProjectContext};
use crate::prompts::sections;
use crate::questions::{Kind, classify};
use crate::screenshot::{self, Shot};
use crate::session::{self, CoachModel, Context, Retrieve, Session, State, Timing};
use crate::settings::{AppSettings, Settings, load_all};
use crate::transcript::{SCREENSHOT, TranscriptLog, YOU};
use crate::update::{self, Available, Release};

use theme::{accent, danger, muted, on_accent, strong};

/// What the tasks that run off the window tell it.
pub enum Event {
    /// What the interview session says: a question heard, the words of an answer, the level of the call.
    Heard(session::Event),
    Shot(Result<Shot, String>),
    Project(Result<Box<ProjectContext>, String>),
    ProjectStage(String),
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
    /// A question heard on the call.
    Heard,
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
    /// When the key was pressed, so the answer can say how long the whole thing took.
    pressed: Instant,
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
    pub session: Session,
    context: Arc<Mutex<Context>>,
    /// What the session last said it was doing.
    pub state: State,
    /// The last thing it told about itself, shown under the header: loading the model, the output followed.
    pub status: String,
    /// The words still being said, or the last thing heard.
    pub caption: String,
    /// The language the call is in, once it is known.
    pub heard_language: Option<String>,
    /// A question typed here is on its way: its answer is `Typed`, not `Heard`.
    typed: bool,
    pub project: Option<Arc<ProjectContext>>,
    /// The step of reading the project folder, while that runs.
    pub analyzing: Option<String>,
    /// Listening was started once; later changes switch the sources and the recogniser in place.
    hearing_on: bool,
    /// The listening settings the session runs with, to tell when they changed.
    hearing_applied: String,
    pub notice: Option<(String, bool)>,
    capturing: Option<Capturing>,
    pub settings_open: bool,
    pub settings_ui: settings_ui::State,
    pub update_ui: update_ui::State,
    pub level: equalizer::Level,
    /// The time of the frame being drawn, in seconds: what the audio level is stamped with.
    clock: f64,
    /// What the window was last told, so a change is applied once.
    applied: Applied,
}

#[derive(Default, PartialEq)]
struct Applied {
    icon: String,
    palette: Option<theme::Palette>,
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
        let coach = Coach::new(CoachConfig::default());
        let context = Arc::new(Mutex::new(Context {
            brief: brief.clone(),
            style: String::new(),
            layout: String::new(),
            retrieve: None,
        }));
        let (events, heard) = channel();
        let session = Session::new(
            context.clone(),
            Arc::new(CoachModel { coach: coach.clone(), handle: handle.clone() }),
            events,
            log,
            crate::languages::normalize(Some(&settings.language)).as_deref(),
            Timing::default(),
        );
        {
            // the session works on its own threads: what it says is carried to the window, which is woken
            let (tx, repaint) = (tx.clone(), repaint.clone());
            std::thread::spawn(move || {
                for event in heard {
                    if tx.send(Event::Heard(event)).is_err() {
                        break;
                    }
                    repaint();
                }
            });
        }
        let mut this = Self {
            coach,
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
            session,
            context,
            state: State::Idle,
            status: String::new(),
            caption: String::new(),
            heard_language: None,
            typed: false,
            project: None,
            analyzing: None,
            hearing_on: false,
            hearing_applied: String::new(),
            notice: None,
            capturing: None,
            settings_open: false,
            settings_ui: settings_ui::State::default(),
            update_ui: update_ui::State::default(),
            level: equalizer::Level::default(),
            clock: 0.0,
            applied: Applied::default(),
        };
        this.apply_coach();
        this.load_project();
        this.sync_context();
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
        let spare =
            crate::accounts::spares(&self.settings.accounts, &self.settings.gateway(), &self.settings.gateway_key);
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
        self.sync_context();
        if self.hearing_on {
            let language = crate::languages::normalize(Some(&self.settings.language));
            if language != self.session.language() {
                self.session.set_language(language.as_deref());
            }
            if self.hearing_key() != self.hearing_applied {
                self.start_hearing();
            }
        }
    }

    /// The settings that decide what is listened to and with what.
    fn hearing_key(&self) -> String {
        let s = &self.settings;
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            s.listen, s.call_source, s.use_mic, s.mic_source, s.stt_engine, s.whisper_model, s.deepgram_key, s.deepgram_model
        )
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
        let title = self.library.current().name.clone();
        self.session.with_log(|log| log.title = title);
        self.load_project();
        self.sync_context();
    }

    pub fn say(&mut self, text: &str, error: bool) {
        self.notice = Some((text.to_owned(), error));
    }

    pub fn busy(&self) -> bool {
        self.thinking() || self.capturing.is_some()
    }

    /// An answer is being written.
    pub fn thinking(&self) -> bool {
        self.state == State::Thinking
    }

    /// What a request is made from: the interview, the style and the project, copied for the session's threads.
    pub fn sync_context(&mut self) {
        let retrieve: Option<Retrieve> = self.project.clone().map(|project| -> Retrieve { Arc::new(move |question: &str| project.find(question)) });
        if let Ok(mut context) = self.context.lock() {
            context.brief = self.brief.clone();
            context.style = self.settings.answer_style.clone();
            context.layout = self.settings.answer_layout.clone();
            context.retrieve = retrieve;
        }
    }

    /// The project of the open interview, read back from where it was analysed.
    pub fn load_project(&mut self) {
        let folder = self.library.current().project.trim().to_owned();
        self.project = (!folder.is_empty())
            .then(|| project::load(&self.paths.interview_dir(), std::path::Path::new(&folder)))
            .flatten()
            .map(Arc::new);
        self.brief.project = self
            .project
            .as_ref()
            .map(|project| ProjectDossier { name: project.name.clone(), dossier: project.dossier.clone() });
    }

    // ---- asking ----------------------------------------------------------------------------------------------

    pub fn ask(&mut self, question: &str) {
        let question = question.trim().to_owned();
        if question.is_empty() || self.busy() {
            return;
        }
        self.typed = true;
        self.session.ask(&question);
    }

    pub fn stop(&mut self) {
        self.session.cancel();
        if let Some(entry) = self.entries.last_mut().filter(|entry| !entry.done) {
            entry.done = true;
        }
    }

    /// Answer what is on the screen: the window steps aside for the moment of the capture.
    pub fn start_screenshot(&mut self) {
        if self.busy() || !self.settings.screenshot_enabled {
            return;
        }
        let now = Instant::now();
        self.capturing = Some(Capturing { since: now, started: false, pressed: now });
        (self.repaint)();
    }

    fn capture_step(&mut self) {
        let Some(capturing) = self.capturing.as_mut() else { return };
        if capturing.started
            || capturing.since.elapsed() < Duration::from_millis(self.settings.screenshot_delay_ms.max(0) as u64)
        {
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
        let pressed = self.capturing.take().map(|capturing| capturing.pressed);
        match result {
            Err(error) => self.say(&error, true),
            Ok(shot) => self.session.ask_screenshot(&shot.data_url(), pressed),
        }
    }

    // ---- hearing ---------------------------------------------------------------------------------------------

    /// Begin listening to what the settings say, with the recogniser they name. Called once when the window opens;
    /// after that a change of the settings switches the sources and the recogniser in place.
    pub fn start_hearing(&mut self) {
        self.hearing_applied = self.hearing_key();
        self.caption.clear();
        let first = !std::mem::replace(&mut self.hearing_on, true);
        let sources = if self.settings.listen || self.settings.use_mic {
            let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
            hearing::sources(&self.settings, move |name| {
                let _ = tx.send(Event::Heard(session::Event::Source(name.to_owned())));
                repaint();
            })
        } else {
            Vec::new()
        };
        if sources.is_empty() {
            if first {
                self.session.start(None, Vec::new());
            } else {
                self.session.set_sources(Vec::new());
            }
            return;
        }
        // the recogniser may need a download first: it says how far it is through the same channel
        let (events, forward) = channel();
        {
            let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
            std::thread::spawn(move || {
                for event in forward {
                    if tx.send(Event::Heard(event)).is_err() {
                        break;
                    }
                    repaint();
                }
            });
        }
        match hearing::transcriber(&self.settings, &self.paths, &self.handle, &events) {
            Ok(transcriber) if first => self.session.start(Some(transcriber), sources),
            Ok(transcriber) => {
                self.session.set_transcriber(Some(transcriber), false);
                self.session.set_sources(sources);
            }
            Err(error) => {
                self.state = State::Error;
                self.say(&error, true);
            }
        }
    }

    /// Stop the threads that listen and answer; the window is closing.
    pub fn shutdown(&mut self) {
        self.session.stop();
    }

    pub fn pause_hearing(&mut self, paused: bool) {
        self.session.pause(paused);
    }

    /// What a session event changes in the window.
    fn heard(&mut self, event: session::Event) {
        use session::Event as E;
        match event {
            E::Turn { speaker, text } => {
                if text != SCREENSHOT {
                    self.caption = if speaker == YOU { format!("{} {text}", self.t("you_said")) } else { text };
                }
            }
            E::SuggestionStart { question } => {
                let source = if question == SCREENSHOT {
                    Source::Screenshot
                } else if std::mem::take(&mut self.typed) {
                    Source::Typed
                } else {
                    Source::Heard
                };
                let kind = if source == Source::Screenshot { Kind::Technical } else { classify(&question) };
                let id = self.next_id;
                self.next_id += 1;
                self.entries.push(Entry {
                    id,
                    question,
                    kind,
                    source,
                    raw: String::new(),
                    done: false,
                    error: None,
                    fell_back: None,
                    first_ms: None,
                    started: Instant::now(),
                });
                self.shown = self.entries.len() - 1;
                self.details_open = self.settings.show_full_answer;
                self.caption.clear();
            }
            E::Chunk(text) => {
                if let Some(entry) = self.entries.last_mut().filter(|entry| !entry.done) {
                    if entry.raw.is_empty() {
                        entry.first_ms = Some(entry.started.elapsed().as_millis());
                    }
                    entry.raw.push_str(&text);
                }
            }
            E::Done(text) => {
                if let Some(entry) = self.entries.last_mut().filter(|entry| !entry.done) {
                    entry.done = true;
                    entry.raw = text;
                }
                if self.settings.save_transcripts {
                    let _ = self.session.with_log(|log| log.save());
                }
            }
            E::Discard => {
                if self.entries.last().is_some_and(|entry| !entry.done || entry.source == Source::Heard) {
                    self.entries.pop();
                    self.shown = self.shown.min(self.entries.len().saturating_sub(1));
                }
            }
            E::Status(text) => {
                const FELL_BACK: &str = "The first model is rate limited or down: answered by ";
                if let Some(model) = text.strip_prefix(FELL_BACK) {
                    if let Some(entry) = self.entries.last_mut() {
                        entry.fell_back = Some(model.to_owned());
                    }
                } else {
                    self.status = text;
                }
            }
            E::State(state) => {
                self.state = state;
                if state != State::Thinking {
                    self.caption.clear();
                }
            }
            E::Level { speaker: _, level } => self.level.push(level, self.clock),
            E::Language(code) => self.heard_language = Some(code),
            E::Source(name) => self.status = self.tf("listening_to", &[("name", &name)]),
            E::Partial(text) => self.caption = text,
            E::Error(text) => match self.entries.last_mut().filter(|entry| !entry.done) {
                Some(entry) => {
                    entry.done = true;
                    entry.error = Some(text);
                }
                None => self.say(&text, true),
            },
        }
    }

    // ---- the project -----------------------------------------------------------------------------------------

    /// Read the folder of the open interview's project and have the AI write its dossier.
    pub fn analyze_project(&mut self) {
        let folder = self.library.current().project.trim().to_owned();
        if folder.is_empty() || self.analyzing.is_some() {
            return;
        }
        self.analyzing = Some("read".to_owned());
        let (coach, tx, repaint, handle) = (self.coach.clone(), self.tx.clone(), self.repaint.clone(), self.handle.clone());
        let (dir, gateway) = (self.paths.interview_dir(), self.settings.gateway());
        std::thread::spawn(move || {
            let stages = (tx.clone(), repaint.clone());
            let complete = |system: &str, prompt: &str, tokens: u32| {
                let mut request = Request::text(system, prompt);
                request.max_tokens = tokens;
                handle.block_on(coach.complete(&request)).map_err(|error| error.message)
            };
            let result = project::analyze(&dir, std::path::Path::new(&folder), &gateway, &complete, |stage, files| {
                let _ = stages.0.send(Event::ProjectStage(if files > 0 { format!("{stage} {files}") } else { stage.to_owned() }));
                (stages.1)();
            })
            .map(Box::new)
            .map_err(|error| error.to_string());
            let _ = tx.send(Event::Project(result));
            repaint();
        });
    }

    fn project_ready(&mut self, result: Result<Box<ProjectContext>, String>) {
        self.analyzing = None;
        match result {
            Err(error) => self.say(&error, true),
            Ok(project) => {
                self.project = Some(Arc::new(*project));
                self.brief.project = self
                    .project
                    .as_ref()
                    .map(|project| ProjectDossier { name: project.name.clone(), dossier: project.dossier.clone() });
                self.sync_context();
                let name = self.project.as_ref().map(|project| project.name.clone()).unwrap_or_default();
                let message = self.tf("project_ready", &[("name", &name)]);
                self.say(&message, false);
            }
        }
    }

    // ---- events ----------------------------------------------------------------------------------------------

    fn drain(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Heard(event) => self.heard(event),
                Event::Shot(result) => self.shot_taken(result),
                Event::Project(result) => self.project_ready(result),
                Event::ProjectStage(stage) => self.analyzing = Some(stage),
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

    fn key_checked(&mut self, key: &str, result: Result<bool, String>) {
        self.settings_ui.checking_key = false;
        match result {
            Err(error) => self.say(&error, true),
            Ok(verified) => match self.settings.add_account(key) {
                Ok(crate::settings::AccountChange::Added(account)) => {
                    self.settings_ui.new_key.clear();
                    self.settings_changed();
                    let message =
                        self.tf(if verified { "saved_as" } else { "saved_unchecked" }, &[("name", &account.name)]);
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
            let result =
                update::download(&release, update::System::current(), &update::downloads_dir(), false, progress)
                    .await
                    .map_err(|error| error.to_string());
            let _ = tx.send(Event::Downloaded(result));
            repaint();
        });
    }

    // ---- export ----------------------------------------------------------------------------------------------

    pub fn export_questions(&mut self, formats: Vec<export::Format>) {
        let (from, into, tx, repaint) =
            (self.paths.transcripts_dir(), self.paths.exports_dir(), self.tx.clone(), self.repaint.clone());
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
        let palette = theme::from_settings(&self.app);
        let palette_changed = self.applied.palette != Some(palette);
        if palette_changed {
            self.applied.palette = Some(palette);
            theme::set_palette(palette);
        }
        if palette_changed || self.applied.scale != self.settings.font_scale {
            self.applied.scale = self.settings.font_scale;
            theme::apply(ctx, self.settings.font_scale as f32 / 100.0);
        }
        let icon_key = format!(
            "{}|{}",
            self.app.app_icon,
            if self.app.app_icon == "theme" { theme::hex_of(palette.accent) } else { String::new() }
        );
        if self.applied.icon != icon_key {
            self.applied.icon = icon_key;
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(icon::shared(&self.app.app_icon, palette.accent))));
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

    /// What the dots show: an error, the model at work, the call being heard, or nothing.
    pub fn mood(&self) -> equalizer::Mood {
        if self.thinking() || self.capturing.is_some() {
            equalizer::Mood::Thinking
        } else if self.state == State::Error || self.entries.last().is_some_and(|entry| entry.error.is_some()) {
            equalizer::Mood::Error
        } else if self.level.value() > 0.02 {
            equalizer::Mood::Listening
        } else {
            equalizer::Mood::Idle
        }
    }

    fn equalizer_dots(&self) -> bool {
        matches!(self.app.equalizer.as_str(), "both" | "dots")
    }

    fn equalizer_background(&self) -> bool {
        matches!(self.app.equalizer.as_str(), "both" | "background")
    }

    fn header(&mut self, ui: &mut Ui) {
        let busy = self.thinking();
        let bar = ui.horizontal(|ui| {
            let status = if self.capturing.is_some() {
                self.t("looking")
            } else if busy {
                self.t("thinking")
            } else {
                self.t("ready")
            };
            if self.equalizer_dots() {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(86.0, 26.0), egui::Sense::hover());
                let time = ui.input(|input| input.time);
                ui.painter().rect_filled(rect, 13.0, theme::mix(theme::panel(), theme::text(), 0.09));
                equalizer::dots(&ui.painter().with_clip_rect(rect), rect, self.level.value(), self.mood(), time);
            } else {
                let dot = if busy || self.capturing.is_some() { accent() } else { theme::good() };
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, dot);
            }
            ui.label(RichText::new(status).color(muted()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(self.t("settings")).color(muted())).frame(false)).clicked() {
                    self.settings_open = !self.settings_open;
                }
                let shot = egui::Button::new(self.t("screenshot")).min_size(egui::vec2(0.0, 34.0));
                if ui
                    .add_enabled(self.settings.screenshot_enabled && !self.busy(), shot)
                    .on_hover_text("Ctrl+Shift+Enter")
                    .clicked()
                {
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

    /// One line about what the window hears: the words being said, or what it is doing about the sound.
    fn caption_line(&self, ui: &mut Ui) {
        let (text, heard) = if self.caption.is_empty() { (self.status.clone(), false) } else { (self.caption.clone(), true) };
        if text.is_empty() || !(self.settings.listen || self.settings.use_mic) {
            return;
        }
        ui.horizontal(|ui| {
            if heard {
                ui.label(RichText::new(self.t("heard")).color(accent()).small());
            }
            ui.add(egui::Label::new(RichText::new(text).color(muted()).small()).truncate());
        });
    }

    fn question_row(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let ask_width = 92.0;
            let hint = self.t("question_hint");
            let idle = !self.busy();
            let edit = egui::TextEdit::singleline(&mut self.input)
                .hint_text(hint)
                .desired_width(ui.available_width() - ask_width - 12.0)
                .margin(egui::vec2(10.0, 8.0));
            let response = ui.add_enabled(idle, edit);
            let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if self.thinking() {
                if ui.add(egui::Button::new(self.t("stop")).min_size(egui::vec2(ask_width, 34.0))).clicked() {
                    self.stop();
                }
            } else {
                let ask = egui::Button::new(RichText::new(self.t("ask")).color(on_accent()))
                    .fill(accent())
                    .min_size(egui::vec2(ask_width, 34.0));
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
            ui.label(RichText::new(self.t("empty_title")).font(theme::semibold(size + 3.0)).color(strong()));
            ui.label(RichText::new(self.t("empty_body")).color(muted()));
            if self.brief.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(self.t("no_context")).color(muted()));
            }
            return;
        }
        let index = self.shown.min(self.entries.len() - 1);
        let translate = |key: &str| text::t(&self.settings.ui_code(), key, &[]);
        let mut details_open = self.details_open;
        ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(self.thinking()).show(ui, |ui| {
            let entry = &self.entries[index];
            let title =
                if entry.source == Source::Screenshot { translate("screenshot") } else { entry.question.clone() };
            ui.label(RichText::new(title).font(theme::semibold(size)).color(muted()));
            ui.add_space(4.0);
            if let Some(error) = &entry.error {
                ui.label(RichText::new(error).color(danger()));
            } else if entry.raw.is_empty() {
                ui.label(RichText::new(if entry.done { "—" } else { "…" }).color(muted()));
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
                    ui.label(
                        RichText::new(self.tf(
                            "history",
                            &[("n", &(shown + 1).to_string()), ("total", &self.entries.len().to_string())],
                        ))
                        .color(muted())
                        .small(),
                    );
                    if ui.add_enabled(shown > 0, egui::Button::new("‹").frame(false)).clicked() {
                        self.shown = shown - 1;
                    }
                }
                if let Some(entry) = self.entries.get(shown) {
                    if !entry.raw.is_empty()
                        && ui
                            .add(egui::Button::new(RichText::new(self.t("copy")).color(muted()).small()).frame(false))
                            .clicked()
                    {
                        let parts = sections(&entry.raw);
                        let text = if parts.answer.is_empty() {
                            parts.points.join("\n").replace("**", "")
                        } else {
                            parts.answer
                        };
                        ui.ctx().copy_text(text);
                    }
                    if let Some(model) = &entry.fell_back {
                        ui.label(RichText::new(self.tf("answered_by", &[("model", model)])).color(muted()).small());
                    }
                }
                // what is left to the left of those: the interview and what the answers know, cut to fit
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(line).color(muted()).small()).truncate());
                });
            });
        });
        if let Some((message, error)) = self.notice.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(message).color(if error { danger() } else { accent() }).small());
                if ui.add(egui::Button::new(RichText::new("×").color(muted())).frame(false)).clicked() {
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
                if self.equalizer_background() {
                    let time = if self.app.animate { ctx.input(|input| input.time) } else { 0.0 };
                    // above the line with the interview and the history, so no dot sits under that text
                    let mut rect = ui.max_rect().expand(8.0);
                    rect.set_bottom(rect.bottom() - 44.0);
                    equalizer::background(
                        &ui.painter().with_clip_rect(rect.shrink(6.0)),
                        rect,
                        self.level.value(),
                        self.mood(),
                        time,
                    );
                }
                self.header(ui);
                self.caption_line(ui);
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
        if self.update_ui.available.is_none()
            && !self.update_ui.requested
            && update::due(&self.app, chrono::Utc::now().timestamp())
        {
            self.update_ui.requested = true;
            self.check_for_update(false);
        }
        let now = ctx.input(|input| input.time);
        self.clock = now;
        self.level.tick(now);
        let moving = self.app.animate && self.app.equalizer != "off";
        if moving {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if self.thinking() {
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
        .with_icon(icon::shared(&app.app_icon, theme::from_settings(app).accent))
        .with_inner_size([900.0, 420.0])
        .with_min_inner_size([560.0, 280.0])
        .with_transparent(true)
        .with_decorations(!settings.frameless);
    if settings.pinned {
        viewport = viewport.with_always_on_top();
    }
    eframe::NativeOptions { viewport, ..Default::default() }
}
