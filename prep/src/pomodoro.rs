//! The focus timer: a block of work, then a short break; after a few blocks, a long one.
//!
//! It is a plain state machine over instants, so it can be tested without waiting. The window asks
//! it for the time left and calls [`Pomodoro::update`] on every frame; it answers with what
//! happened, if anything.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PomodoroSettings {
    pub focus_minutes: u32,
    pub short_minutes: u32,
    pub long_minutes: u32,
    /// A long break comes after this many focus blocks.
    pub long_every: u32,
}

impl Default for PomodoroSettings {
    fn default() -> Self {
        Self { focus_minutes: 25, short_minutes: 5, long_minutes: 15, long_every: 4 }
    }
}

impl PomodoroSettings {
    /// Keeps every length inside what makes sense, whatever the file says.
    pub fn clamped(self) -> Self {
        Self {
            focus_minutes: self.focus_minutes.clamp(1, 120),
            short_minutes: self.short_minutes.clamp(1, 60),
            long_minutes: self.long_minutes.clamp(1, 90),
            long_every: self.long_every.clamp(2, 10),
        }
    }

    fn length(self, phase: Phase) -> Duration {
        let minutes = match phase {
            Phase::Focus => self.focus_minutes,
            Phase::ShortBreak => self.short_minutes,
            Phase::LongBreak => self.long_minutes,
        };
        Duration::from_secs(u64::from(minutes) * 60)
    }
}

/// What the timer reports when a phase ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// A focus block was finished; the break that follows has started.
    FocusDone { next: Phase },
    /// A break is over; the next focus block waits for you to start it.
    BreakDone,
}

#[derive(Debug, Clone)]
pub struct Pomodoro {
    settings: PomodoroSettings,
    phase: Phase,
    running: bool,
    started: Option<Instant>,
    /// Time already spent in this phase before the last pause.
    banked: Duration,
    /// Focus blocks finished since the program started.
    pub blocks: u32,
    /// Focus blocks since the last long break.
    cycle: u32,
}

impl Pomodoro {
    pub fn new(settings: PomodoroSettings) -> Self {
        Self {
            settings: settings.clamped(),
            phase: Phase::Focus,
            running: false,
            started: None,
            banked: Duration::ZERO,
            blocks: 0,
            cycle: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// Whether the timer has been started and not yet finished or reset.
    pub fn in_progress(&self, now: Instant) -> bool {
        self.running || self.elapsed(now) > Duration::ZERO
    }

    pub fn total(&self) -> Duration {
        self.settings.length(self.phase)
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        let live = if self.running {
            self.started.map_or(Duration::ZERO, |at| now.saturating_duration_since(at))
        } else {
            Duration::ZERO
        };
        (self.banked + live).min(self.total())
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.total().saturating_sub(self.elapsed(now))
    }

    /// How much of the phase is behind you, 0 to 1.
    pub fn fraction(&self, now: Instant) -> f32 {
        let total = self.total().as_secs_f32();
        if total <= 0.0 { 1.0 } else { (self.elapsed(now).as_secs_f32() / total).clamp(0.0, 1.0) }
    }

    pub fn start(&mut self, now: Instant) {
        if !self.running {
            self.running = true;
            self.started = Some(now);
        }
    }

    pub fn pause(&mut self, now: Instant) {
        if self.running {
            self.banked = self.elapsed(now);
            self.running = false;
            self.started = None;
        }
    }

    pub fn toggle(&mut self, now: Instant) {
        if self.running { self.pause(now) } else { self.start(now) }
    }

    /// Back to a fresh, stopped focus block.
    pub fn reset(&mut self) {
        self.phase = Phase::Focus;
        self.running = false;
        self.started = None;
        self.banked = Duration::ZERO;
    }

    /// Moves to the next phase without counting the one that was cut short. A skipped break
    /// leads to a stopped focus block; a skipped focus block leads to a running break.
    pub fn skip(&mut self, now: Instant) {
        match self.phase {
            Phase::Focus => self.enter(self.break_after(), true, now),
            Phase::ShortBreak | Phase::LongBreak => self.enter(Phase::Focus, false, now),
        }
    }

    /// New lengths, taking effect at once if the timer is stopped and at the next phase if not.
    pub fn configure(&mut self, settings: PomodoroSettings) {
        self.settings = settings.clamped();
    }

    #[allow(clippy::manual_is_multiple_of)] // is_multiple_of needs a newer Rust than the minimum
    fn break_after(&self) -> Phase {
        if (self.cycle + 1) % self.settings.long_every == 0 { Phase::LongBreak } else { Phase::ShortBreak }
    }

    fn enter(&mut self, phase: Phase, running: bool, now: Instant) {
        self.phase = phase;
        self.banked = Duration::ZERO;
        self.running = running;
        self.started = running.then_some(now);
    }

    /// Call every frame. Ends the phase when its time is up.
    pub fn update(&mut self, now: Instant) -> Option<Signal> {
        if !self.running || self.elapsed(now) < self.total() {
            return None;
        }
        match self.phase {
            Phase::Focus => {
                let next = self.break_after();
                self.blocks += 1;
                self.cycle = if next == Phase::LongBreak { 0 } else { self.cycle + 1 };
                self.enter(next, true, now);
                Some(Signal::FocusDone { next })
            }
            Phase::ShortBreak | Phase::LongBreak => {
                self.enter(Phase::Focus, false, now);
                Some(Signal::BreakDone)
            }
        }
    }
}

/// `mm:ss` for a time left.
pub fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// A soft two-note chime, as mono samples, for the end of a block.
pub fn chime(sample_rate: u32) -> Vec<f32> {
    let note = |hz: f32, seconds: f32, start: usize, out: &mut Vec<f32>| {
        let count = (seconds * sample_rate as f32) as usize;
        if out.len() < start + count {
            out.resize(start + count, 0.0);
        }
        for i in 0..count {
            let t = i as f32 / sample_rate as f32;
            let attack = (t / 0.01).min(1.0);
            let decay = (-t * 4.5).exp();
            let tone = (t * hz * std::f32::consts::TAU).sin() + 0.25 * (t * hz * 2.0 * std::f32::consts::TAU).sin();
            out[start + i] += tone * attack * decay * 0.22;
        }
    };
    let mut pcm = Vec::new();
    note(659.25, 0.9, 0, &mut pcm);
    note(880.0, 1.1, (0.28 * sample_rate as f32) as usize, &mut pcm);
    pcm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, seconds: u64) -> Instant {
        base + Duration::from_secs(seconds)
    }

    fn quick() -> Pomodoro {
        Pomodoro::new(PomodoroSettings { focus_minutes: 2, short_minutes: 1, long_minutes: 3, long_every: 2 })
    }

    #[test]
    fn it_waits_to_be_started_and_counts_down() {
        let t0 = Instant::now();
        let mut timer = quick();
        assert!(!timer.running());
        assert_eq!(timer.remaining(at(t0, 500)), Duration::from_secs(120), "a stopped timer does not move");
        timer.start(t0);
        assert_eq!(timer.remaining(at(t0, 30)), Duration::from_secs(90));
        assert!((timer.fraction(at(t0, 60)) - 0.5).abs() < 0.001);
        assert_eq!(timer.update(at(t0, 119)), None);
    }

    #[test]
    fn pausing_keeps_the_time_and_resuming_goes_on_from_it() {
        let t0 = Instant::now();
        let mut timer = quick();
        timer.start(t0);
        timer.pause(at(t0, 50));
        assert_eq!(timer.remaining(at(t0, 900)), Duration::from_secs(70));
        timer.start(at(t0, 1000));
        assert_eq!(timer.remaining(at(t0, 1010)), Duration::from_secs(60));
        timer.toggle(at(t0, 1010));
        assert!(!timer.running());
    }

    #[test]
    fn a_finished_block_starts_the_break_and_a_finished_break_waits() {
        let t0 = Instant::now();
        let mut timer = quick();
        timer.start(t0);
        assert_eq!(timer.update(at(t0, 120)), Some(Signal::FocusDone { next: Phase::ShortBreak }));
        assert_eq!((timer.phase(), timer.running(), timer.blocks), (Phase::ShortBreak, true, 1));
        assert_eq!(timer.update(at(t0, 181)), Some(Signal::BreakDone));
        assert_eq!((timer.phase(), timer.running()), (Phase::Focus, false), "the next block waits for you");
        assert_eq!(timer.update(at(t0, 9999)), None);
    }

    #[test]
    fn every_few_blocks_the_break_is_long() {
        let t0 = Instant::now();
        let mut timer = quick();
        let mut now = t0;
        let mut breaks = Vec::new();
        for _ in 0..4 {
            timer.start(now);
            now += Duration::from_secs(120);
            if let Some(Signal::FocusDone { next }) = timer.update(now) {
                breaks.push(next);
            }
            now += Duration::from_secs(200);
            timer.update(now);
        }
        assert_eq!(breaks, vec![Phase::ShortBreak, Phase::LongBreak, Phase::ShortBreak, Phase::LongBreak]);
        assert_eq!(timer.blocks, 4);
    }

    #[test]
    fn skipping_does_not_count_a_block() {
        let t0 = Instant::now();
        let mut timer = quick();
        timer.start(t0);
        timer.skip(at(t0, 10));
        assert_eq!((timer.phase(), timer.running(), timer.blocks), (Phase::ShortBreak, true, 0));
        timer.skip(at(t0, 12));
        assert_eq!((timer.phase(), timer.running()), (Phase::Focus, false));
    }

    #[test]
    fn reset_goes_back_to_a_fresh_block() {
        let t0 = Instant::now();
        let mut timer = quick();
        timer.start(t0);
        timer.update(at(t0, 120));
        timer.reset();
        assert_eq!((timer.phase(), timer.running()), (Phase::Focus, false));
        assert_eq!(timer.remaining(at(t0, 130)), Duration::from_secs(120));
        assert!(!timer.in_progress(at(t0, 130)));
    }

    #[test]
    fn silly_lengths_are_brought_back_into_range() {
        let timer =
            Pomodoro::new(PomodoroSettings { focus_minutes: 0, short_minutes: 999, long_minutes: 0, long_every: 0 });
        assert_eq!(timer.total(), Duration::from_secs(60));
        let again = PomodoroSettings { focus_minutes: 9999, ..PomodoroSettings::default() }.clamped();
        assert_eq!(again.focus_minutes, 120);
        assert_eq!(again.long_every, 4);
    }

    #[test]
    fn the_clock_rounds_up_so_it_never_shows_zero_early() {
        assert_eq!(clock(Duration::from_secs(25 * 60)), "25:00");
        assert_eq!(clock(Duration::from_millis(59_400)), "01:00");
        assert_eq!(clock(Duration::ZERO), "00:00");
    }

    #[test]
    fn the_chime_is_audible_but_not_loud_and_ends_quietly() {
        let pcm = chime(24_000);
        assert!(pcm.len() > 24_000 && pcm.len() < 48_000);
        let peak = pcm.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1 && peak < 0.6, "{peak}");
        let tail = pcm[pcm.len() - 200..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(tail < 0.02, "{tail}");
    }
}
