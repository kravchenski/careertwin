//! The tutor: a person on a call who teaches, as opposed to the interviewer, who only asks.
//!
//! The tutor explains when you are lost, checks that you got it, switches between your language
//! and the one you are practising, and never makes you feel foolish. It talks like a colleague with
//! time for you. The few things the program has to know (a term was understood, a note is worth
//! keeping, a picture would help) travel as tags at the end of a message; see [`crate::tags`].

use serde::{Deserialize, Serialize};

use super::interviewer::{Clock, InterviewerSpec};
use super::{HONESTY, Lang};
use crate::brief::Brief;
use crate::coding::Task;
use crate::llm::Message;
use crate::syllabus::Term;

/// What the call is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// A mock interview: the interviewer asks, you answer, nobody teaches until the review.
    #[default]
    Interview,
    /// Terms of a topic: first in your language, then in English.
    Terms,
    /// "Tell me about yourself", coached until it is good.
    Intro,
    /// Typing quietly to the tutor, for instance in a lecture. Nothing is spoken.
    Chat,
    /// Writing code while the tutor watches, helps without giving the answer, and talks it through.
    Coding,
}

impl Mode {
    /// Whether the tutor teaches, rather than interviews.
    pub fn teaches(self) -> bool {
        self != Self::Interview
    }

    /// Whether the call is voice-first; a chat is written.
    pub fn spoken(self) -> bool {
        self != Self::Chat
    }
}

/// The terms of a topic the tutor goes through, with what to say about each.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lesson {
    pub topic: String,
    pub terms: Vec<Term>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Explaining it in your own language.
    Native,
    /// Saying it in English, as in the interview.
    English,
}

/// Where the lesson is, for the window to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonView {
    pub index: usize,
    pub total: usize,
    pub name: String,
    pub stage: Stage,
}

/// How a lesson is going: which term, which stage, how long on it.
#[derive(Debug, Clone)]
pub struct LessonState {
    pub index: usize,
    pub stage: Stage,
    /// Your answers since this stage began.
    pub turns: u32,
    pub total: usize,
}

/// After this many answers in one stage the tutor is told to move on.
pub const STAGE_LIMIT: u32 = 4;

impl LessonState {
    pub fn new(total: usize) -> Self {
        Self { index: 0, stage: Stage::Native, turns: 0, total }
    }

    pub fn done(&self) -> bool {
        self.index >= self.total
    }

    pub fn answered(&mut self) {
        self.turns += 1;
    }

    /// The term was explained in your own words: on to English. Returns whether anything changed.
    pub fn native_done(&mut self) -> bool {
        if self.done() || self.stage != Stage::Native {
            return false;
        }
        self.stage = Stage::English;
        self.turns = 0;
        true
    }

    /// The term was said in English: on to the next term.
    pub fn english_done(&mut self) -> bool {
        if self.done() || self.stage != Stage::English {
            return false;
        }
        self.index += 1;
        self.stage = Stage::Native;
        self.turns = 0;
        true
    }

    pub fn view(&self, lesson: &Lesson) -> Option<LessonView> {
        let term = lesson.terms.get(self.index.min(lesson.terms.len().saturating_sub(1)))?;
        Some(LessonView {
            index: self.index.min(self.total),
            total: self.total,
            name: term.name.clone(),
            stage: self.stage,
        })
    }
}

fn reference(lesson: &Lesson) -> String {
    let mut text = String::new();
    for (number, term) in lesson.terms.iter().enumerate() {
        text.push_str(&format!("{}. {}\n", number + 1, term.name));
        if !term.plain.is_empty() {
            text.push_str(&format!("   Plain: {}\n", term.plain));
        }
        if !term.analogy.is_empty() {
            text.push_str(&format!("   Comparison: {}\n", term.analogy));
        }
        if !term.english.is_empty() {
            text.push_str(&format!("   A good answer in English: {}\n", term.english));
        }
        if !term.follow_ups.is_empty() {
            text.push_str(&format!("   Likely follow-ups: {}\n", term.follow_ups.join(" / ")));
        }
    }
    text
}

/// How the tutor sounds, the same in every mode.
fn manner(spec: &InterviewerSpec) -> String {
    let name = spec.persona.first_name();
    format!(
        "WHO YOU ARE\n\
         You are {name}, {role}. You like explaining things and you are good at it, because you remember how hard \
         it was when you did not know them. You are patient, warm and a little funny when it fits, never a \
         lecturer.\n\n\
         HOW YOU SOUND\n\
         - Like a real colleague who has time for this person: plain spoken words, contractions, short sentences. \
         Not like a textbook and not like an assistant.\n\
         - One to three short sentences a turn. One question at a time, never two.\n\
         - React to what they actually said before you go on, in your own words, and vary how you react. Do not \
         praise every answer; never say \"great question\" or \"great answer\"; never say you are an AI.\n\
         - When they are wrong, say so kindly and plainly, give the right idea in one or two sentences, then check \
         that it landed with a small question. When they are right, say why it is right in a few words and go on.\n\
         - When they say they do not know, that is fine and useful. Say so, then work it out together: ask a small \
         question that leads one step, or give a comparison from everyday life, and let them say the answer \
         themselves. Do not just hand it over.\n\
         - Never give a long explanation. If there is more, say the first part and ask if they want the rest.\n\
         - If they ask you to show, draw, animate or visualise something, or a picture would clearly help a \
         beginner understand, say one short line that you will and end the message with [[show: the topic in a few \
         words]]. Never describe the picture yourself: it is drawn and told by the program.\n\
         - {honesty}\n",
        role = spec.persona.role(),
        honesty = HONESTY,
    )
}

/// What the tags are, and the rule for using them.
fn tag_rules(extra: &str) -> String {
    format!(
        "TAGS\n\
         A tag is written at the very end of a message, in double square brackets, after the last word. The call \
         system removes it before anything is said, so never mention it and never put words after it.\n\
         [[show: topic]] as described above, when a picture is wanted or would clearly help.\n{extra}"
    )
}

fn terms_rules(spec: &InterviewerSpec, lesson: &Lesson) -> String {
    let native = spec.native.name();
    let target = spec.target_name();
    format!(
        "THE LESSON\n\
         Today's topic is \"{topic}\". You go through its terms one by one, in this order. This is your reference: \
         say it in your own words, never read it out.\n{reference}\n\
         HOW EACH TERM GOES\n\
         Stage one, in {native}: ask what the term means, in their own words, in one short question. Listen to the \
         whole answer.\n\
         - If it is right, say so in a few words, finish the stage with the tag [[native: S]] and, in the same \
         message, start stage two by asking them to say it in {target}, the way they would if the interviewer asked.\n\
         - If it is partly right, say what is right, put the rest in one or two plain sentences using the \
         comparison, and ask them to say it again in their own words.\n\
         - If they do not know, do not give the answer yet: ask a small leading question or give the comparison, \
         let them build the idea, then say it in two sentences and ask them to say it back.\n\
         - After at most three exchanges finish the stage in the same way, whatever the result.\n\
         Stage two, in {target}:\n\
         - Accept imperfect {target}. Repeat the correct form naturally inside your reply instead of pointing at \
         the error, and mention at most one slip a turn, the one that would hurt most in an interview.\n\
         - When the answer is solid, ask the follow-up the interviewer would ask next, in {target}, and wait for \
         the reply.\n\
         - After their reply to the follow-up (or after at most three exchanges) finish the stage with the tag \
         [[english: S]] and, in the same message, start the next term with its stage-one question, in {native}.\n\
         A tag belongs only in the message in which you move on, together with the first words of what comes next. \
         Never put a tag in a message that still asks them something about the current stage.\n\
         S is a whole number from 0 to 3, and you must be honest, because it decides what comes back: 0 they could \
         not do it, 1 they needed a lot of help, 2 good with a small slip, 3 solid on their own.\n\
         When the last term is finished, wrap up in two sentences and add [[end]].\n\n{tags}",
        topic = lesson.topic,
        reference = reference(lesson),
        tags = tag_rules(
            "[[native: S]] and [[english: S]] as above.\n\
             [[note: one thing to remember]] when something comes up that is worth keeping (a rule, a phrase, a slip \
             to avoid), under 25 words, in the language you were speaking. Not in every message.\n"
        ),
    )
}

fn intro_rules(spec: &InterviewerSpec) -> String {
    let target = spec.target_name();
    format!(
        "THE LESSON\n\
         The introduction: \"tell me about yourself\" opens nearly every interview and sets the mood for the rest. \
         You coach them to an answer of sixty to ninety seconds in {target}.\n\
         1. As the interviewer would, ask them to introduce themselves. Let them finish.\n\
         2. Coach in at most three sentences: one thing that worked, then the one change that matters most (it is \
         usually one of: too long, a list of jobs instead of a story, nothing about this role, no reason for \
         wanting it). Say what to add from the facts in their context. Never invent a fact for them.\n\
         3. Offer the shape once: who you are and what you do now; two things from before that matter for this \
         role; why this role. Then ask them to say it again with the change.\n\
         4. After two or three tries, or when it is good, say the best version yourself in a few short sentences \
         using only facts from their context, then add [[note: the outline in under 25 words]] and [[end]].\n\n{tags}",
        tags = tag_rules("[[note: ...]] and [[end]] as above.\n"),
    )
}

fn coding_rules(spec: &InterviewerSpec) -> String {
    let task = spec.task.as_ref().map(Task::for_tutor).unwrap_or_default();
    format!(
        "THE LESSON\n\
         A live-coding interview, practised. The person writes code in an editor and you can see it: the latest \
         version comes with their messages, marked CODE. You watch like an interviewer who wants to see them think, \
         and you speak like a person.\n\n{task}\n\
         HOW YOU BEHAVE\n\
         - Mostly you listen. They may think aloud: react in a few words and let them go on. Never read code out \
         loud and never write code for them.\n\
         - At the start, in one or two sentences, tell them the task in your own words, ask if it is clear, and ask \
         what approach they have in mind. Do not suggest one.\n\
         - If the code looks right so far, say nothing about it, or ask about their reasoning. If it looks wrong, do \
         NOT point at the line or say what is wrong: say something like \"check that everything works the way you \
         expect\", or ask them to follow a small input through the code by hand. Only if they stay stuck, point at \
         the area, never at the fix.\n\
         - Never say \"you forgot to\", \"you should\" or \"the bug is\".\n\
         - If they say they do not know, or ask for help, that is fine: solve it together. Ask a small question that \
         leads one step, let them type that step, and if they are lost give one nudge from your private list. Repeat. \
         Never hand over the whole solution.\n\
         - The program also watches the editor and may tell you in a STATE note that a block came in all at once or \
         that the window was left for a while. Calmly ask them to explain that part in their own words, line by line. \
         Never accuse.\n\
         - When they say they are done: ask them to try a tricky input by hand, then the time complexity, then one \
         improvement. Then wrap up in two sentences and add [[end]].\n\
         - Keep every turn to one or two short sentences.\n\n{tags}",
        tags = tag_rules("[[note: ...]] for one thing worth remembering from this task, and [[end]] as above.\n"),
    )
}

fn chat_rules() -> String {
    format!(
        "THE SETTING\n\
         The person is in a lecture or at a desk and writes to you quietly. Nothing is read aloud.\n\
         - Answer in the language they write in. Two or three short sentences, the answer first. If a term needs a \
         comparison, give one in a line.\n\
         - Do not ask questions back unless you truly cannot answer without one. Do not greet again, do not \
         summarise, do not offer a menu of options.\n\
         - A short piece of code or a formula is fine when it is the answer.\n\
         - If they say \"save\", \"note\", \"запиши\" or the like, or you give a definition worth remembering, end \
         with [[note: the one thing to remember, under 25 words]]. Not on every message.\n\n{tags}",
        tags = tag_rules("[[note: ...]] as above.\n"),
    )
}

/// Which language the tutor uses, and when it switches.
fn language_rules(spec: &InterviewerSpec) -> String {
    let (native, target) = (spec.native.name(), spec.target_name());
    if spec.mode == Mode::Coding {
        return format!(
            "\nLANGUAGE\nSpeak {target} by default, because the real interview is in {target}. If they speak or write \
             in {native}, answer in {native}, briefly, and let them choose. Keep terms and code in English.\n"
        );
    }
    let informal = if spec.native == Lang::Ru {
        "In Russian, speak to them informally (на «ты»), as a colleague would.\n"
    } else {
        ""
    };
    format!(
        "\nLANGUAGE\nThe person thinks in {native} and is practising {target}. Follow the stage you are in; if they \
         answer in the other language, accept it and gently steer back. Keep terms and code in English.\n{informal}"
    )
}

/// The instructions of a tutoring call.
pub fn system(spec: &InterviewerSpec, brief: &Brief) -> String {
    let mut text = manner(spec);
    text.push('\n');
    let channel = if spec.mode.spoken() {
        "This is a real-time spoken call, so everything you write is read aloud: no lists, no headings, no markdown, \
         no emojis, no stage directions, no code (say what it does in words).\n"
    } else {
        "This is a chat. Plain text only: no markdown, no emojis.\n"
    };
    text.push_str(channel);
    text.push('\n');
    text.push_str(&match (spec.mode, &spec.lesson) {
        (Mode::Terms, Some(lesson)) => terms_rules(spec, lesson),
        (Mode::Intro, _) => intro_rules(spec),
        (Mode::Chat, _) => chat_rules(),
        (Mode::Coding, _) => coding_rules(spec),
        _ => String::new(),
    });
    text.push_str(&language_rules(spec));
    if !spec.focus.trim().is_empty() {
        text.push_str(&format!("\nFOCUS: concentrate on {}.\n", spec.focus.trim()));
    }
    if spec.mode.spoken() {
        text.push_str(&format!(
            "\nThe call is planned for {} minutes. Messages in square brackets that start with STATE or LESSON are \
             from the call system, not from the person: use them to pace yourself and never read them out.\n",
            spec.minutes
        ));
    }
    text.push_str(&format!("\n{}", brief.render()));
    text
}

/// The canned first line of a chat, so nobody waits for the model to say hello.
pub fn greeting(native: Lang) -> &'static str {
    match native {
        Lang::En => "I'm here. Type anything: a question, a term, a line from the lecture.",
        Lang::Ru => "Я здесь. Пишите что угодно: вопрос, термин, строчку с лекции.",
    }
}

/// The first request of a tutoring call.
pub fn opening(spec: &InterviewerSpec, brief: &Brief) -> Vec<Message> {
    let who = brief.first_name().map_or_else(|| "The person".to_owned(), |n| format!("The person, {n},"));
    let task = match (spec.mode, &spec.lesson) {
        (Mode::Terms, Some(lesson)) => {
            let first = lesson.terms.first().map_or("the first term", |t| t.name.as_str());
            format!(
                "Greet them by first name in one short sentence, say in one more what you will do today ({} terms \
                 of \"{}\", about {} minutes) and start stage one of the first term, \"{first}\".",
                lesson.terms.len(),
                lesson.topic,
                spec.minutes
            )
        }
        (Mode::Intro, _) => "Greet them by first name in one short sentence and ask, as the interviewer would, \
                             for the introduction: \"tell me about yourself\"."
            .to_owned(),
        (Mode::Coding, _) => "Greet them by first name in one short sentence, tell them the task in your own \
                              words in one or two more, and ask what approach they have in mind. Do not hint."
            .to_owned(),
        _ => "Greet them by first name in one short sentence and ask what they would like to work on.".to_owned(),
    };
    vec![
        Message::system(system(spec, brief)),
        Message::user(format!("[STATE: {who} has just joined the call. {task}]")),
    ]
}

/// What to tell the tutor about the clock, the lesson and anything the call wants it to know.
pub fn state_note(
    spec: &InterviewerSpec,
    elapsed: u32,
    answered: usize,
    clock: Clock,
    lesson: Option<&LessonState>,
) -> String {
    let mut note = format!(
        "[STATE: {} of {} minutes used; the person has answered {} time{}.",
        elapsed / 60,
        spec.minutes,
        answered,
        if answered == 1 { "" } else { "s" }
    );
    match clock {
        Clock::Normal => {}
        Clock::AskForQuestions => note.push_str(
            " Time is nearly up: finish the term you are on, give the one thing to remember and say what comes next time.",
        ),
        Clock::Final => note.push_str(
            " Time is up. This is your last message: one sentence on what went well, one on what to do next, then say goodbye and add [[end]].",
        ),
    }
    note.push(']');
    if let (Some(state), Some(lesson)) = (lesson, &spec.lesson) {
        if state.done() {
            note.push_str("\n[LESSON: every term is finished. Wrap up in two sentences and add [[end]].]");
        } else if let Some(term) = lesson.terms.get(state.index) {
            let stage = match state.stage {
                Stage::Native => format!("stage one, in {}", spec.native.name()),
                Stage::English => format!("stage two, in {}", spec.target_name()),
            };
            note.push_str(&format!(
                "\n[LESSON: term {} of {}, \"{}\", {stage}; they have answered {} time{} in this stage.",
                state.index + 1,
                state.total,
                term.name,
                state.turns,
                if state.turns == 1 { "" } else { "s" }
            ));
            if state.turns >= STAGE_LIMIT {
                note.push_str(" That is enough here: finish this stage now with its tag and go on.");
            }
            note.push(']');
        }
    }
    note
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::interviewer::{InterviewerSpec, Persona};
    use crate::session::Track;

    fn term(name: &str) -> Term {
        Term {
            name: name.into(),
            plain: format!("{name} in plain words."),
            analogy: "Like a shop.".into(),
            english: format!("{name} means something."),
            follow_ups: vec!["Why?".into(), "When?".into()],
            visual: String::new(),
        }
    }

    fn spec(mode: Mode) -> InterviewerSpec {
        let mut spec = InterviewerSpec::new(Track::Technical);
        spec.mode = mode;
        spec.persona = Persona::Mentor;
        spec.native = Lang::Ru;
        spec.lesson = (mode == Mode::Terms)
            .then(|| Lesson { topic: "OOP".into(), terms: vec![term("Encapsulation"), term("Polymorphism")] });
        spec
    }

    fn brief() -> Brief {
        let mut brief =
            Brief { candidate: "| Full name | Alex Demo |\n## Skills\n- Python".into(), ..Brief::default() };
        brief.set_vacancy("Backend Engineer", "Acme", "Python.");
        brief
    }

    #[test]
    fn the_tutor_sounds_like_a_person_who_teaches() {
        let text = system(&spec(Mode::Terms), &brief());
        for must in [
            "never say you are an AI",
            "One question at a time",
            "work it out together",
            "never say \"great question\"",
            "Never give a long explanation",
            "Never invent any of them",
        ] {
            assert!(text.to_lowercase().contains(&must.to_lowercase()), "missing: {must}");
        }
        assert!(text.contains("You are Maya"), "the mentor has her own name");
    }

    #[test]
    fn the_terms_lesson_carries_every_term_and_both_stages() {
        let text = system(&spec(Mode::Terms), &brief());
        for must in [
            "Today's topic is \"OOP\"",
            "1. Encapsulation",
            "2. Polymorphism",
            "Plain: Encapsulation in plain words.",
            "Comparison: Like a shop.",
            "Stage one, in Russian",
            "Stage two, in English",
            "[[native: S]]",
            "[[english: S]]",
            "[[end]]",
            "The person thinks in Russian and is practising English",
            "read aloud",
        ] {
            assert!(text.contains(must), "missing: {must}");
        }
        assert!(text.contains("VACANCY: Backend Engineer at Acme"));
    }

    #[test]
    fn another_language_can_be_practised_instead_of_english() {
        let mut german = spec(Mode::Terms);
        german.target = "German".into();
        let text = system(&german, &brief());
        for must in [
            "Stage two, in German",
            "say it in German",
            "Accept imperfect German",
            "The person thinks in Russian and is practising German",
        ] {
            assert!(text.contains(must), "missing: {must}");
        }
        assert!(!text.contains("practising English") && !text.contains("Stage two, in English"));
        let mut state = LessonState::new(2);
        state.native_done();
        assert!(state_note(&german, 60, 1, Clock::Normal, Some(&state)).contains("stage two, in German"));
        // and nothing changes when none is chosen
        assert!(system(&spec(Mode::Terms), &brief()).contains("Stage two, in English"));
        assert_eq!(german.target_name(), "German");
        assert_eq!(spec(Mode::Terms).target_name(), "English");
    }

    #[test]
    fn each_mode_has_its_own_lesson() {
        let intro = system(&spec(Mode::Intro), &brief());
        assert!(intro.contains("tell me about yourself") && intro.contains("sixty to ninety seconds"));
        assert!(intro.contains("Never invent a fact for them"));
        assert!(!intro.contains("HOW EACH TERM GOES"));
        let chat = system(&spec(Mode::Chat), &brief());
        assert!(chat.contains("Nothing is read aloud") && chat.contains("запиши"));
        assert!(chat.contains("This is a chat") && !chat.contains("read aloud, so"));
        assert!(!chat.contains("planned for"), "a chat has no clock");
    }

    #[test]
    fn a_coding_call_speaks_the_practised_language_and_follows_you_into_yours() {
        let mut spec = spec(Mode::Coding);
        spec.task = Some(crate::coding::Task {
            title: "Two sum".into(),
            statement: "Find two numbers.".into(),
            language: "python".into(),
            outline: "A dictionary.".into(),
            hints: vec!["What do you need to remember?".into()],
            ..crate::coding::Task::default()
        });
        let text = system(&spec, &brief());
        for must in [
            "A live-coding interview, practised",
            "TASK: Two sum",
            "PRIVATE NOTES FOR YOU",
            "Solution idea: A dictionary.",
            "check that everything works the way you expect",
            "Never say \"you forgot to\"",
            "solve it together",
            "Never accuse",
            "Speak English by default",
            "answer in Russian, briefly",
        ] {
            assert!(text.contains(must), "missing: {must}");
        }
        assert!(!text.contains("HOW EACH TERM GOES"));
        assert!(opening(&spec, &brief())[1].content.contains("what approach they have in mind"));
        assert!(Mode::Coding.teaches() && Mode::Coding.spoken());
    }

    #[test]
    fn the_opening_names_the_first_term_and_greets_by_name() {
        let messages = opening(&spec(Mode::Terms), &brief());
        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("The person, Alex,"));
        assert!(messages[1].content.contains("2 terms of \"OOP\""));
        assert!(messages[1].content.contains("\"Encapsulation\""));
        let intro = opening(&spec(Mode::Intro), &Brief::default());
        assert!(intro[1].content.contains("tell me about yourself"));
    }

    #[test]
    fn a_lesson_moves_from_your_words_to_english_and_on_to_the_next_term() {
        let mut state = LessonState::new(2);
        assert!(!state.english_done(), "English cannot come before your own words");
        assert!(state.native_done());
        assert!(!state.native_done(), "a tag repeated by the model changes nothing");
        assert_eq!((state.index, state.stage), (0, Stage::English));
        assert!(state.english_done());
        assert_eq!((state.index, state.stage, state.turns), (1, Stage::Native, 0));
        state.native_done();
        state.english_done();
        assert!(state.done());
        assert!(!state.native_done() && !state.english_done());
    }

    #[test]
    fn the_view_says_which_term_and_stage() {
        let lesson = spec(Mode::Terms).lesson.unwrap();
        let mut state = LessonState::new(2);
        let view = state.view(&lesson).unwrap();
        assert_eq!((view.index, view.total, view.name.as_str(), view.stage), (0, 2, "Encapsulation", Stage::Native));
        state.native_done();
        state.english_done();
        assert_eq!(state.view(&lesson).unwrap().name, "Polymorphism");
        state.native_done();
        state.english_done();
        let done = state.view(&lesson).unwrap();
        assert_eq!((done.index, done.total), (2, 2), "all done");
    }

    #[test]
    fn the_state_note_says_where_the_lesson_is_and_when_to_move_on() {
        let spec = spec(Mode::Terms);
        let mut state = LessonState::new(2);
        let note = state_note(&spec, 120, 1, Clock::Normal, Some(&state));
        assert!(note.starts_with("[STATE: 2 of 20 minutes used; the person has answered 1 time."), "{note}");
        assert!(
            note.contains("[LESSON: term 1 of 2, \"Encapsulation\", stage one, in Russian; they have answered 0 times")
        );
        state.native_done();
        state.turns = STAGE_LIMIT;
        let note = state_note(&spec, 300, 4, Clock::Normal, Some(&state));
        assert!(note.contains("stage two, in English") && note.contains("That is enough here"), "{note}");
        state.english_done();
        state.native_done();
        state.english_done();
        assert!(state_note(&spec, 400, 6, Clock::Normal, Some(&state)).contains("every term is finished"));
    }

    #[test]
    fn the_clock_changes_what_the_tutor_does() {
        let spec = spec(Mode::Intro);
        assert!(state_note(&spec, 1100, 5, Clock::AskForQuestions, None).contains("finish the term you are on"));
        let last = state_note(&spec, 1300, 9, Clock::Final, None);
        assert!(last.contains("say goodbye") && last.contains("[[end]]"));
        assert!(!state_note(&spec, 10, 0, Clock::Normal, None).contains("LESSON"));
    }

    #[test]
    fn the_greeting_is_in_your_language_and_modes_know_what_they_are() {
        assert!(greeting(Lang::Ru).starts_with("Я здесь"));
        assert!(greeting(Lang::En).starts_with("I'm here"));
        assert!(Mode::Terms.teaches() && Mode::Chat.teaches() && !Mode::Interview.teaches());
        assert!(Mode::Terms.spoken() && !Mode::Chat.spoken());
        assert_eq!(Mode::default(), Mode::Interview);
    }

    #[test]
    fn the_prompt_never_contains_a_key_shaped_string() {
        for mode in [Mode::Terms, Mode::Intro, Mode::Chat] {
            assert!(!crate::secrets::contains_secret(&system(&spec(mode), &brief()), &[]));
        }
    }
}
