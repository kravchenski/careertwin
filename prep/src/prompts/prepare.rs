//! Preparing before the call: your CV against the vacancy, and the questions it invites.

use serde::{Deserialize, Serialize};

use super::json::{extract_object, strings, text};
use super::{HONESTY, Lang, PromptError};
use crate::brief::Brief;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    pub gap: String,
    pub how_to_address: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub summary: String,
    pub strengths: Vec<String>,
    pub gaps: Vec<Gap>,
    pub likely_questions: Vec<String>,
    pub cv_fixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BankQuestion {
    pub question: String,
    pub kind: String,
    /// What a good answer says, in short first-person lines built from the candidate's facts.
    pub outline: Vec<String>,
}

pub fn analysis_system(language: Lang) -> String {
    format!(
        "You review a candidate's CV against a vacancy before an interview. Use ONLY facts in the candidate \
         context and the documents; never invent experience. Be specific and honest. Write in {lang} and keep \
         company names, technologies and job titles as written. Reply with JSON only: {{\"summary\": str, \
         \"strengths\": [str], \"gaps\": [{{\"gap\": str, \"how_to_address\": str}}], \"likely_questions\": [str], \
         \"cv_fixes\": [str]}}. summary is two or three sentences on how well the CV fits and what the vacancy cares \
         about most. strengths are the real facts to lead with. gaps are requirements the CV does not show, each \
         with an honest way to address it in the interview. likely_questions are the questions those gaps invite. \
         cv_fixes are concrete edits to the CV text, not generic advice. At most five strengths, five gaps, six \
         questions and six fixes, each under 30 words.",
        lang = language.name()
    )
}

pub fn analysis_prompt(brief: &Brief) -> String {
    format!("{}\n\nReview the CV against the vacancy.", brief.render())
}

pub fn parse_analysis(reply: &str) -> Result<Analysis, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("a CV review"))?;
    let gaps: Vec<Gap> = value
        .get("gaps")
        .and_then(|g| g.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let gap = text(item, "gap");
            (!gap.is_empty()).then(|| Gap { gap, how_to_address: text(item, "how_to_address") })
        })
        .take(5)
        .collect();
    let analysis = Analysis {
        summary: text(&value, "summary"),
        strengths: strings(&value, "strengths", 5),
        gaps,
        likely_questions: strings(&value, "likely_questions", 6),
        cv_fixes: strings(&value, "cv_fixes", 6),
    };
    if analysis.summary.is_empty() && analysis.gaps.is_empty() {
        return Err(PromptError::Unusable("a CV review"));
    }
    Ok(analysis)
}

pub fn questions_system(language: Lang) -> String {
    format!(
        "You prepare a candidate for an interview. From the candidate context and the vacancy, write the questions \
         this interviewer is most likely to ask, mixing definitions and technical questions on the vacancy's stack, \
         behavioural questions and motivation. For each give three to five short first-person lines the candidate \
         can say. Definition and technical-knowledge questions get a correct explanation (general knowledge is \
         fine). {honesty} Never write \"in my context\", \"the CV\" or \"not stated\" inside the lines: speak as the \
         candidate. Write in {lang} and keep company names, technologies and job titles as written. Reply with JSON \
         only: {{\"questions\": [{{\"question\": str, \"kind\": str, \"outline\": [str]}}]}} where kind is one of \
         intro, definition, technical, behavioral, motivation, salary, logistics.",
        honesty = HONESTY,
        lang = language.name()
    )
}

pub fn questions_prompt(brief: &Brief, count: usize, focus: &str) -> String {
    let base = format!("{}\n\nWrite {count} questions.", brief.render());
    if focus.trim().is_empty() { base } else { format!("{base} Focus on: {}.", focus.trim()) }
}

pub fn parse_questions(reply: &str, count: usize) -> Result<Vec<BankQuestion>, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("questions"))?;
    let questions: Vec<BankQuestion> = value
        .get("questions")
        .and_then(|q| q.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let question = text(item, "question");
            (!question.is_empty()).then(|| BankQuestion {
                question,
                kind: text(item, "kind"),
                outline: strings(item, "outline", 6),
            })
        })
        .take(count.max(1))
        .collect();
    if questions.is_empty() { Err(PromptError::Unusable("questions")) } else { Ok(questions) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_analysis_is_read_and_capped() {
        let reply = r#"```json
{"summary": "Good fit.", "strengths": ["a","b","c","d","e","f"], "gaps": [{"gap": "Kubernetes", "how_to_address": "say it was a course"}, {"gap": ""}], "likely_questions": ["q"], "cv_fixes": []}
```"#;
        let analysis = parse_analysis(reply).unwrap();
        assert_eq!(analysis.strengths.len(), 5);
        assert_eq!(analysis.gaps.len(), 1);
        assert_eq!(analysis.gaps[0].gap, "Kubernetes");
    }

    #[test]
    fn an_empty_analysis_is_refused() {
        assert!(parse_analysis(r#"{"strengths": ["x"]}"#).is_err());
        assert!(parse_analysis("nope").is_err());
    }

    #[test]
    fn questions_are_read_and_limited() {
        let reply = r#"{"questions": [{"question": "Tell me about yourself", "kind": "intro", "outline": ["I am Alex."]}, {"question": ""}, {"question": "Why Acme?", "kind": "motivation", "outline": []}]}"#;
        let questions = parse_questions(reply, 10).unwrap();
        assert_eq!(questions.len(), 2);
        assert_eq!(parse_questions(reply, 1).unwrap().len(), 1);
        assert!(parse_questions(r#"{"questions": []}"#, 3).is_err());
    }

    #[test]
    fn prompts_carry_the_honesty_rule_and_the_context() {
        let brief = Brief { candidate: "## Skills\n- Python".to_owned(), ..Brief::default() };
        assert!(questions_system(Lang::En).contains("Never invent any of them"));
        assert!(analysis_system(Lang::Ru).contains("Write in Russian"));
        assert!(questions_prompt(&brief, 6, "Kubernetes").contains("Focus on: Kubernetes."));
        assert!(analysis_prompt(&brief).contains("CANDIDATE CONTEXT"));
    }
}
