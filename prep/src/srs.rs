//! Spaced repetition for the phrases and questions you want to keep: a card comes back just
//! before you would forget it. The scheduling is the well-known SM-2 idea in a small form.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::write_private;

pub const DAY: u64 = 24 * 60 * 60;
const MIN_EASE: f32 = 1.3;
const MAX_INTERVAL_DAYS: f32 = 365.0;
const AGAIN_DELAY: u64 = 10 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    /// An English phrase worth saying again.
    Phrase,
    /// A technical question and its model answer.
    Question,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub kind: CardKind,
    pub front: String,
    pub back: String,
    #[serde(default)]
    pub topic: String,
    pub ease: f32,
    pub interval_days: f32,
    pub due: u64,
    pub reps: u32,
    pub lapses: u32,
    pub created: u64,
    #[serde(default)]
    pub last_review: Option<u64>,
}

impl Card {
    pub fn new(kind: CardKind, front: &str, back: &str, topic: &str, now: u64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            kind,
            front: front.trim().to_owned(),
            back: back.trim().to_owned(),
            topic: topic.trim().to_owned(),
            ease: 2.5,
            interval_days: 0.0,
            due: now,
            reps: 0,
            lapses: 0,
            created: now,
            last_review: None,
        }
    }

    pub fn review(&mut self, grade: Grade, now: u64) {
        self.last_review = Some(now);
        match grade {
            Grade::Again => {
                self.lapses += 1;
                self.reps = 0;
                self.interval_days = 0.0;
                self.ease = (self.ease - 0.2).max(MIN_EASE);
                self.due = now + AGAIN_DELAY;
                return;
            }
            Grade::Hard => {
                self.interval_days = (self.interval_days * 1.2).max(1.0);
                self.ease = (self.ease - 0.15).max(MIN_EASE);
            }
            Grade::Good => {
                self.interval_days = match self.reps {
                    0 => 1.0,
                    1 => 3.0,
                    _ => self.interval_days * self.ease,
                };
            }
            Grade::Easy => {
                self.interval_days = (self.interval_days.max(1.0) * self.ease * 1.3).max(4.0);
                self.ease += 0.15;
            }
        }
        self.reps += 1;
        self.interval_days = self.interval_days.min(MAX_INTERVAL_DAYS);
        self.due = now + (self.interval_days * DAY as f32) as u64;
    }

    pub fn is_due(&self, now: u64) -> bool {
        self.due <= now
    }
}

/// How sure you were against whether you were right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Calibration {
    WellCalibrated,
    /// Sure, and wrong: the case that teaches the most.
    Overconfident,
    /// Right, but you doubted it.
    Underconfident,
}

pub fn calibration(correct: bool, confidence: u8) -> Calibration {
    match (correct, confidence) {
        (false, c) if c >= 70 => Calibration::Overconfident,
        (true, c) if c < 40 => Calibration::Underconfident,
        _ => Calibration::WellCalibrated,
    }
}

/// The grade a concept check earns: right and sure is good, right by luck is hard, wrong is again.
pub fn grade_for(correct: bool, confidence: u8) -> Grade {
    match (correct, confidence) {
        (false, _) => Grade::Again,
        (true, c) if c < 40 => Grade::Hard,
        (true, c) if c >= 85 => Grade::Easy,
        (true, _) => Grade::Good,
    }
}

/// The Brier score of your confidence over a history of `(right, confidence %)`: 0 is perfect,
/// 0.25 is what always answering 50% earns.
pub fn brier(history: &[(bool, u8)]) -> Option<f32> {
    if history.is_empty() {
        return None;
    }
    let total: f32 = history
        .iter()
        .map(|&(correct, confidence)| {
            let p = f32::from(confidence.min(100)) / 100.0;
            let outcome = if correct { 1.0 } else { 0.0 };
            (p - outcome).powi(2)
        })
        .sum();
    Some(total / history.len() as f32)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Deck {
    pub cards: Vec<Card>,
}

impl Deck {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_vec_pretty(self).context("could not write the deck")?;
        write_private(path, &text)
    }

    /// Adds a card unless the same front is already in the deck; says whether it was added.
    pub fn add(&mut self, card: Card) -> bool {
        let key = card.front.to_lowercase();
        if self.cards.iter().any(|c| c.front.to_lowercase() == key) {
            return false;
        }
        self.cards.push(card);
        true
    }

    pub fn due(&self, now: u64) -> Vec<&Card> {
        let mut due: Vec<&Card> = self.cards.iter().filter(|c| c.is_due(now)).collect();
        due.sort_by_key(|c| c.due);
        due
    }

    pub fn review(&mut self, id: &str, grade: Grade, now: u64) -> bool {
        match self.cards.iter_mut().find(|c| c.id == id) {
            Some(card) => {
                card.review(grade, now);
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.cards.retain(|c| c.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn card() -> Card {
        Card::new(CardKind::Phrase, "  I'd like to walk you through it  ", "Я хотел бы показать вам", "intro", NOW)
    }

    #[test]
    fn a_new_card_is_due_at_once_and_trimmed() {
        let c = card();
        assert!(c.is_due(NOW));
        assert_eq!(c.front, "I'd like to walk you through it");
        assert_eq!(c.id.len(), 32);
    }

    #[test]
    fn good_answers_stretch_the_interval() {
        let mut c = card();
        c.review(Grade::Good, NOW);
        assert_eq!(c.interval_days, 1.0);
        assert_eq!(c.due, NOW + DAY);
        c.review(Grade::Good, NOW + DAY);
        assert_eq!(c.interval_days, 3.0);
        c.review(Grade::Good, NOW + 4 * DAY);
        assert!((c.interval_days - 7.5).abs() < 0.001, "{}", c.interval_days);
        assert_eq!(c.reps, 3);
    }

    #[test]
    fn again_brings_the_card_back_soon_and_counts_a_lapse() {
        let mut c = card();
        c.review(Grade::Good, NOW);
        c.review(Grade::Good, NOW + DAY);
        c.review(Grade::Again, NOW + 4 * DAY);
        assert_eq!(c.lapses, 1);
        assert_eq!(c.reps, 0);
        assert_eq!(c.due, NOW + 4 * DAY + AGAIN_DELAY);
        assert!(c.ease < 2.5);
    }

    #[test]
    fn ease_never_drops_below_the_floor_and_intervals_are_capped() {
        let mut c = card();
        for _ in 0..30 {
            c.review(Grade::Again, NOW);
        }
        assert_eq!(c.ease, MIN_EASE);
        let mut c = card();
        for step in 0..40 {
            c.review(Grade::Easy, NOW + step * DAY);
        }
        assert!(c.interval_days <= MAX_INTERVAL_DAYS);
    }

    #[test]
    fn easy_goes_further_than_good_and_hard_less() {
        let mut easy = card();
        let mut good = card();
        let mut hard = card();
        for c in [&mut easy, &mut good, &mut hard] {
            c.review(Grade::Good, NOW);
        }
        easy.review(Grade::Easy, NOW + DAY);
        good.review(Grade::Good, NOW + DAY);
        hard.review(Grade::Hard, NOW + DAY);
        assert!(easy.interval_days > good.interval_days && good.interval_days > hard.interval_days);
    }

    #[test]
    fn confidence_decides_the_grade_and_the_calibration_note() {
        assert_eq!(grade_for(true, 95), Grade::Easy);
        assert_eq!(grade_for(true, 60), Grade::Good);
        assert_eq!(grade_for(true, 20), Grade::Hard);
        assert_eq!(grade_for(false, 90), Grade::Again);
        assert_eq!(calibration(false, 90), Calibration::Overconfident);
        assert_eq!(calibration(true, 10), Calibration::Underconfident);
        assert_eq!(calibration(true, 80), Calibration::WellCalibrated);
        assert_eq!(calibration(false, 20), Calibration::WellCalibrated);
    }

    #[test]
    fn the_brier_score_rewards_honest_confidence() {
        assert_eq!(brier(&[]), None);
        let perfect = brier(&[(true, 100), (false, 0)]).unwrap();
        let coin = brier(&[(true, 50), (false, 50)]).unwrap();
        let bluffing = brier(&[(false, 100), (true, 0)]).unwrap();
        assert_eq!(perfect, 0.0);
        assert!((coin - 0.25).abs() < 0.001);
        assert_eq!(bluffing, 1.0);
    }

    #[test]
    fn the_deck_rejects_duplicates_and_lists_what_is_due_oldest_first() {
        let mut deck = Deck::default();
        let mut a = Card::new(CardKind::Phrase, "first", "1", "", NOW);
        a.due = NOW - 100;
        let mut b = Card::new(CardKind::Question, "second", "2", "", NOW);
        b.due = NOW - 500;
        let mut later = Card::new(CardKind::Phrase, "later", "3", "", NOW);
        later.due = NOW + DAY;
        assert!(deck.add(a));
        assert!(deck.add(b));
        assert!(deck.add(later));
        assert!(!deck.add(Card::new(CardKind::Phrase, "FIRST", "dup", "", NOW)));
        let due: Vec<&str> = deck.due(NOW).iter().map(|c| c.front.as_str()).collect();
        assert_eq!(due, vec!["second", "first"]);
    }

    #[test]
    fn the_deck_survives_a_round_trip_and_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deck.json");
        let mut deck = Deck::default();
        deck.add(card());
        deck.save(&path).unwrap();
        assert_eq!(Deck::load(&path).cards.len(), 1);
        fs::write(&path, "{ broken").unwrap();
        assert!(Deck::load(&path).cards.is_empty());
        assert!(Deck::load(&dir.path().join("none.json")).cards.is_empty());
    }

    #[test]
    fn review_and_remove_by_id() {
        let mut deck = Deck::default();
        let c = card();
        let id = c.id.clone();
        deck.add(c);
        assert!(deck.review(&id, Grade::Good, NOW));
        assert!(!deck.review("nope", Grade::Good, NOW));
        deck.remove(&id);
        assert!(deck.cards.is_empty());
    }
}
