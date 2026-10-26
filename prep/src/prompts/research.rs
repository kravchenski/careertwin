//! Researching a vacancy: which topics will be asked (ranked by the 80/20 rule), the terms inside
//! each, and, after a lesson, the few lines worth keeping.
//!
//! The model knows what interviews for a role usually ask; it does not know the interviewers. The
//! prompts say so, and keep the plain explanations in the language you think in.

use serde::{Deserialize, Serialize};

use super::json::{extract_object, strings, text};
use super::{Lang, PromptError};
use crate::brief::{Brief, truncate_chars};
use crate::syllabus::{Syllabus, Term, Topic, TopicKind};

const MAX_TOPICS: usize = 14;
const MAX_TERMS: usize = 6;
const VACANCY_CHARS: usize = 4000;

fn vacancy_text(brief: &Brief) -> String {
    let head = [brief.job_title.as_str(), brief.company.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" at ");
    format!("VACANCY: {head}\n{}", truncate_chars(&brief.job_description, VACANCY_CHARS)).trim().to_owned()
}

pub fn map_system(native: Lang) -> String {
    format!(
        "You prepare a candidate for a technical job interview. Read the vacancy and decide which topics the \
         interviewers are most likely to ask about, then rank them. The aim is the 80/20 rule: a few topics \
         account for most of the questions, and they must stand out.\n\
         - Start from the vacancy: its stack, duties and seniority. Add the fundamentals that interviews for this \
         kind of role check even when the vacancy does not name them (object-oriented programming, HTTP and REST, \
         databases and SQL, testing, git, how the web works, and so on, as fits the role).\n\
         - You know what such interviews usually ask; you do not know the company or the interviewers. Invent \
         nothing about them.\n\
         - Give 8 to 14 topics. Each name is short, in English, as an interviewer would say it (\"Object-oriented \
         programming\", \"Spring Boot\", \"REST and HTTP\", \"SQL and indexes\").\n\
         - likelihood is 1 to 10: how likely a question on it is in a first technical interview. effort is 1 to \
         10: how much work it takes a beginner to answer it well. Use the whole range; not everything is a 9.\n\
         - kind is one of concept, framework, language, practice, behaviour.\n\
         - why is one short sentence in {lang} that says what in the vacancy makes the topic likely.\n\
         Reply with JSON only: {{\"role\": str, \"summary\": str, \"topics\": [{{\"name\": str, \"kind\": str, \
         \"why\": str, \"likelihood\": int, \"effort\": int}}]}}. role is the job title in English. summary is two \
         sentences in {lang} on what this interview will be about.",
        lang = native.name()
    )
}

pub fn map_prompt(brief: &Brief) -> String {
    let skills = brief.skill_terms();
    let known = if skills.is_empty() {
        String::new()
    } else {
        format!("\n\nThe candidate already lists these skills: {}.", skills.join(", "))
    };
    format!("{}{known}\n\nRank the topics for this vacancy.", vacancy_text(brief))
}

pub fn parse_map(reply: &str, native: Lang) -> Result<Syllabus, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("a list of topics"))?;
    let mut topics: Vec<Topic> = Vec::new();
    for item in value.get("topics").and_then(|t| t.as_array()).into_iter().flatten() {
        let name = text(item, "name");
        if name.is_empty() || topics.iter().any(|t| t.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let number = |key: &str| item.get(key).and_then(|v| v.as_f64()).map_or(5, |n| n.round().clamp(1.0, 10.0) as u8);
        topics.push(Topic {
            name,
            kind: TopicKind::parse(&text(item, "kind")),
            why: text(item, "why"),
            likelihood: number("likelihood"),
            effort: number("effort"),
            terms: Vec::new(),
        });
        if topics.len() == MAX_TOPICS {
            break;
        }
    }
    if topics.is_empty() {
        return Err(PromptError::Unusable("a list of topics"));
    }
    Ok(Syllabus { role: text(&value, "role"), summary: text(&value, "summary"), topics, native, ..Syllabus::default() })
}

pub fn terms_system(native: Lang) -> String {
    format!(
        "You teach one topic of a technical interview to a complete beginner who must then say it in English. \
         Write 4 to 6 terms an interviewer would ask the candidate to define or explain on this topic, the most \
         commonly asked first. For each term:\n\
         - name: the term in English, as it is said in an interview.\n\
         - plain: two or three short sentences in {lang}, in plain words, for someone who has never programmed. \
         Explain any word that is not everyday. No jargon for its own sake.\n\
         - analogy: one sentence in {lang} comparing it to something from everyday life (a kitchen, a post office, \
         a library, a road), and the comparison must really fit.\n\
         - english: a good spoken answer in English, two or three sentences in simple words (B1 to B2), the way a \
         candidate would say it in an interview. Accurate, and the same facts as plain.\n\
         - follow_ups: two questions the interviewer is likely to ask next, in English.\n\
         - visual: one sentence in English that says what a short animated picture should show to make the term \
         click (objects, arrows, what moves).\n\
         Reply with JSON only: {{\"terms\": [{{\"name\": str, \"plain\": str, \"analogy\": str, \"english\": str, \
         \"follow_ups\": [str], \"visual\": str}}]}}",
        lang = native.name()
    )
}

pub fn terms_prompt(brief: &Brief, topic: &Topic) -> String {
    format!(
        "{}\n\nTOPIC: {} ({})\nWhy it is likely: {}",
        vacancy_text(brief),
        topic.name,
        topic.kind.label(),
        topic.why
    )
}

pub fn parse_terms(reply: &str) -> Result<Vec<Term>, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("terms"))?;
    let terms: Vec<Term> = value
        .get("terms")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = text(item, "name");
            let plain = text(item, "plain");
            (!name.is_empty() && !plain.is_empty()).then(|| Term {
                name,
                plain,
                analogy: text(item, "analogy"),
                english: text(item, "english"),
                follow_ups: strings(item, "follow_ups", 3),
                visual: text(item, "visual"),
            })
        })
        .take(MAX_TERMS)
        .collect();
    if terms.is_empty() { Err(PromptError::Unusable("terms")) } else { Ok(terms) }
}

/// What a lesson leaves behind: the lines worth keeping, the slips, and a reason to come back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Digest {
    pub vital: Vec<String>,
    pub mistakes: Vec<String>,
    pub next: String,
    pub hook: String,
}

pub fn digest_system(native: Lang) -> String {
    format!(
        "You write the notes after a tutoring session for the person who just took it. Use the 80/20 rule: keep \
         the 20% of what was covered that matters most, not a summary of everything.\n\
         - vital: at most 5 lines, each under 25 words, each one fact, rule or phrase to remember, taken from what \
         was really said in the session. Not generic advice.\n\
         - mistakes: at most 3 slips the person made, each with the fix, under 25 words. May be empty.\n\
         - next: one sentence on the single best thing to do next time.\n\
         - hook: one sentence that gives a real reason to come back: what is already learned and what is close. \
         No flattery, no clichés, no exclamation marks.\n\
         Write in {lang}; keep terms and code as they are written in English. Reply with JSON only: \
         {{\"vital\": [str], \"mistakes\": [str], \"next\": str, \"hook\": str}}",
        lang = native.name()
    )
}

pub fn digest_prompt(transcript: &str, topic: &str) -> String {
    let head =
        if topic.trim().is_empty() { String::new() } else { format!("TOPIC OF THE SESSION: {}\n\n", topic.trim()) };
    format!("{head}SESSION TRANSCRIPT\n{}\n\nWrite the notes.", truncate_chars(transcript, 9000))
}

pub fn parse_digest(reply: &str) -> Result<Digest, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("notes"))?;
    let digest = Digest {
        vital: strings(&value, "vital", 5),
        mistakes: strings(&value, "mistakes", 3),
        next: text(&value, "next"),
        hook: text(&value, "hook"),
    };
    if digest.vital.is_empty() && digest.next.is_empty() {
        return Err(PromptError::Unusable("notes"));
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief() -> Brief {
        let mut brief = Brief { candidate: "## Skills\n- Python\n- FastAPI".to_owned(), ..Brief::default() };
        brief.set_vacancy("AI native full-stack developer", "Acme", "TypeScript, React, Node and LLM APIs.");
        brief
    }

    #[test]
    fn the_map_prompt_asks_for_a_ranking_and_keeps_to_what_is_known() {
        let system = map_system(Lang::Ru);
        for must in ["80/20", "Invent nothing", "likelihood", "effort", "in Russian", "8 to 14 topics"] {
            assert!(system.contains(must), "missing: {must}");
        }
        let prompt = map_prompt(&brief());
        assert!(prompt.contains("VACANCY: AI native full-stack developer at Acme"));
        assert!(prompt.contains("TypeScript, React"));
        assert!(prompt.contains("Python, FastAPI"), "{prompt}");
    }

    #[test]
    fn a_map_is_read_ranked_and_cleaned() {
        let reply = r#"```json
{"role": "AI-native Full-stack Developer", "summary": "Беседа о вебе и LLM.",
 "topics": [
  {"name": "Object-oriented programming", "kind": "concept", "why": "база", "likelihood": 9, "effort": 3},
  {"name": "object-oriented programming", "kind": "concept", "likelihood": 1, "effort": 1},
  {"name": "React", "kind": "Framework", "likelihood": 12, "effort": 0},
  {"name": "", "likelihood": 5},
  {"name": "LLM APIs", "kind": "weird", "likelihood": "high"}
 ]}
```"#;
        let syllabus = parse_map(reply, Lang::Ru).unwrap();
        assert_eq!(syllabus.role, "AI-native Full-stack Developer");
        assert_eq!(syllabus.native, Lang::Ru);
        let names: Vec<&str> = syllabus.topics.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Object-oriented programming", "React", "LLM APIs"],
            "a repeat and an empty name are dropped"
        );
        assert_eq!((syllabus.topics[1].likelihood, syllabus.topics[1].effort), (10, 1), "numbers are kept in range");
        assert_eq!(syllabus.topics[1].kind, TopicKind::Framework);
        assert_eq!((syllabus.topics[2].likelihood, syllabus.topics[2].kind), (5, TopicKind::Concept));
    }

    #[test]
    fn a_map_without_topics_is_refused_and_too_many_are_cut() {
        assert!(parse_map(r#"{"topics": []}"#, Lang::En).is_err());
        assert!(parse_map("I would suggest OOP.", Lang::En).is_err());
        let many: Vec<String> =
            (0..30).map(|i| format!(r#"{{"name": "T{i}", "likelihood": 5, "effort": 5}}"#)).collect();
        let reply = format!(r#"{{"topics": [{}]}}"#, many.join(","));
        assert_eq!(parse_map(&reply, Lang::En).unwrap().topics.len(), MAX_TOPICS);
    }

    #[test]
    fn the_terms_prompt_wants_plain_words_an_analogy_and_a_picture() {
        let system = terms_system(Lang::Ru);
        for must in ["complete beginner", "in Russian", "analogy", "visual", "B1 to B2", "4 to 6 terms"] {
            assert!(system.contains(must), "missing: {must}");
        }
        let topic =
            Topic { name: "REST".into(), kind: TopicKind::Concept, why: "web back end".into(), ..Topic::default() };
        let prompt = terms_prompt(&brief(), &topic);
        assert!(prompt.contains("TOPIC: REST (concept)") && prompt.contains("web back end"));
    }

    #[test]
    fn terms_are_read_and_those_without_an_explanation_are_dropped() {
        let reply = r#"{"terms": [
 {"name": "Idempotency", "plain": "Повтор запроса не меняет результат.", "analogy": "Кнопка лифта.",
  "english": "An operation is idempotent if doing it twice has the same effect as once.",
  "follow_ups": ["Is POST idempotent?", "Why does it matter for retries?", "x", "y"], "visual": "A lift button pressed three times."},
 {"name": "Broken", "plain": ""},
 {"plain": "no name"}
]}"#;
        let terms = parse_terms(reply).unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].follow_ups.len(), 3, "follow-ups are capped");
        assert!(terms[0].english.starts_with("An operation"));
        assert!(parse_terms(r#"{"terms": []}"#).is_err());
        assert!(parse_terms("nothing").is_err());
    }

    #[test]
    fn the_digest_keeps_the_few_lines_that_matter() {
        let system = digest_system(Lang::Ru);
        assert!(system.contains("80/20") && system.contains("at most 5 lines") && system.contains("No flattery"));
        let reply = r#"{"vital": ["a", "b", "c", "d", "e", "f"], "mistakes": ["m"], "next": "Do X.", "hook": "Two terms left."}"#;
        let digest = parse_digest(reply).unwrap();
        assert_eq!(digest.vital.len(), 5);
        assert_eq!((digest.next.as_str(), digest.hook.as_str()), ("Do X.", "Two terms left."));
        assert!(parse_digest(r#"{"vital": [], "next": ""}"#).is_err());
        let prompt = digest_prompt("TUTOR: hi", "OOP");
        assert!(prompt.starts_with("TOPIC OF THE SESSION: OOP") && prompt.contains("TUTOR: hi"));
        assert!(!digest_prompt("x", " ").contains("TOPIC OF THE SESSION"));
    }
}
