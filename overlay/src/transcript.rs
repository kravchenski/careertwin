//! Plain-text record of an interview, kept on this machine only.
//!
//! The same Markdown the Python overlay writes (`data/interviews/2026-10-27_1313.md`), so old and new records
//! read the same, and [`crate::export`] can list the questions of both.

use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime};

pub const INTERVIEWER: &str = "Interviewer";
pub const YOU: &str = "You";
/// What a screenshot question is called in the record: the question is the picture.
pub const SCREENSHOT: &str = "[screenshot]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub speaker: String,
    pub text: String,
    pub at: NaiveDateTime,
}

impl Turn {
    pub fn line(&self) -> String {
        format!("{}: {}", self.speaker, self.text)
    }
}

/// What a finished interview file holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// The name of the interview, when the file says it.
    pub title: String,
    pub started: Option<NaiveDateTime>,
    pub turns: Vec<Turn>,
    /// (question, answer) as the window showed them.
    pub suggestions: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct TranscriptLog {
    pub title: String,
    pub turns: Vec<Turn>,
    pub suggestions: Vec<(String, String)>,
    pub directory: PathBuf,
    pub started: NaiveDateTime,
}

impl TranscriptLog {
    pub fn new(directory: impl Into<PathBuf>, title: &str) -> Self {
        Self { title: title.to_owned(), turns: Vec::new(), suggestions: Vec::new(), directory: directory.into(), started: Local::now().naive_local() }
    }

    pub fn add(&mut self, speaker: &str, text: &str) {
        self.turns.push(Turn { speaker: speaker.to_owned(), text: text.to_owned(), at: Local::now().naive_local() });
    }

    /// The last few turns as `Speaker: text` lines, for the prompt.
    pub fn history(&self, limit: usize) -> Vec<String> {
        self.turns[self.turns.len().saturating_sub(limit)..].iter().map(Turn::line).collect()
    }

    pub fn render(&self) -> String {
        let record = Record { title: self.title.clone(), started: Some(self.started), turns: self.turns.clone(), suggestions: self.suggestions.clone() };
        render(&record)
    }

    pub fn save(&self) -> std::io::Result<Option<PathBuf>> {
        if self.turns.is_empty() {
            return Ok(None);
        }
        std::fs::create_dir_all(&self.directory)?;
        let path = self.directory.join(format!("{}.md", self.started.format("%Y-%m-%d_%H%M")));
        std::fs::write(&path, self.render())?;
        Ok(Some(path))
    }
}

/// The Markdown of a record.
pub fn render(record: &Record) -> String {
    let started = record.started.unwrap_or_default();
    let mut lines = vec![
        if record.title.is_empty() {
            format!("# Interview {}", started.format("%Y-%m-%d %H:%M"))
        } else {
            format!("# Interview {} — {}", started.format("%Y-%m-%d %H:%M"), record.title)
        },
        String::new(),
    ];
    for turn in &record.turns {
        lines.push(format!("**{} {}:** {}", turn.at.format("%H:%M:%S"), turn.speaker, turn.text));
    }
    if !record.suggestions.is_empty() {
        lines.push(String::new());
        lines.push("## Suggestions shown".to_owned());
        lines.push(String::new());
        for (question, answer) in &record.suggestions {
            lines.push(format!("- *{question}*\n\n  {answer}"));
        }
    }
    lines.join("\n") + "\n"
}

/// Read one record back. A file that is not one of ours gives an empty record, never an error.
pub fn parse(text: &str, fallback_start: Option<NaiveDateTime>) -> Record {
    let mut record = Record { started: fallback_start, ..Record::default() };
    let mut lines = text.lines().peekable();
    if let Some(head) = lines.peek().and_then(|line| line.strip_prefix("# Interview ")) {
        let (when, title) = head.split_once(" — ").map_or((head, ""), |(when, title)| (when, title));
        if let Ok(started) = NaiveDateTime::parse_from_str(when.trim(), "%Y-%m-%d %H:%M") {
            record.started = Some(started);
        }
        record.title = title.trim().to_owned();
        lines.next();
    }
    let day = record.started.map(|started| started.date()).unwrap_or_default();
    let mut in_suggestions = false;
    let mut current: Option<(String, Vec<String>)> = None;
    let flush = |current: &mut Option<(String, Vec<String>)>, record: &mut Record| {
        if let Some((question, answer)) = current.take() {
            record.suggestions.push((question, answer.join("\n").trim().to_owned()));
        }
    };
    for line in lines {
        if line.trim() == "## Suggestions shown" {
            in_suggestions = true;
            continue;
        }
        if in_suggestions {
            if let Some(question) = line.strip_prefix("- *").and_then(|rest| rest.strip_suffix('*')) {
                flush(&mut current, &mut record);
                current = Some((question.to_owned(), Vec::new()));
            } else if let Some((_, answer)) = current.as_mut() {
                answer.push(line.strip_prefix("  ").unwrap_or(line).to_owned());
            }
            continue;
        }
        if let Some(turn) = parse_turn(line, day) {
            record.turns.push(turn);
        }
    }
    flush(&mut current, &mut record);
    record
}

/// `**13:13:55 Interviewer:** text`
fn parse_turn(line: &str, day: NaiveDate) -> Option<Turn> {
    let rest = line.strip_prefix("**")?;
    let (head, text) = rest.split_once(":** ").or_else(|| rest.strip_suffix(":**").map(|head| (head, "")))?;
    let (time, speaker) = head.split_once(' ')?;
    let time = NaiveTime::parse_from_str(time, "%H:%M:%S").ok()?;
    Some(Turn { speaker: speaker.to_owned(), text: text.to_owned(), at: day.and_time(time) })
}

/// Every saved interview of a folder, oldest first. The date of a file without a header comes from its name.
pub fn read_all(directory: &Path) -> Vec<Record> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "md")).collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let named = NaiveDateTime::parse_from_str(&format!("{stem}00"), "%Y-%m-%d_%H%M%S").ok();
            let record = parse(&text, named);
            (!record.turns.is_empty() || !record.suggestions.is_empty()).then_some(record)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 27).unwrap().and_hms_opt(h, m, s).unwrap()
    }

    #[test]
    fn a_record_is_written_and_read_back() {
        let record = Record {
            title: "Vention: Data Engineering Internship (Gdańsk)".into(),
            started: Some(at(13, 13, 0)),
            turns: vec![
                Turn { speaker: INTERVIEWER.into(), text: "What is a left join?".into(), at: at(13, 13, 55) },
                Turn { speaker: YOU.into(), text: "It keeps every row on the left.".into(), at: at(13, 14, 20) },
            ],
            suggestions: vec![("What is a left join?".into(), "A join that keeps the left rows.\nEven without a match.".into())],
        };
        let text = render(&record);
        assert!(text.starts_with("# Interview 2026-10-27 13:13 — Vention: Data Engineering Internship (Gdańsk)\n\n**13:13:55 Interviewer:** What is a left join?"));
        assert_eq!(parse(&text, None), record);
    }

    #[test]
    fn a_record_of_the_python_overlay_is_read_too() {
        let text = "# Interview 2026-10-27 13:13\n\n**13:13:55 Interviewer:** [screenshot]\n**13:14:15 Interviewer:** [screenshot]\n\n## Suggestions shown\n\n- *[screenshot]*\n\n  There is no task or question to answer in the provided image.\n- *[screenshot]*\n\n  Another one.\n";
        let record = parse(text, None);
        assert_eq!((record.title.as_str(), record.turns.len(), record.suggestions.len()), ("", 2, 2));
        assert_eq!(record.turns[1].at, at(13, 14, 15));
        assert_eq!(record.suggestions[0], (SCREENSHOT.to_owned(), "There is no task or question to answer in the provided image.".to_owned()));
    }

    #[test]
    fn a_file_that_is_not_ours_gives_an_empty_record() {
        assert_eq!(parse("hello\nworld", None), Record::default());
        assert!(parse("# Interview garbage\n**x**", None).turns.is_empty());
    }

    #[test]
    fn the_log_keeps_the_last_turns_and_saves_only_when_there_is_something() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = TranscriptLog::new(dir.path().join("interviews"), "Interview 3");
        assert_eq!(log.save().unwrap(), None);
        for n in 1..=9 {
            log.add(INTERVIEWER, &format!("question {n}"));
        }
        assert_eq!(log.history(2), ["Interviewer: question 8", "Interviewer: question 9"]);
        let path = log.save().unwrap().unwrap();
        assert!(path.file_name().unwrap().to_string_lossy().ends_with(".md"));
        let all = read_all(path.parent().unwrap());
        assert_eq!((all.len(), all[0].title.as_str(), all[0].turns.len()), (1, "Interview 3", 9));
    }

    #[test]
    fn a_file_without_a_header_is_dated_by_its_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("2026-10-22_1023.md"), "**10:23:05 Interviewer:** hi there\n").unwrap();
        let all = read_all(dir.path());
        assert_eq!(all[0].started.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-10-22 10:23");
        assert_eq!(all[0].turns[0].at.format("%Y-%m-%d").to_string(), "2026-10-22");
    }
}
