//! The plan for a vacancy, built on the 80/20 rule: a few topics account for most of the questions
//! you will be asked, so they come first.
//!
//! A [`Syllabus`] is the researched list of topics and the terms inside each. [`Progress`] is how
//! well you know each term, in your own language and in English, and the *coverage* is the share of
//! the likely questions that you can already answer.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::write_private;
use crate::prompts::Lang;

/// The share of the likely questions that the "vital few" topics must cover.
pub const VITAL_SHARE: f32 = 0.8;
/// A term is learned when you can explain it in your language and say it in English.
pub const MAX_LEVEL: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TopicKind {
    /// An idea: OOP, REST, transactions.
    #[default]
    Concept,
    /// A framework or a tool: Spring, React, Docker.
    Framework,
    /// The language itself and its standard library.
    Language,
    /// Things you do: algorithms, debugging, design.
    Practice,
    /// Questions about you and how you work.
    Behaviour,
}

impl TopicKind {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "framework" | "tool" | "library" => Self::Framework,
            "language" => Self::Language,
            "practice" | "coding" | "algorithm" | "design" => Self::Practice,
            "behaviour" | "behavior" | "behavioural" | "behavioral" | "soft" => Self::Behaviour,
            _ => Self::Concept,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Concept => "concept",
            Self::Framework => "framework",
            Self::Language => "language",
            Self::Practice => "practice",
            Self::Behaviour => "behaviour",
        }
    }
}

/// One thing the interviewer may ask you to define or explain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Term {
    pub name: String,
    /// What it is, in plain words in your language, for someone who has never heard of it.
    pub plain: String,
    /// One everyday comparison that makes it click.
    pub analogy: String,
    /// A good spoken answer in English, two or three sentences.
    pub english: String,
    /// What the interviewer asks next, in English.
    pub follow_ups: Vec<String>,
    /// What a picture should show, in a sentence; the visual lesson starts from it.
    pub visual: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Topic {
    pub name: String,
    pub kind: TopicKind,
    /// Why the vacancy makes it likely, in a line.
    pub why: String,
    /// How likely it is to come up, 1 to 10.
    pub likelihood: u8,
    /// How much work it takes to learn, 1 to 10.
    pub effort: u8,
    pub terms: Vec<Term>,
}

impl Topic {
    /// Likelihood for the effort: the best topics to learn first score high.
    pub fn value(&self) -> f32 {
        f32::from(self.likelihood.clamp(1, 10)) / f32::from(self.effort.clamp(1, 10))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Syllabus {
    pub role: String,
    pub company: String,
    pub summary: String,
    pub topics: Vec<Topic>,
    pub made: u64,
    /// Fingerprint of the vacancy it was made for, to know when it is out of date.
    pub key: String,
    /// The language the plain explanations are written in.
    pub native: Lang,
}

/// A short fingerprint of the vacancy text, so a changed vacancy is noticed.
pub fn vacancy_key(title: &str, company: &str, description: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [title, company, description] {
        hasher.update(part.trim().to_lowercase().as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(&hasher.finalize()[..8])
}

impl Syllabus {
    pub fn load(path: &Path) -> Option<Self> {
        let syllabus: Self = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
        (!syllabus.topics.is_empty()).then_some(syllabus)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).context("could not write the plan")?;
        write_private(path, &bytes)
    }

    /// The topics that together hold [`VITAL_SHARE`] of the likelihood, most likely first. There is
    /// always at least one while there is any topic.
    pub fn vital(&self) -> Vec<usize> {
        let total: f32 = self.topics.iter().map(|t| f32::from(t.likelihood.clamp(1, 10))).sum();
        if total <= 0.0 {
            return Vec::new();
        }
        let mut order: Vec<usize> = (0..self.topics.len()).collect();
        order.sort_by(|a, b| {
            self.topics[*b]
                .likelihood
                .cmp(&self.topics[*a].likelihood)
                .then_with(|| self.topics[*b].value().total_cmp(&self.topics[*a].value()))
                .then_with(|| a.cmp(b))
        });
        let mut covered = 0.0;
        let mut vital = Vec::new();
        for index in order {
            vital.push(index);
            covered += f32::from(self.topics[index].likelihood.clamp(1, 10));
            if covered / total >= VITAL_SHARE {
                break;
            }
        }
        vital
    }

    pub fn is_vital(&self, topic: usize) -> bool {
        self.vital().contains(&topic)
    }

    /// The order to study in: the vital topics first, the quick wins among them first, then the rest.
    pub fn study_order(&self) -> Vec<usize> {
        let vital = self.vital();
        let mut first = vital.clone();
        first.sort_by(|a, b| self.topics[*b].value().total_cmp(&self.topics[*a].value()).then_with(|| a.cmp(b)));
        let mut rest: Vec<usize> = (0..self.topics.len()).filter(|i| !vital.contains(i)).collect();
        rest.sort_by(|a, b| self.topics[*b].likelihood.cmp(&self.topics[*a].likelihood).then_with(|| a.cmp(b)));
        first.extend(rest);
        first
    }

    pub fn topic_index(&self, name: &str) -> Option<usize> {
        self.topics.iter().position(|t| t.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The share of the likely questions you can already answer, 0 to 1: each topic counts as much
    /// as it is likely, and as much as you know of its terms.
    pub fn coverage(&self, progress: &Progress) -> f32 {
        let total: f32 = self.topics.iter().map(|t| f32::from(t.likelihood.clamp(1, 10))).sum();
        if total <= 0.0 {
            return 0.0;
        }
        let known: f32 = self
            .topics
            .iter()
            .map(|topic| f32::from(topic.likelihood.clamp(1, 10)) * progress.topic_mastery(topic))
            .sum();
        (known / total).clamp(0.0, 1.0)
    }

    /// The share of the vital topics' likelihood that you know: how close the 80% is.
    pub fn vital_coverage(&self, progress: &Progress) -> f32 {
        let vital = self.vital();
        let total: f32 = vital.iter().map(|i| f32::from(self.topics[*i].likelihood.clamp(1, 10))).sum();
        if total <= 0.0 {
            return 0.0;
        }
        let known: f32 = vital
            .iter()
            .map(|i| f32::from(self.topics[*i].likelihood.clamp(1, 10)) * progress.topic_mastery(&self.topics[*i]))
            .sum();
        (known / total).clamp(0.0, 1.0)
    }

    /// The next topic to work on: the first in study order that is not learned yet and has terms.
    pub fn next_topic(&self, progress: &Progress) -> Option<usize> {
        self.study_order()
            .into_iter()
            .find(|i| !self.topics[*i].terms.is_empty() && progress.topic_mastery(&self.topics[*i]) < 0.99)
    }
}

/// The key a term is remembered under.
pub fn term_key(topic: &str, term: &str) -> String {
    format!("{}::{}", topic.trim().to_lowercase(), term.trim().to_lowercase())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TermProgress {
    /// How well you explained it in your own language, 0 to 3.
    pub native: u8,
    /// How well you said it in English, 0 to 3.
    pub english: u8,
    pub attempts: u32,
    pub last: u64,
}

impl TermProgress {
    /// 0 to 1: both halves count equally.
    pub fn mastery(&self) -> f32 {
        f32::from(self.native.min(MAX_LEVEL) + self.english.min(MAX_LEVEL)) / f32::from(2 * MAX_LEVEL)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub terms: HashMap<String, TermProgress>,
}

/// Which half of a term was just practised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Native,
    English,
}

impl Progress {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).context("could not write the progress")?;
        write_private(path, &bytes)
    }

    pub fn get(&self, topic: &str, term: &str) -> TermProgress {
        self.terms.get(&term_key(topic, term)).copied().unwrap_or_default()
    }

    /// Records how well one half of a term went, 0 to 3. A better result replaces a worse one; a
    /// worse one lowers it by a step only, so one bad day does not erase what you know.
    pub fn record(&mut self, topic: &str, term: &str, half: Half, score: u8, now: u64) {
        let score = score.min(MAX_LEVEL);
        let entry = self.terms.entry(term_key(topic, term)).or_default();
        let slot = match half {
            Half::Native => &mut entry.native,
            Half::English => &mut entry.english,
        };
        *slot = if score >= *slot { score } else { slot.saturating_sub(1).max(score) };
        entry.attempts += 1;
        entry.last = now;
    }

    pub fn topic_mastery(&self, topic: &Topic) -> f32 {
        if topic.terms.is_empty() {
            return 0.0;
        }
        topic.terms.iter().map(|term| self.get(&topic.name, &term.name).mastery()).sum::<f32>()
            / topic.terms.len() as f32
    }

    /// The first term of the topic that is not learned yet.
    pub fn next_term(&self, topic: &Topic) -> Option<usize> {
        topic.terms.iter().position(|term| self.get(&topic.name, &term.name).mastery() < 0.99)
    }
}

/// What to do today, in order, for the home page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// There is no plan yet: research the vacancy.
    Research,
    /// Cards that have come due.
    Review(usize),
    /// The next topic of the plan, by index.
    Topic(usize),
    /// The plan is learned: a mock interview is the next thing.
    Mock,
}

pub fn today(syllabus: Option<&Syllabus>, progress: &Progress, due_cards: usize) -> Vec<Step> {
    let mut steps = Vec::new();
    if due_cards > 0 {
        steps.push(Step::Review(due_cards));
    }
    match syllabus {
        None => steps.push(Step::Research),
        Some(syllabus) => match syllabus.next_topic(progress) {
            Some(topic) => steps.push(Step::Topic(topic)),
            None => steps.push(Step::Mock),
        },
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(name: &str, likelihood: u8, effort: u8, terms: &[&str]) -> Topic {
        Topic {
            name: name.into(),
            likelihood,
            effort,
            terms: terms.iter().map(|t| Term { name: (*t).into(), ..Term::default() }).collect(),
            ..Topic::default()
        }
    }

    fn plan() -> Syllabus {
        Syllabus {
            role: "AI-native full-stack developer".into(),
            topics: vec![
                topic("OOP", 9, 2, &["Encapsulation", "Polymorphism", "Inheritance"]),
                topic("REST", 8, 3, &["Idempotency", "Status codes"]),
                topic("Kubernetes", 2, 9, &["Pod"]),
                topic("SQL", 7, 4, &["Index", "Join"]),
                topic("Rust lifetimes", 1, 10, &["Lifetime"]),
            ],
            ..Syllabus::default()
        }
    }

    #[test]
    fn a_few_topics_hold_most_of_the_likelihood() {
        let plan = plan();
        // 9 + 8 + 7 = 24 of 27 is 89%: three of five topics are the vital few
        let vital = plan.vital();
        assert_eq!(vital, vec![0, 1, 3]);
        assert!(plan.is_vital(0) && !plan.is_vital(2) && !plan.is_vital(4));
    }

    #[test]
    fn the_vital_few_stop_as_soon_as_eighty_percent_is_covered() {
        let plan = Syllabus {
            topics: vec![topic("a", 10, 1, &[]), topic("b", 1, 1, &[]), topic("c", 1, 1, &[]), topic("d", 1, 1, &[])],
            ..Syllabus::default()
        };
        // 10 of 13 is 77%: a needs one more; 11 of 13 is 85%
        assert_eq!(plan.vital().len(), 2);
        let one = Syllabus { topics: vec![topic("only", 5, 5, &[])], ..Syllabus::default() };
        assert_eq!(one.vital(), vec![0]);
        assert!(Syllabus::default().vital().is_empty());
    }

    #[test]
    fn the_study_order_puts_quick_wins_of_the_vital_few_first() {
        let plan = plan();
        // value: OOP 4.5, REST 2.67, SQL 1.75; then the rest by likelihood
        assert_eq!(plan.study_order(), vec![0, 1, 3, 2, 4]);
    }

    #[test]
    fn progress_raises_quickly_and_falls_slowly() {
        let mut progress = Progress::default();
        progress.record("OOP", "Encapsulation", Half::Native, 3, 10);
        assert_eq!(progress.get("oop", "encapsulation").native, 3, "keys ignore case");
        progress.record("OOP", "Encapsulation", Half::Native, 0, 20);
        assert_eq!(progress.get("OOP", "Encapsulation").native, 2, "one bad attempt costs one step");
        progress.record("OOP", "Encapsulation", Half::English, 9, 30);
        let entry = progress.get("OOP", "Encapsulation");
        assert_eq!((entry.english, entry.attempts, entry.last), (3, 3, 30), "a silly score is capped");
        assert!((entry.mastery() - 5.0 / 6.0).abs() < 0.001);
    }

    #[test]
    fn coverage_weighs_each_topic_by_how_likely_it_is() {
        let plan = plan();
        let mut progress = Progress::default();
        assert_eq!(plan.coverage(&progress), 0.0);
        for term in ["Encapsulation", "Polymorphism", "Inheritance"] {
            progress.record("OOP", term, Half::Native, 3, 1);
            progress.record("OOP", term, Half::English, 3, 1);
        }
        // OOP is 9 of 27
        assert!((plan.coverage(&progress) - 9.0 / 27.0).abs() < 0.001);
        // and 9 of the vital 24
        assert!((plan.vital_coverage(&progress) - 9.0 / 24.0).abs() < 0.001);
        // learning the unlikely topic barely moves it
        progress.record("Rust lifetimes", "Lifetime", Half::Native, 3, 1);
        progress.record("Rust lifetimes", "Lifetime", Half::English, 3, 1);
        assert!((plan.coverage(&progress) - 10.0 / 27.0).abs() < 0.001);
    }

    #[test]
    fn the_next_topic_is_the_best_one_that_is_not_learned() {
        let plan = plan();
        let mut progress = Progress::default();
        assert_eq!(plan.next_topic(&progress), Some(0));
        for term in ["Encapsulation", "Polymorphism", "Inheritance"] {
            progress.record("OOP", term, Half::Native, 3, 1);
            progress.record("OOP", term, Half::English, 3, 1);
        }
        assert_eq!(plan.next_topic(&progress), Some(1));
        assert_eq!(progress.next_term(&plan.topics[1]), Some(0));
        progress.record("REST", "Idempotency", Half::Native, 3, 1);
        progress.record("REST", "Idempotency", Half::English, 3, 1);
        assert_eq!(progress.next_term(&plan.topics[1]), Some(1));
    }

    #[test]
    fn a_topic_without_terms_is_not_offered_to_study() {
        let mut plan = plan();
        plan.topics[0].terms.clear();
        assert_eq!(plan.next_topic(&Progress::default()), Some(1));
        assert_eq!(plan.topics[0].value(), 4.5);
    }

    #[test]
    fn today_starts_with_cards_that_are_due_then_the_plan() {
        let plan = plan();
        let progress = Progress::default();
        assert_eq!(today(None, &progress, 0), vec![Step::Research]);
        assert_eq!(today(Some(&plan), &progress, 4), vec![Step::Review(4), Step::Topic(0)]);
        let mut done = Progress::default();
        for topic in &plan.topics {
            for term in &topic.terms {
                done.record(&topic.name, &term.name, Half::Native, 3, 1);
                done.record(&topic.name, &term.name, Half::English, 3, 1);
            }
        }
        assert_eq!(today(Some(&plan), &done, 0), vec![Step::Mock]);
    }

    #[test]
    fn the_vacancy_fingerprint_changes_with_the_text_only() {
        let a = vacancy_key("Dev", "Acme", "Rust and Kubernetes");
        assert_eq!(a, vacancy_key("  dev ", "ACME", "rust and kubernetes "));
        assert_ne!(a, vacancy_key("Dev", "Acme", "Rust and Docker"));
        assert_ne!(vacancy_key("ab", "c", ""), vacancy_key("a", "bc", ""), "the parts do not run together");
    }

    #[test]
    fn plans_and_progress_round_trip_and_bad_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("syllabus.json");
        plan().save(&path).unwrap();
        assert_eq!(Syllabus::load(&path).unwrap().topics.len(), 5);
        fs::write(&path, "{ nope").unwrap();
        assert!(Syllabus::load(&path).is_none());
        assert!(Syllabus::load(&dir.path().join("missing")).is_none());
        let empty = dir.path().join("empty.json");
        Syllabus::default().save(&empty).unwrap();
        assert!(Syllabus::load(&empty).is_none(), "a plan without topics is no plan");
        let progress_path = dir.path().join("progress.json");
        let mut progress = Progress::default();
        progress.record("OOP", "Encapsulation", Half::Native, 2, 5);
        progress.save(&progress_path).unwrap();
        assert_eq!(Progress::load(&progress_path).get("OOP", "Encapsulation").native, 2);
        assert!(Progress::load(&dir.path().join("none")).terms.is_empty());
    }

    #[test]
    fn kinds_are_read_forgivingly() {
        assert_eq!(TopicKind::parse("Framework"), TopicKind::Framework);
        assert_eq!(TopicKind::parse(" behavioral "), TopicKind::Behaviour);
        assert_eq!(TopicKind::parse("coding"), TopicKind::Practice);
        assert_eq!(TopicKind::parse("???"), TopicKind::Concept);
    }
}
