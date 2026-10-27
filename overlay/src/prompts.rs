//! Prompt text and output parsing for the answer coach.

use std::collections::HashSet;
use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::brief::InterviewBrief;
pub use crate::prompt_text::{SCREENSHOT_SYSTEM, SYSTEM};
use crate::prompt_text::{KIND_HINTS, LAYOUT_HINTS, LISTENING_HINTS, STYLE_HINTS};
use crate::questions::{Kind, detect_language};
use crate::{languages, questions};

pub const SKIP: &str = "SKIP";

fn lookup(table: &'static [(&'static str, &'static str)], key: &str, fallback: &str) -> &'static str {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .or_else(|| table.iter().find(|(name, _)| *name == fallback))
        .map_or("", |(_, text)| text)
}

pub fn kind_hint(kind: Kind) -> &'static str {
    lookup(KIND_HINTS, kind.as_str(), "technical")
}

pub fn layout_hint(layout: &str) -> &'static str {
    lookup(LAYOUT_HINTS, layout, "lines")
}

pub fn style_hint(style: &str) -> &'static str {
    lookup(STYLE_HINTS, style, "standard")
}

/// `language` is the interview language; without one it is guessed from the question. `code` is what the
/// search of the candidate's project found for this question. `listening` is set when the words came from the
/// call audio and not from the person at the window: `heard` for a finished utterance, `hearing` for one
/// still being said.
#[allow(clippy::too_many_arguments)]
pub fn user_prompt(
    brief: &InterviewBrief,
    question: &str,
    kind: Kind,
    history: &[String],
    language: Option<&str>,
    style: &str,
    layout: &str,
    code: &str,
    listening: Option<&str>,
) -> String {
    let name = languages::name(Some(language.filter(|code| !code.is_empty()).unwrap_or_else(|| detect_language(question))));
    let recent = if history.is_empty() {
        "(start of the interview)".to_owned()
    } else {
        history[history.len().saturating_sub(6)..].join("\n")
    };
    let excerpts = if code.is_empty() { String::new() } else { format!("RELEVANT CODE FROM THE PROJECT (excerpts)\n{code}\n\n") };
    let heard = listening.and_then(|key| LISTENING_HINTS.iter().find(|(name, _)| *name == key)).map_or(String::new(), |(_, text)| format!("{text}\n"));
    format!(
        "{}\n\n{excerpts}RECENT CONVERSATION\n{recent}\n\nINTERVIEWER'S QUESTION: {question}\nTYPE: {}. {}\n\
INTERVIEW LANGUAGE: {name}. Write your reply in {name}.\n{}\n{}\n{heard}",
        brief.render(Some(kind)),
        kind.as_str(),
        kind_hint(kind),
        style_hint(style),
        layout_hint(layout),
    )
    .trim_end()
    .to_owned()
}

/// What goes next to the picture. The CV stays out: a task on the screen needs the stack only.
pub fn screenshot_prompt(brief: &InterviewBrief, language: Option<&str>, layout: &str) -> String {
    let name = languages::name(language);
    let stack = brief.stack_hint(8);
    let stack = if stack.is_empty() { "(not stated)".to_owned() } else { stack };
    format!(
        "CANDIDATE STACK: {stack}\nINTERVIEW LANGUAGE: {name}. Write the lines and prose in {name}.\n{}\nSolve what the screenshot shows.",
        layout_hint(layout)
    )
}

/// Whether a question is worth a request at all; see [`questions::worth_an_answer`].
pub fn worth_asking(text: &str, language: Option<&str>) -> bool {
    questions::worth_an_answer(text, language)
}

/// The parts of a reply, tolerant of one that is still streaming.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sections {
    pub points: Vec<String>,
    pub answer: String,
    pub key: Vec<String>,
    pub context: String,
    /// NEXT: what the interviewer will probably ask next.
    pub follow_ups: Vec<String>,
}

impl Sections {
    pub fn has_details(&self) -> bool {
        !self.answer.is_empty() || !self.key.is_empty() || !self.context.is_empty() || !self.follow_ups.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggestion {
    pub sections: Sections,
    pub raw: String,
}

impl Suggestion {
    /// Plain text for the API and agents: the **bold** markers are for the window.
    pub fn plain_points(&self) -> Vec<String> {
        self.sections.points.iter().map(|line| line.replace("**", "")).collect()
    }
}

static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:[-•]|\*(?!\*)|\d+[.)])\s*").expect("a fixed pattern"));
static MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?mi)^[ \t]*(?:\*\*)?(POINTS|ANSWER|KEY|CONTEXT|(?-i:NEXT))(?:\*\*)?[ \t]*:(?:\*\*)?").expect("a fixed pattern")
});

fn strip_bullet(line: &str) -> String {
    BULLET.replace(line, "").trim().to_owned()
}

/// The lines of a list; a fenced code block stays in one piece, with its indentation.
fn bullets(block: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut code: Option<Vec<String>> = None;
    for line in block.lines() {
        if let Some(open) = code.as_mut() {
            open.push(line.to_owned());
            if line.trim().starts_with("```") && open.len() > 1 {
                items.push(open.join("\n"));
                code = None;
            }
        } else if line.trim().starts_with("```") {
            code = Some(vec![line.trim().to_owned()]);
        } else if !line.trim().is_empty() {
            items.push(strip_bullet(line));
        }
    }
    if let Some(open) = code {
        items.push(open.join("\n")); // a block that is still streaming
    }
    items
}

static PARTIAL_MARKERS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    ["POINTS", "ANSWER", "KEY", "CONTEXT", "NEXT"]
        .iter()
        .flat_map(|word| (1..=word.len()).map(|size| word[..size].to_lowercase()))
        .collect()
});

/// A heading that is still streaming ("ANSW") is not a line of the reply: leave it out for now.
fn without_partial_marker(text: &str) -> &str {
    let Some((head, last)) = text.rsplit_once('\n') else { return text };
    let bare = last.trim().trim_matches('*').to_lowercase();
    if PARTIAL_MARKERS.contains(&bare) && !BULLET.is_match(last).unwrap_or(false) { head } else { text }
}

/// Split a reply into POINTS, ANSWER, KEY, CONTEXT and NEXT; text without markers is the answer.
pub fn sections(text: &str) -> Sections {
    let text = without_partial_marker(text);
    let found: Vec<_> = MARKER.captures_iter(text).flatten().collect();
    if found.is_empty() {
        return Sections { answer: text.trim().to_owned(), ..Sections::default() };
    }
    let mut blocks: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (index, capture) in found.iter().enumerate() {
        let whole = capture.get(0).expect("group 0 always exists");
        let end = found.get(index + 1).map_or(text.len(), |next| next.get(0).expect("group 0 always exists").start());
        blocks.insert(capture[1].to_uppercase(), text[whole.end()..end].trim().to_owned());
    }
    let block = |name: &str| blocks.get(name).cloned().unwrap_or_default();
    let (mut points_block, mut answer) = (block("POINTS"), block("ANSWER"));
    if let Some(fence) = points_block.find("```").filter(|_| answer.is_empty()) {
        answer = points_block[fence..].trim().to_owned();
        points_block.truncate(fence);
    }
    Sections {
        points: bullets(&points_block),
        answer,
        key: bullets(&block("KEY")),
        context: block("CONTEXT"),
        follow_ups: bullets(&block("NEXT")),
    }
}

pub fn parse_suggestion(text: &str) -> Suggestion {
    let raw = text.trim().to_owned();
    Suggestion { sections: sections(&raw), raw }
}

/// Points and answer so far from a half-streamed reply.
pub fn split_partial(text: &str) -> (Vec<String>, String) {
    let parts = sections(text);
    (parts.points, parts.answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief() -> InterviewBrief {
        InterviewBrief::new("## Contact\n| Full name | Alex |\n")
    }

    #[test]
    fn the_prompt_names_the_kind_the_language_and_the_question() {
        let text = user_prompt(&brief(), "Чому ви хочете працювати в нас?", Kind::Motivation, &[], None, "standard", "lines", "", None);
        assert!(text.contains("INTERVIEWER'S QUESTION: Чому ви хочете працювати в нас?"));
        assert!(text.contains("TYPE: motivation. Motivation: say how real interests connect"));
        assert!(text.contains("INTERVIEW LANGUAGE: Ukrainian. Write your reply in Ukrainian.") && !text.contains("Russian"));
        assert!(text.contains("RECENT CONVERSATION\n(start of the interview)"));
        assert!(text.contains("ANSWER STYLE: standard") && text.contains("ANSWER LAYOUT: lines to say"));
        let russian = user_prompt(&brief(), "Почему вы хотите у нас работать?", Kind::Motivation, &[], None, "standard", "lines", "", None);
        assert!(russian.contains("INTERVIEW LANGUAGE: Russian"));
    }

    #[test]
    fn a_fixed_language_beats_the_guess() {
        let text = user_prompt(&brief(), "Tell me about yourself", Kind::Intro, &[], Some("de"), "concise", "main", "", None);
        assert!(text.contains("INTERVIEW LANGUAGE: German") && text.contains("ANSWER STYLE: concise") && text.contains("main answer only"));
    }

    #[test]
    fn only_the_last_six_turns_and_the_project_code_go_in() {
        let history: Vec<String> = (1..=9).map(|n| format!("Interviewer: turn {n}")).collect();
        let text = user_prompt(&brief(), "How?", Kind::Technical, &history, Some("en"), "standard", "lines", "--- a.py ---\nprint(1)", Some("heard"));
        assert!(!text.contains("turn 3") && text.contains("turn 4") && text.contains("turn 9"));
        assert!(text.contains("RELEVANT CODE FROM THE PROJECT (excerpts)\n--- a.py ---\nprint(1)"));
        assert!(text.contains("WHAT WAS HEARD: it came from the call audio"));
        assert!(text.contains("Technical question. Pick the shape that fits"));
    }

    #[test]
    fn a_screenshot_prompt_carries_the_stack_not_the_cv() {
        let mut brief = InterviewBrief::new("## Skills\n| Strong | Python, Docker |\n");
        brief.documents.push(crate::brief::Document::new("cv.pdf", "CV text", "cv"));
        let text = screenshot_prompt(&brief, Some("pl"), "main");
        assert!(text.contains("CANDIDATE STACK: Python, Docker") && text.contains("Polish") && !text.contains("CV text"));
        assert!(screenshot_prompt(&InterviewBrief::new(""), None, "lines").contains("(not stated)"));
    }

    #[test]
    fn a_reply_is_split_into_its_sections() {
        let reply = "POINTS:\n- I build **APIs** with Python\n- I test with **pytest**\nANSWER:\nI build APIs.\nKEY:\n- **REST**: stateless\nCONTEXT:\nUsed at work.\nNEXT:\n- What about SQL? - I tune queries.";
        let parts = sections(reply);
        assert_eq!(parts.points, ["I build **APIs** with Python", "I test with **pytest**"]);
        assert_eq!(parts.answer, "I build APIs.");
        assert_eq!(parts.key, ["**REST**: stateless"]);
        assert_eq!(parts.context, "Used at work.");
        assert_eq!(parts.follow_ups, ["What about SQL? - I tune queries."]);
        assert!(parts.has_details());
    }

    #[test]
    fn text_without_markers_is_the_answer_and_markers_may_be_bold_or_lowercase() {
        assert_eq!(sections("Just a sentence.").answer, "Just a sentence.");
        let parts = sections("**POINTS:**\n- one\n**Answer:** two sentences here.");
        assert_eq!(parts.points, ["one"]);
        assert_eq!(parts.answer, "two sentences here.");
    }

    #[test]
    fn a_heading_that_is_still_streaming_is_not_a_line() {
        assert_eq!(sections("POINTS:\n- first line\nANSW").points, ["first line"]);
        assert_eq!(sections("POINTS:\n- first line\nANSW").answer, "");
        assert_eq!(split_partial("POINTS:\n- a\nANSWER:\nhello").1, "hello");
    }

    #[test]
    fn code_stays_in_one_piece_and_code_under_points_moves_to_the_answer() {
        let reply = "POINTS:\n- two pointers\n```python\ndef f(x):\n    return x\n```";
        let parts = sections(reply);
        assert_eq!(parts.points, ["- two pointers".trim_start_matches("- ")]);
        assert!(parts.answer.starts_with("```python") && parts.answer.contains("    return x"));
        let streaming = sections("ANSWER:\n```python\ndef f():");
        assert_eq!(streaming.answer, "```python\ndef f():");
        assert_eq!(bullets("- a\n```sql\nselect 1\n```\n- b"), ["a", "```sql\nselect 1\n```", "b"]);
    }

    #[test]
    fn numbered_lines_and_a_lone_next_word_in_prose_are_handled() {
        assert_eq!(bullets("1. one\n2) two\n• three"), ["one", "two", "three"]);
        // NEXT is a heading only in capitals; "next:" in prose is not one
        assert_eq!(sections("ANSWER:\nthe next: step is easy").follow_ups, Vec::<String>::new());
    }

    #[test]
    fn a_suggestion_keeps_the_raw_text_and_plain_points() {
        let suggestion = parse_suggestion("  POINTS:\n- I use **Docker**\nANSWER:\nYes.  ");
        assert_eq!(suggestion.plain_points(), ["I use Docker"]);
        assert!(suggestion.raw.starts_with("POINTS:") && suggestion.raw.ends_with("Yes."));
        assert_eq!(parse_suggestion(SKIP).sections.answer, SKIP);
    }
}
