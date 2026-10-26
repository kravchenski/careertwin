//! Live coding: writing a task for the vacancy, and the quiet observer that decides, every time the
//! code settles, whether the tutor should say anything.
//!
//! The observer's first rule is to stay silent. A real interviewer lets a person think, and a hint
//! that was not needed teaches nothing. When it does speak it asks, and does not tell: it never
//! names the bug, the line or the fix, and it never writes code.

use serde_json::Value;

use super::json::{extract_object, strings, text};
use super::{Lang, PromptError};
use crate::brief::{Brief, truncate_chars};
use crate::coding::{Example, Observation, Status, Task, comment, language_id, language_name};

const MAX_EXAMPLES: usize = 3;
const MAX_HINTS: usize = 4;
const MAX_CODE_CHARS: usize = 6000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// A five-to-ten-minute exercise: a loop, a map, a string.
    WarmUp,
    /// A real interview task: fifteen to twenty-five minutes.
    Interview,
}

impl Level {
    fn describe(self) -> &'static str {
        match self {
            Self::WarmUp => {
                "a warm-up that a junior can finish in 5 to 10 minutes: one loop, one data structure, a few lines"
            }
            Self::Interview => {
                "a task of the size set in a real interview, 15 to 25 minutes: a small function or class with an \
                 edge case or two and a choice of data structure"
            }
        }
    }
}

pub fn task_system(native: Lang, language: &str, level: Level) -> String {
    format!(
        "You set a live-coding task for a job interview, in {code}. It is {size}.\n\
         - It must be solvable in {code} alone, with no libraries beyond the standard one, and checkable by reading \
         the code.\n\
         - Choose something an interviewer for this vacancy would really ask: it should exercise what the vacancy \
         needs. Do not copy a famous puzzle word for word; vary it.\n\
         - statement: what to do, in {lang}, clear and short, with the names of the function and its inputs. Names \
         of code things stay in English.\n\
         - starter: the code the editor starts with: the signature and a one-line comment in English, never any part \
         of the solution.\n\
         - examples: two or three, each {{\"input\": str, \"output\": str}}, small and correct.\n\
         - outline: for the tutor only: the idea of a good solution in two sentences.\n\
         - pitfalls: for the tutor only: two to four mistakes or edge cases to look for.\n\
         - hints: for the tutor only: three nudges from vague to specific, each a question or a pointer, never code.\n\
         - complexity: for the tutor only: the time and space of a good solution.\n\
         Reply with JSON only: {{\"title\": str, \"statement\": str, \"starter\": str, \"examples\": [{{\"input\": \
         str, \"output\": str}}], \"outline\": str, \"pitfalls\": [str], \"hints\": [str], \"complexity\": str}}",
        code = language_name(language),
        size = level.describe(),
        lang = native.name(),
    )
}

pub fn task_prompt(brief: &Brief, topic: &str, avoid: &[String]) -> String {
    let mut text = brief.render();
    if !topic.trim().is_empty() {
        text.push_str(&format!("\n\nTOPIC TO EXERCISE: {}", topic.trim()));
    }
    if !avoid.is_empty() {
        text.push_str(&format!("\n\nDo not repeat these tasks: {}.", avoid.join("; ")));
    }
    text.push_str("\n\nSet the task.");
    text
}

pub fn parse_task(reply: &str, language: &str) -> Result<Task, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("a task"))?;
    let title = text(&value, "title");
    let statement = text(&value, "statement");
    if title.is_empty() || statement.is_empty() {
        return Err(PromptError::Unusable("a task"));
    }
    let language = language_id(language).unwrap_or("python");
    let examples: Vec<Example> = value
        .get("examples")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let (input, output) = (text(item, "input"), text(item, "output"));
            (!input.is_empty() && !output.is_empty()).then_some(Example { input, output })
        })
        .take(MAX_EXAMPLES)
        .collect();
    let mut starter = value.get("starter").and_then(Value::as_str).unwrap_or_default().trim_end().to_owned();
    if starter.trim().is_empty() {
        starter = format!("{} write your solution here\n", comment(language));
    }
    starter.push('\n');
    Ok(Task {
        title,
        statement,
        language: language.to_owned(),
        starter,
        examples,
        outline: text(&value, "outline"),
        pitfalls: strings(&value, "pitfalls", 5),
        hints: strings(&value, "hints", MAX_HINTS),
        complexity: text(&value, "complexity"),
    })
}

pub fn observe_system(native: Lang) -> String {
    format!(
        "You are the tutor of a person doing a live-coding task, and you watch their code as it is written. Each \
         time the code settles you decide whether to say something. You are not a compiler and not a teacher who \
         fixes things; you are an interviewer who wants to see the person think.\n\n\
         RULES\n\
         - Your default is to say nothing (\"say\": \"\"). People need quiet to think. A hint that was not needed \
         teaches nothing.\n\
         - on_track: the code is going somewhere sensible, even if unfinished. Say nothing.\n\
         - wrong_way: there is a mistake, or the approach will not work. NEVER name the mistake, the line or the \
         fix, and never say \"you forgot\" or \"you should\". Ask them to check it themselves. Level 1: a question \
         that makes them trace the code with a small input (\"Take [3, 1, 2] and follow it through; what is the \
         value after the second pass?\"), or simply \"check that everything works the way you expect\". Level 2, \
         only if you already spoke about it and nothing changed: point at the area without the answer (\"have another \
         look at how the loop ends\"). Level 3 only when they said they do not know or asked for help: lead them \
         through it in small steps, a question at a time, and let them type every change.\n\
         - stuck: nothing useful has happened for a while. Ask what they are thinking, or give a level-1 nudge on the \
         approach as a question (\"what would let you find a number you have seen before in one step?\"). Use the \
         private nudges of the task in order, from vague to specific, and not one you have already used.\n\
         - done: the code looks complete and right. Do not praise it; ask them to try it on a tricky input, or \
         what the time complexity is.\n\
         - explain: a block appeared all at once, OR the editor says the window was out of focus for 30 seconds or \
         more and the code now holds a good deal that was not there before. This comes before every other status, even \
         when the code is right. Say it calmly and without accusing: ask them to walk you through that part, or their \
         approach, in their own words. Nothing more.\n\
         - Never write code, never read code aloud, never reveal the private notes of the task.\n\
         - What you say is spoken: one or two short plain sentences in {lang}.\n\n\
         Reply with JSON only: {{\"status\": \"on_track\" | \"stuck\" | \"wrong_way\" | \"done\" | \"explain\", \
         \"say\": str, \"level\": 0 | 1 | 2 | 3}}",
        lang = native.name()
    )
}

pub fn observe_prompt(task: &Task, code: &str, said_before: &[String], signals: &str, last_words: &[String]) -> String {
    let before = if said_before.is_empty() {
        "(nothing yet)".to_owned()
    } else {
        said_before.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n")
    };
    let talk = if last_words.is_empty() { "(nothing)".to_owned() } else { last_words.join("\n") };
    format!(
        "{task}\nTHE CODE NOW ({language})\n```\n{code}\n```\n\nWHAT YOU ALREADY SAID TO THEM ABOUT THIS\n{before}\n\n\
         WHAT WAS LAST SAID ALOUD\n{talk}\n\nWHAT THE EDITOR SAW\n{signals}\n\nDecide.",
        task = task.for_tutor(),
        language = language_name(&task.language),
        code = truncate_chars(code, MAX_CODE_CHARS),
    )
}

pub fn parse_observation(reply: &str) -> Result<Observation, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("an observation"))?;
    let status = Status::parse(&text(&value, "status"));
    let level = value.get("level").and_then(Value::as_u64).map_or(0, |n| n.min(3) as u8);
    let mut say = text(&value, "say");
    // an on-track code is left alone, whatever the model felt like adding
    if status == Status::OnTrack && level == 0 {
        say.clear();
    }
    // spoken: one or two sentences
    let sentences: Vec<&str> = say.split_inclusive(['.', '?', '!']).collect();
    if sentences.len() > 3 {
        say = sentences[..2].concat().trim().to_owned();
    }
    Ok(Observation { status, say, level })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Task {
        Task {
            title: "Two sum".into(),
            statement: "Найди два числа с нужной суммой.".into(),
            language: "python".into(),
            examples: vec![Example { input: "[2,7], 9".into(), output: "[0,1]".into() }],
            outline: "A dictionary of seen numbers.".into(),
            pitfalls: vec!["the same element twice".into()],
            hints: vec!["What do you need to remember?".into()],
            complexity: "O(n)".into(),
            ..Task::default()
        }
    }

    #[test]
    fn the_task_prompt_asks_for_a_hidden_solution_and_a_clean_starter() {
        let system = task_system(Lang::Ru, "python", Level::Interview);
        for must in [
            "in Python",
            "in Russian",
            "never any part of the solution",
            "for the tutor only",
            "15 to 25 minutes",
            "Do not copy a famous puzzle",
        ] {
            assert!(system.contains(must), "missing: {must}");
        }
        assert!(task_system(Lang::En, "java", Level::WarmUp).contains("5 to 10 minutes"));
        let mut brief = Brief { candidate: "## Skills\n- Python".into(), ..Brief::default() };
        brief.set_vacancy("Backend", "Acme", "FastAPI.");
        let prompt = task_prompt(&brief, "dictionaries", &["Two sum".into()]);
        assert!(
            prompt.contains("TOPIC TO EXERCISE: dictionaries") && prompt.contains("Do not repeat these tasks: Two sum")
        );
        assert!(prompt.contains("VACANCY: Backend at Acme") && prompt.ends_with("Set the task."));
    }

    #[test]
    fn a_task_is_read_with_its_examples_and_private_notes() {
        let reply = r#"```json
{"title": "Two sum", "statement": "Найди индексы.", "starter": "def two_sum(nums, target):\n    # return the indexes",
 "examples": [{"input": "[2,7,11], 9", "output": "[0,1]"}, {"input": "", "output": "x"}, {"input": "[3,3], 6", "output": "[0,1]"}],
 "outline": "Dictionary.", "pitfalls": ["same element twice", "no answer"], "hints": ["h1", "h2", "h3", "h4", "h5"], "complexity": "O(n)"}
```"#;
        let task = parse_task(reply, "Python").unwrap();
        assert_eq!((task.title.as_str(), task.language.as_str()), ("Two sum", "python"));
        assert_eq!(task.examples.len(), 2, "an example without an input is dropped");
        assert!(task.starter.starts_with("def two_sum") && task.starter.ends_with('\n'));
        assert_eq!((task.pitfalls.len(), task.hints.len()), (2, 4));
        assert_eq!(task.complexity, "O(n)");
    }

    #[test]
    fn a_task_without_a_starter_gets_a_comment_in_its_language() {
        let reply = r#"{"title": "T", "statement": "S"}"#;
        assert!(parse_task(reply, "sql").unwrap().starter.starts_with("-- write your solution here"));
        assert!(parse_task(reply, "rust").unwrap().starter.starts_with("// write your solution here"));
        assert!(parse_task(reply, "klingon").unwrap().starter.starts_with("# write"), "an unknown language is Python");
        assert!(parse_task(r#"{"title": "T"}"#, "python").is_err());
        assert!(parse_task("no json", "python").is_err());
    }

    #[test]
    fn the_observer_is_told_to_stay_quiet_and_never_to_give_the_answer() {
        let system = observe_system(Lang::Ru);
        for must in [
            "Your default is to say nothing",
            "NEVER name the mistake, the line or the fix",
            "never say \"you forgot\"",
            "check that everything works the way you expect",
            "Level 3 only when they said they do not know",
            "let them type every change",
            "without accusing",
            "never reveal the private notes",
            "in Russian",
        ] {
            assert!(system.contains(must), "missing: {must}");
        }
    }

    #[test]
    fn the_observer_sees_the_task_the_code_what_it_said_and_what_the_editor_saw() {
        let prompt = observe_prompt(
            &task(),
            "def f(): pass",
            &["Check the loop.".into()],
            "A block appeared all at once.",
            &["TUTOR: hello".into()],
        );
        for must in [
            "TASK: Two sum",
            "PRIVATE NOTES FOR YOU",
            "THE CODE NOW (Python)",
            "def f(): pass",
            "- Check the loop.",
            "A block appeared all at once.",
            "TUTOR: hello",
        ] {
            assert!(prompt.contains(must), "missing: {must}");
        }
        assert!(observe_prompt(&task(), "x", &[], "", &[]).contains("(nothing yet)"));
    }

    #[test]
    fn an_observation_is_read_and_a_quiet_one_stays_quiet() {
        let loud = parse_observation(r#"{"status": "wrong_way", "say": "Take [3, 1, 2] and follow it through. What is x after the second pass?", "level": 1}"#).unwrap();
        assert_eq!((loud.status, loud.level), (Status::WrongWay, 1));
        assert!(loud.say.starts_with("Take [3, 1, 2]"));
        let quiet = parse_observation(r#"{"status": "on_track", "say": "Nice, keep going!", "level": 0}"#).unwrap();
        assert_eq!(quiet.say, "", "an on-track code is left alone");
        assert!(parse_observation("I think it is fine.").is_err());
        assert_eq!(parse_observation(r#"{"status": "done", "say": "x", "level": 9}"#).unwrap().level, 3);
    }

    #[test]
    fn a_long_speech_is_cut_to_two_sentences() {
        let reply = r#"{"status": "stuck", "say": "One. Two. Three. Four. Five.", "level": 1}"#;
        assert_eq!(parse_observation(reply).unwrap().say, "One. Two.");
    }
}
