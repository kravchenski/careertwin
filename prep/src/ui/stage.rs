//! The stage of a visual explanation: panels floating over the screen, the sphere in the corner as
//! the narrator with the words under it, and the controls of a video. It is used inside a call, when
//! the tutor draws something, and on its own.

use std::time::{Duration, Instant};

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Layout, Rect, RichText, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use super::Core;
use super::holo::{self, Clock};
use super::orb;
use super::theme::{self, BERRY, BLUSH, FAINT, LINE, MUTED, SNOW};
use super::widgets::{Icon, ghost_button, icon_button};
use crate::explain::{Cue, Panel, Phase, Player, Storyboard};
use crate::i18n::K;

/// How long the panels of the last scene take to fade when the next one comes.
const FADE_OUT: Duration = Duration::from_millis(350);
/// How long a new panel takes to appear.
const FADE_IN: f32 = 0.55;

pub enum Mood {
    /// The model is writing the story.
    Making,
    Failed(String),
    Ready(Box<Player>),
}

pub struct Picture {
    pub id: u64,
    pub topic: String,
    /// What the person knows or asked, kept for asking again for a simpler version.
    pub context: String,
    pub simpler: bool,
    pub state: Mood,
    /// Set when you spoke or typed: the next frame pauses the story.
    pub pause_soon: bool,
    /// Set when a scene could not be given a voice.
    pub voice_problem: Option<String>,
    scene_at: Instant,
    shown: Option<usize>,
    left_behind: Option<(usize, Instant)>,
    level: f32,
}

impl Picture {
    pub fn making(id: u64, topic: &str, context: &str, simpler: bool) -> Self {
        Self {
            id,
            topic: topic.trim().to_owned(),
            context: context.trim().to_owned(),
            simpler,
            state: Mood::Making,
            pause_soon: false,
            voice_problem: None,
            scene_at: Instant::now(),
            shown: None,
            left_behind: None,
            level: 0.0,
        }
    }

    pub fn ready(&mut self, board: Storyboard) {
        self.state = Mood::Ready(Box::new(Player::new(board)));
        self.scene_at = Instant::now();
    }

    pub fn player(&self) -> Option<&Player> {
        match &self.state {
            Mood::Ready(player) => Some(player),
            _ => None,
        }
    }

    pub fn player_mut(&mut self) -> Option<&mut Player> {
        match &mut self.state {
            Mood::Ready(player) => Some(player),
            _ => None,
        }
    }
}

/// What the person did with the stage this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// The story played to the end.
    Finished,
    /// The close button.
    Closed,
    /// "Explain more simply".
    Simpler,
}

/// The words of a scene: spoken ones bright, the one being said lighter, the rest dim.
fn caption(text: &str, progress: f32, width: f32, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let words: Vec<&str> = text.split_whitespace().collect();
    let spoken = (progress * 1.08 * words.len() as f32).floor() as usize;
    for (index, word) in words.iter().enumerate() {
        let color = if index < spoken {
            SNOW
        } else if index == spoken {
            BLUSH
        } else {
            theme::mix(FAINT, MUTED, 0.35)
        };
        let piece = if index == 0 { (*word).to_owned() } else { format!(" {word}") };
        job.append(&piece, 0.0, TextFormat { font_id: theme::serif(size), color, ..Default::default() });
    }
    job
}

fn continuing(board: &Storyboard, scene: usize, panel: &Panel) -> bool {
    scene
        .checked_sub(1)
        .and_then(|i| board.scenes.get(i))
        .is_some_and(|before| before.panels.iter().any(|p| p.id == panel.id && p.kind == panel.kind))
}

/// Does what the player asks of the speakers.
fn perform(core: &Core, cue: Cue) -> bool {
    match cue {
        Cue::Play(speech) => {
            core.services.speaker.stop();
            core.services.speaker.enqueue(speech.pcm, speech.sample_rate);
            false
        }
        Cue::Stop => {
            core.services.speaker.stop();
            false
        }
        Cue::Finished => true,
    }
}

/// Plays and paints the stage in `area`. Returns what the person did.
pub fn paint(core: &mut Core, stage: &mut Picture, ui: &mut Ui, area: Rect) -> Outcome {
    let tr = core.tr;
    let now = Instant::now();
    let mut outcome = Outcome::Nothing;
    let seconds = core.started.elapsed().as_secs_f32();
    ui.ctx().request_repaint_after(Duration::from_millis(33));
    // the header: the title at the left, the controls at the right
    let header = Rect::from_min_size(area.min, vec2(area.width(), 44.0));
    let captions = Rect::from_min_max(pos2(area.left(), area.bottom() - 124.0), area.max);
    let stage_area =
        Rect::from_min_max(pos2(area.left(), header.bottom() + 6.0), pos2(area.right(), captions.top() - 6.0));

    match &mut stage.state {
        Mood::Making => {
            let orb_size = 180.0;
            let rect = Rect::from_center_size(area.center() - vec2(0.0, 30.0), Vec2::splat(orb_size));
            let params = orb::Params { time: seconds, level: 0.16 + 0.08 * (seconds * 3.0).sin().abs(), spin: 1.6 };
            core.paint_orb(ui, rect, &params, 14.0);
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(Rect::from_center_size(rect.center_bottom() + vec2(0.0, 40.0), vec2(area.width(), 40.0))),
                |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(tr.get(K::StageMaking)).font(theme::serif_italic(21.0)).color(MUTED));
                    });
                },
            );
            return close_button(core, ui, header, outcome);
        }
        Mood::Failed(message) => {
            let message = message.clone();
            ui.scope_builder(
                UiBuilder::new().max_rect(Rect::from_center_size(area.center(), vec2(560.0, 200.0))),
                |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(tr.get(K::StageFailed)).font(theme::serif(24.0)).color(SNOW));
                        ui.label(RichText::new(message).font(theme::sans(14.5)).color(BERRY));
                        ui.add_space(8.0);
                        if ghost_button(ui, tr.get(K::StageAgain)).clicked() {
                            outcome = Outcome::Simpler;
                        }
                    });
                },
            );
            return close_button(core, ui, header, outcome);
        }
        Mood::Ready(_) => {}
    }

    let id = stage.id;
    let Mood::Ready(player) = &mut stage.state else { return outcome };
    // voices, one by one, a little ahead
    for index in player.voices_to_ask_for() {
        let say = player.board.scenes[index].say.clone();
        core.narrate(id, index, say, player.board.native);
    }
    if std::mem::take(&mut stage.pause_soon)
        && let Some(cue) = player.pause(now)
    {
        perform(core, cue);
    }
    if let Some(cue) = player.update(now)
        && perform(core, cue)
    {
        outcome = Outcome::Finished;
    }
    if stage.shown != Some(player.index) {
        stage.left_behind = stage.shown.map(|index| (index, now));
        stage.scene_at = now;
        stage.shown = Some(player.index);
    }
    let progress = player.progress(now);
    let index = player.index;

    // the title and the scene dots
    ui.scope_builder(UiBuilder::new().max_rect(header).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.label(RichText::new(&player.board.title).font(theme::serif(24.0)).color(SNOW));
    });

    // the panels
    if let Some(scene) = player.board.scenes.get(index) {
        let rects = holo::layout(scene.panels.len(), stage_area);
        let since = now.saturating_duration_since(stage.scene_at).as_secs_f32();
        let painter = ui.painter_at(stage_area.expand(24.0));
        if let Some((before, left_at)) = stage.left_behind {
            let gone = now.saturating_duration_since(left_at);
            if gone < FADE_OUT
                && let Some(old) = player.board.scenes.get(before)
            {
                let old_rects = holo::layout(old.panels.len(), stage_area);
                for (panel, rect) in old.panels.iter().zip(old_rects) {
                    if scene.panels.iter().any(|p| p.id == panel.id && p.kind == panel.kind) {
                        continue;
                    }
                    let clock = Clock {
                        appear: 1.0 - gone.as_secs_f32() / FADE_OUT.as_secs_f32(),
                        progress: 1.0,
                        time: seconds,
                        base: panel.count(),
                    };
                    holo::paint_panel(&painter, rect, panel, &clock);
                }
            }
        }
        for (i, (panel, rect)) in scene.panels.iter().zip(rects).enumerate() {
            let appear = if continuing(&player.board, index, panel) { 1.0 } else { holo::ease(since / FADE_IN) };
            let drift =
                vec2((seconds * 0.7 + i as f32 * 1.7).sin() * 3.0, (seconds * 0.6 + i as f32 * 2.3).cos() * 2.5);
            let rise = (1.0 - appear) * 14.0;
            let clock = Clock { appear, progress, time: seconds, base: player.board.already_shown(index, panel) };
            holo::paint_panel(&painter, rect.translate(drift + vec2(0.0, rise)), panel, &clock);
        }
    }

    // the narrator: the sphere at the left, the words next to it
    let speaking = player.phase() == Phase::Playing;
    let target = if speaking { core.services.speaker.level().max(0.12) } else { 0.04 };
    stage.level += (target - stage.level) * 0.25;
    let orb_size = 104.0 * (1.0 + 0.08 * stage.level);
    let orb_rect = Rect::from_center_size(pos2(captions.left() + 62.0, captions.center().y), Vec2::splat(orb_size));
    let params =
        orb::Params { time: seconds, level: stage.level, spin: if speaking { 1.0 + stage.level } else { 0.5 } };
    core.paint_orb(ui, orb_rect, &params, if speaking { 24.0 } else { 10.0 });
    let words = Rect::from_min_max(
        pos2(captions.left() + 132.0, captions.top() + 6.0),
        pos2(captions.right() - 6.0, captions.bottom() - 4.0),
    );
    if let Some(scene) = player.board.scenes.get(index) {
        ui.scope_builder(UiBuilder::new().max_rect(words).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.label(caption(&scene.say, progress, words.width(), 24.0));
        });
    }

    // the controls
    let controls = Rect::from_min_size(pos2(header.right() - 330.0, header.top()), vec2(330.0, header.height()));
    let total = player.total();
    ui.scope_builder(UiBuilder::new().max_rect(controls).layout(Layout::right_to_left(Align::Center)), |ui| {
        if icon_button(ui, Icon::Close, tr.get(K::StageClose), 36.0, None).clicked() {
            outcome = Outcome::Closed;
        }
        if ghost_button(ui, tr.get(K::StageSimpler)).clicked() {
            outcome = Outcome::Simpler;
        }
        if icon_button(ui, Icon::Refresh, tr.get(K::StageReplay), 36.0, None).clicked() {
            for cue in player.replay(now) {
                perform(core, cue);
            }
        }
        if icon_button(ui, Icon::Skip, tr.get(K::StageNext), 36.0, None).clicked() {
            for cue in player.next(now) {
                perform(core, cue);
            }
        }
        let (icon, label) = if player.phase() == Phase::Paused || player.finished() {
            (Icon::Play, K::StagePlay)
        } else {
            (Icon::Pause, K::StagePause)
        };
        if icon_button(ui, icon, tr.get(label), 36.0, Some(theme::CRANBERRY)).clicked()
            && let Some(cue) = player.toggle(now)
        {
            perform(core, cue);
        }
        if icon_button(ui, Icon::Prev, tr.get(K::StagePrevious), 36.0, None).clicked() {
            for cue in player.previous(now) {
                perform(core, cue);
            }
        }
    });
    // where in the story we are
    let dots_y = header.bottom() - 4.0;
    let left = area.center().x - total as f32 * 7.0;
    for i in 0..total {
        let center = pos2(left + 7.0 + i as f32 * 14.0, dots_y);
        let (color, radius) = if i < index {
            (BERRY, 3.5)
        } else if i == index {
            (BLUSH, 4.5)
        } else {
            (LINE, 3.0)
        };
        ui.painter().circle_filled(center, radius, color);
    }
    if let Some(problem) = &stage.voice_problem {
        let note = Rect::from_min_size(
            pos2(captions.left() + 132.0, captions.bottom() - 22.0),
            vec2(captions.width() - 140.0, 18.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(note), |ui| {
            ui.label(RichText::new(problem).font(theme::mono(11.5)).color(FAINT));
        });
    }
    ui.allocate_rect(area, Sense::hover());
    outcome
}

/// The close button alone, for the moments before there is a story.
fn close_button(core: &Core, ui: &mut Ui, header: Rect, outcome: Outcome) -> Outcome {
    let tr = core.tr;
    let mut outcome = outcome;
    let at = Rect::from_min_size(pos2(header.right() - 44.0, header.top()), vec2(44.0, header.height()));
    ui.scope_builder(UiBuilder::new().max_rect(at).layout(Layout::right_to_left(Align::Center)), |ui| {
        if icon_button(ui, Icon::Close, tr.get(K::StageClose), 36.0, None).clicked() {
            outcome = Outcome::Closed;
        }
    });
    outcome
}

/// A shorter name for what is being explained, for titles and notes.
pub fn shorten(text: &str, limit: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= limit {
        return text;
    }
    let cut: String = text.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}
