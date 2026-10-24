//! Concept checks: a question, a few answers, how sure you are. Being sure and wrong is the
//! case that teaches the most, so the confidence you give decides how soon the card comes back.

use serde::{Deserialize, Serialize};

use super::json::{extract_object, strings, text};
use super::{Lang, PromptError};
use crate::brief::Brief;

pub const MIN_OPTIONS: usize = 3;
pub const MAX_OPTIONS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptCard {
    pub question: String,
    pub options: Vec<String>,
    pub answer_index: usize,
    pub explanation: String,
    #[serde(default)]
    pub topic: String,
    /// 1 easy, 2 interview level, 3 hard.
    pub difficulty: u8,
    #[serde(default)]
    pub answered: Option<Answered>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answered {
    /// `None` is "I don't know".
    pub choice: Option<usize>,
    /// 0 to 100.
    pub confidence: u8,
    pub thinking: String,
    pub correct: bool,
    pub at: u64,
}

impl ConceptCard {
    /// Records your answer and says whether it was right.
    pub fn answer(&mut self, choice: Option<usize>, confidence: u8, thinking: &str, now: u64) -> bool {
        let correct = choice == Some(self.answer_index);
        self.answered = Some(Answered {
            choice,
            confidence: confidence.min(100),
            thinking: thinking.trim().to_owned(),
            correct,
            at: now,
        });
        correct
    }
}

pub fn system(language: Lang) -> String {
    format!(
        "You write concept checks for a software engineer preparing for technical interviews. Reply with JSON only: \
         {{\"cards\": [{{\"question\": str, \"options\": [str], \"answer_index\": int, \"explanation\": str, \
         \"topic\": str, \"difficulty\": 1-3}}]}}.\n\n\
         Rules\n\
         - Write in {lang}. Keep technology names, identifiers and commands as they are normally written.\n\
         - Ask about reasoning, trade-offs, behaviour and prediction (\"what happens when...\", \"why would you...\", \
         \"which is the cause\"), the way a good interviewer does. No trivia, no \"which year\", no definitions \
         that can be guessed from the words.\n\
         - Three or four options. Exactly one is correct. The wrong ones are plausible: each is a misconception a \
         real engineer might hold. Never use \"all of the above\" or \"none of the above\". Options are similar in \
         length, so the right one cannot be guessed from its size.\n\
         - answer_index is the zero-based position of the correct option.\n\
         - explanation: two to four sentences: why the right option is right and why the tempting wrong one is not. \
         Be exact: real names, numbers, defaults. If you are not sure of a detail, do not use it.\n\
         - difficulty: 1 for fundamentals, 2 for a typical interview, 3 for a hard follow-up.\n\
         - Never state anything about the candidate's own experience; these are general knowledge questions.",
        lang = language.name()
    )
}

pub fn prompt(brief: &Brief, topic: &str, count: usize, difficulty: u8) -> String {
    let level = match difficulty {
        1 => "fundamentals",
        3 => "hard follow-up questions",
        _ => "a typical interview",
    };
    let stack = brief.skill_terms().into_iter().take(10).collect::<Vec<_>>().join(", ");
    let vacancy = if brief.has_vacancy() {
        format!(
            "\nVACANCY: {} {}\n{}",
            brief.job_title,
            brief.company,
            crate::brief::truncate_chars(&brief.job_description, 2500)
        )
    } else {
        String::new()
    };
    format!(
        "TOPIC: {topic}\nWrite {count} concept checks at the level of {level}.\n\
         The candidate's stack (to choose relevant examples, not to claim experience): {}.{vacancy}",
        if stack.is_empty() { "not stated" } else { &stack }
    )
}

pub fn parse(reply: &str, count: usize) -> Result<Vec<ConceptCard>, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("questions"))?;
    let cards: Vec<ConceptCard> = value
        .get("cards")
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let question = text(item, "question");
            let mut options = strings(item, "options", MAX_OPTIONS);
            let given = item
                .get("answer_index")
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))?
                as usize;
            // the right option is named by its text first: removing a duplicate shifts the positions
            let right = options.get(given)?.to_lowercase();
            let mut unique = std::collections::HashSet::new();
            options.retain(|option| unique.insert(option.to_lowercase()));
            let answer_index = options.iter().position(|option| option.to_lowercase() == right)?;
            if question.is_empty() || options.len() < MIN_OPTIONS {
                return None;
            }
            let explanation = text(item, "explanation");
            if explanation.is_empty() {
                return None;
            }
            let difficulty = item.get("difficulty").and_then(|d| d.as_u64()).unwrap_or(2).clamp(1, 3) as u8;
            let mut card = ConceptCard {
                question,
                options,
                answer_index,
                explanation,
                topic: text(item, "topic"),
                difficulty,
                answered: None,
            };
            let state = seed(&card.question);
            shuffle(&mut card, state);
            Some(card)
        })
        .take(count.max(1))
        .collect();
    if cards.is_empty() { Err(PromptError::Unusable("usable questions")) } else { Ok(cards) }
}

fn seed(question: &str) -> u64 {
    let random = uuid::Uuid::new_v4().as_u128() as u64;
    question
        .bytes()
        .fold(random ^ 0x9E37_79B9_7F4A_7C15, |acc, b| acc.rotate_left(5) ^ u64::from(b))
        .wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Models like to put the right answer first or in the middle; this puts it anywhere.
pub fn shuffle(card: &mut ConceptCard, mut state: u64) {
    if state == 0 {
        state = 0x1234_5678_9ABC_DEF1;
    }
    let right = card.options[card.answer_index].clone();
    for i in (1..card.options.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        card.options.swap(i, j);
    }
    card.answer_index = card.options.iter().position(|o| *o == right).unwrap_or(0);
}

/// Topics worth drilling: the candidate's own skills first, then the classics.
pub fn topics(brief: &Brief) -> Vec<String> {
    let mut topics: Vec<String> = brief.skill_terms().into_iter().take(6).collect();
    for classic in [
        "SQL and indexes",
        "Async and concurrency",
        "REST and API design",
        "Caching",
        "System design basics",
        "Testing",
        "Docker and CI",
        "Git",
    ] {
        if !topics.iter().any(|t| t.eq_ignore_ascii_case(classic)) {
            topics.push(classic.to_owned());
        }
    }
    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> String {
        r#"{"cards": [
          {"question": "Why does an index on orders(customer_id) help?", "options": ["It avoids a full table scan", "It compresses rows", "It caches results", "It locks the table"], "answer_index": 0, "explanation": "A btree lets the planner seek.", "topic": "SQL", "difficulty": 2},
          {"question": "Only two options", "options": ["a", "b"], "answer_index": 0, "explanation": "x"},
          {"question": "Answer out of range", "options": ["a", "b", "c"], "answer_index": 7, "explanation": "x"},
          {"question": "", "options": ["a", "b", "c"], "answer_index": 0, "explanation": "x"},
          {"question": "No explanation", "options": ["a", "b", "c"], "answer_index": 0},
          {"question": "Duplicates collapse", "options": ["same", "SAME", "other", "third"], "answer_index": "2", "explanation": "x", "difficulty": 9}
        ]}"#
        .to_owned()
    }

    #[test]
    fn only_well_formed_cards_survive() {
        let cards = parse(&reply(), 10).unwrap();
        let questions: Vec<&str> = cards.iter().map(|c| c.question.as_str()).collect();
        assert_eq!(questions, vec!["Why does an index on orders(customer_id) help?", "Duplicates collapse"]);
    }

    #[test]
    fn shuffling_keeps_the_right_answer_pointing_at_the_right_text() {
        for _ in 0..50 {
            let cards = parse(&reply(), 10).unwrap();
            assert_eq!(cards[0].options[cards[0].answer_index], "It avoids a full table scan");
            assert_eq!(cards[1].options[cards[1].answer_index], "other");
            assert_eq!(cards[0].options.len(), 4);
        }
    }

    #[test]
    fn the_right_answer_does_not_always_land_in_the_same_place() {
        let mut positions = std::collections::HashSet::new();
        for _ in 0..80 {
            positions.insert(parse(&reply(), 1).unwrap()[0].answer_index);
        }
        assert!(positions.len() >= 3, "{positions:?}");
    }

    #[test]
    fn duplicates_are_collapsed_and_difficulty_is_clamped() {
        let cards = parse(&reply(), 10).unwrap();
        assert_eq!(cards[1].options.len(), 3);
        assert_eq!(cards[1].difficulty, 3);
    }

    #[test]
    fn the_count_limits_the_result() {
        assert_eq!(parse(&reply(), 1).unwrap().len(), 1);
    }

    #[test]
    fn nothing_usable_is_an_error() {
        assert!(parse("no json", 3).is_err());
        assert!(parse(r#"{"cards": []}"#, 3).is_err());
        assert!(
            parse(r#"{"cards": [{"question": "q", "options": ["a"], "answer_index": 0, "explanation": "e"}]}"#, 3)
                .is_err()
        );
    }

    #[test]
    fn answering_records_confidence_and_correctness() {
        let mut card = parse(&reply(), 1).unwrap().remove(0);
        let right = card.answer_index;
        assert!(card.answer(Some(right), 250, "  because  ", 5));
        let answered = card.answered.clone().unwrap();
        assert!(answered.correct && answered.confidence == 100 && answered.thinking == "because");
        let wrong = (right + 1) % card.options.len();
        assert!(!card.answer(Some(wrong), 90, "", 6));
        assert!(!card.answer(None, 0, "", 7));
        assert_eq!(card.answered.unwrap().choice, None);
    }

    #[test]
    fn the_prompt_names_the_topic_level_and_stack_without_claiming_experience() {
        let brief =
            Brief { candidate: include_str!("../../tests/fixtures/ABOUT.demo.md").to_owned(), ..Brief::default() };
        let text = prompt(&brief, "PostgreSQL indexes", 5, 3);
        assert!(
            text.contains("TOPIC: PostgreSQL indexes")
                && text.contains("5 concept checks")
                && text.contains("hard follow-up")
        );
        assert!(text.contains("FastAPI") && text.contains("not to claim experience"));
        assert!(system(Lang::En).contains("Never state anything about the candidate's own experience"));
    }

    #[test]
    fn topics_start_with_the_candidates_skills_and_add_the_classics_once() {
        let brief =
            Brief { candidate: include_str!("../../tests/fixtures/ABOUT.demo.md").to_owned(), ..Brief::default() };
        let topics = topics(&brief);
        assert!(topics.iter().any(|t| t == "Python"));
        assert!(topics.iter().any(|t| t == "SQL and indexes"));
        let unique: std::collections::HashSet<String> = topics.iter().map(|t| t.to_lowercase()).collect();
        assert_eq!(unique.len(), topics.len());
        assert!(super::topics(&Brief::default()).len() >= 8);
    }
}
