//! The equalizer of dots: seven dots next to the status, and the same thing as a dot matrix behind the window.
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
    at: f64,
}

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
        self.at = now;
    }

    pub fn value(&self) -> f32 {
        self.value
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

fn mood_color(mood: Mood) -> Color32 {
    match mood {
        Mood::Idle | Mood::Paused => theme::muted(),
        Mood::Listening => theme::accent(),
        Mood::Thinking => theme::warn(),
        Mood::Error => theme::danger(),
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Seven dots in a row, as many lit as the level says.
pub const DOTS: usize = 7;

pub fn dots(painter: &Painter, rect: Rect, level: f32, mood: Mood, time: f64) {
    let bars = heights(DOTS, time, level, mood);
    let color = mood_color(mood);
    let spacing = (rect.width() - 12.0) / (DOTS - 1) as f32;
    for (index, height) in bars.iter().enumerate() {
        let lit = (height * 1.8).clamp(0.0, 1.0) > 0.2 + 0.08 * index as f32 / DOTS as f32;
        let centre = Pos2::new(rect.left() + 6.0 + index as f32 * spacing, rect.center().y);
        let (radius, fill) = if lit { (3.6, color) } else { (2.8, with_alpha(theme::muted(), 80)) };
        painter.circle_filled(centre, radius, fill);
    }
}

/// How far apart the dots of the background are.
const STEP: f32 = 15.0;
/// The tallest a bar of the background goes, as a part of the window's height.
const REACH: f32 = 0.62;

/// What the matrix would draw: (column, row) of each lit dot and how bright, counting rows from the bottom.
/// Kept apart from the painting so it can be checked.
pub fn lit_dots(width: f32, height: f32, level: f32, mood: Mood, time: f64) -> Vec<(usize, usize, f32)> {
    let columns = (width / STEP).floor() as usize;
    let rows = ((height * REACH) / STEP).floor() as usize;
    if columns == 0 || rows == 0 {
        return Vec::new();
    }
    let mut lit = Vec::new();
    for (column, bar) in heights(columns, time, level, mood).into_iter().enumerate() {
        let count = ((bar * rows as f32).round() as usize).min(rows);
        for row in 0..count {
            // bright at the bottom of a bar, fading upward, the top dot a little brighter
            let fade = 1.0 - row as f32 / rows as f32;
            let top = if row + 1 == count { 0.25 } else { 0.0 };
            lit.push((column, row, (0.35 + 0.65 * fade + top).min(1.0)));
        }
    }
    lit
}

/// The dot matrix behind the window, low in contrast so that the text on it can be read.
pub fn background(painter: &Painter, rect: Rect, level: f32, mood: Mood, time: f64) {
    let color = mood_color(mood);
    let accent = if mood == Mood::Listening { theme::accent() } else { mix_accent(color) };
    let columns = (rect.width() / STEP).floor() as usize;
    let rows = ((rect.height() * REACH) / STEP).floor() as usize;
    if columns == 0 || rows == 0 {
        return;
    }
    let left = rect.left() + (rect.width() - (columns - 1) as f32 * STEP) / 2.0;
    let bottom = rect.bottom() - STEP * 0.9;
    let place = |column: usize, row: usize| Pos2::new(left + column as f32 * STEP, bottom - row as f32 * STEP);
    for column in 0..columns {
        for row in 0..rows {
            painter.circle_filled(place(column, row), 1.4, with_alpha(accent, 14));
        }
    }
    for (column, row, brightness) in lit_dots(rect.width(), rect.height(), level, mood, time) {
        painter.circle_filled(place(column, row), 2.5, with_alpha(accent, (26.0 + 104.0 * brightness) as u8));
    }
}

/// The accent, softened toward the colour of the mood, so a waiting window stays in the theme's colour.
fn mix_accent(mood: Color32) -> Color32 {
    theme::mix(theme::accent(), mood, 0.35)
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
    fn the_matrix_fits_the_window_and_gets_taller_with_the_level() {
        let quiet = lit_dots(900.0, 400.0, 0.0, Mood::Listening, 3.0);
        let loud = lit_dots(900.0, 400.0, 1.0, Mood::Listening, 3.0);
        assert!(loud.len() > quiet.len() * 2, "{} vs {}", loud.len(), quiet.len());
        let (columns, rows) = (60, 16);
        assert!(
            loud.iter()
                .all(|(column, row, brightness)| *column < columns && *row < rows && (0.0..=1.0).contains(brightness))
        );
        assert!(lit_dots(10.0, 10.0, 1.0, Mood::Listening, 0.0).is_empty()); // too small for a dot
        // a dot at the bottom of a bar is brighter than one near the top
        let bar: Vec<_> = loud.iter().filter(|(column, ..)| *column == 30).collect();
        assert!(bar.first().unwrap().2 > bar[bar.len() / 2].2);
    }
}
