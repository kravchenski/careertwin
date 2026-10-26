//! Notes that are worth coming back to. After a lesson the tutor keeps the few lines that matter
//! (the 20% that gives 80%), and what you type to the tutor in a lecture is kept too. Nothing is
//! kept just to fill a page: a note is short and says one thing.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::write_private;

/// The most characters a note keeps; a longer one is cut at a word.
pub const MAX_NOTE_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    /// One of the few things to remember from a lesson.
    #[default]
    Vital,
    /// Something you wrote or asked in a lecture.
    Lecture,
    /// A slip to avoid next time.
    Mistake,
    /// Anything else you wanted to keep.
    Idea,
}

impl NoteKind {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "lecture" | "lectures" => Self::Lecture,
            "mistake" | "slip" | "error" => Self::Mistake,
            "idea" | "own" => Self::Idea,
            _ => Self::Vital,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub kind: NoteKind,
    /// The topic it belongs to; may be empty.
    #[serde(default)]
    pub topic: String,
    pub text: String,
    pub created: u64,
    #[serde(default)]
    pub starred: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Notebook {
    pub notes: Vec<Note>,
}

/// `text` cut to [`MAX_NOTE_CHARS`] at a word, on one line.
pub fn tidy(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_NOTE_CHARS {
        return one_line;
    }
    let cut: String = one_line.chars().take(MAX_NOTE_CHARS).collect();
    let end = cut.rfind(' ').unwrap_or(cut.len());
    format!("{}…", cut[..end].trim_end_matches([',', ';', ':', ' ']))
}

impl Notebook {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).context("could not write the notes")?;
        write_private(path, &bytes)
    }

    /// Adds a note unless the same thing is already there. Returns whether it was added.
    pub fn add(&mut self, kind: NoteKind, topic: &str, text: &str, now: u64) -> bool {
        let text = tidy(text);
        if text.is_empty() {
            return false;
        }
        let topic = topic.trim().to_owned();
        if self.notes.iter().any(|n| n.text.eq_ignore_ascii_case(&text) && n.topic.eq_ignore_ascii_case(&topic)) {
            return false;
        }
        self.notes.push(Note {
            id: uuid::Uuid::new_v4().simple().to_string(),
            kind,
            topic,
            text,
            created: now,
            starred: false,
        });
        true
    }

    pub fn remove(&mut self, id: &str) {
        self.notes.retain(|n| n.id != id);
    }

    pub fn toggle_star(&mut self, id: &str) {
        if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
            note.starred = !note.starred;
        }
    }

    /// Newest first; starred notes before the others. `query` filters by text and topic.
    pub fn listing(&self, query: &str) -> Vec<&Note> {
        let needle = query.trim().to_lowercase();
        let mut found: Vec<&Note> = self
            .notes
            .iter()
            .filter(|n| {
                needle.is_empty() || n.text.to_lowercase().contains(&needle) || n.topic.to_lowercase().contains(&needle)
            })
            .collect();
        found.sort_by(|a, b| {
            b.starred.cmp(&a.starred).then_with(|| b.created.cmp(&a.created)).then_with(|| a.id.cmp(&b.id))
        });
        found
    }

    pub fn topics(&self) -> Vec<String> {
        let mut topics: Vec<String> = Vec::new();
        for note in &self.notes {
            if !note.topic.is_empty() && !topics.iter().any(|t| t.eq_ignore_ascii_case(&note.topic)) {
                topics.push(note.topic.clone());
            }
        }
        topics.sort_by_key(|t| t.to_lowercase());
        topics
    }

    /// Everything as Markdown, grouped by topic, to read elsewhere or print.
    pub fn markdown(&self) -> String {
        let mut out = String::from("# Notes\n");
        let mut groups: Vec<(String, Vec<&Note>)> = self.topics().into_iter().map(|t| (t, Vec::new())).collect();
        let mut loose: Vec<&Note> = Vec::new();
        for note in self.listing("") {
            match groups.iter_mut().find(|(topic, _)| topic.eq_ignore_ascii_case(&note.topic)) {
                Some((_, list)) => list.push(note),
                None => loose.push(note),
            }
        }
        let line = |note: &Note| format!("- {}{}\n", if note.starred { "★ " } else { "" }, note.text);
        for (topic, list) in &groups {
            out.push_str(&format!("\n## {topic}\n"));
            list.iter().for_each(|n| out.push_str(&line(n)));
        }
        if !loose.is_empty() {
            out.push_str("\n## Other\n");
            loose.iter().for_each(|n| out.push_str(&line(n)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_one_tidy_line_and_a_repeat_is_not_added() {
        let mut book = Notebook::default();
        assert!(book.add(NoteKind::Vital, " OOP ", "Four ideas:\n  encapsulation,   inheritance", 5));
        assert_eq!(book.notes[0].text, "Four ideas: encapsulation, inheritance");
        assert_eq!(book.notes[0].topic, "OOP");
        assert!(!book.add(NoteKind::Vital, "oop", "FOUR IDEAS: encapsulation, inheritance", 6));
        assert!(book.add(NoteKind::Vital, "REST", "FOUR IDEAS: encapsulation, inheritance", 7), "another topic");
        assert!(!book.add(NoteKind::Lecture, "", "   ", 8), "nothing to keep");
        assert_eq!(book.notes.len(), 2);
    }

    #[test]
    fn a_long_note_is_cut_at_a_word() {
        let long = "word ".repeat(200);
        let tidy = tidy(&long);
        assert!(tidy.chars().count() <= MAX_NOTE_CHARS + 1, "{}", tidy.chars().count());
        assert!(tidy.ends_with("word…"), "{tidy}");
    }

    #[test]
    fn the_listing_puts_stars_first_then_the_newest_and_filters() {
        let mut book = Notebook::default();
        book.add(NoteKind::Vital, "OOP", "old", 1);
        book.add(NoteKind::Lecture, "", "newer, about indexes", 2);
        book.add(NoteKind::Vital, "SQL", "newest", 3);
        let old = book.notes[0].id.clone();
        book.toggle_star(&old);
        let texts: Vec<&str> = book.listing("").iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, vec!["old", "newest", "newer, about indexes"]);
        assert_eq!(book.listing("INDEX").len(), 1);
        assert_eq!(book.listing("sql").len(), 1, "the topic is searched too");
        book.toggle_star(&old);
        assert!(!book.notes[0].starred);
        book.toggle_star("missing");
    }

    #[test]
    fn removing_and_topics() {
        let mut book = Notebook::default();
        book.add(NoteKind::Vital, "sql", "a", 1);
        book.add(NoteKind::Vital, "OOP", "b", 2);
        book.add(NoteKind::Vital, "SQL", "c", 3);
        assert_eq!(book.topics(), vec!["OOP", "sql"]);
        let id = book.notes[1].id.clone();
        book.remove(&id);
        assert_eq!(book.notes.len(), 2);
        assert_eq!(book.topics(), vec!["sql"]);
    }

    #[test]
    fn markdown_groups_by_topic() {
        let mut book = Notebook::default();
        book.add(NoteKind::Vital, "OOP", "Encapsulation hides state.", 1);
        book.add(NoteKind::Lecture, "", "Ask about the project defence.", 2);
        let first = book.notes[0].id.clone();
        book.toggle_star(&first);
        let text = book.markdown();
        assert!(text.contains("## OOP\n- ★ Encapsulation hides state."), "{text}");
        assert!(text.contains("## Other\n- Ask about the project defence."), "{text}");
    }

    #[test]
    fn kinds_are_read_forgivingly_and_files_round_trip() {
        assert_eq!(NoteKind::parse("Lecture"), NoteKind::Lecture);
        assert_eq!(NoteKind::parse("slip"), NoteKind::Mistake);
        assert_eq!(NoteKind::parse("whatever"), NoteKind::Vital);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");
        let mut book = Notebook::default();
        book.add(NoteKind::Idea, "x", "y", 1);
        book.save(&path).unwrap();
        assert_eq!(Notebook::load(&path).notes.len(), 1);
        fs::write(&path, "oops").unwrap();
        assert!(Notebook::load(&path).notes.is_empty());
    }
}
