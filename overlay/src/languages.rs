//! Interview language: which language answers, hints and the window speak.

use std::collections::VecDeque;

/// The names used inside prompts.
pub const NAMES: &[(&str, &str)] = &[
    ("en", "English"),
    ("ru", "Russian"),
    ("pl", "Polish"),
    ("uk", "Ukrainian"),
    ("be", "Belarusian"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("nl", "Dutch"),
    ("cs", "Czech"),
    ("sk", "Slovak"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("no", "Norwegian"),
    ("fi", "Finnish"),
    ("tr", "Turkish"),
    ("ro", "Romanian"),
    ("hu", "Hungarian"),
    ("bg", "Bulgarian"),
    ("lt", "Lithuanian"),
    ("lv", "Latvian"),
    ("et", "Estonian"),
    ("el", "Greek"),
    ("he", "Hebrew"),
    ("ar", "Arabic"),
    ("zh", "Chinese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("hi", "Hindi"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
];

/// The languages the window itself speaks, in the order of the picker, with their own names.
pub const UI_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("ru", "Русский"),
    ("pl", "Polski"),
    ("uk", "Українська"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("es", "Español"),
];

/// English name for a language code, used inside prompts.
pub fn name(code: Option<&str>) -> String {
    let Some(code) = code.filter(|code| !code.is_empty()) else { return "English".to_owned() };
    let lower = code.to_lowercase();
    match NAMES.iter().find(|(known, _)| *known == lower) {
        Some((_, label)) => (*label).to_owned(),
        None => {
            let mut chars = code.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect())
                .unwrap_or_default()
        }
    }
}

/// `ru-RU`, `RU`, `Russian` -> `ru`; `auto` or empty -> None.
pub fn normalize(value: Option<&str>) -> Option<String> {
    let text = value.unwrap_or("").trim().to_lowercase();
    if text.is_empty() || text == "auto" {
        return None;
    }
    if NAMES.iter().any(|(code, _)| *code == text) {
        return Some(text);
    }
    let base = text.replace('_', "-").split('-').next().unwrap_or("").to_owned();
    if NAMES.iter().any(|(code, _)| *code == base) {
        return Some(base);
    }
    if let Some((code, _)) = NAMES.iter().find(|(_, label)| label.to_lowercase() == text) {
        return Some((*code).to_owned());
    }
    (base.chars().all(char::is_alphabetic) && matches!(base.chars().count(), 2 | 3)).then_some(base)
}

/// Settles on the interview language and does not flip on one odd utterance. A fixed language always wins;
/// in auto mode the language is the majority of the last few confident detections of the interviewer.
#[derive(Debug)]
pub struct LanguageTracker {
    pub fixed: Option<String>,
    pub current: Option<String>,
    min_probability: f32,
    window: usize,
    recent: VecDeque<String>,
}

impl LanguageTracker {
    pub fn new(fixed: Option<&str>) -> Self {
        let fixed = normalize(fixed);
        Self { current: fixed.clone(), fixed, min_probability: 0.6, window: 5, recent: VecDeque::new() }
    }

    pub fn set_fixed(&mut self, value: Option<&str>) -> Option<String> {
        self.fixed = normalize(value);
        self.recent.clear();
        self.current = self.fixed.clone();
        self.current.clone()
    }

    /// Feed one detection; the language is returned when it changed.
    pub fn observe(&mut self, code: Option<&str>, probability: f32) -> Option<String> {
        let code = code.filter(|code| !code.is_empty())?;
        if self.fixed.is_some() || probability < self.min_probability {
            return None;
        }
        if self.recent.len() == self.window {
            self.recent.pop_front();
        }
        self.recent.push_back(code.to_owned());
        let mut best: Option<(&String, usize)> = None;
        for candidate in &self.recent {
            let count = self.recent.iter().filter(|other| *other == candidate).count();
            if best.is_none_or(|(_, top)| count > top) {
                best = Some((candidate, count));
            }
        }
        let winner = best?.0.clone();
        (self.current.as_deref() != Some(winner.as_str())).then(|| {
            self.current = Some(winner.clone());
            winner
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_a_region_or_a_name_all_mean_the_same_language() {
        for text in ["ru", "RU", "ru-RU", "ru_RU", "Russian"] {
            assert_eq!(normalize(Some(text)).as_deref(), Some("ru"), "{text}");
        }
        assert_eq!(normalize(Some("auto")), None);
        assert_eq!(normalize(Some("")), None);
        assert_eq!(normalize(None), None);
        assert_eq!(normalize(Some("xx")).as_deref(), Some("xx")); // two letters: kept, the model may know it
        assert_eq!(normalize(Some("12")), None);
    }

    #[test]
    fn prompts_name_the_language_in_english() {
        assert_eq!(name(Some("pl")), "Polish");
        assert_eq!(name(None), "English");
        assert_eq!(name(Some("qq")), "Qq");
    }

    #[test]
    fn the_language_follows_the_majority_not_one_odd_utterance() {
        let mut tracker = LanguageTracker::new(None);
        assert_eq!(tracker.observe(Some("ru"), 0.9).as_deref(), Some("ru"));
        assert_eq!(tracker.observe(Some("en"), 0.9), None); // one odd one
        assert_eq!(tracker.observe(Some("ru"), 0.9), None);
        assert_eq!(tracker.observe(Some("en"), 0.4), None); // too unsure to count
        assert_eq!(tracker.observe(Some("en"), 0.9), None);
        assert_eq!(tracker.observe(Some("en"), 0.9).as_deref(), Some("en"));
    }

    #[test]
    fn a_fixed_language_always_wins() {
        let mut tracker = LanguageTracker::new(Some("de"));
        assert_eq!(tracker.observe(Some("ru"), 1.0), None);
        assert_eq!(tracker.current.as_deref(), Some("de"));
        assert_eq!(tracker.set_fixed(None), None);
    }
}
