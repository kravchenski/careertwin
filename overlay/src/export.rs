//! Export the questions of the saved interviews, for analysis: which were asked, of what kind, in which
//! language, whether the window answered, and what it said.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::languages;
use crate::questions::{Kind, classify, detect_language, worth_an_answer};
use crate::transcript::{INTERVIEWER, Record, SCREENSHOT, read_all};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Csv,
    Markdown,
    Json,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Csv, Format::Markdown, Format::Json];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Markdown => "md",
            Format::Json => "json",
        }
    }
}

/// One question that was put in an interview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub date: String,
    pub time: String,
    /// The name of the interview, or its date and time when the record has no name.
    pub interview: String,
    pub kind: String,
    pub language: String,
    pub question: String,
    pub answered: bool,
    pub answer: String,
}

/// The questions of these records, in the order they were asked. A question is what the interviewer said and
/// was worth an answer, what was typed, and a screenshot; what the window showed for it is the answer.
pub fn rows(records: &[Record]) -> Vec<Row> {
    let mut out = Vec::new();
    for record in records {
        let interview = if record.title.is_empty() {
            record.started.map(|started| started.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
        } else {
            record.title.clone()
        };
        let mut shown = record.suggestions.iter().collect::<Vec<_>>();
        for turn in record.turns.iter().filter(|turn| turn.speaker == INTERVIEWER) {
            let screenshot = turn.text == SCREENSHOT;
            let position = shown.iter().position(|(question, _)| *question == turn.text);
            if !screenshot && position.is_none() && !worth_an_answer(&turn.text, None) {
                continue; // something said that was not put to the candidate
            }
            let answer = position.map(|index| shown.remove(index).1.clone());
            let (kind, language) = if screenshot {
                ("screenshot".to_owned(), String::new())
            } else {
                let kind: Kind = classify(&turn.text);
                (kind.as_str().to_owned(), detect_language(&turn.text).to_owned())
            };
            out.push(Row {
                date: turn.at.format("%Y-%m-%d").to_string(),
                time: turn.at.format("%H:%M:%S").to_string(),
                interview: interview.clone(),
                kind,
                language,
                question: turn.text.clone(),
                answered: answer.as_ref().is_some_and(|answer| !answer.is_empty()),
                answer: answer.unwrap_or_default(),
            });
        }
    }
    out
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_owned() }
}

/// A byte order mark first, so Excel reads the Cyrillic and the accents as they are.
pub fn to_csv(rows: &[Row]) -> String {
    let mut out = String::from("\u{feff}date,time,interview,kind,language,question,answered,answer\r\n");
    for row in rows {
        let cells = [
            &row.date,
            &row.time,
            &row.interview,
            &row.kind,
            &row.language,
            &row.question,
            &(if row.answered { "yes" } else { "no" }).to_owned(),
            &row.answer,
        ];
        out.push_str(&cells.iter().map(|cell| csv_cell(cell)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

pub fn to_json(rows: &[Row]) -> String {
    let items: Vec<_> = rows
        .iter()
        .map(|row| {
            json!({
                "date": row.date, "time": row.time, "interview": row.interview, "kind": row.kind,
                "language": row.language, "question": row.question, "answered": row.answered, "answer": row.answer,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({"count": rows.len(), "questions": items})).unwrap_or_default() + "\n"
}

/// A readable list by interview, with how many questions of each kind, to hand to a model for analysis.
pub fn to_markdown(rows: &[Row]) -> String {
    let mut out = format!("# Interview questions\n\n{} questions", rows.len());
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for row in rows {
        match kinds.iter_mut().find(|(kind, _)| *kind == row.kind) {
            Some((_, count)) => *count += 1,
            None => kinds.push((row.kind.clone(), 1)),
        }
    }
    kinds.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    if !kinds.is_empty() {
        out.push_str(": ");
        out.push_str(&kinds.iter().map(|(kind, count)| format!("{kind} {count}")).collect::<Vec<_>>().join(", "));
    }
    out.push('\n');
    let mut current = "";
    let mut number = 0;
    for row in rows {
        if row.interview != current {
            out.push_str(&format!("\n## {}\n\n", row.interview));
            current = &row.interview;
            number = 0;
        }
        number += 1;
        let language =
            if row.language.is_empty() { String::new() } else { format!(", {}", languages::name(Some(&row.language))) };
        out.push_str(&format!("{number}. **{}** ({}{language}, {} {})\n", row.question, row.kind, row.date, row.time));
        if row.answered {
            for line in row.answer.lines() {
                out.push_str(&format!("   > {line}\n"));
            }
        } else {
            out.push_str("   > (no answer shown)\n");
        }
    }
    out
}

pub fn render(rows: &[Row], format: Format) -> String {
    match format {
        Format::Csv => to_csv(rows),
        Format::Markdown => to_markdown(rows),
        Format::Json => to_json(rows),
    }
}

/// Write the questions of every saved interview in each chosen format; returns the files written.
pub fn export_questions(
    transcripts: &Path,
    into: &Path,
    formats: &[Format],
    stamp: &str,
) -> std::io::Result<Vec<PathBuf>> {
    let all = rows(&read_all(transcripts));
    std::fs::create_dir_all(into)?;
    let mut written = Vec::new();
    for format in formats {
        let path = into.join(format!("questions-{stamp}.{}", format.extension()));
        std::fs::write(&path, render(&all, *format))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{Turn, YOU};
    use chrono::NaiveDate;

    fn record() -> Record {
        let day = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let turn = |speaker: &str, text: &str, h, m, s| Turn {
            speaker: speaker.into(),
            text: text.into(),
            at: day.and_hms_opt(h, m, s).unwrap(),
        };
        Record {
            title: "Vention".into(),
            started: Some(day.and_hms_opt(13, 0, 0).unwrap()),
            turns: vec![
                turn(INTERVIEWER, "Hello, nice to meet you.", 13, 1, 0),
                turn(INTERVIEWER, "What is a left join?", 13, 2, 0),
                turn(YOU, "It keeps the left rows.", 13, 2, 30),
                turn(INTERVIEWER, "Розкажіть про себе", 13, 5, 0),
                turn(INTERVIEWER, SCREENSHOT, 13, 9, 0),
            ],
            suggestions: vec![
                ("What is a left join?".into(), "A join that keeps\nthe left rows, \"always\"".into()),
                (SCREENSHOT.into(), "def f(): pass".into()),
            ],
        }
    }

    #[test]
    fn the_questions_are_the_interviewers_words_that_were_put_to_the_candidate() {
        let rows = rows(&[record()]);
        let asked: Vec<_> = rows.iter().map(|row| row.question.as_str()).collect();
        assert_eq!(asked, ["What is a left join?", "Розкажіть про себе", SCREENSHOT]); // not the greeting, not "You"
        assert_eq!((rows[0].kind.as_str(), rows[0].language.as_str(), rows[0].answered), ("definition", "en", true));
        assert_eq!((rows[1].kind.as_str(), rows[1].language.as_str(), rows[1].answered), ("intro", "uk", false));
        assert_eq!((rows[2].kind.as_str(), rows[2].answer.as_str()), ("screenshot", "def f(): pass"));
        assert_eq!(
            (rows[0].interview.as_str(), rows[0].date.as_str(), rows[0].time.as_str()),
            ("Vention", "2026-10-27", "13:02:00")
        );
    }

    #[test]
    fn a_question_that_was_only_typed_and_answered_is_a_question_too() {
        let mut record = record();
        record.turns.push(Turn { speaker: INTERVIEWER.into(), text: "heap".into(), at: record.turns[0].at });
        record.suggestions.push(("heap".into(), "A tree-based structure.".into()));
        assert!(rows(&[record]).iter().any(|row| row.question == "heap" && row.answered));
    }

    #[test]
    fn a_record_without_a_name_is_named_by_its_start() {
        let mut record = record();
        record.title.clear();
        assert_eq!(rows(&[record])[0].interview, "2026-10-27 13:00");
    }

    #[test]
    fn csv_quotes_what_needs_it_and_starts_with_a_byte_order_mark() {
        let csv = to_csv(&rows(&[record()]));
        assert!(csv.starts_with("\u{feff}date,time,interview,kind,language,question,answered,answer\r\n"));
        assert!(csv.contains("2026-10-27,13:02:00,Vention,definition,en,What is a left join?,yes,\"A join that keeps\nthe left rows, \"\"always\"\"\"\r\n"));
        assert!(csv.contains("Розкажіть про себе,no,\r\n"));
    }

    #[test]
    fn json_and_markdown_carry_the_same_questions() {
        let rows = rows(&[record()]);
        let json: serde_json::Value = serde_json::from_str(&to_json(&rows)).unwrap();
        assert_eq!((json["count"].as_u64(), json["questions"][1]["kind"].as_str()), (Some(3), Some("intro")));
        let md = to_markdown(&rows);
        assert!(
            md.starts_with("# Interview questions\n\n3 questions: ")
                && md.contains("## Vention\n\n1. **What is a left join?** (definition, English, 2026-10-27 13:02:00)")
        );
        assert!(
            md.contains("   > A join that keeps\n   > the left rows")
                && md.contains("2. **Розкажіть про себе** (intro, Ukrainian")
                && md.contains("(no answer shown)")
        );
    }

    #[test]
    fn the_export_writes_one_file_per_chosen_format() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = dir.path().join("interviews");
        std::fs::create_dir(&transcripts).unwrap();
        std::fs::write(transcripts.join("2026-10-27_1300.md"), crate::transcript::render(&record())).unwrap();
        let written = export_questions(&transcripts, &dir.path().join("exports"), &Format::ALL, "2026-10-27").unwrap();
        let names: Vec<_> =
            written.iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["questions-2026-10-27.csv", "questions-2026-10-27.md", "questions-2026-10-27.json"]);
        assert!(std::fs::read_to_string(&written[2]).unwrap().contains("\"count\": 3"));
        assert!(
            export_questions(&dir.path().join("missing"), &dir.path().join("e2"), &[Format::Csv], "x").unwrap().len()
                == 1
        ); // nothing to export is an empty file, not an error
    }
}
