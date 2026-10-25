//! The call: the glowing sphere, the tutor's answer under it (word by word as it is spoken), what
//! you said, and a bar at the bottom to talk, type or hang up. Every kind of call looks like this:
//! a mock interview, a terms lesson, the introduction coach and the quiet chat.

use std::any::Any;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Frame, Key, Margin, Rect, RichText, Sense, TextEdit, Ui, Vec2, vec2};

use super::code::{self, CodeSession, Narrator};
use super::orb;
use super::stage::{self, Outcome, Picture};
use super::theme::{self, BERRY, BLUSH, FAINT, LINE, MUTED, SNOW, SURFACE};
use super::widgets::{self, Icon, ghost_button, icon_button, level_bars};
use super::{Core, Nav, column};
use crate::conversation::{Command, Event, Handle, State};
use crate::i18n::K;
use crate::prompts::interviewer::InterviewerSpec;
use crate::prompts::tutor::{LessonView, Mode, Stage};
use crate::session::{Session, Speaker as Who, Store};
use crate::syllabus::Half;
use crate::tags::Tag;

const LEVEL_HISTORY: usize = 90;

pub struct Sentence {
    pub text: String,
    pub start: Instant,
    pub end: Instant,
}

pub struct Item {
    pub who: Who,
    pub text: String,
}

/// What the call asks the window to do with its own data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// A term, or half of it, was finished.
    Progress { term: String, half: Half, score: u8 },
    /// The tutor wrote something worth keeping.
    Note(String),
    /// The tutor wants to show a picture of this.
    Show(String),
}

pub struct LiveView {
    pub session: Session,
    pub spec: InterviewerSpec,
    handle: Handle,
    _microphone: Option<Box<dyn Any + Send>>,
    pub state: State,
    pub items: Vec<Item>,
    pub captions: Vec<Sentence>,
    pub levels: VecDeque<f32>,
    mic_level: f32,
    speaker_level: f32,
    level_tick: u32,
    pub typed: String,
    pub muted: bool,
    pub error: Option<String>,
    pub show_transcript: bool,
    pub finished: bool,
    pub has_microphone: bool,
    /// Where the terms lesson is, when there is one.
    pub lesson: Option<LessonView>,
    /// Whether the tutor speaks; off, it only writes.
    pub voice_on: bool,
    /// What the call has asked the window to do, waiting for the next frame.
    pub effects: Vec<Effect>,
    /// The picture being shown instead of the sphere, when the tutor drew something.
    pub stage: Option<Picture>,
    /// The editor and the watcher, in a live-coding call.
    pub coding: Option<CodeSession>,
    started: Instant,
    orb_level: f32,
}

impl LiveView {
    pub fn new(
        session: Session,
        spec: InterviewerSpec,
        handle: Handle,
        microphone: Option<Box<dyn Any + Send>>,
    ) -> Self {
        let has_microphone = microphone.is_some();
        let voice_on = spec.mode.spoken();
        Self {
            session,
            spec,
            handle,
            _microphone: microphone,
            state: State::Starting,
            items: Vec::new(),
            captions: Vec::new(),
            levels: VecDeque::with_capacity(LEVEL_HISTORY),
            mic_level: 0.0,
            speaker_level: 0.0,
            level_tick: 0,
            typed: String::new(),
            muted: false,
            error: None,
            show_transcript: false,
            finished: false,
            has_microphone,
            lesson: None,
            voice_on,
            effects: Vec::new(),
            stage: None,
            coding: None,
            started: Instant::now(),
            orb_level: 0.0,
        }
    }

    pub fn send(&self, command: Command) {
        self.handle.send(command);
    }

    /// A chat is written: no microphone, no clock, a log of what was said.
    pub fn is_chat(&self) -> bool {
        self.spec.mode == Mode::Chat
    }

    /// Takes in what the call has reported since the last frame.
    pub fn poll(&mut self, store: &Store, now: Instant) {
        while let Ok(event) = self.handle.events.try_recv() {
            match event {
                Event::State(state) => self.state = state,
                Event::Caption { text, seconds } => {
                    let start = self.captions.last().map_or(now, |last| last.end.max(now));
                    let end = start + Duration::from_secs_f32(seconds.max(0.2));
                    self.captions.push(Sentence { text, start, end });
                }
                Event::Turn { who, text, language, append } => {
                    if who == Who::Interviewer {
                        self.captions.clear();
                    }
                    // you spoke over it: the picture waits for the answer
                    if who == Who::You
                        && let Some(stage) = self.stage.as_mut()
                    {
                        stage.pause_soon = true;
                    }
                    self.error = None;
                    if append && self.items.last().is_some_and(|i| i.who == who) {
                        if let Some(last) = self.items.last_mut() {
                            last.text.push(' ');
                            last.text.push_str(&text);
                        }
                        if let Some(turn) = self.session.turns.iter_mut().rev().find(|t| t.who == who) {
                            turn.text.push(' ');
                            turn.text.push_str(&text);
                        }
                    } else {
                        self.items.push(Item { who, text: text.clone() });
                        self.session.add_turn(who, &text, language);
                    }
                    let _ = store.save(&self.session);
                }
                Event::Level { mic, speaker } => {
                    self.mic_level = mic;
                    self.speaker_level = speaker;
                    self.level_tick += 1;
                    #[allow(clippy::manual_is_multiple_of)] // is_multiple_of needs a newer Rust than the minimum
                    let every_third = self.level_tick % 3 == 0;
                    if every_third {
                        if self.levels.len() == LEVEL_HISTORY {
                            self.levels.pop_front();
                        }
                        self.levels.push_back(if self.state == State::Speaking { speaker } else { mic });
                    }
                }
                Event::Error(message) => self.error = Some(message),
                Event::Lesson(view) => self.lesson = Some(view),
                Event::TermProgress { name, half, score, .. } => {
                    self.effects.push(Effect::Progress { term: name, half, score });
                }
                Event::Tag(Tag::Note(text)) => self.effects.push(Effect::Note(text)),
                Event::Tag(Tag::Show(topic)) => self.effects.push(Effect::Show(topic)),
                Event::Tag(_) => {}
                Event::Finished => self.finished = true,
            }
        }
    }

    pub fn elapsed(&self) -> u32 {
        self.started.elapsed().as_secs().min(u64::from(u32::MAX)) as u32
    }

    /// The tutor's words, with what is already said bright and what is still to come dim.
    fn caption_job(&self, now: Instant, width: f32, size: f32) -> LayoutJob {
        let mut job = LayoutJob::default();
        job.wrap.max_width = width;
        job.halign = Align::Center;
        let font = theme::serif(size);
        let mut first = true;
        for sentence in &self.captions {
            let words: Vec<&str> = sentence.text.split_whitespace().collect();
            let total = words.len().max(1);
            let progress = if now >= sentence.end {
                1.0
            } else if now <= sentence.start {
                0.0
            } else {
                now.duration_since(sentence.start).as_secs_f32()
                    / sentence.end.duration_since(sentence.start).as_secs_f32().max(0.01)
            };
            let spoken = (progress * total as f32).floor() as usize;
            for (index, word) in words.iter().enumerate() {
                let color = if now >= sentence.end || index < spoken {
                    SNOW
                } else if index == spoken && now > sentence.start {
                    BLUSH
                } else {
                    theme::mix(FAINT, MUTED, 0.35)
                };
                let text = if first { (*word).to_owned() } else { format!(" {word}") };
                first = false;
                job.append(&text, 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
            }
        }
        job
    }

    fn last_of(&self, who: Who) -> Option<&str> {
        self.items.iter().rev().find(|i| i.who == who).map(|i| i.text.as_str())
    }

    /// What the model should know when it draws something: the lesson and what was just said.
    fn show_context(&self) -> String {
        let mut context = String::new();
        if !self.spec.focus.is_empty() {
            context.push_str(&format!("The lesson is about {}. ", self.spec.focus));
        }
        if let Some(lesson) = &self.lesson
            && let Some(term) = self.spec.lesson.as_ref().and_then(|l| l.terms.iter().find(|t| t.name == lesson.name))
        {
            context.push_str(&format!("The term now: {}. In plain words: {} ", term.name, term.plain));
        }
        if let Some(said) = self.last_of(Who::You) {
            context.push_str(&format!("They just said: \"{}\".", stage::shorten(said, 200)));
        }
        context
    }

    /// The tutor wants to show a picture: it is written in the background, and the tutor holds its
    /// nudges until it has been seen.
    fn begin_show(&mut self, core: &mut Core, topic: &str, simpler: bool) {
        let context = self.show_context();
        self.stage = Some(core.begin_stage(topic, &context, simpler));
        self.send(Command::Hold(true));
    }

    /// The picture is over, or was closed: the tutor goes on, and asks what it asked for.
    fn end_show(&mut self, core: &Core, stage: &Picture, outcome: Outcome) {
        core.services.speaker.stop();
        self.send(Command::Hold(false));
        let title = stage.player().map_or_else(|| stage.topic.clone(), |p| p.board.title.clone());
        match outcome {
            Outcome::Finished => {
                self.send(Command::Context(format!(
                    "you have just shown an animated explanation called \"{title}\" and they watched all of it"
                )));
                if let Some(check) = stage.player().map(|p| p.board.check.clone()).filter(|c| !c.is_empty()) {
                    self.send(Command::Speak(check));
                }
            }
            _ => self.send(Command::Context(format!(
                "you started showing an animated explanation called \"{title}\" and they closed it before the end"
            ))),
        }
    }
}

fn clock(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn status_text(core: &Core, view: &LiveView) -> String {
    let tr = core.tr;
    let name = view.spec.persona.first_name();
    match view.state {
        State::Starting => tr.get(K::StatusStarting).to_owned(),
        State::Thinking => tr.fill(K::StatusThinking, &[("name", name)]),
        State::Speaking => tr.fill(K::StatusSpeaking, &[("name", name)]),
        State::Listening if view.is_chat() => tr.get(K::StatusChat).to_owned(),
        State::Listening => tr.get(K::StatusListening).to_owned(),
        State::Muted => tr.get(K::StatusMuted).to_owned(),
        State::Finished => tr.get(K::StatusFinished).to_owned(),
    }
}

/// The row that says which term of the lesson this is and which half of it.
fn lesson_row(core: &Core, lesson: &LessonView, ui: &mut Ui) {
    let tr = core.tr;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr.fill(
                K::LessonTerm,
                &[("n", &(lesson.index + 1).min(lesson.total).to_string()), ("total", &lesson.total.to_string())],
            ))
            .font(theme::mono(12.0))
            .color(MUTED),
        );
        ui.label(RichText::new(&lesson.name).font(theme::semibold(15.0)).color(SNOW));
        let (label, color) = match lesson.stage {
            Stage::Native => (K::StageNative, BLUSH),
            Stage::English => (K::StageEnglish, BERRY),
        };
        ui.label(RichText::new(tr.get(label).to_uppercase()).font(theme::mono_bold(11.0)).color(color));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(lesson.total as f32 * 14.0, 10.0), Sense::hover());
            for i in 0..lesson.total {
                let center = egui::pos2(rect.left() + 7.0 + i as f32 * 14.0, rect.center().y);
                let (fill, radius) = if i < lesson.index {
                    (BERRY, 4.0)
                } else if i == lesson.index {
                    (BLUSH, 4.5)
                } else {
                    (LINE, 3.5)
                };
                ui.painter().circle_filled(center, radius, fill);
            }
        });
    });
}

pub fn ui(core: &mut Core, view: &mut LiveView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let now = Instant::now();
    let ctx = ui.ctx().clone();
    // the words light up as they are spoken, so a speaking tutor needs smooth frames
    let tick = if view.state == State::Speaking { 33 } else { 100 };
    ctx.request_repaint_after(Duration::from_millis(tick));
    view.poll(&core.store, now);
    for effect in std::mem::take(&mut view.effects) {
        match effect {
            Effect::Show(topic) => view.begin_show(core, &topic, false),
            other => core.apply_effect(&view.spec, other),
        }
    }
    if view.finished {
        return Some(Nav::CallEnded);
    }
    // Space skips what the tutor is saying, unless a text box has the keyboard
    if view.state == State::Speaking && !ctx.egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Space)) {
        view.send(Command::Interrupt);
    }
    if let Some(mut picture) = view.stage.take() {
        let area = ui.available_rect_before_wrap().shrink2(vec2(24.0, 6.0));
        match stage::paint(core, &mut picture, ui, area) {
            Outcome::Nothing => view.stage = Some(picture),
            Outcome::Simpler => {
                view.end_show(core, &picture, Outcome::Closed);
                view.begin_show(core, &picture.topic, true);
            }
            done => view.end_show(core, &picture, done),
        }
        return None;
    }
    if view.coding.is_some() {
        return coding_ui(core, view, ui);
    }
    let chat = view.is_chat();
    let orb_size = if chat {
        (ui.available_height() * 0.26).clamp(120.0, 190.0)
    } else {
        (ui.available_height() * 0.42).clamp(170.0, 340.0)
    };
    let scroll_to_end = chat;
    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(scroll_to_end).show(ui, |ui| {
        column(ui, 780.0, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&view.session.title).font(theme::serif(26.0)).color(SNOW));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if chat {
                        ui.label(RichText::new(clock(view.elapsed())).font(theme::mono(13.0)).color(MUTED));
                    } else {
                        let total = view.spec.total_seconds();
                        ui.label(
                            RichText::new(format!("{} / {}", clock(view.elapsed()), clock(total)))
                                .font(theme::mono(13.0))
                                .color(MUTED),
                        );
                        ui.label(
                            RichText::new(format!("● {}", tr.get(K::Live))).font(theme::mono_bold(12.0)).color(BERRY),
                        );
                    }
                });
            });
            if !chat {
                let remaining = view.spec.total_seconds().saturating_sub(view.elapsed());
                widgets::progress(
                    ui,
                    ui.available_width(),
                    1.0 - remaining as f32 / view.spec.total_seconds().max(1) as f32,
                );
            }
            if let Some(lesson) = view.lesson.clone() {
                ui.add_space(8.0);
                lesson_row(core, &lesson, ui);
            }

            // what is happening, above the sphere
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(status_text(core, view)).font(theme::serif_italic(21.0)).color(MUTED));
            });

            // the sphere moves with the voice: it swells a little and drifts
            let target = match view.state {
                State::Speaking => view.speaker_level,
                State::Listening => view.mic_level,
                State::Thinking => {
                    0.14 + 0.08
                        * (view.elapsed() as f32 * 3.0 + now.elapsed().subsec_millis() as f32 * 0.003).sin().abs()
                }
                _ => 0.0,
            };
            view.orb_level += (target - view.orb_level) * 0.22;
            let spin = match view.state {
                State::Thinking => 1.5,
                State::Speaking => 1.0 + view.orb_level,
                State::Listening => 0.75,
                State::Muted | State::Starting => 0.4,
                State::Finished => 0.2,
            };
            let seconds = core.started.elapsed().as_secs_f32();
            let params = orb::Params { time: seconds, level: view.orb_level, spin };
            let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), orb_size), Sense::hover());
            let fps = match view.state {
                State::Speaking => 24.0,
                State::Listening => 20.0,
                _ => 12.0,
            };
            let swell = 1.0 + 0.07 * view.orb_level;
            let drift = (seconds * 1.1).sin() * 3.0 * (0.4 + view.orb_level);
            let rect = Rect::from_center_size(area.center() + vec2(0.0, drift), Vec2::splat(orb_size * swell));
            core.paint_orb(ui, rect, &params, fps);
            ui.add_space(8.0);

            if chat {
                chat_log(core, view, now, ui);
            } else {
                spoken_answer(core, view, now, ui);
            }

            if let Some(error) = view.error.clone() {
                ui.add_space(12.0);
                Frame::new()
                    .fill(theme::alpha(BERRY, 0.12))
                    .stroke(egui::Stroke::new(1.0, theme::mix(LINE, BERRY, 0.7)))
                    .corner_radius(14)
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.label(RichText::new(error).font(theme::sans(14.5)).color(SNOW));
                        ui.horizontal(|ui| {
                            if ghost_button(ui, tr.get(K::Retry)).clicked() {
                                view.send(Command::Retry);
                                view.error = None;
                            }
                            if ghost_button(ui, tr.get(K::OpenSettings)).clicked() {
                                core.settings_open = true;
                            }
                        });
                    });
            }

            if !chat {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if ghost_button(ui, tr.get(K::Transcript)).clicked() {
                        view.show_transcript = !view.show_transcript;
                    }
                });
                if view.show_transcript {
                    widgets::card_frame().show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .stick_to_bottom(true)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for item in &view.items {
                                    let who = if item.who == Who::You {
                                        tr.get(K::You).to_owned()
                                    } else {
                                        view.spec.persona.first_name().to_uppercase()
                                    };
                                    widgets::caps(ui, &who);
                                    ui.label(RichText::new(&item.text).color(if item.who == Who::You {
                                        MUTED
                                    } else {
                                        SNOW
                                    }));
                                    ui.add_space(6.0);
                                }
                            });
                    });
                }
            }
            ui.add_space(30.0);
        });
    });
    None
}

/// A call with an editor: the tutor at the left, the code at the right.
fn coding_ui(core: &mut Core, view: &mut LiveView, ui: &mut Ui) -> Option<Nav> {
    let now = Instant::now();
    let area = ui.available_rect_before_wrap().shrink2(vec2(20.0, 8.0));
    let left_width = (area.width() * 0.34).clamp(300.0, 430.0);
    let left = Rect::from_min_max(area.min, egui::pos2(area.left() + left_width, area.bottom()));
    let right = Rect::from_min_max(egui::pos2(area.left() + left_width + 18.0, area.top()), area.max);
    let target = match view.state {
        State::Speaking => view.speaker_level,
        State::Listening => view.mic_level,
        State::Thinking => 0.16,
        _ => 0.0,
    };
    view.orb_level += (target - view.orb_level) * 0.22;
    let (status, level) = (status_text(core, view), view.orb_level);
    let speaking = view.state == State::Speaking;
    let job = if view.captions.is_empty() {
        view.last_of(Who::Interviewer).map(|text| {
            let mut job = LayoutJob::default();
            job.wrap.max_width = left_width - 12.0;
            job.halign = Align::Center;
            job.append(
                text,
                0.0,
                TextFormat { font_id: theme::serif(19.0), color: theme::mix(MUTED, SNOW, 0.25), ..Default::default() },
            );
            job
        })
    } else {
        Some(view.caption_job(now, left_width - 12.0, 20.0))
    };
    let words: Vec<String> = view
        .items
        .iter()
        .rev()
        .take(4)
        .rev()
        .map(|item| format!("{}: {}", if item.who == Who::You { "PERSON" } else { "TUTOR" }, item.text))
        .collect();
    let (state, stage_open) = (view.state, view.stage.is_some());
    let mut commands = Vec::new();
    if let Some(coding) = view.coding.as_mut() {
        let narrator = Narrator {
            status: &status,
            text: job,
            level,
            spin: if speaking { 1.0 + level } else { 0.7 },
            fps: if speaking { 24.0 } else { 14.0 },
        };
        code::tutor_side(core, ui, left, coding, narrator);
        commands = code::editor(core, coding, ui, right, state, stage_open, &words);
    }
    for command in commands {
        view.send(command);
    }
    if let Some(error) = view.error.clone() {
        let at = Rect::from_min_size(egui::pos2(right.left(), right.bottom() - 70.0), vec2(right.width(), 60.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
            Frame::new()
                .fill(theme::alpha(BERRY, 0.14))
                .stroke(egui::Stroke::new(1.0, theme::mix(LINE, BERRY, 0.7)))
                .corner_radius(12)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.label(RichText::new(error).font(theme::sans(13.5)).color(SNOW));
                    if ghost_button(ui, core.tr.get(K::Retry)).clicked() {
                        view.send(Command::Retry);
                        view.error = None;
                    }
                });
        });
    }
    None
}

/// The tutor's words as they are spoken, and under them what you said.
fn spoken_answer(core: &Core, view: &LiveView, now: Instant, ui: &mut Ui) {
    let tr = core.tr;
    let width = ui.available_width().min(680.0);
    ui.vertical_centered(|ui| {
        ui.set_max_width(width);
        if view.captions.is_empty() {
            if let Some(text) = view.last_of(Who::Interviewer) {
                ui.label(RichText::new(text).font(theme::serif(23.0)).color(theme::mix(MUTED, SNOW, 0.25)));
            }
        } else {
            ui.label(view.caption_job(now, width, 25.0));
        }
        if let Some(text) = view.last_of(Who::You) {
            ui.add_space(10.0);
            ui.label(RichText::new(format!("{}: {text}", tr.get(K::You))).font(theme::sans(15.0)).color(FAINT));
        }
    });
}

/// A written conversation: what was said before, dimmer, and the latest answer big.
fn chat_log(core: &Core, view: &LiveView, now: Instant, ui: &mut Ui) {
    let tr = core.tr;
    let width = ui.available_width().min(680.0);
    ui.vertical_centered(|ui| {
        ui.set_max_width(width);
        let skip = view.items.len().saturating_sub(8);
        let last_tutor = view.items.iter().rposition(|i| i.who == Who::Interviewer);
        for (index, item) in view.items.iter().enumerate().skip(skip) {
            if item.who == Who::You {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{}: {}", tr.get(K::You), item.text)).font(theme::sans(15.0)).color(FAINT),
                );
                ui.add_space(4.0);
            } else if Some(index) == last_tutor && view.captions.is_empty() {
                ui.label(RichText::new(&item.text).font(theme::serif(23.0)).color(SNOW));
            } else {
                ui.label(RichText::new(&item.text).font(theme::serif(18.0)).color(theme::mix(MUTED, SNOW, 0.2)));
            }
        }
        if !view.captions.is_empty() {
            ui.label(view.caption_job(now, width, 23.0));
        }
    });
}

/// The bar along the bottom: microphone, a box to type in, the level, skip and hang up.
pub fn composer(core: &mut Core, view: &mut LiveView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let chat = view.is_chat();
    egui::Panel::bottom("composer").frame(Frame::new().fill(theme::BG).inner_margin(Margin::symmetric(20, 14))).show(
        ui,
        |ui| {
            column(ui, 780.0, |ui| {
                Frame::new()
                    .fill(SURFACE)
                    .stroke(egui::Stroke::new(1.0, LINE))
                    .corner_radius(30)
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if !chat {
                                let (icon, label) =
                                    if view.muted { (Icon::MicOff, K::Unmute) } else { (Icon::Mic, K::Mute) };
                                let tint = (!view.muted && view.has_microphone && view.state == State::Listening)
                                    .then_some(theme::CRANBERRY);
                                if icon_button(ui, icon, tr.get(label), 44.0, tint).clicked() {
                                    view.muted = !view.muted;
                                    view.send(Command::Mute(view.muted));
                                }
                            }
                            let send_width = 44.0;
                            let reserved = send_width + if chat { 150.0 } else { 140.0 + 120.0 + 130.0 };
                            let edit = ui.add(
                                TextEdit::singleline(&mut view.typed)
                                    .hint_text(if chat { tr.get(K::WriteToTutor) } else { tr.get(K::TypeAnswer) })
                                    .frame(Frame::NONE)
                                    .desired_width((ui.available_width() - reserved).max(120.0))
                                    .margin(vec2(8.0, 12.0)),
                            );
                            let submit = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                            let has_text = !view.typed.trim().is_empty();
                            if has_text
                                && (icon_button(ui, Icon::Send, tr.get(K::Send), 38.0, Some(BERRY)).clicked() || submit)
                            {
                                let text = std::mem::take(&mut view.typed);
                                view.send(Command::Say(text));
                                edit.request_focus();
                            }
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                let end_label = if chat { K::CloseChat } else { K::EndCall };
                                if widgets::primary_button(ui, tr.get(end_label), true).clicked() {
                                    view.send(Command::Finish);
                                    nav = Some(Nav::CallEnded);
                                }
                                if view.spec.mode.spoken() {
                                    let (icon, label) = if view.voice_on {
                                        (Icon::Speaker, K::VoiceOff)
                                    } else {
                                        (Icon::Speaker, K::VoiceOn)
                                    };
                                    let tint = (!view.voice_on).then_some(theme::COCOA);
                                    if icon_button(ui, icon, tr.get(label), 38.0, tint).clicked() {
                                        view.voice_on = !view.voice_on;
                                        view.send(Command::Voice(view.voice_on));
                                    }
                                }
                                if !chat {
                                    if view.state == State::Speaking && ghost_button(ui, tr.get(K::Interrupt)).clicked()
                                    {
                                        view.send(Command::Interrupt);
                                    }
                                    level_bars(
                                        ui,
                                        vec2(110.0, 28.0),
                                        &view.levels,
                                        if view.state == State::Speaking { BLUSH } else { BERRY },
                                    );
                                }
                            });
                        });
                    });
            });
        },
    );
    nav
}
