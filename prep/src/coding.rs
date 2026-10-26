//! Live coding: a small task, the languages it can be in, and what the tutor concludes when it
//! looks at your code.

use serde::{Deserialize, Serialize};

/// The languages a task can be in: an id for the prompts and the highlighting, and a name.
pub const LANGUAGES: [(&str, &str); 9] = [
    ("python", "Python"),
    ("javascript", "JavaScript"),
    ("typescript", "TypeScript"),
    ("java", "Java"),
    ("csharp", "C#"),
    ("go", "Go"),
    ("rust", "Rust"),
    ("cpp", "C++"),
    ("sql", "SQL"),
];

/// The id of the language named in free text ("TS", "node", "c sharp"), or `None`.
pub fn language_id(text: &str) -> Option<&'static str> {
    let lower = text.trim().to_lowercase();
    let lower = lower.trim_matches(|c: char| !c.is_alphanumeric() && c != '#' && c != '+');
    match lower {
        "python" | "py" | "python3" => Some("python"),
        "javascript" | "js" | "node" | "nodejs" | "node.js" => Some("javascript"),
        "typescript" | "ts" => Some("typescript"),
        "java" => Some("java"),
        "c#" | "csharp" | "c sharp" | ".net" | "dotnet" => Some("csharp"),
        "go" | "golang" => Some("go"),
        "rust" | "rs" => Some("rust"),
        "c++" | "cpp" => Some("cpp"),
        "sql" | "postgresql" | "postgres" | "mysql" => Some("sql"),
        _ => None,
    }
}

pub fn language_name(id: &str) -> &'static str {
    LANGUAGES.iter().find(|(known, _)| *known == id).map_or("Python", |(_, name)| name)
}

/// How a line comment starts, for the starter code.
pub fn comment(id: &str) -> &'static str {
    match id {
        "python" => "#",
        "sql" => "--",
        _ => "//",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Example {
    pub input: String,
    pub output: String,
}

/// A task for the candidate, and what only the tutor knows about it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub title: String,
    /// What to do, in the language the person thinks in.
    pub statement: String,
    pub language: String,
    /// The code the editor starts with: a signature and a comment, never the solution.
    pub starter: String,
    pub examples: Vec<Example>,
    /// For the tutor only: the idea of a good solution.
    pub outline: String,
    /// For the tutor only: the mistakes and edge cases to look for.
    pub pitfalls: Vec<String>,
    /// For the tutor only: nudges from vague to specific.
    pub hints: Vec<String>,
    /// For the tutor only: the time and space a good solution needs.
    pub complexity: String,
}

impl Task {
    /// What the tutor is told: the task, and its private notes.
    pub fn for_tutor(&self) -> String {
        let mut text =
            format!("TASK: {}\nLanguage: {}\n{}\n", self.title, language_name(&self.language), self.statement);
        for (number, example) in self.examples.iter().enumerate() {
            text.push_str(&format!("Example {}: {} -> {}\n", number + 1, example.input, example.output));
        }
        text.push_str(
            "\nPRIVATE NOTES FOR YOU (never say these out loud, never paste them; the person must not hear them):\n",
        );
        if !self.outline.is_empty() {
            text.push_str(&format!("- Solution idea: {}\n", self.outline));
        }
        if !self.complexity.is_empty() {
            text.push_str(&format!("- A good solution takes: {}\n", self.complexity));
        }
        for pitfall in &self.pitfalls {
            text.push_str(&format!("- Look for: {pitfall}\n"));
        }
        for (number, hint) in self.hints.iter().enumerate() {
            text.push_str(&format!("- Nudge {} (from vague to specific): {hint}\n", number + 1));
        }
        text
    }
}

/// What the tutor concludes when it looks at the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Going the right way: leave them alone.
    #[default]
    OnTrack,
    /// Nothing has happened for a while.
    Stuck,
    /// The code has a mistake in it, or the approach will not work.
    WrongWay,
    /// It looks finished.
    Done,
    /// Something came in that the person should explain.
    Explain,
}

impl Status {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().replace([' ', '-'], "_").as_str() {
            "stuck" => Self::Stuck,
            "wrong_way" | "wrong_direction" | "wrong" | "bug" | "off_track" => Self::WrongWay,
            "done" | "finished" | "complete" => Self::Done,
            "explain" | "needs_explaining" | "needs_explanation" => Self::Explain,
            _ => Self::OnTrack,
        }
    }
}

/// What the tutor made of the code at one moment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub status: Status,
    /// What to say, if anything: one or two short sentences.
    pub say: String,
    /// How much it gives away: 0 nothing, 1 a question, 2 a place, 3 working it out together.
    pub level: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_recognised_by_their_nicknames() {
        for (said, id) in [
            ("TS", "typescript"),
            ("Node.js", "javascript"),
            ("c sharp", "csharp"),
            ("C++", "cpp"),
            (" Python3 ", "python"),
            ("golang", "go"),
            ("Postgres", "sql"),
        ] {
            assert_eq!(language_id(said), Some(id), "{said}");
        }
        assert_eq!(language_id("cobol"), None);
        assert_eq!(language_name("csharp"), "C#");
        assert_eq!(language_name("nonsense"), "Python");
        assert_eq!(comment("python"), "#");
        assert_eq!(comment("sql"), "--");
        assert_eq!(comment("rust"), "//");
    }

    #[test]
    fn the_tutor_is_told_the_task_and_its_private_notes_apart() {
        let task = Task {
            title: "Two sum".into(),
            statement: "Find two numbers that add up to the target.".into(),
            language: "python".into(),
            examples: vec![Example { input: "[2,7,11], 9".into(), output: "[0,1]".into() }],
            outline: "One pass with a dictionary of seen numbers.".into(),
            pitfalls: vec!["the same element used twice".into()],
            hints: vec!["What do you need to remember about the numbers already seen?".into()],
            complexity: "O(n) time, O(n) space".into(),
            ..Task::default()
        };
        let text = task.for_tutor();
        assert!(text.starts_with("TASK: Two sum\nLanguage: Python\n"));
        assert!(text.contains("Example 1: [2,7,11], 9 -> [0,1]"));
        let private = text.split("PRIVATE NOTES").nth(1).unwrap();
        assert!(private.contains("dictionary of seen numbers") && private.contains("Look for: the same element"));
        assert!(private.contains("Nudge 1 (from vague to specific)") && private.contains("O(n) time"));
        assert!(!text.split("PRIVATE NOTES").next().unwrap().contains("dictionary"), "the statement leaks nothing");
    }

    #[test]
    fn statuses_are_read_forgivingly() {
        assert_eq!(Status::parse("wrong direction"), Status::WrongWay);
        assert_eq!(Status::parse("On-Track"), Status::OnTrack);
        assert_eq!(Status::parse("Needs explaining"), Status::Explain);
        assert_eq!(Status::parse("finished"), Status::Done);
        assert_eq!(Status::parse("???"), Status::OnTrack);
    }
}
