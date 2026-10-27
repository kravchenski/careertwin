//! Preparing before the call: a bank of likely questions and a review of your CV for the vacancy.

use serde_json::Value;
use thiserror::Error;

use crate::brief::InterviewBrief;
use crate::languages;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct PrepareError(pub String);

pub const QUESTIONS_SYSTEM: &str = "You prepare a candidate for an interview. From the candidate context and the vacancy, write \
the questions this interviewer is most likely to ask, mixing definitions and technical \
questions on the vacancy's stack, behavioral questions and motivation. For each give three to \
five short first-person lines the candidate can say. Definition and technical-knowledge \
questions get a correct explanation (general knowledge is fine). Statements about the \
candidate's own experience, employers, numbers and skills must come ONLY from the context; if \
the candidate lacks experience, a line says so plainly (\"I haven't used X in production\") and \
the next lines bridge to the closest real experience. Never write \"in my context\", \"the CV\" or \
\"not stated\" inside the lines: speak as the candidate. Write everything in the requested \
language and keep company names, technologies and job titles as written. Reply with JSON only: \
{\"questions\": [{\"question\": str, \"kind\": str, \"outline\": [str]}]} where kind is one of \
intro, definition, technical, behavioral, motivation, salary, logistics.";

pub const ANALYSIS_SYSTEM: &str = "You review a candidate's CV against a vacancy before an interview. Use ONLY facts in the \
candidate context and the documents; never invent experience. Be specific and honest. Write \
everything in the requested language and keep company names, technologies and job titles as \
written. Reply with JSON only: {\"summary\": str, \"strengths\": [str], \"gaps\": [{\"gap\": str, \
\"how_to_address\": str}], \"likely_questions\": [str], \"cv_fixes\": [str]}. summary is two or \
three sentences on how well the CV fits and what the vacancy cares about most. strengths are \
the real facts to lead with. gaps are requirements the CV does not show, each with an honest \
way to address it in the interview. likely_questions are the questions those gaps invite. \
cv_fixes are concrete edits to the CV text, not generic advice. Keep it short: at most five \
strengths, five gaps, six questions and six fixes, each under 30 words.";

pub fn questions_prompt(brief: &InterviewBrief, count: usize, focus: &str, language: Option<&str>) -> String {
    let prompt = format!("{}\n\nWrite {count} questions in {}.", brief.render(None), languages::name(language));
    if focus.trim().is_empty() { prompt } else { format!("{prompt} Focus on: {}.", focus.trim()) }
}

pub fn analysis_prompt(brief: &InterviewBrief, language: Option<&str>) -> String {
    format!("{}\n\nReview the CV against the vacancy. Write in {}.", brief.render(None), languages::name(language))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankQuestion {
    pub question: String,
    pub kind: String,
    pub outline: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gap {
    pub gap: String,
    pub how_to_address: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    pub summary: String,
    pub strengths: Vec<String>,
    pub gaps: Vec<Gap>,
    pub likely_questions: Vec<String>,
    pub cv_fixes: Vec<String>,
}

/// The first JSON object in a model's reply: models wrap it in a code fence or a sentence.
pub fn json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| serde_json::from_str::<Value>(&text[start..=end]).ok()).flatten().filter(Value::is_object)
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).map(|items| items.iter().map(|item| item.as_str().map_or_else(|| item.to_string(), str::to_owned)).collect()).unwrap_or_default()
}

fn field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").trim().to_owned()
}

pub fn parse_questions(text: &str, count: usize) -> Result<Vec<BankQuestion>, PrepareError> {
    let error = || PrepareError("The model did not return questions. Try again or pick another model.".into());
    let data = json_object(text).ok_or_else(error)?;
    let items = data.get("questions").and_then(Value::as_array).filter(|items| !items.is_empty()).ok_or_else(error)?;
    let questions: Vec<BankQuestion> = items
        .iter()
        .filter(|item| item.is_object() && !field(item, "question").is_empty())
        .map(|item| BankQuestion { question: field(item, "question"), kind: field(item, "kind"), outline: strings(item.get("outline")).into_iter().take(6).collect() })
        .take(count)
        .collect();
    if questions.is_empty() { Err(error()) } else { Ok(questions) }
}

pub fn parse_analysis(text: &str) -> Result<Analysis, PrepareError> {
    let error = || PrepareError("The model did not return a CV review. Try again or pick another model.".into());
    let data = json_object(text).ok_or_else(error)?;
    let summary = field(&data, "summary");
    let gaps: Vec<Gap> = data
        .get("gaps")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object() && !field(item, "gap").is_empty()).map(|item| Gap { gap: field(item, "gap"), how_to_address: field(item, "how_to_address") }).collect())
        .unwrap_or_default();
    if summary.is_empty() && gaps.is_empty() {
        return Err(error());
    }
    Ok(Analysis { summary, strengths: strings(data.get("strengths")), gaps, likely_questions: strings(data.get("likely_questions")), cv_fixes: strings(data.get("cv_fixes")) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_is_found_inside_a_fence_or_a_sentence() {
        assert!(json_object("```json\n{\"a\": 1}\n```").is_some());
        assert!(json_object("Here you go: {\"a\": {\"b\": 2}} done").is_some());
        assert!(json_object("no json").is_none() && json_object("} {").is_none() && json_object("{broken").is_none());
        assert!(json_object("[1, 2]").is_none());
    }

    #[test]
    fn the_prompts_carry_the_background_the_count_the_focus_and_the_language() {
        let brief = InterviewBrief::new("## Skills\n| Strong | Python |\n");
        let text = questions_prompt(&brief, 8, " SQL, system design ", Some("pl"));
        assert!(text.contains("CANDIDATE CONTEXT") && text.ends_with("Write 8 questions in Polish. Focus on: SQL, system design."));
        assert!(!questions_prompt(&brief, 5, "", None).contains("Focus on"));
        assert!(analysis_prompt(&brief, Some("ru")).ends_with("Review the CV against the vacancy. Write in Russian."));
    }

    #[test]
    fn a_question_bank_is_read_and_cut_to_the_count() {
        let reply = r#"```json
{"questions": [{"question": "What is a join?", "kind": "definition", "outline": ["a", "b", "c", "d", "e", "f", "g"]},
               {"question": ""}, {"question": "Why us?", "kind": "motivation"}, "junk", {"question": "Third"}]}
```"#;
        let questions = parse_questions(reply, 2).unwrap();
        assert_eq!(questions.len(), 2);
        assert_eq!((questions[0].question.as_str(), questions[0].outline.len()), ("What is a join?", 6));
        assert_eq!((questions[1].question.as_str(), questions[1].outline.len()), ("Why us?", 0));
        for bad in ["", "no json", r#"{"questions": []}"#, r#"{"questions": [{"kind": "x"}]}"#, "{\"x\": 1}"] {
            assert!(parse_questions(bad, 5).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_cv_review_is_read() {
        let reply = r#"{"summary": " Good fit. ", "strengths": ["Python"], "gaps": [{"gap": "Spark", "how_to_address": "Say so"}, {"gap": ""}],
                        "likely_questions": ["Why Spark?"], "cv_fixes": ["Add numbers"]}"#;
        let analysis = parse_analysis(reply).unwrap();
        assert_eq!(analysis.summary, "Good fit.");
        assert_eq!(analysis.gaps, [Gap { gap: "Spark".into(), how_to_address: "Say so".into() }]);
        assert_eq!((analysis.strengths, analysis.likely_questions, analysis.cv_fixes), (vec!["Python".to_owned()], vec!["Why Spark?".to_owned()], vec!["Add numbers".to_owned()]));
        assert!(parse_analysis("{}").is_err() && parse_analysis("sorry").is_err());
    }
}
