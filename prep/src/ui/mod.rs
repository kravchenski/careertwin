//! The window: a pill of three tabs at the top, a centred column under it, and a conversation in
//! the middle of it. History and settings slide over it when asked for.

pub mod account;
pub mod concepts;
pub mod focus;
pub mod help;
pub mod history;
pub mod home;
pub mod live;
pub mod orb;
pub mod phrases;
pub mod prepare;
pub mod review;
pub mod settings;
pub mod setup;
pub mod theme;
pub mod translate;
pub mod usage;
pub mod widgets;

use std::sync::Arc;
use std::time::Instant;

use egui::{Align, Frame, Layout, Margin, Rect, ScrollArea, Ui, UiBuilder, pos2, vec2};

use crate::brief::Brief;
use crate::config::{Paths, Settings};
use crate::i18n::{K, Tr};
use crate::jobs::Jobs;
use crate::ops::OpError;
use crate::pomodoro::Pomodoro;
use crate::prompts::Lang;
use crate::prompts::cards::ConceptCard;
use crate::prompts::feedback::Feedback;
use crate::prompts::interviewer::InterviewerSpec;
use crate::prompts::prepare::{Analysis, BankQuestion};
use crate::prompts::translate::Translation;
use crate::services::{Problems, Services};
use crate::session::{Session, Store, Track};
use crate::speech::Transcript;
use crate::srs::Deck;
use crate::stats::Stats;

use self::theme::{BG, LINE, SNOW};
use self::widgets::{Icon, icon_button, pill_tabs, pill_width};

/// Where the person is in the program.
pub enum Screen {
    Home,
    Setup(Box<setup::SetupView>),
    Live(Box<live::LiveView>),
    Review(Box<review::ReviewView>),
    Concepts(Box<concepts::ConceptsView>),
    Translate(Box<translate::TranslateView>),
    Prepare(Box<prepare::PrepareView>),
    Phrases(Box<phrases::PhrasesView>),
}

/// What a screen asks the window to do next.
#[derive(Debug, Clone)]
pub enum Nav {
    Home,
    SetupCall,
    Concepts,
    Phrases,
    Translate,
    Prepare,
    StartCall {
        spec: Box<InterviewerSpec>,
        headphones: bool,
    },
    /// The call is over, because you ended it or the time ran out.
    CallEnded,
    /// A call that starts with this question.
    PracticeQuestion(String),
    /// A session from the history.
    Open(String),
}

/// Results of background work, picked up on the next frame.
pub enum Outcome {
    Review { session_id: String, result: Result<Feedback, OpError> },
    Spoke(Result<(), String>),
    Heard { slot: u32, result: Result<Transcript, OpError> },
    Cards { topic: String, result: Result<Vec<ConceptCard>, OpError> },
    Translation(Result<Translation, OpError>),
    Analysis(Result<Analysis, OpError>),
    Questions(Result<Vec<BankQuestion>, OpError>),
    Models(Result<Vec<String>, String>),
    Downloaded { name: String, result: Result<(), String> },
}

/// A model being fetched for the offline engines.
pub struct Download {
    pub name: String,
    pub progress: Arc<crate::speech::download::Progress>,
    pub failed: Option<String>,
    pub finished: bool,
}

/// The vacancy you are preparing for. It is kept between runs and the interviewer uses it.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Vacancy {
    pub title: String,
    pub company: String,
    pub description: String,
}

impl Vacancy {
    fn file(paths: &Paths) -> std::path::PathBuf {
        paths.data.join("vacancy.json")
    }

    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(Self::file(paths))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = crate::config::write_private(&Self::file(paths), &bytes);
        }
    }
}

pub struct Core {
    pub paths: Paths,
    pub settings: Settings,
    pub services: Services,
    pub problems: Problems,
    pub store: Store,
    pub deck: Deck,
    pub brief: Option<Brief>,
    pub brief_note: Option<String>,
    pub jobs: Jobs<Outcome>,
    pub tr: Tr,
    pub track: Track,
    pub started: Instant,
    pub history_open: bool,
    pub settings_open: bool,
    pub usage_open: bool,
    pub help_open: bool,
    /// The menu over the panel at the bottom left, and its language choice.
    pub account_open: bool,
    pub language_open: bool,
    /// Where the panel was drawn last frame, so a click outside the menu can tell.
    pub account_chip: Rect,
    /// Set by Quit in the menu; the window closes on the next frame.
    pub quit: bool,
    pub stats: Stats,
    pub pomodoro: Pomodoro,
    /// Asks the window to draw again.
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// A message for the person that no open screen has a place for.
    pub notice: Option<String>,
    pub notice_at: Option<Instant>,
    pub vacancy: Vacancy,
    /// Models the gateway offered when last asked, for the picker in Settings.
    pub models: Vec<String>,
    /// The offline models being fetched or fetched already.
    pub downloads: Vec<Download>,
    /// The sphere's picture, drawn again every frame it is on screen.
    orb: Option<egui::TextureHandle>,
    orb_made: Option<(Instant, usize)>,
    /// How many times the sphere's picture was made; the tests check that it is not every frame.
    pub orb_renders: usize,
}

impl Core {
    pub fn now(&self) -> u64 {
        crate::session::now()
    }

    /// Paints the sphere in `rect`. The picture is made fresh from `params` at most `fps` times a
    /// second (a few times a second when the window is in the background); in between, the last
    /// one is painted again, so a sphere that only breathes costs the processor almost nothing.
    pub fn paint_orb(&mut self, ui: &Ui, rect: Rect, params: &orb::Params, fps: f32) {
        let context = ui.ctx();
        let focused = context.input(|i| i.viewport().focused.unwrap_or(true));
        let interval = std::time::Duration::from_secs_f32(1.0 / if focused { fps.max(1.0) } else { 2.0 });
        let size = (rect.width() * 0.85).round().clamp(128.0, 256.0) as usize;
        let now = Instant::now();
        let stale = self.orb_made.is_none_or(|(at, made)| made != size || now.duration_since(at) >= interval);
        if stale {
            let image = orb::render_at(params, size);
            match &mut self.orb {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => self.orb = Some(context.load_texture("orb", image, egui::TextureOptions::LINEAR)),
            }
            self.orb_made = Some((now, size));
            self.orb_renders += 1;
        }
        if let Some(texture) = &self.orb {
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture.id(), rect, uv, egui::Color32::WHITE);
        }
        let wait = self.orb_made.map_or(interval, |(at, _)| interval.saturating_sub(now.duration_since(at)));
        context.request_repaint_after(wait.max(std::time::Duration::from_millis(1)));
    }

    /// Fetches the models the offline engines need, if they are chosen and not on disk yet.
    pub fn prefetch_local(&mut self) {
        let settings = self.settings.effective();
        if cfg!(feature = "local-stt") && settings.stt.engine == crate::config::SttEngine::Local {
            let model = crate::speech::local_stt_model(&settings);
            self.download(model.label, vec![crate::models::stt_asset(model)], self.paths.models_dir());
        }
        if cfg!(feature = "local-tts")
            && settings.tts.engine == crate::config::TtsEngine::Local
            && let Some(assets) = crate::models::kokoro_assets(&settings.tts.voice_en)
        {
            self.download("Kokoro", assets, crate::speech::local_tts_folder(&self.paths));
        }
    }

    fn download(&mut self, name: &str, assets: Vec<crate::speech::download::Asset>, folder: std::path::PathBuf) {
        let progress = Arc::new(crate::speech::download::Progress::default());
        self.downloads.retain(|d| d.name != name);
        self.downloads.push(Download {
            name: name.to_owned(),
            progress: Arc::clone(&progress),
            failed: None,
            finished: false,
        });
        let label = self.tr.fill(K::Downloading, &[("name", name), ("percent", "0")]);
        let name = name.to_owned();
        self.jobs.spawn(label, async move {
            let result = crate::speech::download::ensure(&assets, &folder, &progress).await.map_err(|e| e.to_string());
            Outcome::Downloaded { name, result }
        });
    }

    /// The offline models that are still being fetched.
    pub fn downloading(&self) -> Vec<&Download> {
        self.downloads.iter().filter(|d| !d.finished && d.failed.is_none()).collect()
    }

    /// Saves new settings and rebuilds everything that depends on them.
    pub fn apply_settings(&mut self, settings: Settings) -> anyhow::Result<()> {
        settings.save(&self.paths)?;
        let (services, problems) = crate::services::build(&settings, &self.paths);
        self.tr = Tr::new(settings.ui_language);
        self.services = services;
        self.problems = problems;
        match Brief::load(&settings.effective()) {
            Ok((brief, warnings)) => {
                self.brief = Some(brief);
                self.brief_note = warnings.first().cloned();
            }
            Err(error) => {
                self.brief = None;
                self.brief_note = Some(error.to_string());
            }
        }
        self.pomodoro.configure(settings.pomodoro);
        self.settings = settings;
        self.prefetch_local();
        Ok(())
    }

    /// Asks the gateway which models it offers.
    pub fn load_models(&mut self, settings: &Settings) {
        let effective = settings.effective();
        let secrets = effective.secrets();
        let label = self.tr.get(K::LoadModels).to_owned();
        self.jobs.spawn(label, async move {
            let result = match crate::llm::HttpLlm::new(&effective.llm, secrets) {
                Ok(llm) => llm.models().await.map_err(|e| e.to_string()),
                Err(error) => Err(error.to_string()),
            };
            Outcome::Models(result)
        });
    }

    /// Speaks a sample in the voice being edited, before it is saved.
    pub fn test_voice(&mut self, settings: &Settings) {
        let effective = settings.effective();
        let paths = self.paths.clone();
        let speaker = self.services.speaker.clone();
        let phrase = self.tr.get(K::TestPhrase).to_owned();
        let label = self.tr.get(K::TestVoice).to_owned();
        speaker.stop();
        self.jobs.spawn(label, async move {
            let tts = match crate::speech::build_tts(&effective, &paths) {
                Ok(Some(tts)) => tts,
                Ok(None) => return Outcome::Spoke(Ok(())),
                Err(error) => return Outcome::Spoke(Err(error.to_string())),
            };
            let voice = crate::speech::voice_for(&effective, Lang::En);
            match tts.synthesize(phrase, voice).await {
                Ok(speech) => {
                    speaker.enqueue(speech.pcm, speech.sample_rate);
                    Outcome::Spoke(Ok(()))
                }
                Err(error) => Outcome::Spoke(Err(error.to_string())),
            }
        });
    }

    /// Your profile with the vacancy you are preparing for, as the interviewer should see it.
    pub fn working_brief(&self) -> Brief {
        let mut brief = self.brief.clone().unwrap_or_default();
        brief.set_vacancy(&self.vacancy.title, &self.vacancy.company, &self.vacancy.description);
        brief
    }

    /// Your name for the panel at the bottom: what you typed, or what your notes say.
    pub fn display_name(&self) -> String {
        let typed = self.settings.profile.name.trim();
        if !typed.is_empty() {
            return typed.to_owned();
        }
        self.brief.as_ref().and_then(Brief::candidate_name).unwrap_or_default()
    }

    /// Your address for the panel at the bottom: what you typed, or the first one in your notes.
    pub fn display_email(&self) -> String {
        let typed = self.settings.profile.email.trim();
        if !typed.is_empty() {
            return typed.to_owned();
        }
        self.brief.as_ref().and_then(Brief::email).unwrap_or_default()
    }

    pub fn save_stats(&self) {
        let _ = self.stats.save(&self.paths.stats_file());
    }

    pub fn save_vacancy(&self) {
        self.vacancy.save(&self.paths);
    }

    /// Whether the vacancy is written in Russian, so its questions are read out in Russian.
    pub fn vacancy_language_is_russian(&self) -> bool {
        let letters: Vec<char> = self.vacancy.description.chars().filter(|c| c.is_alphabetic()).collect();
        !letters.is_empty()
            && letters.iter().filter(|c| ('\u{400}'..='\u{4FF}').contains(*c)).count() * 2 > letters.len()
    }

    /// Starts writing a set of concept-check questions.
    pub fn make_cards(&mut self, topic: String, count: usize, difficulty: u8) {
        let llm = self.services.llm.clone();
        let model = self.settings.effective().llm.model;
        let brief = self.working_brief();
        let label = self.tr.get(K::CardsMaking).to_owned();
        self.jobs.spawn(label, async move {
            let result =
                crate::ops::concept_cards(llm.as_ref(), &model, &brief, &topic, count, difficulty, Lang::En).await;
            Outcome::Cards { topic, result }
        });
    }

    pub fn translate(&mut self, text: String) {
        let llm = self.services.llm.clone();
        let model = self.settings.effective().llm.model;
        let ui = self.ui_lang();
        let label = self.tr.get(K::TranslateBusy).to_owned();
        self.jobs.spawn(label, async move {
            Outcome::Translation(crate::ops::translate(llm.as_ref(), &model, &text, ui).await)
        });
    }

    pub fn analyze(&mut self) {
        let llm = self.services.llm.clone();
        let model = self.settings.effective().llm.model;
        let brief = self.working_brief();
        let language = self.ui_lang();
        let label = self.tr.get(K::CompareCv).to_owned();
        self.jobs.spawn(label, async move {
            Outcome::Analysis(crate::ops::analyze(llm.as_ref(), &model, &brief, language).await)
        });
    }

    pub fn question_bank(&mut self, count: usize) {
        let llm = self.services.llm.clone();
        let model = self.settings.effective().llm.model;
        let brief = self.working_brief();
        let label = self.tr.get(K::MakeQuestions).to_owned();
        self.jobs.spawn(label, async move {
            Outcome::Questions(crate::ops::question_bank(llm.as_ref(), &model, &brief, count, "", Lang::En).await)
        });
    }

    /// The language of the interface as a language for the model to write notes in.
    pub fn ui_lang(&self) -> Lang {
        if self.tr.is_russian() { Lang::Ru } else { Lang::En }
    }

    /// Starts making the review of a finished call.
    pub fn spawn_review(&mut self, session: &Session) {
        let llm = self.services.llm.clone();
        let model = self.settings.effective().llm.model;
        let brief = self.working_brief();
        let session = session.clone();
        let ui = self.ui_lang();
        let label = self.tr.get(K::ReviewTitle).to_owned();
        self.jobs.spawn(label, async move {
            let result = crate::ops::feedback(llm.as_ref(), &model, &session, &brief, ui).await;
            Outcome::Review { session_id: session.id, result }
        });
    }

    /// Says `text` aloud in the interviewer's voice, cutting off whatever was playing.
    pub fn speak(&mut self, text: String, language: Lang) {
        let Some(tts) = self.services.tts.clone() else { return };
        let speaker = self.services.speaker.clone();
        let voice = crate::speech::voice_for(&self.settings.effective(), language);
        let label = self.tr.get(K::Listen).to_owned();
        speaker.stop();
        self.jobs.spawn(label, async move {
            let spoken = crate::sentences::clean_for_speech(&text);
            match tts.synthesize(spoken, voice).await {
                Ok(speech) => {
                    speaker.stop();
                    speaker.enqueue(speech.pcm, speech.sample_rate);
                    Outcome::Spoke(Ok(()))
                }
                Err(error) => Outcome::Spoke(Err(error.to_string())),
            }
        });
    }

    /// Listens for one thing you say in English; the result comes back as [`Outcome::Heard`].
    pub fn hear(&mut self, slot: u32) {
        self.hear_in(slot, Some("en"));
    }

    /// Listens for one thing you say; `language` is a code, or `None` to let the recogniser decide.
    pub fn hear_in(&mut self, slot: u32, language: Option<&str>) {
        let services = self.services.clone();
        let effective = self.settings.effective();
        let device = effective.audio.input_device.clone();
        let hint = crate::speech::SttHint {
            language: language.map(str::to_owned),
            vocabulary: self.brief.as_ref().map(Brief::vocabulary).unwrap_or_default(),
        };
        let label = self.tr.get(K::Listening).to_owned();
        self.jobs.spawn(label, async move {
            let result = crate::ops::hear(&services, &device, hint, std::time::Duration::from_secs(8)).await;
            Outcome::Heard { slot, result }
        });
    }

    pub fn recent(&self, track: Track, limit: usize) -> Vec<Session> {
        self.store.list_track(track).into_iter().take(limit).collect()
    }
}

pub struct PrepApp {
    pub core: Core,
    pub screen: Screen,
    pub history: history::HistoryState,
    pub draft: Option<settings::Draft>,
}

impl PrepApp {
    /// `repaint` asks the window to draw again, for work that finishes in the background. The theme
    /// is installed separately, with [`theme::install`], on the egui context.
    pub fn new(
        paths: Paths,
        settings: Settings,
        services: Services,
        problems: Problems,
        runtime: tokio::runtime::Handle,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let wake = Arc::clone(&repaint);
        let jobs = Jobs::new(runtime, repaint);
        let (brief, brief_note) = match Brief::load(&settings.effective()) {
            Ok((brief, warnings)) => (Some(brief), warnings.first().cloned()),
            Err(error) => (None, Some(error.to_string())),
        };
        let vacancy = Vacancy::load(&paths);
        let store = Store::new(paths.sessions_dir());
        let deck = Deck::load(&paths.deck_file());
        let tr = Tr::new(settings.ui_language);
        let stats = Stats::load(&paths.stats_file());
        let pomodoro = Pomodoro::new(settings.pomodoro);
        let mut app = Self {
            core: Core {
                paths,
                settings,
                services,
                problems,
                store,
                deck,
                brief,
                brief_note,
                jobs,
                tr,
                track: Track::Technical,
                started: Instant::now(),
                history_open: false,
                settings_open: false,
                usage_open: false,
                help_open: false,
                account_open: false,
                language_open: false,
                account_chip: Rect::NOTHING,
                quit: false,
                stats,
                pomodoro,
                wake,
                notice: None,
                notice_at: None,
                vacancy,
                models: Vec::new(),
                downloads: Vec::new(),
                orb: None,
                orb_made: None,
                orb_renders: 0,
            },
            screen: Screen::Home,
            history: history::HistoryState::default(),
            draft: None,
        };
        app.core.prefetch_local();
        app
    }

    /// Draws one frame of the whole window.
    pub fn show(&mut self, ui: &mut Ui) {
        for (_, outcome) in self.core.jobs.poll() {
            self.apply(outcome);
        }
        if let Some(signal) = self.core.pomodoro.update(Instant::now()) {
            self.core.on_timer(signal);
        }
        self.shortcuts(ui);
        account::bar(&mut self.core, ui);
        let mut next = self.top_bar(ui);
        if let Screen::Live(view) = &mut self.screen {
            next = next.or(live::composer(&mut self.core, view, ui));
        }
        egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ui, |ui| {
            let screen_nav = match &mut self.screen {
                Screen::Home => home::ui(&mut self.core, ui),
                Screen::Setup(view) => setup::ui(&mut self.core, view, ui),
                Screen::Live(view) => live::ui(&mut self.core, view, ui),
                Screen::Review(view) => review::ui(&mut self.core, view, ui),
                Screen::Concepts(view) => concepts::ui(&mut self.core, view, ui),
                Screen::Translate(view) => translate::ui(&mut self.core, view, ui),
                Screen::Prepare(view) => prepare::ui(&mut self.core, view, ui),
                Screen::Phrases(view) => phrases::ui(&mut self.core, view, ui),
            };
            next = next.take().or(screen_nav);
        });
        let ctx = ui.ctx().clone();
        next = next.or(history::ui(&mut self.core, &mut self.history, &ctx));
        settings::show(&mut self.core, &mut self.draft, &ctx);
        usage::show(&mut self.core, &ctx);
        help::show(&mut self.core, &ctx);
        account::menu(&mut self.core, &ctx);
        focus::toast(&mut self.core, &ctx);
        if self.core.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(nav) = next {
            self.go(nav);
            ui.ctx().request_repaint();
        }
    }

    /// The keys that work on every screen.
    fn shortcuts(&mut self, ui: &mut Ui) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        if ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(ctrl_shift, Key::Comma))) {
            self.core.account_open = false;
            self.core.settings_open = !self.core.settings_open;
        }
        if ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(ctrl_shift, Key::T))) {
            self.core.pomodoro.toggle(Instant::now());
        }
    }

    fn go(&mut self, nav: Nav) {
        match nav {
            Nav::Home => self.screen = Screen::Home,
            Nav::SetupCall => self.screen = Screen::Setup(Box::new(setup::SetupView::new(&self.core))),
            Nav::StartCall { spec, headphones } => self.start_call(*spec, headphones),
            Nav::CallEnded => self.end_call(),
            Nav::Concepts => self.screen = Screen::Concepts(Box::new(concepts::ConceptsView::new(&self.core))),
            Nav::Phrases => self.screen = Screen::Phrases(Box::default()),
            Nav::Translate => self.screen = Screen::Translate(Box::default()),
            Nav::Prepare => self.screen = Screen::Prepare(Box::default()),
            Nav::PracticeQuestion(question) => {
                let mut view = setup::SetupView::new(&self.core);
                view.spec.focus = format!("Begin with this question: {question}");
                self.screen = Screen::Setup(Box::new(view));
            }
            Nav::Open(id) => self.open(&id),
        }
    }

    fn start_call(&mut self, spec: InterviewerSpec, headphones: bool) {
        let core = &mut self.core;
        let effective = core.settings.effective();
        let brief = core.working_brief();
        let (mic, guard) = match (core.services.mic)(&effective.audio.input_device) {
            Ok((frames, guard)) => (Some(frames), Some(guard)),
            Err(_) => (None, None),
        };
        let voice = crate::speech::voice_for(&effective, spec.language);
        let title = format!("{} · {}", setup::title_for(core), setup::persona_label(core, spec.persona));
        let mut session = Session::new(core.track, crate::session::Kind::Live, &title);
        session.setup = serde_json::to_value(&spec).unwrap_or_default();
        let _ = core.store.save(&session);
        let config = crate::conversation::Config {
            spec: spec.clone(),
            brief: brief.clone(),
            knobs: crate::conversation::Knobs::default(),
            model: effective.llm.interviewer_model().to_owned(),
            headphones,
            voice,
            stt_hint: crate::speech::SttHint {
                language: match effective.stt.language.as_str() {
                    "" | "auto" => None,
                    other => Some(other.to_owned()),
                },
                vocabulary: brief.vocabulary(),
            },
        };
        let deps = crate::conversation::Deps {
            llm: core.services.llm.clone(),
            stt: core.services.stt.clone().unwrap_or_else(|| Arc::new(NoRecognition)),
            tts: core.services.tts.clone(),
            speaker: core.services.speaker.clone(),
            mic,
            wake: Arc::clone(&core.wake),
        };
        let handle = crate::conversation::start(deps, config);
        self.screen = Screen::Live(Box::new(live::LiveView::new(session, spec, handle, guard)));
    }

    fn end_call(&mut self) {
        if let Screen::Live(view) = std::mem::replace(&mut self.screen, Screen::Home) {
            let mut session = view.session.clone();
            if session.candidate_turns() == 0 {
                let _ = self.core.store.delete(&session.id);
                return;
            }
            session.finish();
            let _ = self.core.store.save(&session);
            let words = session
                .turns
                .iter()
                .filter(|t| t.who == crate::session::Speaker::You)
                .map(|t| t.text.split_whitespace().count() as u32)
                .sum();
            self.core.stats.add_practice(self.core.now(), view.elapsed(), words);
            self.core.save_stats();
            self.core.spawn_review(&session);
            self.screen = Screen::Review(Box::new(review::ReviewView::new(session)));
        }
    }

    /// Opens a session from the history the way it was left.
    fn open(&mut self, id: &str) {
        let Ok(session) = self.core.store.load(id) else { return };
        self.core.track = session.track;
        self.core.history_open = false;
        match session.kind {
            crate::session::Kind::Live => {
                if session.feedback.is_none() && session.candidate_turns() > 0 {
                    self.core.spawn_review(&session);
                }
                self.screen = Screen::Review(Box::new(review::ReviewView::new(session)));
            }
            crate::session::Kind::Cards => {
                self.screen = Screen::Concepts(Box::new(concepts::ConceptsView::from_session(session)))
            }
            crate::session::Kind::Translate => {
                self.screen = Screen::Translate(Box::new(translate::TranslateView::from_session(session)))
            }
            crate::session::Kind::Prepare => self.screen = Screen::Prepare(Box::default()),
        }
    }

    /// Takes the result of a piece of background work to where it belongs.
    fn apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Review { session_id, result } => {
                if let Ok(mut session) = self.core.store.load(&session_id)
                    && let Ok(feedback) = &result
                {
                    session.feedback = Some(feedback.clone());
                    let _ = self.core.store.save(&session);
                }
                if let Screen::Review(view) = &mut self.screen
                    && view.session.id == session_id
                {
                    match result {
                        Ok(feedback) => {
                            view.session.feedback = Some(feedback);
                            view.state = review::State::Ready;
                        }
                        Err(error) => view.state = review::State::Failed(error.to_string()),
                    }
                }
            }
            Outcome::Downloaded { name, result } => {
                if let Some(download) = self.core.downloads.iter_mut().find(|d| d.name == name) {
                    match result {
                        Ok(()) => download.finished = true,
                        Err(why) => download.failed = Some(why),
                    }
                }
            }
            Outcome::Models(Ok(models)) => self.core.models = models,
            Outcome::Models(Err(error)) => {
                if let Some(draft) = &mut self.draft {
                    draft.message = Some((false, error));
                }
            }
            Outcome::Spoke(Ok(())) => {}
            Outcome::Spoke(Err(error)) => {
                if self.core.settings_open {
                    if let Some(draft) = &mut self.draft {
                        draft.message = Some((false, error));
                    }
                } else {
                    self.notify(error);
                }
            }
            Outcome::Cards { topic, result } => {
                if let Screen::Concepts(view) = &mut self.screen {
                    match result {
                        Ok(cards) => {
                            view.topic = topic;
                            view.cards = cards;
                            view.index = 0;
                            view.choice = None;
                            view.answered = false;
                            view.phase = concepts::Phase::Playing;
                        }
                        Err(error) => view.phase = concepts::Phase::Failed(error.to_string()),
                    }
                }
            }
            Outcome::Translation(result) => {
                if let Screen::Translate(view) = &mut self.screen {
                    view.working = false;
                    match result {
                        Ok(translation) => {
                            view.result = Some(translation);
                            view.keep(&self.core);
                        }
                        Err(error) => view.error = Some(error.to_string()),
                    }
                }
            }
            Outcome::Analysis(result) => {
                if let Screen::Prepare(view) = &mut self.screen {
                    view.comparing = false;
                    match result {
                        Ok(analysis) => view.analysis = Some(analysis),
                        Err(error) => view.error = Some(error.to_string()),
                    }
                }
            }
            Outcome::Questions(result) => {
                if let Screen::Prepare(view) = &mut self.screen {
                    view.asking = false;
                    match result {
                        Ok(questions) => view.questions = questions,
                        Err(error) => view.error = Some(error.to_string()),
                    }
                }
            }
            Outcome::Heard { slot, result } if slot >= translate::DICTATE => {
                if let Screen::Translate(view) = &mut self.screen {
                    view.dictating = false;
                    match result {
                        Ok(heard) => {
                            if !view.input.trim().is_empty() {
                                view.input.push(' ');
                            }
                            view.input.push_str(heard.text.trim());
                        }
                        Err(error) => view.error = Some(error.to_string()),
                    }
                }
            }
            Outcome::Heard { slot, result } => {
                if let Screen::Translate(view) = &mut self.screen {
                    view.listening_slot = None;
                    match result {
                        Ok(heard) => {
                            if let Some(target) = view.target(slot as usize) {
                                let comparison = crate::textdiff::compare(&target, &heard.text);
                                view.drills.insert(slot as usize, review::Drill { heard: heard.text, comparison });
                            }
                        }
                        Err(error) => view.error = Some(error.to_string()),
                    }
                } else if let Screen::Review(view) = &mut self.screen {
                    view.listening_slot = None;
                    match result {
                        Ok(heard) => {
                            let target = view
                                .feedback()
                                .and_then(|f| f.answers.get(slot as usize))
                                .map(|a| a.better_answer.clone());
                            if let Some(target) = target {
                                let comparison = crate::textdiff::compare(&target, &heard.text);
                                view.drills.insert(slot as usize, review::Drill { heard: heard.text, comparison });
                            }
                        }
                        Err(error) => view.drill_error = Some(error.to_string()),
                    }
                }
            }
        }
    }

    /// A short message for the person; shown on the screen that is open.
    fn notify(&mut self, message: String) {
        match &mut self.screen {
            Screen::Live(view) => view.error = Some(message),
            Screen::Review(view) => view.drill_error = Some(message),
            Screen::Translate(view) => view.error = Some(message),
            Screen::Prepare(view) => view.error = Some(message),
            _ => self.core.notice = Some(message),
        }
    }

    fn top_bar(&mut self, ui: &mut Ui) -> Option<Nav> {
        let mut nav = None;
        egui::Panel::top("bar")
            .exact_size(70.0)
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(20, 10)))
            .show(ui, |ui| {
                let bar = ui.max_rect();
                let tr = self.core.tr;
                let labels = [tr.get(K::TabEnglish), tr.get(K::TabTechnical), tr.get(K::TabInterview)];
                let width = pill_width(ui, &labels);
                let pill = Rect::from_center_size(bar.center(), vec2(width, 46.0));
                let selected = Track::ALL.iter().position(|t| *t == self.core.track).unwrap_or(0);
                let clicked =
                    ui.scope_builder(UiBuilder::new().max_rect(pill), |ui| pill_tabs(ui, &labels, selected)).inner;
                if let Some(index) = clicked {
                    self.core.track = Track::ALL[index];
                    nav = Some(Nav::Home);
                }
                let left = Rect::from_min_size(pos2(bar.left(), bar.center().y - 20.0), vec2(110.0, 40.0));
                ui.scope_builder(UiBuilder::new().max_rect(left).layout(Layout::left_to_right(Align::Center)), |ui| {
                    if icon_button(ui, Icon::List, tr.get(K::Sessions), 38.0, None).clicked() {
                        self.core.history_open = !self.core.history_open;
                    }
                    if icon_button(ui, Icon::Plus, tr.get(K::NewSession), 38.0, None).clicked() {
                        nav = Some(Nav::Home);
                    }
                });
                let size = vec2(focus::PILL_WIDTH, 44.0);
                let right = Rect::from_min_size(pos2(bar.right() - size.x, bar.center().y - size.y / 2.0), size);
                ui.scope_builder(UiBuilder::new().max_rect(right), |ui| focus::pill(&mut self.core, ui));
            });
        nav
    }
}

/// Puts `add` in a column of at most `width`, centred in the space that is left.
pub fn column<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let available = ui.available_rect_before_wrap();
    let width = width.min(available.width() - 40.0).max(320.0);
    let rect = Rect::from_min_size(
        pos2(available.center().x - width / 2.0, available.top() + 8.0),
        vec2(width, available.height()),
    );
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.set_width(width);
        add(ui)
    })
    .inner
}

/// A column that scrolls when it is longer than the window.
pub fn scrolling_column<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| column(ui, width, add)).inner
}

#[cfg(feature = "gui")]
pub fn window_options() -> eframe::NativeOptions {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png")).unwrap_or_default();
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CareerTwin Prep")
            .with_app_id("careertwin-prep")
            .with_icon(icon)
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    }
}

/// A thin line under a heading.
pub fn rule(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

pub const TEXT: egui::Color32 = SNOW;

/// Stands in when speech recognition is not set up: the call goes on with typed answers.
struct NoRecognition;

impl crate::speech::Stt for NoRecognition {
    fn transcribe(
        &self,
        _: Vec<i16>,
        _: crate::speech::SttHint,
    ) -> futures_util::future::BoxFuture<'static, Result<crate::speech::Transcript, crate::speech::SpeechError>> {
        Box::pin(async {
            Err(crate::speech::SpeechError::Status(
                "Speech recognition is not set up, so I cannot hear you. Type your answer, or open Settings.".into(),
            ))
        })
    }
}
