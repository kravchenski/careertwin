//! "Say it in English": you give a thought in Russian (or a rough English sentence) and get it
//! back in natural English, in three registers, to listen to and say again.

use serde::{Deserialize, Serialize};

use super::json::{extract_object, strings, text};
use super::{Lang, PromptError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Translation {
    pub detected: Lang,
    pub natural: String,
    pub formal: String,
    pub short: String,
    pub notes: Vec<String>,
}

impl Translation {
    pub fn all(&self) -> [(&'static str, &str); 3] {
        [("Natural", &self.natural), ("Formal", &self.formal), ("Short", &self.short)]
    }
}

pub fn system(ui: Lang) -> String {
    format!(
        "You help a Russian-speaking software engineer say things in natural, professional English for job \
         interviews and work. The input is either Russian (translate it) or rough English (polish it). Reply with \
         JSON only: {{\"detected\": \"ru\" or \"en\", \"natural\": str, \"formal\": str, \"short\": str, \
         \"notes\": [str]}}.\n\n\
         Rules\n\
         - natural: what a fluent colleague would say aloud. formal: the same meaning for an interview or an \
         email. short: the same meaning in as few words as stays natural.\n\
         - Keep the meaning exactly. Never add a fact, number, name, tool or claim the speaker did not say, and \
         never make the speaker sound more experienced than they said.\n\
         - Write for the ear: contractions are fine, no markdown, no lists, no quotes around the sentences.\n\
         - Keep technology names, identifiers and commands as they are normally written.\n\
         - notes: up to three short remarks in {ui} about useful word choices or common mistakes in this \
         sentence (for example a false friend or a preposition). Empty if there is nothing to teach.",
        ui = ui.name()
    )
}

pub fn prompt(input: &str) -> String {
    format!("Input:\n{}", crate::brief::truncate_chars(input.trim(), 1500))
}

pub fn parse(reply: &str) -> Result<Translation, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("a translation"))?;
    let natural = text(&value, "natural");
    if natural.is_empty() {
        return Err(PromptError::Unusable("a translation"));
    }
    let or_natural = |key: &str| {
        let s = text(&value, key);
        if s.is_empty() { natural.clone() } else { s }
    };
    Ok(Translation {
        detected: if text(&value, "detected").eq_ignore_ascii_case("en") { Lang::En } else { Lang::Ru },
        formal: or_natural("formal"),
        short: or_natural("short"),
        notes: strings(&value, "notes", 3),
        natural,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_translation_is_read_with_its_three_registers() {
        let t = parse(r#"{"detected": "ru", "natural": "I added an index.", "formal": "I introduced an index.", "short": "Added an index.", "notes": ["a", "b", "c", "d"]}"#).unwrap();
        assert_eq!(t.detected, Lang::Ru);
        assert_eq!(t.all()[1], ("Formal", "I introduced an index."));
        assert_eq!(t.notes.len(), 3);
    }

    #[test]
    fn missing_registers_fall_back_to_the_natural_one() {
        let t = parse(r#"{"detected": "en", "natural": "Fine."}"#).unwrap();
        assert_eq!(t.detected, Lang::En);
        assert_eq!((t.formal.as_str(), t.short.as_str()), ("Fine.", "Fine."));
        assert!(t.notes.is_empty());
    }

    #[test]
    fn no_natural_sentence_is_an_error() {
        assert!(parse(r#"{"formal": "x"}"#).is_err());
        assert!(parse("hello").is_err());
    }

    #[test]
    fn the_rules_forbid_adding_facts() {
        let text = system(Lang::Ru);
        assert!(text.contains("Never add a fact") && text.contains("never make the speaker sound more experienced"));
        assert!(text.contains("remarks in Russian"));
    }

    #[test]
    fn long_input_is_capped() {
        assert!(prompt(&"я ".repeat(5000)).chars().count() < 1600);
    }
}
