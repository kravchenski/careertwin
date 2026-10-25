//! Watching a live-coding session: what changed in the editor, whether a block came in all at once,
//! whether the window was left for long, and when it is time for the tutor to look at the code.
//!
//! It sees only what happens inside this program's window and editor: it cannot see other windows
//! or the screen, and it does not try to. What it can say is "a block of 300 characters appeared at
//! once" or "the window was out of focus for a minute"; the tutor decides what to make of it.

use std::time::{Duration, Instant};

/// A block this long appearing at once is not typing.
pub const PASTE_CHARS: usize = 40;
/// Away for at least this long is worth mentioning; shorter is a glance at the clock.
pub const AWAY_SECONDS: u64 = 8;
/// How long the code must sit still before the tutor looks at it.
const SETTLE: Duration = Duration::from_secs(5);
/// The least time between two looks, unless a block was pasted.
const BETWEEN_LOOKS: Duration = Duration::from_secs(20);
/// No typing for this long counts as being stuck.
const IDLE: Duration = Duration::from_secs(60);
/// Between two looks while idle.
const BETWEEN_IDLE_LOOKS: Duration = Duration::from_secs(75);
/// A jump in length that is too big to be typing, when no paste was reported.
const SUDDEN_GROWTH: usize = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paste {
    /// Seconds since the session began.
    pub at: u32,
    pub chars: usize,
    pub lines: usize,
    /// Whether the tutor has been told.
    pub told: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Away {
    pub at: u32,
    pub seconds: u32,
    pub told: bool,
}

/// Why the tutor should look at the code now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The code stopped changing for a few seconds.
    Settled,
    /// Nothing has happened for a long time.
    Idle,
    /// A block came in all at once.
    Pasted,
    /// The window was left for a while and something was written after.
    Returned,
}

#[derive(Debug)]
pub struct Watcher {
    started: Instant,
    code: String,
    changed_at: Option<Instant>,
    looked_at: Option<Instant>,
    looked_code: String,
    pub pastes: Vec<Paste>,
    pub away: Vec<Away>,
    away_since: Option<Instant>,
    /// A look is in progress.
    pub looking: bool,
}

impl Watcher {
    pub fn new(code: &str, now: Instant) -> Self {
        Self {
            started: now,
            code: code.to_owned(),
            changed_at: None,
            looked_at: None,
            looked_code: code.to_owned(),
            pastes: Vec::new(),
            away: Vec::new(),
            away_since: None,
            looking: false,
        }
    }

    fn seconds(&self, now: Instant) -> u32 {
        now.saturating_duration_since(self.started).as_secs().min(u64::from(u32::MAX)) as u32
    }

    /// The editor holds `code` now. `pasted` is the length of a paste the window reported this
    /// frame, `typed` how many characters were typed.
    pub fn edited(&mut self, code: &str, pasted: usize, typed: usize, now: Instant) {
        if code == self.code {
            return;
        }
        let grown = code.chars().count().saturating_sub(self.code.chars().count());
        let block = if pasted > 0 {
            pasted
        } else if grown >= SUDDEN_GROWTH && typed * 4 < grown {
            grown
        } else {
            0
        };
        if block >= PASTE_CHARS {
            let lines = code.lines().count().saturating_sub(self.code.lines().count()).max(1);
            self.pastes.push(Paste { at: self.seconds(now), chars: block, lines, told: false });
        }
        self.code = code.to_owned();
        self.changed_at = Some(now);
    }

    /// Whether the window has the keyboard. Being away is only recorded when it was long enough to
    /// be more than a glance.
    pub fn focus(&mut self, focused: bool, now: Instant) {
        match (focused, self.away_since) {
            (false, None) => self.away_since = Some(now),
            (true, Some(since)) => {
                self.away_since = None;
                let gone = now.saturating_duration_since(since);
                if gone >= Duration::from_secs(AWAY_SECONDS) {
                    self.away.push(Away { at: self.seconds(since), seconds: gone.as_secs() as u32, told: false });
                }
            }
            _ => {}
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    /// Whether the tutor should look at the code now, and why. Never while a look is going on.
    pub fn due(&self, now: Instant) -> Option<Reason> {
        if self.looking {
            return None;
        }
        let since_look = self.looked_at.map(|at| now.saturating_duration_since(at));
        if self.pastes.iter().any(|p| !p.told) {
            return Some(Reason::Pasted);
        }
        let changed_since_look = self.code != self.looked_code;
        let still_for = self.changed_at.map(|at| now.saturating_duration_since(at));
        if changed_since_look && still_for.is_some_and(|d| d >= SETTLE) && since_look.is_none_or(|d| d >= BETWEEN_LOOKS)
        {
            return if self.away.iter().any(|a| !a.told && a.seconds >= 20) {
                Some(Reason::Returned)
            } else {
                Some(Reason::Settled)
            };
        }
        let quiet = still_for.unwrap_or_else(|| now.saturating_duration_since(self.started));
        if !changed_since_look && quiet >= IDLE && since_look.is_none_or(|d| d >= BETWEEN_IDLE_LOOKS) {
            return Some(Reason::Idle);
        }
        None
    }

    /// A look begins: everything seen so far is told, and the code is remembered.
    pub fn look(&mut self, now: Instant) {
        self.looked_at = Some(now);
        self.looked_code = self.code.clone();
        self.looking = true;
    }

    /// The look is over.
    pub fn looked(&mut self) {
        self.looking = false;
    }

    /// The things the tutor has not been told yet, in words for it. Marks them as told.
    pub fn signals(&mut self, now: Instant) -> String {
        let mut lines = Vec::new();
        let here = self.seconds(now);
        for paste in self.pastes.iter_mut().filter(|p| !p.told) {
            paste.told = true;
            lines.push(format!(
                "A block of {} characters ({} line{}) appeared all at once {} seconds ago; it was not typed.",
                paste.chars,
                paste.lines,
                if paste.lines == 1 { "" } else { "s" },
                here.saturating_sub(paste.at)
            ));
        }
        for gone in self.away.iter_mut().filter(|a| !a.told) {
            gone.told = true;
            lines.push(format!(
                "The window was out of focus for {} seconds, {} seconds ago.",
                gone.seconds,
                here.saturating_sub(gone.at + gone.seconds)
            ));
        }
        if let Some(at) = self.changed_at {
            let quiet = now.saturating_duration_since(at).as_secs();
            if quiet >= 30 {
                lines.push(format!("Nothing has been typed for {quiet} seconds."));
            }
        } else if here >= 30 {
            lines.push(format!("Nothing has been typed yet; the session is {here} seconds old."));
        }
        lines.push(format!("The session is {} minutes old.", here / 60));
        lines.join("\n")
    }

    /// A line for the review: how the session went in these terms.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.pastes.is_empty() {
            let chars: usize = self.pastes.iter().map(|p| p.chars).sum();
            parts.push(format!("{} block(s) of {} characters in total appeared all at once", self.pastes.len(), chars));
        }
        if !self.away.is_empty() {
            let seconds: u32 = self.away.iter().map(|a| a.seconds).sum();
            parts.push(format!("the window was left {} time(s), {} seconds in total", self.away.len(), seconds));
        }
        if parts.is_empty() { "nothing unusual".to_owned() } else { parts.join("; ") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, seconds: u64) -> Instant {
        base + Duration::from_secs(seconds)
    }

    #[test]
    fn typing_is_not_a_paste_and_a_reported_paste_is() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("def f():\n", t0);
        watcher.edited("def f():\n    return 1\n", 0, 12, at(t0, 5));
        assert!(watcher.pastes.is_empty(), "twelve typed characters");
        let block = "    total = 0\n    for x in items:\n        total += x\n    return total\n";
        watcher.edited(&format!("def f(items):\n{block}"), block.len(), 0, at(t0, 9));
        assert_eq!(watcher.pastes.len(), 1);
        assert_eq!(watcher.pastes[0].chars, block.len());
        assert!(watcher.pastes[0].lines >= 3);
        assert_eq!(watcher.pastes[0].at, 9);
    }

    #[test]
    fn a_small_paste_is_just_a_few_words() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        watcher.edited("items", 5, 0, at(t0, 1));
        assert!(watcher.pastes.is_empty());
    }

    #[test]
    fn a_jump_in_length_without_typing_is_a_paste_even_if_nobody_said_so() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("x = 1\n", t0);
        let big = "y".repeat(200);
        watcher.edited(&format!("x = 1\n{big}"), 0, 0, at(t0, 4));
        assert_eq!(watcher.pastes.len(), 1);
        // but fast typing, with its keys counted, is not
        let mut typist = Watcher::new("", t0);
        typist.edited(&"z".repeat(100), 0, 100, at(t0, 4));
        assert!(typist.pastes.is_empty());
    }

    #[test]
    fn leaving_the_window_is_noted_only_when_it_was_long_enough_to_matter() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        watcher.focus(false, at(t0, 10));
        watcher.focus(true, at(t0, 13));
        assert!(watcher.away.is_empty(), "three seconds is a glance");
        watcher.focus(false, at(t0, 20));
        watcher.focus(false, at(t0, 25));
        watcher.focus(true, at(t0, 80));
        assert_eq!(watcher.away.len(), 1);
        assert_eq!((watcher.away[0].at, watcher.away[0].seconds), (20, 60));
        watcher.focus(true, at(t0, 90));
        assert_eq!(watcher.away.len(), 1, "coming back twice is nothing");
    }

    #[test]
    fn the_code_is_looked_at_once_it_has_settled_and_not_too_often() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        assert_eq!(watcher.due(at(t0, 3)), None, "nothing has changed");
        watcher.edited("def f(): pass", 0, 13, at(t0, 4));
        assert_eq!(watcher.due(at(t0, 6)), None, "still being typed");
        assert_eq!(watcher.due(at(t0, 10)), Some(Reason::Settled));
        watcher.look(at(t0, 10));
        assert_eq!(watcher.due(at(t0, 11)), None, "a look is going on");
        watcher.looked();
        watcher.edited("def f(): return 1", 0, 9, at(t0, 12));
        assert_eq!(watcher.due(at(t0, 18)), None, "too soon after the last look");
        assert_eq!(watcher.due(at(t0, 31)), Some(Reason::Settled));
        watcher.look(at(t0, 31));
        watcher.looked();
        assert_eq!(watcher.due(at(t0, 50)), None, "nothing new to see");
    }

    #[test]
    fn a_long_silence_asks_for_a_look_but_not_again_at_once() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("def f(): pass", t0);
        assert_eq!(watcher.due(at(t0, 40)), None);
        assert_eq!(watcher.due(at(t0, 65)), Some(Reason::Idle));
        watcher.look(at(t0, 65));
        watcher.looked();
        assert_eq!(watcher.due(at(t0, 100)), None);
        assert_eq!(watcher.due(at(t0, 145)), Some(Reason::Idle));
    }

    #[test]
    fn a_paste_is_looked_at_at_once_and_only_once() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        watcher.edited(&"a = 1\n".repeat(20), 120, 0, at(t0, 3));
        assert_eq!(watcher.due(at(t0, 3)), Some(Reason::Pasted));
        let text = watcher.signals(at(t0, 3));
        assert!(text.contains("appeared all at once") && text.contains("it was not typed"), "{text}");
        watcher.look(at(t0, 3));
        watcher.looked();
        assert_ne!(watcher.due(at(t0, 4)), Some(Reason::Pasted), "the tutor has been told");
    }

    #[test]
    fn coming_back_after_a_long_absence_with_new_code_is_noticed() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        watcher.focus(false, at(t0, 10));
        watcher.focus(true, at(t0, 70));
        watcher.edited("def f(): return 1", 0, 17, at(t0, 72));
        assert_eq!(watcher.due(at(t0, 80)), Some(Reason::Returned));
        let text = watcher.signals(at(t0, 80));
        assert!(text.contains("out of focus for 60 seconds"), "{text}");
        assert!(!watcher.signals(at(t0, 81)).contains("out of focus"), "it is told once");
    }

    #[test]
    fn the_signals_always_say_how_old_the_session_is_and_mention_long_silences() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        watcher.edited("x", 0, 1, at(t0, 5));
        let text = watcher.signals(at(t0, 125));
        assert!(text.contains("Nothing has been typed for 120 seconds"), "{text}");
        assert!(text.contains("The session is 2 minutes old."), "{text}");
    }

    #[test]
    fn the_summary_for_the_review_is_plain() {
        let t0 = Instant::now();
        let mut watcher = Watcher::new("", t0);
        assert_eq!(watcher.summary(), "nothing unusual");
        watcher.edited(&"a = 1\n".repeat(30), 180, 0, at(t0, 3));
        watcher.focus(false, at(t0, 10));
        watcher.focus(true, at(t0, 40));
        let summary = watcher.summary();
        assert!(
            summary.contains("1 block(s) of 180 characters") && summary.contains("left 1 time(s), 30 seconds"),
            "{summary}"
        );
    }
}
