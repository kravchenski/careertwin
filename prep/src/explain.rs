//! A visual explanation: a short animated story, told scene by scene, around the sphere.
//!
//! A [`Storyboard`] is what the model writes: scenes, each with a line to say and a few panels to
//! show (a flow of boxes, a row of cells, a bit of code, a comparison, and so on). A [`Player`]
//! plays it: it knows which scene is on, how far into it the narration is, and when to go on.
//! It never touches the speakers or the screen; it tells the window what to do, and the window
//! tells it what happened. That keeps the timing easy to test.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::prompts::Lang;
use crate::speech::Speech;

/// Panels on screen at once, at most.
pub const MAX_PANELS: usize = 4;
/// Scenes in one explanation, at most.
pub const MAX_SCENES: usize = 10;
/// A breath between two scenes.
const PAUSE: Duration = Duration::from_millis(700);
/// Words a second when there is no voice, to time the captions.
const READING_SPEED: f32 = 2.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PanelKind {
    /// A few short lines, shown one after another.
    #[default]
    Text,
    /// Boxes joined by arrows: a journey from left to right.
    Flow,
    /// A row of cells, some lit: an array, a queue, a table of buckets.
    Cells,
    /// A little code, with the lines that matter lit.
    Code,
    /// Two things side by side.
    Compare,
    /// Numbered steps, the current one lit.
    Steps,
    /// "This is like that": the everyday picture next to the technical one.
    Analogy,
    /// Bars stacked from top to bottom: layers of a system.
    Layers,
}

impl PanelKind {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "flow" | "arrows" | "pipeline" | "diagram" => Self::Flow,
            "cells" | "array" | "row" | "boxes" | "queue" | "stack" | "table" => Self::Cells,
            "code" | "snippet" => Self::Code,
            "compare" | "comparison" | "versus" | "vs" => Self::Compare,
            "steps" | "list" | "numbered" | "algorithm" => Self::Steps,
            "analogy" | "like" | "metaphor" => Self::Analogy,
            "layers" | "stack_of_layers" | "tiers" => Self::Layers,
            _ => Self::Text,
        }
    }
}

/// One side of a comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Side {
    pub title: String,
    pub lines: Vec<String>,
}

/// One floating panel. A panel that keeps its `id` from one scene to the next stays where it is
/// and grows, instead of appearing again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panel {
    pub id: String,
    pub kind: PanelKind,
    pub title: String,
    /// The lines of text, code, steps or layers; for an analogy, the thing and what it is like.
    pub lines: Vec<String>,
    /// The boxes of a flow or the cells of a row.
    pub items: Vec<String>,
    /// Cells to light up.
    pub highlight: Vec<usize>,
    /// The box or step that is the current one.
    pub active: Option<usize>,
    /// A line under the picture.
    pub note: String,
    pub left: Side,
    pub right: Side,
}

impl Panel {
    /// How many things the panel reveals one after another.
    pub fn count(&self) -> usize {
        match self.kind {
            PanelKind::Text | PanelKind::Code | PanelKind::Steps | PanelKind::Layers => self.lines.len(),
            PanelKind::Flow | PanelKind::Cells => self.items.len(),
            PanelKind::Compare => self.left.lines.len().max(self.right.lines.len()) + 1,
            PanelKind::Analogy => 2,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    /// What the tutor says over it, in plain words: it is read aloud.
    pub say: String,
    pub panels: Vec<Panel>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Storyboard {
    pub title: String,
    pub scenes: Vec<Scene>,
    /// A question to ask afterwards, to see whether it landed.
    pub check: String,
    /// The language the narration is written in.
    pub native: Lang,
}

impl Storyboard {
    /// How many items of the panel `id` the previous scene already showed, so they do not appear
    /// again: the panel grows instead.
    pub fn already_shown(&self, scene: usize, panel: &Panel) -> usize {
        let Some(before) = scene.checked_sub(1).and_then(|i| self.scenes.get(i)) else { return 0 };
        before
            .panels
            .iter()
            .find(|p| p.id == panel.id && p.kind == panel.kind)
            .map_or(0, Panel::count)
            .min(panel.count())
    }
}

/// How visible item `index` of `count` is, 0 to 1, `progress` of the way through its scene. The
/// first `base` items were there already. The rest come one by one over the first part of the scene.
pub fn item_visibility(progress: f32, index: usize, base: usize, count: usize) -> f32 {
    if index < base {
        return 1.0;
    }
    let new = count.saturating_sub(base).max(1) as f32;
    let slot = (index - base) as f32;
    // the reveal fills the scene from 5% to 85%
    let start = 0.05 + 0.80 * slot / new;
    let width = (0.80 / new).clamp(0.05, 0.25);
    ((progress - start) / width).clamp(0.0, 1.0)
}

/// How long a line takes to say when nothing is speaking it.
pub fn reading_time(text: &str) -> Duration {
    Duration::from_secs_f32((text.split_whitespace().count() as f32 / READING_SPEED).max(1.5))
}

/// The voice of one scene.
#[derive(Debug, Clone)]
pub enum Audio {
    /// Not asked for yet, or on its way.
    Pending,
    Ready(Speech),
    /// There is no voice for it: the captions carry the scene.
    Silent,
}

/// What the window must do for the player.
#[derive(Debug, Clone, PartialEq)]
pub enum Cue {
    /// Speak this.
    Play(Speech),
    /// Go quiet at once.
    Stop,
    /// The last scene is over.
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The scene is waiting for its voice.
    Waiting,
    Playing,
    Paused,
    Finished,
}

pub struct Player {
    pub board: Storyboard,
    pub index: usize,
    phase: Phase,
    started: Option<Instant>,
    banked: Duration,
    duration: Duration,
    audio: Vec<Audio>,
    requested: Vec<bool>,
    /// The voice of the scene that is on, to say again after a pause.
    current: Option<Speech>,
    /// Paused before the scene had begun: waiting for its voice, not for you.
    unstarted: bool,
}

impl Player {
    pub fn new(board: Storyboard) -> Self {
        let count = board.scenes.len();
        Self {
            board,
            index: 0,
            phase: Phase::Waiting,
            started: None,
            banked: Duration::ZERO,
            duration: Duration::ZERO,
            audio: vec![Audio::Pending; count],
            requested: vec![false; count],
            current: None,
            unstarted: false,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn scene(&self) -> Option<&Scene> {
        self.board.scenes.get(self.index)
    }

    pub fn total(&self) -> usize {
        self.board.scenes.len()
    }

    /// Scenes whose voice should be asked for now: the one on and the next two. Each is named once.
    pub fn voices_to_ask_for(&mut self) -> Vec<usize> {
        let mut wanted = Vec::new();
        for index in self.index..(self.index + 3).min(self.total()) {
            if !self.requested[index] {
                self.requested[index] = true;
                wanted.push(index);
            }
        }
        wanted
    }

    /// A voice arrived, or could not be made.
    pub fn set_voice(&mut self, index: usize, speech: Option<Speech>) {
        if let Some(slot) = self.audio.get_mut(index) {
            *slot = match speech {
                Some(speech) => Audio::Ready(speech),
                None => Audio::Silent,
            };
        }
    }

    /// Whether any scene has been left without a voice, so the window can say so once.
    pub fn has_silent_scene(&self) -> bool {
        self.audio.iter().any(|a| matches!(a, Audio::Silent))
    }

    /// How far into the scene the narration is, 0 to 1.
    pub fn progress(&self, now: Instant) -> f32 {
        if self.phase == Phase::Waiting || self.duration.is_zero() {
            return 0.0;
        }
        (self.elapsed(now).as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn elapsed(&self, now: Instant) -> Duration {
        let live = match (self.phase, self.started) {
            (Phase::Playing, Some(at)) => now.saturating_duration_since(at),
            _ => Duration::ZERO,
        };
        self.banked + live
    }

    fn begin_scene(&mut self, now: Instant) -> Option<Cue> {
        let scene = self.board.scenes.get(self.index)?;
        let (duration, cue) = match &self.audio[self.index] {
            Audio::Pending => return None,
            Audio::Ready(speech) => {
                self.current = Some(speech.clone());
                (Duration::from_secs_f32(speech.seconds()) + PAUSE, Some(Cue::Play(speech.clone())))
            }
            Audio::Silent => {
                self.current = None;
                (reading_time(&scene.say) + PAUSE, None)
            }
        };
        self.duration = duration;
        self.banked = Duration::ZERO;
        self.started = Some(now);
        self.phase = Phase::Playing;
        cue
    }

    /// Call every frame. Starts a scene once its voice is there, and goes on when it is over.
    pub fn update(&mut self, now: Instant) -> Option<Cue> {
        match self.phase {
            Phase::Waiting => self.begin_scene(now),
            Phase::Playing if self.elapsed(now) >= self.duration => {
                if self.index + 1 >= self.total() {
                    self.phase = Phase::Finished;
                    self.started = None;
                    Some(Cue::Finished)
                } else {
                    self.index += 1;
                    self.phase = Phase::Waiting;
                    self.started = None;
                    self.begin_scene(now)
                }
            }
            _ => None,
        }
    }

    pub fn pause(&mut self, now: Instant) -> Option<Cue> {
        match self.phase {
            Phase::Playing => {
                self.banked = self.elapsed(now);
                self.started = None;
                self.unstarted = false;
                self.phase = Phase::Paused;
                Some(Cue::Stop)
            }
            // the voice is still on its way: when it comes, the scene must not start by itself
            Phase::Waiting => {
                self.phase = Phase::Paused;
                self.unstarted = true;
                Some(Cue::Stop)
            }
            Phase::Paused | Phase::Finished => None,
        }
    }

    /// Goes on from where it stopped: the scene's voice starts again from its beginning, and the
    /// pictures wait for it.
    pub fn resume(&mut self, now: Instant) -> Option<Cue> {
        if self.phase != Phase::Paused {
            return None;
        }
        if std::mem::take(&mut self.unstarted) {
            self.phase = Phase::Waiting;
            return self.begin_scene(now);
        }
        self.phase = Phase::Playing;
        self.banked = Duration::ZERO;
        self.started = Some(now);
        self.current.clone().map(Cue::Play)
    }

    pub fn toggle(&mut self, now: Instant) -> Option<Cue> {
        match self.phase {
            Phase::Playing => self.pause(now),
            Phase::Paused => self.resume(now),
            Phase::Finished => self.restart(now),
            Phase::Waiting => self.pause(now),
        }
    }

    /// Jumps to a scene and starts it as soon as its voice is ready.
    pub fn go_to(&mut self, index: usize, now: Instant) -> Vec<Cue> {
        let index = index.min(self.total().saturating_sub(1));
        self.index = index;
        self.phase = Phase::Waiting;
        self.started = None;
        self.banked = Duration::ZERO;
        self.unstarted = false;
        let mut cues = vec![Cue::Stop];
        cues.extend(self.begin_scene(now));
        cues
    }

    pub fn next(&mut self, now: Instant) -> Vec<Cue> {
        self.go_to(self.index + 1, now)
    }

    pub fn previous(&mut self, now: Instant) -> Vec<Cue> {
        self.go_to(self.index.saturating_sub(1), now)
    }

    fn restart(&mut self, now: Instant) -> Option<Cue> {
        self.go_to(0, now).into_iter().find(|cue| matches!(cue, Cue::Play(_)))
    }

    pub fn replay(&mut self, now: Instant) -> Vec<Cue> {
        self.go_to(0, now)
    }

    pub fn finished(&self) -> bool {
        self.phase == Phase::Finished
    }

    /// Jumps to `fraction` of the scene that is playing, for checking how it looks at that moment.
    pub fn seek(&mut self, fraction: f32, now: Instant) {
        if self.phase == Phase::Playing {
            self.banked = self.duration.mul_f32(fraction.clamp(0.0, 0.999));
            self.started = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(say: &str) -> Scene {
        Scene {
            say: say.into(),
            panels: vec![Panel {
                id: "a".into(),
                kind: PanelKind::Text,
                lines: vec!["x".into(); 3],
                ..Panel::default()
            }],
        }
    }

    fn board(scenes: usize) -> Storyboard {
        Storyboard {
            title: "How it works".into(),
            scenes: (0..scenes).map(|i| scene(&format!("Scene {i} says a few words here."))).collect(),
            ..Storyboard::default()
        }
    }

    fn voice(seconds: f32) -> Speech {
        Speech { pcm: vec![0.0; (seconds * 1000.0) as usize], sample_rate: 1000 }
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn items_come_one_by_one_and_the_ones_already_there_stay() {
        // three new items: each has its moment, in order
        let v = |p: f32, i: usize| item_visibility(p, i, 0, 3);
        assert_eq!(v(0.0, 0), 0.0);
        assert!(v(0.31, 0) > 0.99 && v(0.31, 1) < 0.01, "the first is up before the second begins");
        assert!(v(0.6, 1) > 0.99 && v(0.6, 2) < 0.2);
        assert_eq!(v(1.0, 2), 1.0);
        // the first two were on screen in the last scene
        assert_eq!(item_visibility(0.0, 0, 2, 4), 1.0);
        assert_eq!(item_visibility(0.0, 1, 2, 4), 1.0);
        assert_eq!(item_visibility(0.0, 2, 2, 4), 0.0);
        assert_eq!(item_visibility(1.0, 3, 2, 4), 1.0);
        // nothing to reveal does not divide by zero
        assert_eq!(item_visibility(0.5, 0, 0, 0), 1.0);
    }

    #[test]
    fn a_panel_that_keeps_its_id_grows_instead_of_appearing_again() {
        let mut board = board(2);
        board.scenes[0].panels[0].lines = vec!["one".into(), "two".into()];
        board.scenes[1].panels[0].lines = vec!["one".into(), "two".into(), "three".into()];
        let panel = board.scenes[1].panels[0].clone();
        assert_eq!(board.already_shown(1, &panel), 2);
        assert_eq!(board.already_shown(0, &board.scenes[0].panels[0]), 0, "the first scene has nothing before it");
        let mut other = panel.clone();
        other.id = "b".into();
        assert_eq!(board.already_shown(1, &other), 0, "another panel is new");
        let mut cells = panel;
        cells.kind = PanelKind::Cells;
        assert_eq!(board.already_shown(1, &cells), 0, "a different kind is a different picture");
    }

    #[test]
    fn a_scene_waits_for_its_voice_then_plays_it_and_goes_on() {
        let t0 = Instant::now();
        let mut player = Player::new(board(2));
        assert_eq!(player.voices_to_ask_for(), vec![0, 1], "this scene and the next ones");
        assert!(player.voices_to_ask_for().is_empty(), "each is asked for once");
        assert_eq!(player.update(t0), None, "no voice yet: nothing starts");
        assert_eq!(player.phase(), Phase::Waiting);
        player.set_voice(0, Some(voice(2.0)));
        assert!(matches!(player.update(at(t0, 100)), Some(Cue::Play(_))));
        assert_eq!(player.phase(), Phase::Playing);
        assert!((player.progress(at(t0, 1200)) - 1100.0 / 2700.0).abs() < 0.01, "the pause after the voice counts");
        player.set_voice(1, Some(voice(1.0)));
        assert_eq!(player.update(at(t0, 2000)), None, "still the first scene");
        // the first scene ends after the voice and the breath; the second starts at once
        assert!(matches!(player.update(at(t0, 2900)), Some(Cue::Play(_))));
        assert_eq!(player.index, 1);
        assert_eq!(player.update(at(t0, 2900 + 1800)), Some(Cue::Finished));
        assert!(player.finished());
    }

    #[test]
    fn a_scene_without_a_voice_is_timed_by_reading() {
        let t0 = Instant::now();
        let mut player = Player::new(board(1));
        player.voices_to_ask_for();
        player.set_voice(0, None);
        assert_eq!(player.update(t0), None, "no sound to play");
        assert_eq!(player.phase(), Phase::Playing);
        assert!(player.has_silent_scene());
        // "Scene 0 says a few words here." is six words: a bit over two seconds, and the breath
        assert_eq!(player.update(at(t0, 2000)), None);
        assert_eq!(player.update(at(t0, 3500)), Some(Cue::Finished));
    }

    #[test]
    fn pausing_stops_the_voice_and_resuming_says_the_scene_again() {
        let t0 = Instant::now();
        let mut player = Player::new(board(2));
        player.voices_to_ask_for();
        player.set_voice(0, Some(voice(3.0)));
        player.update(t0);
        assert_eq!(player.pause(at(t0, 1000)), Some(Cue::Stop));
        assert_eq!(player.phase(), Phase::Paused);
        assert_eq!(player.update(at(t0, 60_000)), None, "a paused scene does not go on by itself");
        assert!((player.progress(at(t0, 60_000)) - 1000.0 / 3700.0).abs() < 0.01);
        assert!(matches!(player.resume(at(t0, 61_000)), Some(Cue::Play(_))));
        assert_eq!(player.phase(), Phase::Playing);
        assert!(player.progress(at(t0, 61_000)) < 0.01, "the scene begins again with its voice");
        assert_eq!(player.pause(at(t0, 61_100)), Some(Cue::Stop));
        assert_eq!(player.pause(at(t0, 61_200)), None, "pausing twice does nothing");
    }

    #[test]
    fn pausing_before_the_voice_arrives_keeps_the_scene_from_starting() {
        let t0 = Instant::now();
        let mut player = Player::new(board(1));
        player.voices_to_ask_for();
        assert_eq!(player.pause(t0), Some(Cue::Stop), "paused while still waiting for the voice");
        player.set_voice(0, Some(voice(2.0)));
        assert_eq!(player.update(at(t0, 500)), None, "the voice came, and the scene stays put");
        assert_eq!(player.phase(), Phase::Paused);
        assert!(matches!(player.resume(at(t0, 900)), Some(Cue::Play(_))), "play begins it");
        assert_eq!(player.phase(), Phase::Playing);
        assert!(player.progress(at(t0, 900)) < 0.01);
    }

    #[test]
    fn you_can_skip_back_and_replay() {
        let t0 = Instant::now();
        let mut player = Player::new(board(3));
        player.voices_to_ask_for();
        for i in 0..3 {
            player.set_voice(i, Some(voice(1.0)));
        }
        player.update(t0);
        let cues = player.next(at(t0, 100));
        assert_eq!((player.index, cues[0].clone()), (1, Cue::Stop));
        assert!(cues.iter().any(|c| matches!(c, Cue::Play(_))), "the new scene starts at once");
        player.previous(at(t0, 200));
        assert_eq!(player.index, 0);
        player.previous(at(t0, 300));
        assert_eq!(player.index, 0, "there is nothing before the first scene");
        player.go_to(99, at(t0, 400));
        assert_eq!(player.index, 2, "past the end is the last scene");
        player.replay(at(t0, 500));
        assert_eq!(player.index, 0);
    }

    #[test]
    fn a_finished_story_can_be_played_again_with_the_same_button() {
        let t0 = Instant::now();
        let mut player = Player::new(board(1));
        player.voices_to_ask_for();
        player.set_voice(0, Some(voice(1.0)));
        player.update(t0);
        assert_eq!(player.update(at(t0, 2000)), Some(Cue::Finished));
        assert!(matches!(player.toggle(at(t0, 2100)), Some(Cue::Play(_))));
        assert_eq!(player.phase(), Phase::Playing);
        assert_eq!(player.index, 0);
    }

    #[test]
    fn seeking_moves_inside_the_scene_that_is_playing() {
        let t0 = Instant::now();
        let mut player = Player::new(board(1));
        player.voices_to_ask_for();
        player.set_voice(0, Some(voice(4.0)));
        player.update(t0);
        player.seek(0.5, t0);
        assert!((player.progress(t0) - 0.5).abs() < 0.01);
        player.seek(5.0, t0);
        assert!(player.progress(t0) > 0.99 && !player.finished(), "it stops short of the end");
        player.pause(t0);
        player.seek(0.1, t0);
        assert!(player.progress(t0) > 0.9, "a paused scene stays where it is");
    }

    #[test]
    fn an_empty_story_does_not_play_or_panic() {
        let t0 = Instant::now();
        let mut player = Player::new(Storyboard::default());
        assert!(player.voices_to_ask_for().is_empty());
        assert_eq!(player.update(t0), None);
        assert!(player.scene().is_none());
        assert_eq!(player.progress(t0), 0.0);
        player.go_to(0, t0);
    }

    #[test]
    fn panels_know_how_many_things_they_reveal() {
        let flow = Panel { kind: PanelKind::Flow, items: vec!["a".into(), "b".into()], ..Panel::default() };
        let compare = Panel {
            kind: PanelKind::Compare,
            left: Side { title: "L".into(), lines: vec!["1".into(), "2".into()] },
            right: Side { title: "R".into(), lines: vec!["1".into()] },
            ..Panel::default()
        };
        assert_eq!(
            (flow.count(), compare.count(), Panel { kind: PanelKind::Analogy, ..Panel::default() }.count()),
            (2, 3, 2)
        );
        assert_eq!(PanelKind::parse(" Pipeline "), PanelKind::Flow);
        assert_eq!(PanelKind::parse("queue"), PanelKind::Cells);
        assert_eq!(PanelKind::parse("whatever"), PanelKind::Text);
    }

    #[test]
    fn reading_takes_time_even_for_one_word() {
        assert!(reading_time("Hi").as_secs_f32() >= 1.5);
        assert!(reading_time(&"word ".repeat(26)).as_secs_f32() > 9.0);
    }
}
