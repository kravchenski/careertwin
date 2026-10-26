//! The call: the glowing sphere, what the interviewer is saying (word by word as it is spoken),
//! what you said, and a bar at the bottom to talk, type or hang up.

use std::any::Any;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Frame, Key, Margin, Rect, RichText, Sense, TextEdit, Ui, Vec2, vec2};

use super::orb;
use super::theme::{self, BERRY, BLUSH, FAINT, LINE, MUTED, SNOW, SURFACE};
use super::widgets::{self, Icon, ghost_button, icon_button, level_bars};
use super::{Core, Nav, column, scrolling_column};
use crate::conversation::{Command, Event, Handle, State};
use crate::i18n::K;
use crate::prompts::interviewer::InterviewerSpec;
use crate::session::{Session, Speaker as Who, Store};

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
            started: Instant::now(),
            orb_level: 0.0,
        }
    }

    pub fn send(&self, command: Command) {
        self.handle.send(command);
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
                Event::Finished => self.finished = true,
            }
        }
    }

    pub fn elapsed(&self) -> u32 {
        self.started.elapsed().as_secs().min(u64::from(u32::MAX)) as u32
    }

    /// The interviewer's words, with what is already said bright and what is still to come dim.
    fn caption_job(&self, now: Instant, width: f32) -> LayoutJob {
        let mut job = LayoutJob::default();
        job.wrap.max_width = width;
        job.halign = Align::Center;
        let font = theme::serif(25.0);
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
        State::Listening => tr.get(K::StatusListening).to_owned(),
        State::Muted => tr.get(K::StatusMuted).to_owned(),
        State::Finished => tr.get(K::StatusFinished).to_owned(),
    }
}

pub fn ui(core: &mut Core, view: &mut LiveView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let now = Instant::now();
    let ctx = ui.ctx().clone();
    // the words light up as they are spoken, so a speaking interviewer needs smooth frames
    let tick = if view.state == State::Speaking { 33 } else { 100 };
    ctx.request_repaint_after(Duration::from_millis(tick));
    view.poll(&core.store, now);
    if view.finished {
        return Some(Nav::CallEnded);
    }
    // Space skips what the interviewer is saying, unless a text box has the keyboard
    if view.state == State::Speaking && !ctx.egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Space)) {
        view.send(Command::Interrupt);
    }
    let orb_size = (ui.available_height() * 0.42).clamp(170.0, 340.0);
    scrolling_column(ui, 780.0, |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(&view.session.title).font(theme::serif(28.0)).color(SNOW));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let total = view.spec.total_seconds();
                ui.label(
                    RichText::new(format!("{} / {}", clock(view.elapsed()), clock(total)))
                        .font(theme::mono(13.0))
                        .color(MUTED),
                );
                ui.label(RichText::new(format!("● {}", tr.get(K::Live))).font(theme::mono_bold(12.0)).color(BERRY));
            });
        });
        let remaining = view.spec.total_seconds().saturating_sub(view.elapsed());
        widgets::progress(ui, ui.available_width(), 1.0 - remaining as f32 / view.spec.total_seconds().max(1) as f32);

        // the sphere
        let target = match view.state {
            State::Speaking => view.speaker_level,
            State::Listening => view.mic_level,
            State::Thinking => {
                0.14 + 0.08 * (view.elapsed() as f32 * 3.0 + now.elapsed().subsec_millis() as f32 * 0.003).sin().abs()
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
        let params = orb::Params { time: core.started.elapsed().as_secs_f32(), level: view.orb_level, spin };
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), orb_size), Sense::hover());
        let fps = match view.state {
            State::Speaking => 24.0,
            State::Listening => 20.0,
            _ => 12.0,
        };
        core.paint_orb(ui, Rect::from_center_size(area.center(), Vec2::splat(orb_size)), &params, fps);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(status_text(core, view)).font(theme::serif_italic(21.0)).color(MUTED));
        });
        ui.add_space(10.0);

        // what is being said
        let width = ui.available_width().min(680.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(width);
            if view.captions.is_empty() {
                if let Some(text) = view.last_of(Who::Interviewer) {
                    ui.label(RichText::new(text).font(theme::serif(23.0)).color(theme::mix(MUTED, SNOW, 0.25)));
                }
            } else {
                ui.label(view.caption_job(now, width));
            }
            if let Some(text) = view.last_of(Who::You) {
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{}: {text}", tr.get(K::You))).font(theme::sans(15.0)).color(FAINT));
            }
        });

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

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            if ghost_button(ui, tr.get(K::Transcript)).clicked() {
                view.show_transcript = !view.show_transcript;
            }
        });
        if view.show_transcript {
            widgets::card_frame().show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(240.0).stick_to_bottom(true).auto_shrink([false, true]).show(
                    ui,
                    |ui| {
                        for item in &view.items {
                            let who = if item.who == Who::You {
                                tr.get(K::You).to_owned()
                            } else {
                                view.spec.persona.first_name().to_uppercase()
                            };
                            widgets::caps(ui, &who);
                            ui.label(RichText::new(&item.text).color(if item.who == Who::You { MUTED } else { SNOW }));
                            ui.add_space(6.0);
                        }
                    },
                );
            });
        }
        ui.add_space(30.0);
    });
    None
}

/// The bar along the bottom: microphone, a box to type in, the level, skip and hang up.
pub fn composer(core: &mut Core, view: &mut LiveView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
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
                            let (icon, label) =
                                if view.muted { (Icon::MicOff, K::Unmute) } else { (Icon::Mic, K::Mute) };
                            let tint = (!view.muted && view.has_microphone && view.state == State::Listening)
                                .then_some(theme::CRANBERRY);
                            if icon_button(ui, icon, tr.get(label), 44.0, tint).clicked() {
                                view.muted = !view.muted;
                                view.send(Command::Mute(view.muted));
                            }
                            let send_width = 44.0;
                            let reserved = send_width + 140.0 + 120.0 + 130.0;
                            let edit = ui.add(
                                TextEdit::singleline(&mut view.typed)
                                    .hint_text(tr.get(K::TypeAnswer))
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
                                if widgets::primary_button(ui, tr.get(K::EndCall), true).clicked() {
                                    view.send(Command::Finish);
                                    nav = Some(Nav::CallEnded);
                                }
                                if view.state == State::Speaking && ghost_button(ui, tr.get(K::Interrupt)).clicked() {
                                    view.send(Command::Interrupt);
                                }
                                level_bars(
                                    ui,
                                    vec2(110.0, 28.0),
                                    &view.levels,
                                    if view.state == State::Speaking { BLUSH } else { BERRY },
                                );
                            });
                        });
                    });
            });
        },
    );
    nav
}
