//! Live coding: a page to choose the task, and the call with an editor beside the tutor.
//!
//! The tutor watches the editor like an interviewer would: it lets you think, it notices when a
//! block comes in all at once or the window is left for a long time, and it asks (never tells) when
//! something looks wrong. The sphere, the words under it and the bar at the bottom are the same as
//! in every other call.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{
    Align, Event as InputEvent, FontId, Frame, Id, Layout, Margin, Rect, RichText, ScrollArea, Sense, TextEdit, Ui,
    UiBuilder, Vec2, vec2,
};

use super::orb;
use super::syntax;
use super::theme::{self, BERRY, BLUSH, FAINT, LINE, MUTED, SNOW};
use super::widgets::{self, chip, ghost_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::coding::{LANGUAGES, Task, language_id};
use crate::conversation::{Command, State};
use crate::i18n::K;
use crate::prompts::coding::Level;
use crate::watch::Watcher;

/// The page where the language and the size of the task are chosen.
pub struct SetupView {
    pub language: &'static str,
    pub level: Level,
    pub topic: String,
    pub working: bool,
    pub error: Option<String>,
}

impl SetupView {
    pub fn new(core: &Core) -> Self {
        Self {
            language: guess_language(core),
            level: Level::Interview,
            topic: String::new(),
            working: false,
            error: None,
        }
    }
}

/// The language the vacancy asks for, if it names one; the first of your skills otherwise.
pub fn guess_language(core: &Core) -> &'static str {
    let text = format!("{} {}", core.vacancy.title, core.vacancy.description).to_lowercase();
    let mut best: Option<(usize, &'static str)> = None;
    for word in ["typescript", "javascript", "python", "java ", "c#", "golang", "rust", "c++", "sql"] {
        if let Some(position) = text.find(word)
            && best.is_none_or(|(at, _)| position < at)
        {
            best = language_id(word.trim()).map(|id| (position, id));
        }
    }
    if let Some((_, id)) = best {
        return id;
    }
    core.brief
        .as_ref()
        .and_then(|brief| brief.skill_terms().into_iter().find_map(|skill| language_id(&skill)))
        .unwrap_or("python")
}

pub fn setup(core: &mut Core, view: &mut SetupView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut start = false;
    scrolling_column(ui, 720.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::CodeTitle));
        ui.label(RichText::new(tr.get(K::CodeIntro)).font(theme::sans(16.0)).color(MUTED));
        ui.add_space(18.0);
        widgets::caps(ui, tr.get(K::CodeLanguage));
        ui.horizontal_wrapped(|ui| {
            for (id, name) in LANGUAGES {
                if chip(ui, name, view.language == id).clicked() {
                    view.language = id;
                }
            }
        });
        ui.add_space(14.0);
        widgets::caps(ui, tr.get(K::CodeLevel));
        ui.horizontal_wrapped(|ui| {
            for (level, label) in [(Level::WarmUp, K::CodeWarmUp), (Level::Interview, K::CodeInterview)] {
                if chip(ui, tr.get(label), view.level == level).clicked() {
                    view.level = level;
                }
            }
        });
        ui.add_space(14.0);
        let label = widgets::caps(ui, tr.get(K::CodeTopic));
        ui.add(
            TextEdit::singleline(&mut view.topic)
                .hint_text(tr.get(K::CodeTopicHint))
                .desired_width(f32::INFINITY)
                .margin(vec2(12.0, 9.0)),
        )
        .labelled_by(label.id);
        ui.add_space(12.0);
        ui.label(RichText::new(tr.get(K::CodeLimits)).font(theme::sans(13.5)).color(FAINT));
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            if primary_button(ui, tr.get(K::CodeStart), !view.working).clicked() {
                start = true;
            }
            if view.working {
                ui.spinner();
                ui.label(RichText::new(tr.get(K::CodePreparing)).font(theme::serif_italic(17.0)).color(MUTED));
            }
        });
        if let Some(error) = &view.error {
            ui.add_space(8.0);
            ui.label(RichText::new(error).font(theme::sans(14.5)).color(BERRY));
        }
    });
    if start {
        view.working = true;
        view.error = None;
        core.make_task(view.language, view.level, &view.topic);
        ui.ctx().request_repaint();
    }
    if view.working {
        ui.ctx().request_repaint_after(Duration::from_millis(300));
    }
    nav
}

/// Everything about the coding half of a call.
pub struct CodeSession {
    pub task: Task,
    pub text: String,
    pub watcher: Watcher,
    /// What the tutor has said about the code on its own, so it does not say it twice.
    pub said: Vec<String>,
    pub task_open: bool,
    sent: String,
    changed_at: Option<Instant>,
}

impl CodeSession {
    pub fn new(task: Task) -> Self {
        let text = task.starter.clone();
        Self {
            watcher: Watcher::new(&text, Instant::now()),
            sent: String::new(),
            changed_at: None,
            text,
            task,
            said: Vec::new(),
            task_open: true,
        }
    }
}

const EDITOR_ID: &str = "code-editor";

fn editor_font() -> FontId {
    theme::mono(15.5)
}

/// The numbers of the lines, in the same font and spacing as the editor.
fn gutter(text: &str) -> String {
    (1..=text.lines().count().max(1) + usize::from(text.ends_with('\n')))
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The editor half of a call. Returns what the person asked for, if anything.
pub fn editor(
    core: &mut Core,
    code: &mut CodeSession,
    ui: &mut Ui,
    area: Rect,
    state: State,
    stage_open: bool,
    last_words: &[String],
) -> Vec<Command> {
    let tr = core.tr;
    let now = Instant::now();
    let mut commands = Vec::new();
    let id = Id::unique(EDITOR_ID);
    let had_focus = ui.ctx().memory(|m| m.has_focus(id));
    // what the window says happened to the text this frame
    let (pasted, typed) = ui.input(|input| {
        let mut pasted = 0;
        let mut typed = 0;
        for event in &input.events {
            match event {
                InputEvent::Paste(text) => pasted += text.chars().count(),
                InputEvent::Text(text) => typed += text.chars().count(),
                _ => {}
            }
        }
        (pasted, typed)
    });
    let focused = ui.ctx().input(|i| i.viewport().focused.unwrap_or(true));
    code.watcher.focus(focused, now);

    let mut name = None;
    ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
        ui.horizontal(|ui| {
            name = Some(
                ui.label(
                    RichText::new(crate::coding::language_name(&code.task.language))
                        .font(theme::mono_bold(12.0))
                        .color(BLUSH),
                ),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ghost_button(ui, tr.get(K::CodeStuck)).clicked() {
                    commands.push(Command::Say(tr.get(K::CodeStuckSay).to_owned()));
                }
                if primary_button(ui, tr.get(K::CodeDone), true).clicked() {
                    commands.push(Command::Say(tr.get(K::CodeDoneSay).to_owned()));
                }
            });
        });
        ui.add_space(6.0);
        let height = ui.available_height().max(120.0);
        Frame::new()
            .fill(theme::mix(theme::BG, theme::COCOA, 0.55))
            .stroke(egui::Stroke::new(1.0, LINE))
            .corner_radius(14)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_min_height(height - 18.0);
                ScrollArea::vertical().auto_shrink([false, false]).max_height(height - 18.0).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(gutter(&code.text)).font(editor_font()).color(FAINT));
                        let language = code.task.language.clone();
                        let mut layouter =
                            move |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| -> Arc<egui::Galley> {
                                let job = syntax::highlight(text.as_str(), &language, &editor_font(), wrap);
                                ui.fonts_mut(|fonts| fonts.layout_job(job))
                            };
                        let response = ui.add(
                            TextEdit::multiline(&mut code.text)
                                .id(id)
                                .font(editor_font())
                                .code_editor()
                                .lock_focus(true)
                                .frame(Frame::NONE)
                                .desired_width(f32::INFINITY)
                                .desired_rows(14)
                                .layouter(&mut layouter),
                        );
                        if let Some(name) = &name {
                            response.labelled_by(name.id);
                        }
                    });
                });
            });
    });

    if code.text.contains('\t') {
        code.text = code.text.replace('\t', "    ");
    }
    // a paste only counts when it landed in the editor
    code.watcher.edited(&code.text, if had_focus { pasted } else { 0 }, typed, now);
    if code.text != code.sent {
        code.changed_at.get_or_insert(now);
    }
    // the tutor sees the code with whatever you say next, once it has been still for a moment
    if code.changed_at.is_some_and(|at| now.duration_since(at) >= Duration::from_millis(1500)) && code.text != code.sent
    {
        code.sent = code.text.clone();
        code.changed_at = None;
        commands.push(Command::Code(code.text.clone()));
    }
    ui.ctx().request_repaint_after(Duration::from_millis(500));

    // the quiet look: now and then, or at once when something odd came in
    if state == State::Listening
        && !stage_open
        && let Some(reason) = code.watcher.due(now)
    {
        code.watcher.look(now);
        core.observe_code(&code.task, &code.text, &code.said, &code.watcher.signals(now), last_words);
        let _ = reason;
    }
    commands
}

/// The tutor's half of a call: the sphere, the words, and the task.
pub fn tutor_side(core: &mut Core, ui: &mut Ui, area: Rect, code: &mut CodeSession, words: Narrator<'_>) {
    let tr = core.tr;
    let seconds = core.started.elapsed().as_secs_f32();
    ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
        let orb_size = 132.0;
        let (rect, _) = ui.allocate_exact_size(vec2(area.width(), orb_size), Sense::hover());
        let params = orb::Params { time: seconds, level: words.level, spin: words.spin };
        core.paint_orb(ui, Rect::from_center_size(rect.center(), Vec2::splat(orb_size)), &params, words.fps);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(words.status).font(theme::serif_italic(18.0)).color(MUTED));
        });
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(area.width() - 12.0);
            match words.text {
                Some(job) => ui.label(job),
                None => ui.label(""),
            };
        });
        ui.add_space(12.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            widgets::card_frame().inner_margin(Margin::same(14)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    widgets::caps(ui, tr.get(K::CodeTask));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if code.task_open { "−" } else { "+" };
                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                            code.task_open = !code.task_open;
                        }
                    });
                });
                ui.label(RichText::new(&code.task.title).font(theme::serif(20.0)).color(SNOW));
                if code.task_open {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&code.task.statement).font(theme::sans(14.5)).color(SNOW));
                    if !code.task.examples.is_empty() {
                        ui.add_space(8.0);
                        widgets::caps(ui, tr.get(K::CodeExamples));
                        for example in &code.task.examples {
                            ui.label(
                                RichText::new(format!("{}  →  {}", example.input, example.output))
                                    .font(theme::mono(12.5))
                                    .color(MUTED),
                            );
                        }
                    }
                }
            });
        });
    });
}

/// What the tutor's half needs from the call to draw itself.
pub struct Narrator<'a> {
    pub status: &'a str,
    pub text: Option<egui::text::LayoutJob>,
    pub level: f32,
    pub spin: f32,
    pub fps: f32,
}
