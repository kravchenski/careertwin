//! The equalizer: a matrix of dots behind the window that rises and falls with the call.
//!
//! It follows the level of the call audio when there is one, breathes when nothing is heard, and runs a wave
//! while an answer is being written.

use egui::{Color32, Painter, Pos2, Rect};

use super::theme;

/// What the window is doing, which sets how the dots move and what colour they have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Listening,
    Thinking,
    Paused,
    Error,
}

/// The level that decays by itself: a loud moment fades out in half a second.
#[derive(Debug, Default)]
pub struct Level {
    value: f32,
    /// What is drawn: `value` followed with a short attack and a longer release, so the dots swell and settle instead of
    /// jumping.
    shown: f32,
    at: f64,
}

/// How quickly the drawn level rises to a louder sound, and how slowly it lets go of it, in seconds.
const ATTACK: f32 = 0.05;
const RELEASE: f32 = 0.30;

/// How fast the level falls, in whole scales per second.
const FALL_PER_SECOND: f32 = 2.0;

impl Level {
    /// A new level from the audio: it only ever lifts the dots, the decay brings them down.
    pub fn push(&mut self, level: f32, now: f64) {
        self.tick(now);
        self.value = self.value.max(level.clamp(0.0, 1.0));
    }

    pub fn tick(&mut self, now: f64) {
        let elapsed = (now - self.at).max(0.0) as f32;
        self.value = (self.value - FALL_PER_SECOND * elapsed).max(0.0);
        let tau = if self.value > self.shown { ATTACK } else { RELEASE };
        self.shown += (self.value - self.shown) * (1.0 - (-elapsed / tau).exp());
        self.at = now;
    }

    /// The level as it is drawn.
    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn smoothed(&self) -> f32 {
        self.shown
    }
}

/// How tall each of `columns` bars is, 0 to 1, at `time` seconds.
pub fn heights(columns: usize, time: f64, level: f32, mood: Mood) -> Vec<f32> {
    let (base, amplitude) = match mood {
        Mood::Idle => (0.10, 0.20),
        Mood::Paused => (0.02, 0.03),
        Mood::Error => (0.04, 0.05),
        Mood::Thinking => (0.24, 0.62),
        Mood::Listening => (0.08 + level * 0.5, 0.15 + level * 0.8),
    };
    let time = time as f32;
    (0..columns)
        .map(|index| {
            let position = if columns > 1 { index as f32 / (columns - 1) as f32 } else { 0.5 };
            let slow = (time * 0.9 - index as f32 * 0.21 + 1.3).sin() * 0.5 + 0.5;
            let quick = (time * 1.7 + index as f32 * 0.55).sin() * 0.5 + 0.5;
            let mut envelope = 0.45 * slow + 0.55 * quick;
            if mood == Mood::Thinking {
                envelope = 0.2 * envelope + 0.8 * ((time * 3.2 - position * 9.0).sin() * 0.5 + 0.5); // a wave that travels
            }
            let middle = 0.62 + 0.38 * (1.0 - (2.0 * position - 1.0).abs()); // the middle is taller than the edges
            ((base + envelope * amplitude) * middle).clamp(0.0, 1.0)
        })
        .collect()
}

pub fn mood_color(mood: Mood) -> Color32 {
    match mood {
        Mood::Idle | Mood::Paused => theme::muted(),
        Mood::Listening => theme::accent(),
        Mood::Thinking => theme::strong(),
        Mood::Error => theme::danger(),
    }
}

/// How long the dots take to pass from the way one mood moves and looks to the way the next does.
const FADE: f64 = 0.45;

fn smooth(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0) as f32;
    t * t * (3.0 - 2.0 * t)
}

/// The motion of the dots over time. A change of mood (listening, thinking, paused) is not a jump: for a moment the dots
/// are the two moods mixed, in height and in colour.
#[derive(Debug)]
pub struct Motion {
    mood: Mood,
    previous: Mood,
    changed_at: f64,
}

impl Default for Motion {
    fn default() -> Self {
        Self { mood: Mood::Idle, previous: Mood::Idle, changed_at: f64::NEG_INFINITY }
    }
}

impl Motion {
    fn weight(&mut self, mood: Mood, time: f64) -> f32 {
        if mood != self.mood {
            // half way through a change the new change starts from what the first had reached
            self.previous = if smooth((time - self.changed_at) / FADE) < 0.5 { self.previous } else { self.mood };
            self.mood = mood;
            self.changed_at = time;
        }
        smooth((time - self.changed_at) / FADE)
    }

    /// How tall each of `columns` dots is at `time`, with the change of mood eased.
    pub fn bars(&mut self, columns: usize, time: f64, level: f32, mood: Mood) -> Vec<f32> {
        let weight = self.weight(mood, time);
        let now = heights(columns, time, level, self.mood);
        if weight >= 1.0 {
            return now;
        }
        let before = heights(columns, time, level, self.previous);
        now.iter().zip(before).map(|(new, old)| old + (new - old) * weight).collect()
    }

    /// The colour of the dots at `time`.
    pub fn color(&mut self, mood: Mood, time: f64) -> Color32 {
        let weight = self.weight(mood, time);
        theme::mix(mood_color(self.previous), mood_color(self.mood), weight)
    }
}

/// How far apart the dots of the background are.
const STEP: f32 = 15.0;

/// How many columns of dots a background of this size has, and how many rows. The tallest bar can reach the top of it.
pub fn grid(width: f32, height: f32) -> (usize, usize) {
    ((width / STEP).floor() as usize, (height / STEP).floor() as usize)
}

/// What the matrix draws: (column, row, brightness) of each lit dot, counting rows from the bottom. The dot a bar is
/// just reaching is only partly lit, so a bar that grows a little changes the picture a little. Kept apart from the
/// painting so it can be checked.
pub fn lit_dots(bars: &[f32], rows: usize) -> Vec<(usize, usize, f32)> {
    let mut lit = Vec::new();
    if rows == 0 {
        return lit;
    }
    for (column, bar) in bars.iter().enumerate() {
        let exact = bar.clamp(0.0, 1.0) * rows as f32;
        let whole = exact.floor() as usize;
        let part = exact - whole as f32;
        let count = (whole + usize::from(part > 0.03)).min(rows);
        for row in 0..count {
            // bright at the bottom of a bar, fading upward, the top dot a little brighter
            let fade = 1.0 - row as f32 / rows as f32;
            let top = if row + 1 == count { 0.25 } else { 0.0 };
            let reaching = if row == whole { part } else { 1.0 };
            lit.push((column, row, ((0.35 + 0.65 * fade + top) * reaching).min(1.0)));
        }
    }
    lit
}

/// The dot matrix behind the window, low in contrast so that the text on it can be read. `bars` are the heights of its
/// columns (see [`grid`] and [`Motion::bars`]); `color` is the colour of the mood; `strength` (0 to 1) is how visible the
/// dots are, so that they step back while an answer is on the screen.
pub fn background(painter: &Painter, rect: Rect, bars: &[f32], color: Color32, strength: f32) {
    let (columns, rows) = grid(rect.width(), rect.height());
    if columns == 0 || rows == 0 || bars.is_empty() {
        return;
    }
    let accent = theme::mix(theme::accent(), color, 0.35);
    let left = rect.left() + (rect.width() - (columns - 1) as f32 * STEP) / 2.0;
    let bottom = rect.bottom() - STEP * 0.9;
    let place = |column: usize, row: usize| Pos2::new(left + column as f32 * STEP, bottom - row as f32 * STEP);
    for column in 0..columns {
        for row in 0..rows {
            painter.circle_filled(place(column, row), 1.4, with_alpha(accent, (14.0 * strength) as u8));
        }
    }
    for (column, row, brightness) in lit_dots(bars, rows) {
        let alpha = (26.0 + 104.0 * brightness) * strength;
        painter.circle_filled(place(column, row), 2.5, with_alpha(accent, alpha as u8));
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bars_stay_between_nothing_and_full_in_every_mood() {
        for mood in [Mood::Idle, Mood::Listening, Mood::Thinking, Mood::Paused, Mood::Error] {
            for level in [0.0, 0.4, 1.0] {
                for step in 0..40 {
                    let bars = heights(40, f64::from(step) * 0.37, level, mood);
                    assert_eq!(bars.len(), 40);
                    assert!(bars.iter().all(|bar| (0.0..=1.0).contains(bar)), "{mood:?} {level}");
                }
            }
        }
        assert!(heights(0, 1.0, 0.5, Mood::Idle).is_empty());
        assert_eq!(heights(1, 1.0, 0.5, Mood::Listening).len(), 1);
    }

    fn average(mood: Mood, level: f32) -> f32 {
        let samples: Vec<f32> = (0..60).flat_map(|step| heights(30, f64::from(step) * 0.21, level, mood)).collect();
        samples.iter().sum::<f32>() / samples.len() as f32
    }

    #[test]
    fn louder_audio_makes_taller_bars_and_a_quiet_window_stays_low() {
        assert!(average(Mood::Listening, 1.0) > average(Mood::Listening, 0.5) + 0.1);
        assert!(average(Mood::Listening, 0.5) > average(Mood::Listening, 0.0) + 0.1);
        assert!(average(Mood::Thinking, 0.0) > average(Mood::Idle, 0.0) + 0.05);
        assert!(average(Mood::Paused, 1.0) < 0.1 && average(Mood::Error, 1.0) < 0.1);
    }

    #[test]
    fn the_middle_is_taller_than_the_edges() {
        let bars = heights(41, 2.0, 0.8, Mood::Listening);
        let edge = (bars[0] + bars[40]) / 2.0;
        let middle: f32 = bars[16..25].iter().sum::<f32>() / 9.0;
        assert!(middle > edge);
    }

    #[test]
    fn the_level_decays_by_itself_and_only_loud_moments_lift_it() {
        let mut level = Level::default();
        level.push(0.8, 10.0);
        assert!((level.value() - 0.8).abs() < 1e-6);
        level.push(0.3, 10.1); // quieter: the dots do not drop at once, nor rise
        assert!(level.value() < 0.8 && level.value() > 0.5);
        level.tick(11.0);
        assert!(level.value() < 0.01);
        level.push(5.0, 11.0); // out of range is clamped
        assert!((level.value() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_drawn_level_follows_a_sound_quickly_and_lets_go_of_it_slowly() {
        let mut level = Level::default();
        level.push(1.0, 10.0);
        level.tick(10.016); // one frame later: on its way up, not there
        assert!(level.smoothed() > 0.1 && level.smoothed() < 0.6, "{}", level.smoothed());
        level.tick(10.3);
        let risen = level.smoothed();
        assert!(risen > 0.3, "{risen}"); // the level itself has fallen to 0.4 by now, and the drawn one has caught up with it
        level.tick(10.9); // the sound is long over (the level itself fell to nothing in half a second)
        assert!(level.smoothed() < risen);
        level.tick(12.0);
        assert!(level.smoothed() < 0.02);
    }

    #[test]
    fn a_change_of_mood_is_a_slide_not_a_jump() {
        let mut motion = Motion::default();
        let level = 0.6;
        let quiet = motion.bars(7, 5.0, level, Mood::Idle);
        let at_change = motion.bars(7, 5.0, level, Mood::Thinking);
        assert_eq!(quiet.len(), 7);
        // the moment of the change looks like the mood before it
        assert!(quiet.iter().zip(&at_change).all(|(a, b)| (a - b).abs() < 0.02), "{quiet:?} {at_change:?}");
        let half = motion.bars(7, 5.0 + FADE / 2.0, level, Mood::Thinking);
        let after = motion.bars(7, 5.0 + FADE * 2.0, level, Mood::Thinking);
        assert_eq!(after, heights(7, 5.0 + FADE * 2.0, level, Mood::Thinking));
        let (old, new) =
            (heights(7, 5.0 + FADE / 2.0, level, Mood::Idle), heights(7, 5.0 + FADE / 2.0, level, Mood::Thinking));
        for index in 0..7 {
            let (low, high) = (old[index].min(new[index]) - 1e-4, old[index].max(new[index]) + 1e-4);
            assert!(
                (low..=high).contains(&half[index]),
                "dot {index}: {} not between {} and {}",
                half[index],
                old[index],
                new[index]
            );
        }
        // the colour slides too
        let mut motion = Motion::default();
        motion.color(Mood::Idle, 1.0);
        let start = motion.color(Mood::Listening, 1.0);
        let end = motion.color(Mood::Listening, 1.0 + FADE * 2.0);
        assert_eq!(start, mood_color(Mood::Idle));
        assert_eq!(end, mood_color(Mood::Listening));
    }

    #[test]
    fn the_matrix_fits_the_window_gets_taller_with_the_level_and_lights_the_top_dot_by_degrees() {
        let (columns, rows) = grid(900.0, 400.0);
        assert_eq!((columns, rows), (60, 26));
        let bars = |level: f32| heights(columns, 3.0, level, Mood::Listening);
        let (quiet, loud) = (lit_dots(&bars(0.0), rows), lit_dots(&bars(1.0), rows));
        assert!(loud.len() > quiet.len() * 2, "{} vs {}", loud.len(), quiet.len());
        assert!(
            loud.iter()
                .all(|(column, row, brightness)| *column < columns && *row < rows && (0.0..=1.0).contains(brightness))
        );
        assert!(lit_dots(&bars(1.0), 0).is_empty() && lit_dots(&[], 10).is_empty());
        // a bar that is a little taller lights its top dot a little more, not a whole dot at once
        let half = lit_dots(&[3.5 / rows as f32], rows); // three and a half dots tall
        let top = half.last().unwrap();
        assert_eq!(half.len(), 4);
        assert!(top.2 > 0.0 && top.2 < half[2].2, "the partly reached dot is dimmer than the whole ones: {half:?}");
    }
}
