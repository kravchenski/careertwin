//! The review after a call: what was good, what was wrong, how to say it better.
//!
//! The model never quotes the candidate from memory. It points at numbered turns and the program
//! fills in the words from the transcript, and a language note is kept only when the words it
//! quotes were really said.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::json::{extract_object, strings, text};
use super::{HONESTY, Lang, PromptError};
use crate::brief::Brief;
use crate::session::{Session, Speaker, Track};

const MAX_ANSWERS: usize = 8;
const MAX_NOTES: usize = 8;
const MAX_PHRASES: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub name: String,
    /// 1 (weak) to 5 (strong).
    pub value: u8,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnswerReview {
    pub question: String,
    pub you_said: String,
    pub good: Vec<String>,
    pub issues: Vec<String>,
    /// What to say next time, spoken style, built only from the candidate's real facts.
    pub better_answer: String,
    /// Points a strong answer would have made.
    pub missed_points: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LanguageNote {
    pub said: String,
    pub better: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Phrase {
    pub english: String,
    /// What it means, in the language of the interface.
    pub meaning: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Feedback {
    pub summary: String,
    pub scores: Vec<Score>,
    pub answers: Vec<AnswerReview>,
    pub language_notes: Vec<LanguageNote>,
    pub priorities: Vec<String>,
    pub phrases: Vec<Phrase>,
}

/// The score names the review fills in for a track.
pub fn score_names(track: Track, language: Lang) -> Vec<&'static str> {
    let mut names = match track {
        Track::English => vec!["Fluency", "Grammar", "Vocabulary", "Clarity", "Confidence"],
        Track::Technical => vec!["Accuracy", "Depth", "Structure", "Communication"],
        Track::Interview => vec!["Structure", "Relevance", "Specificity", "Communication"],
    };
    if track != Track::English && language == Lang::En {
        names.push("English");
    }
    names
}

pub fn system(track: Track, language: Lang, ui: Lang) -> String {
    let names = score_names(track, language).join(", ");
    let focus = match track {
        Track::English => {
            "This was an English conversation practice. Judge the candidate's English: fluency, grammar, \
             vocabulary, clarity. Technical correctness matters little here."
        }
        Track::Technical => {
            "This was a technical interview. Judge technical accuracy first: name precisely what is wrong and \
             the correct fact, and which points a strong answer would have made. If you are not certain of a \
             detail, say \"verify this\" instead of guessing."
        }
        Track::Interview => {
            "This was a general job interview. Judge the structure of the answers (situation, action, result), \
             relevance to the vacancy, specificity (real examples, numbers) and honesty."
        }
    };
    format!(
        "You are an interview coach reviewing a practice call after it ended. {focus}\n\n\
         The transcript is numbered: candidate turns look like \"[3] CANDIDATE: ...\". Refer to a candidate turn by \
         its number; never quote from memory.\n\n\
         Reply with JSON only, in this shape:\n\
         {{\"summary\": str, \"scores\": [{{\"name\": str, \"value\": 1-5, \"note\": str}}], \
         \"answers\": [{{\"turn\": int, \"good\": [str], \"issues\": [str], \"better_answer\": str, \
         \"missed_points\": [str]}}], \
         \"language_notes\": [{{\"said\": str, \"better\": str, \"why\": str}}], \
         \"priorities\": [str], \"phrases\": [{{\"english\": str, \"meaning\": str}}]}}\n\n\
         Rules\n\
         - scores: exactly these names, in this order: {names}. 1 is weak, 5 is strong. Be honest, not kind: a \
         3 is a normal first attempt.\n\
         - summary: two or three sentences, what went well and what to work on, plainly.\n\
         - answers: review at most {MAX_ANSWERS} of the candidate's most important answers (skip greetings). \
         good and issues are short, specific points (at most 3 each). better_answer is what the candidate could say \
         next time: spoken style, first person, at most 90 words, in {lang}. {honesty}\n\
         - language_notes: only for mistakes that are really in the candidate's turns. \"said\" must be copied \
         exactly from a candidate turn (a short fragment); \"better\" is the corrected phrase; \"why\" is one short \
         reason. At most {MAX_NOTES}. Skip anything that is only a speech-recognition slip.\n\
         - priorities: the three most useful things to practise next, each one short sentence.\n\
         - phrases: up to {MAX_PHRASES} useful {lang} phrases to learn (from your better answers or the call), with \
         the meaning written in {ui_lang}.\n\
         - Write summary, notes, issues and priorities in {ui_lang}. Quoted words and better answers stay in {lang}.\n\
         - If the candidate said almost nothing, say so and keep the lists short. Never invent what was not said.",
        lang = language.name(),
        ui_lang = ui.name(),
        honesty = HONESTY,
    )
}

/// The transcript with numbered candidate turns, then the context the review may rely on.
pub fn prompt(session: &Session, brief: &Brief) -> String {
    let mut number = 0;
    let mut lines = Vec::new();
    for turn in &session.turns {
        match turn.who {
            Speaker::Interviewer => lines.push(format!("INTERVIEWER: {}", turn.text)),
            Speaker::You => {
                number += 1;
                lines.push(format!("[{number}] CANDIDATE: {}", turn.text));
            }
        }
    }
    format!("{}\n\nTRANSCRIPT\n{}\n\nReview this call.", brief.render(), lines.join("\n"))
}

/// Reads the model's review and checks it against the transcript.
pub fn parse(reply: &str, session: &Session, track: Track, language: Lang) -> Result<Feedback, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("a review"))?;
    let summary = text(&value, "summary");
    let answers = answers(&value, session);
    let notes = language_notes(&value, session);
    if summary.is_empty() && answers.is_empty() && notes.is_empty() {
        return Err(PromptError::Unusable("a review"));
    }
    Ok(Feedback {
        summary,
        scores: scores(&value, track, language),
        answers,
        language_notes: notes,
        priorities: strings(&value, "priorities", 3),
        phrases: phrases(&value),
    })
}

fn scores(value: &Value, track: Track, language: Lang) -> Vec<Score> {
    let given = value.get("scores").and_then(Value::as_array).cloned().unwrap_or_default();
    score_names(track, language)
        .into_iter()
        .filter_map(|name| {
            let item = given.iter().find(|item| text(item, "name").eq_ignore_ascii_case(name))?;
            let raw = item.get("value").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))?;
            Some(Score { name: name.to_owned(), value: raw.round().clamp(1.0, 5.0) as u8, note: text(item, "note") })
        })
        .collect()
}

/// The numbered candidate turns with the question that came before each.
fn candidate_turns(session: &Session) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut last_question = String::new();
    for turn in &session.turns {
        match turn.who {
            Speaker::Interviewer => last_question = turn.text.clone(),
            Speaker::You => out.push((last_question.clone(), turn.text.clone())),
        }
    }
    out
}

fn answers(value: &Value, session: &Session) -> Vec<AnswerReview> {
    let turns = candidate_turns(session);
    let mut seen = std::collections::HashSet::new();
    value
        .get("answers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let number =
                item.get("turn").and_then(|t| t.as_u64().or_else(|| t.as_str().and_then(|s| s.trim().parse().ok())))?
                    as usize;
            let (question, you_said) = turns.get(number.checked_sub(1)?)?.clone();
            seen.insert(number).then_some(())?;
            Some(AnswerReview {
                question,
                you_said,
                good: strings(item, "good", 3),
                issues: strings(item, "issues", 3),
                better_answer: text(item, "better_answer"),
                missed_points: strings(item, "missed_points", 4),
            })
        })
        .take(MAX_ANSWERS)
        .collect()
}

fn normalized(text: &str) -> String {
    let cleaned: String =
        text.to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'').collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn language_notes(value: &Value, session: &Session) -> Vec<LanguageNote> {
    let said_by_candidate: Vec<String> =
        session.turns.iter().filter(|t| t.who == Speaker::You).map(|t| normalized(&t.text)).collect();
    value
        .get("language_notes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let said = text(item, "said");
            let better = text(item, "better");
            let wanted = normalized(&said);
            // a note about words nobody said is the model making things up
            if wanted.is_empty() || better.is_empty() || !said_by_candidate.iter().any(|turn| turn.contains(&wanted)) {
                return None;
            }
            Some(LanguageNote { said, better, why: text(item, "why") })
        })
        .take(MAX_NOTES)
        .collect()
}

fn phrases(value: &Value) -> Vec<Phrase> {
    value
        .get("phrases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let english = text(item, "english");
            (!english.is_empty()).then(|| Phrase { english, meaning: text(item, "meaning") })
        })
        .take(MAX_PHRASES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Kind;

    fn session() -> Session {
        let mut s = Session::new(Track::Technical, Kind::Live, "t");
        s.add_turn(Speaker::Interviewer, "Hi Alex. What do you work on?", None);
        s.add_turn(Speaker::You, "I build the order API with FastAPI.", None);
        s.add_turn(Speaker::Interviewer, "What happened after the release?", None);
        s.add_turn(Speaker::You, "The latency was doubled, I have added a index.", None);
        s
    }

    fn reply(extra: &str) -> String {
        format!(
            r#"```json
{{"summary": "Solid start.", "scores": [
  {{"name": "accuracy", "value": 4, "note": "right"}},
  {{"name": "Depth", "value": "3", "note": "thin"}},
  {{"name": "Structure", "value": 9, "note": "clamped"}},
  {{"name": "Unknown", "value": 5, "note": "dropped"}}],
 "answers": [
  {{"turn": 2, "good": ["named the cause"], "issues": ["no number"], "better_answer": "p95 doubled; I added an index.", "missed_points": ["alert"]}},
  {{"turn": 2, "good": ["dup"]}},
  {{"turn": 9, "good": ["no such turn"]}},
  {{"turn": 0, "good": ["zero"]}}],
 "language_notes": [
  {{"said": "I have added a index", "better": "I added an index", "why": "a before a vowel sound"}},
  {{"said": "we utilised synergies", "better": "x", "why": "invented"}}],
 "priorities": ["numbers", "a", "b", "c"],
 "phrases": [{{"english": "p95 latency doubled", "meaning": "p95 удвоился"}}, {{"english": ""}}]{extra}}}
```"#
        )
    }

    #[test]
    fn the_review_is_read_and_checked_against_the_transcript() {
        let feedback = parse(&reply(""), &session(), Track::Technical, Lang::En).unwrap();
        assert_eq!(feedback.summary, "Solid start.");
        // only the real names, in order, clamped, parsed from strings
        let names: Vec<&str> = feedback.scores.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Accuracy", "Depth", "Structure"]);
        assert_eq!(feedback.scores[1].value, 3);
        assert_eq!(feedback.scores[2].value, 5);
        // the words and the question come from the transcript, not from the model
        assert_eq!(feedback.answers.len(), 1);
        assert_eq!(feedback.answers[0].question, "What happened after the release?");
        assert_eq!(feedback.answers[0].you_said, "The latency was doubled, I have added a index.");
        assert_eq!(feedback.answers[0].issues, vec!["no number"]);
        // a note about words nobody said is dropped
        assert_eq!(feedback.language_notes.len(), 1);
        assert_eq!(feedback.language_notes[0].better, "I added an index");
        assert_eq!(feedback.priorities.len(), 3);
        assert_eq!(feedback.phrases.len(), 1);
    }

    #[test]
    fn the_english_score_is_only_asked_for_in_english() {
        assert!(score_names(Track::Technical, Lang::En).contains(&"English"));
        assert!(!score_names(Track::Technical, Lang::Ru).contains(&"English"));
        assert!(!score_names(Track::English, Lang::En).contains(&"English"));
        assert_eq!(score_names(Track::English, Lang::En).len(), 5);
    }

    #[test]
    fn a_reply_with_nothing_in_it_is_refused() {
        assert!(parse("sorry, I cannot", &session(), Track::Technical, Lang::En).is_err());
        assert!(parse(r#"{"scores": []}"#, &session(), Track::Technical, Lang::En).is_err());
    }

    #[test]
    fn a_review_with_only_a_summary_is_still_useful() {
        let feedback = parse(r#"{"summary": "Too short to judge."}"#, &session(), Track::Interview, Lang::En).unwrap();
        assert_eq!(feedback.summary, "Too short to judge.");
        assert!(feedback.answers.is_empty() && feedback.scores.is_empty());
    }

    #[test]
    fn note_matching_ignores_case_and_punctuation() {
        let value: Value = serde_json::from_str(
            r#"{"language_notes": [{"said": "I HAVE ADDED a index!", "better": "I added an index", "why": "x"}]}"#,
        )
        .unwrap();
        assert_eq!(language_notes(&value, &session()).len(), 1);
    }

    #[test]
    fn the_prompt_numbers_only_the_candidate_turns() {
        let text = prompt(&session(), &Brief::default());
        assert!(text.contains("INTERVIEWER: Hi Alex."));
        assert!(text.contains("[1] CANDIDATE: I build the order API"));
        assert!(text.contains("[2] CANDIDATE: The latency was doubled"));
        assert!(!text.contains("[3]"));
    }

    #[test]
    fn the_system_prompt_names_the_scores_and_forbids_inventing() {
        let text = system(Track::Technical, Lang::En, Lang::Ru);
        assert!(text.contains("Accuracy, Depth, Structure, Communication, English"));
        assert!(text.contains("Never invent what was not said"));
        assert!(text.contains("copied exactly"));
        assert!(text.contains("verify this"));
        assert!(text.contains("Write summary, notes, issues and priorities in Russian"));
        assert!(text.contains("Never invent any of them"));
        let english = system(Track::English, Lang::En, Lang::En);
        assert!(english.contains("Fluency, Grammar, Vocabulary, Clarity, Confidence"));
    }

    #[test]
    fn a_missing_question_before_a_turn_is_just_empty() {
        let mut s = Session::new(Track::English, Kind::Live, "t");
        s.add_turn(Speaker::You, "Hello there.", None);
        let feedback = parse(r#"{"answers": [{"turn": 1, "good": ["ok"]}]}"#, &s, Track::English, Lang::En).unwrap();
        assert_eq!(feedback.answers[0].question, "");
        assert_eq!(feedback.answers[0].you_said, "Hello there.");
    }
}
