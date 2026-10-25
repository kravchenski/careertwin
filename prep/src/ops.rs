//! The things you ask of the model outside a live call: the review after one, concept checks,
//! translations, and the analysis of a vacancy. Each asks once more when the answer cannot be read.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use thiserror::Error;

use crate::audio::vad::{self, Segmenter};
use crate::brief::Brief;
use crate::coding::{Observation, Task};
use crate::explain::Storyboard;
use crate::llm::{ChatOptions, Llm, LlmError, Message};
use crate::prompts::cards::{self, ConceptCard};
use crate::prompts::coding as coding_prompts;
use crate::prompts::explain as explaining;
use crate::prompts::feedback::{self, Feedback};
use crate::prompts::interviewer::InterviewerSpec;
use crate::prompts::prepare::{self, Analysis, BankQuestion};
use crate::prompts::research::{self, Digest};
use crate::prompts::translate::{self, Translation};
use crate::prompts::{Lang, PromptError};
use crate::services::Services;
use crate::session::Session;
use crate::speech::{SttHint, Transcript};
use crate::syllabus::{Syllabus, Term, Topic, vacancy_key};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OpError {
    #[error("{0}")]
    Llm(String),
    #[error("{0}")]
    Prompt(String),
    #[error("{0}")]
    Speech(String),
}

impl From<LlmError> for OpError {
    fn from(error: LlmError) -> Self {
        Self::Llm(error.to_string())
    }
}

impl From<PromptError> for OpError {
    fn from(error: PromptError) -> Self {
        Self::Prompt(error.to_string())
    }
}

/// One request; an empty reply is asked for again with more room, because a model that reasons
/// first can spend a small allowance on thoughts and say nothing.
async fn ask(llm: &dyn Llm, model: &str, system: &str, user: &str, tokens: u32) -> Result<String, LlmError> {
    let messages = vec![Message::system(system), Message::user(user)];
    match llm.complete(messages.clone(), ChatOptions::new(model).temperature(0.3).max_tokens(tokens)).await {
        Err(LlmError::Empty) => {
            llm.complete(messages, ChatOptions::new(model).temperature(0.3).max_tokens(tokens * 3)).await
        }
        other => other,
    }
}

/// Asks for JSON and reads it; when it cannot be read, says so to the model and asks once more.
async fn ask_json<T>(
    llm: &dyn Llm,
    model: &str,
    system: &str,
    user: &str,
    tokens: u32,
    parse: impl Fn(&str) -> Result<T, PromptError>,
) -> Result<T, OpError> {
    let reply = ask(llm, model, system, user, tokens).await?;
    match parse(&reply) {
        Ok(value) => Ok(value),
        Err(_) => {
            let again = format!(
                "{user}\n\nYour last reply could not be read. Reply with the JSON object only, in exactly the requested shape."
            );
            let reply = ask(llm, model, system, &again, tokens).await?;
            Ok(parse(&reply)?)
        }
    }
}

/// The review of a finished call.
pub async fn feedback(
    llm: &dyn Llm,
    model: &str,
    session: &Session,
    brief: &Brief,
    ui: Lang,
) -> Result<Feedback, OpError> {
    if session.candidate_turns() == 0 {
        return Err(OpError::Prompt("There was nothing said to review.".into()));
    }
    let spec: Option<InterviewerSpec> = serde_json::from_value(session.setup.clone()).ok();
    let language = spec.as_ref().map_or(Lang::En, |s| s.language);
    let system = feedback::system(session.track, language, ui);
    let user = feedback::prompt(session, brief);
    ask_json(llm, model, &system, &user, 3500, |reply| feedback::parse(reply, session, session.track, language)).await
}

pub async fn concept_cards(
    llm: &dyn Llm,
    model: &str,
    brief: &Brief,
    topic: &str,
    count: usize,
    difficulty: u8,
    language: Lang,
) -> Result<Vec<ConceptCard>, OpError> {
    let system = cards::system(language);
    let user = cards::prompt(brief, topic, count, difficulty);
    let mut made = ask_json(llm, model, &system, &user, 3000, |reply| cards::parse(reply, count)).await?;
    for card in &mut made {
        if card.topic.is_empty() {
            card.topic = topic.to_owned();
        }
    }
    Ok(made)
}

pub async fn translate(llm: &dyn Llm, model: &str, text: &str, ui: Lang) -> Result<Translation, OpError> {
    if text.trim().is_empty() {
        return Err(OpError::Prompt("Write or say something first.".into()));
    }
    ask_json(llm, model, &translate::system(ui), &translate::prompt(text), 900, translate::parse).await
}

pub async fn analyze(llm: &dyn Llm, model: &str, brief: &Brief, language: Lang) -> Result<Analysis, OpError> {
    if brief.is_empty() {
        return Err(OpError::Prompt("There is no ABOUT.md or CV to compare with the vacancy.".into()));
    }
    ask_json(
        llm,
        model,
        &prepare::analysis_system(language),
        &prepare::analysis_prompt(brief),
        2500,
        prepare::parse_analysis,
    )
    .await
}

pub async fn question_bank(
    llm: &dyn Llm,
    model: &str,
    brief: &Brief,
    count: usize,
    focus: &str,
    language: Lang,
) -> Result<Vec<BankQuestion>, OpError> {
    ask_json(
        llm,
        model,
        &prepare::questions_system(language),
        &prepare::questions_prompt(brief, count, focus),
        3000,
        |reply| prepare::parse_questions(reply, count),
    )
    .await
}

/// How far the research of a vacancy has got, for the window to show.
#[derive(Debug, Default)]
pub struct ResearchProgress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
}

/// How many topics have their terms written at the same time.
const RESEARCH_PARALLEL: usize = 3;
/// The topics whose terms are written during the research: the vital few, up to this many. The
/// others get theirs when you ask for them.
const RESEARCH_TOPICS: usize = 8;

/// Researches a vacancy: ranks the topics by the 80/20 rule, then writes the terms of the vital
/// few, three at a time. A topic whose terms could not be written is kept without them and can be
/// asked for again; only when none could be written is the research a failure.
pub async fn research(
    llm: &dyn Llm,
    model: &str,
    brief: &Brief,
    native: Lang,
    progress: &ResearchProgress,
) -> Result<Syllabus, OpError> {
    if brief.job_description.trim().is_empty() {
        return Err(OpError::Prompt("Paste the vacancy first: the research starts from its text.".into()));
    }
    progress.done.store(0, Ordering::Relaxed);
    progress.total.store(1, Ordering::Relaxed);
    let mut syllabus =
        ask_json(llm, model, &research::map_system(native), &research::map_prompt(brief), 2500, |reply| {
            research::parse_map(reply, native)
        })
        .await?;
    if syllabus.role.is_empty() {
        syllabus.role = brief.job_title.clone();
    }
    syllabus.company = brief.company.clone();
    syllabus.key = vacancy_key(&brief.job_title, &brief.company, &brief.job_description);
    syllabus.made = crate::session::now();
    let wanted: Vec<usize> = syllabus.vital().into_iter().take(RESEARCH_TOPICS).collect();
    progress.total.store(wanted.len() + 1, Ordering::Relaxed);
    progress.done.store(1, Ordering::Relaxed);
    let written: Vec<(usize, Result<Vec<Term>, OpError>)> = futures_util::stream::iter(wanted.iter().copied())
        .map(|index| {
            let topic = syllabus.topics[index].clone();
            async move { (index, topic_terms(llm, model, brief, &topic, native).await) }
        })
        .buffer_unordered(RESEARCH_PARALLEL)
        .inspect(|_| {
            progress.done.fetch_add(1, Ordering::Relaxed);
        })
        .collect()
        .await;
    let mut first_error = None;
    let mut any = false;
    for (index, result) in written {
        match result {
            Ok(terms) => {
                syllabus.topics[index].terms = terms;
                any = true;
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match (any, first_error) {
        (false, Some(error)) => Err(error),
        _ => Ok(syllabus),
    }
}

/// The terms of one topic, in plain words in your language with a model answer in English.
pub async fn topic_terms(
    llm: &dyn Llm,
    model: &str,
    brief: &Brief,
    topic: &Topic,
    native: Lang,
) -> Result<Vec<Term>, OpError> {
    ask_json(
        llm,
        model,
        &research::terms_system(native),
        &research::terms_prompt(brief, topic),
        3500,
        research::parse_terms,
    )
    .await
}

/// The few lines worth keeping from a session.
pub async fn digest(
    llm: &dyn Llm,
    model: &str,
    session: &Session,
    topic: &str,
    native: Lang,
) -> Result<Digest, OpError> {
    if session.candidate_turns() == 0 {
        return Err(OpError::Prompt("There was nothing said to take notes from.".into()));
    }
    ask_json(
        llm,
        model,
        &research::digest_system(native),
        &research::digest_prompt(&session.transcript(), topic),
        1200,
        research::parse_digest,
    )
    .await
}

/// A short animated explanation of `topic` for a complete beginner. `context` says what the person
/// already knows or just asked; `simpler` is for when the first one was not understood.
pub async fn storyboard(
    llm: &dyn Llm,
    model: &str,
    topic: &str,
    context: &str,
    native: Lang,
    simpler: bool,
) -> Result<Storyboard, OpError> {
    if topic.trim().is_empty() {
        return Err(OpError::Prompt("Say what to explain.".into()));
    }
    ask_json(llm, model, &explaining::system(native, simpler), &explaining::prompt(topic, context), 4500, |reply| {
        explaining::parse(reply, native)
    })
    .await
}

/// What kind of task to set.
pub struct TaskRequest<'a> {
    pub language: &'a str,
    pub level: coding_prompts::Level,
    pub topic: &'a str,
    /// Titles of tasks set lately, so this one is different.
    pub avoid: &'a [String],
}

/// A live-coding task for the vacancy.
pub async fn coding_task(
    llm: &dyn Llm,
    model: &str,
    brief: &Brief,
    native: Lang,
    request: TaskRequest<'_>,
) -> Result<Task, OpError> {
    ask_json(
        llm,
        model,
        &coding_prompts::task_system(native, request.language, request.level),
        &coding_prompts::task_prompt(brief, request.topic, request.avoid),
        2500,
        |reply| coding_prompts::parse_task(reply, request.language),
    )
    .await
}

/// What the tutor looks at: the code, what it said before, and what the editor saw.
pub struct Look<'a> {
    pub code: &'a str,
    pub said: &'a [String],
    pub signals: &'a str,
    pub last_words: &'a [String],
}

/// The tutor looks at the code: should it say anything, and what?
pub async fn observe(
    llm: &dyn Llm,
    model: &str,
    task: &Task,
    look: Look<'_>,
    native: Lang,
) -> Result<Observation, OpError> {
    ask_json(
        llm,
        model,
        &coding_prompts::observe_system(native),
        &coding_prompts::observe_prompt(task, look.code, look.said, look.signals, look.last_words),
        500,
        coding_prompts::parse_observation,
    )
    .await
}

/// Listens for one thing you say and writes it down: for "say it yourself" drills and dictation.
pub async fn hear(services: &Services, device: &str, hint: SttHint, wait: Duration) -> Result<Transcript, OpError> {
    let stt = services
        .stt
        .clone()
        .ok_or_else(|| OpError::Speech("Speech recognition is not set up. Open Settings.".into()))?;
    let (mut frames, _microphone) = (services.mic)(device).map_err(|e| OpError::Speech(e.to_string()))?;
    let mut segmenter =
        Segmenter::new(vad::Options { end_silence_ms: 900, max_utterance_ms: 30_000, ..vad::Options::default() });
    let started = tokio::time::Instant::now();
    let pcm = loop {
        let remaining = wait.saturating_sub(started.elapsed());
        match tokio::time::timeout(remaining.max(Duration::from_millis(1)), frames.recv()).await {
            Ok(Some(frame)) => {
                if let vad::Event::Finished(pcm) = segmenter.push(&frame) {
                    break pcm;
                }
                // give up waiting for a voice that never comes, but never cut one off
                if !segmenter.active() && started.elapsed() >= wait {
                    return Err(OpError::Speech("Nothing was heard.".into()));
                }
            }
            Ok(None) => return Err(OpError::Speech("The microphone stopped.".into())),
            Err(_) => match segmenter.flush() {
                Some(pcm) => break pcm,
                None => return Err(OpError::Speech("Nothing was heard.".into())),
            },
        }
    };
    let transcript = stt.transcribe(pcm, hint).await.map_err(|e| OpError::Speech(e.to_string()))?;
    if transcript.text.trim().is_empty() {
        return Err(OpError::Speech("Nothing was heard.".into()));
    }
    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmError;
    use crate::session::{Kind, Speaker, Track};
    use crate::testing::{FakeLlm, FakeSpeaker, FakeStt, Reply, Talker};
    use std::sync::Arc;

    fn brief() -> Brief {
        Brief { candidate: include_str!("../tests/fixtures/ABOUT.demo.md").to_owned(), ..Brief::default() }
    }

    fn session() -> Session {
        let mut s = Session::new(Track::Technical, Kind::Live, "t");
        s.add_turn(Speaker::Interviewer, "What do you work on?", None);
        s.add_turn(Speaker::You, "I build the order API.", None);
        s
    }

    fn json(text: &str) -> Reply {
        Reply { pieces: vec![text.to_owned()], ..Reply::default() }
    }

    #[tokio::test]
    async fn the_review_is_asked_for_and_read() {
        let llm =
            FakeLlm::new(vec![json(r#"{"summary": "Good start.", "answers": [{"turn": 1, "good": ["clear"]}]}"#)]);
        let result = feedback(llm.as_ref(), "m", &session(), &brief(), Lang::Ru).await.unwrap();
        assert_eq!(result.summary, "Good start.");
        assert_eq!(result.answers[0].you_said, "I build the order API.");
        let request = llm.last_request();
        assert!(request[0].content.contains("Write summary, notes, issues and priorities in Russian"));
        assert!(request[1].content.contains("[1] CANDIDATE: I build the order API."));
    }

    #[tokio::test]
    async fn an_unreadable_review_is_asked_for_once_more() {
        let llm = FakeLlm::new(vec![json("Sure! Here is my review in prose."), json(r#"{"summary": "Fine."}"#)]);
        let result = feedback(llm.as_ref(), "m", &session(), &brief(), Lang::En).await.unwrap();
        assert_eq!(result.summary, "Fine.");
        assert_eq!(llm.calls(), 2);
        assert!(llm.last_request()[1].content.contains("JSON object only"));
    }

    #[tokio::test]
    async fn two_unreadable_reviews_are_an_error() {
        let llm = FakeLlm::new(vec![json("no"), json("still no")]);
        let error = feedback(llm.as_ref(), "m", &session(), &brief(), Lang::En).await.unwrap_err();
        assert!(matches!(error, OpError::Prompt(_)), "{error}");
    }

    #[tokio::test]
    async fn an_empty_call_has_nothing_to_review_and_costs_no_request() {
        let llm = FakeLlm::new(vec![]);
        let empty = Session::new(Track::English, Kind::Live, "t");
        assert!(feedback(llm.as_ref(), "m", &empty, &brief(), Lang::En).await.is_err());
        assert_eq!(llm.calls(), 0);
    }

    #[tokio::test]
    async fn an_empty_reply_is_asked_again_with_more_room() {
        let llm = FakeLlm::new(vec![Reply::failing(LlmError::Empty), json(r#"{"summary": "ok"}"#)]);
        assert_eq!(feedback(llm.as_ref(), "m", &session(), &brief(), Lang::En).await.unwrap().summary, "ok");
        assert_eq!(llm.calls(), 2);
    }

    #[tokio::test]
    async fn a_model_error_comes_through_as_text() {
        let llm = FakeLlm::new(vec![Reply::failing(LlmError::Unreachable("No AI gateway answers".into()))]);
        let error = translate(llm.as_ref(), "m", "привет", Lang::Ru).await.unwrap_err();
        assert!(error.to_string().contains("No AI gateway"));
    }

    #[tokio::test]
    async fn cards_get_their_topic() {
        let llm = FakeLlm::new(vec![json(
            r#"{"cards": [{"question": "Why an index?", "options": ["a seek", "a copy", "a lock"], "answer_index": 0, "explanation": "A btree seeks."}]}"#,
        )]);
        let made = concept_cards(llm.as_ref(), "m", &brief(), "SQL", 3, 2, Lang::En).await.unwrap();
        assert_eq!(made[0].topic, "SQL");
        assert!(llm.last_request()[1].content.contains("TOPIC: SQL"));
    }

    #[tokio::test]
    async fn translation_rejects_nothing_and_reads_the_rest() {
        let llm = FakeLlm::new(vec![json(r#"{"detected": "ru", "natural": "I added an index."}"#)]);
        assert!(translate(llm.as_ref(), "m", "   ", Lang::Ru).await.is_err());
        assert_eq!(llm.calls(), 0);
        assert_eq!(
            translate(llm.as_ref(), "m", "Я добавил индекс", Lang::Ru).await.unwrap().natural,
            "I added an index."
        );
    }

    #[tokio::test]
    async fn analysis_and_the_question_bank_work_and_refuse_an_empty_profile() {
        let llm = FakeLlm::new(vec![
            json(r#"{"summary": "Fits.", "gaps": [{"gap": "Kubernetes", "how_to_address": "course only"}]}"#),
            json(
                r#"{"questions": [{"question": "Tell me about yourself", "kind": "intro", "outline": ["I am Alex."]}]}"#,
            ),
        ]);
        assert_eq!(analyze(llm.as_ref(), "m", &brief(), Lang::En).await.unwrap().gaps.len(), 1);
        assert_eq!(question_bank(llm.as_ref(), "m", &brief(), 5, "", Lang::En).await.unwrap().len(), 1);
        assert!(analyze(llm.as_ref(), "m", &Brief::default(), Lang::En).await.is_err());
    }

    const MAP: &str = r#"{"role": "Backend Engineer", "summary": "About the web.", "topics": [
      {"name": "OOP", "kind": "concept", "why": "base", "likelihood": 9, "effort": 2},
      {"name": "REST", "kind": "concept", "why": "api", "likelihood": 8, "effort": 3},
      {"name": "SQL", "kind": "concept", "why": "db", "likelihood": 7, "effort": 4},
      {"name": "Kubernetes", "kind": "framework", "why": "ops", "likelihood": 2, "effort": 9},
      {"name": "Rust", "kind": "language", "why": "x", "likelihood": 1, "effort": 10}]}"#;
    const TERMS: &str = r#"{"terms": [{"name": "Term", "plain": "Plain.", "analogy": "Like a shop.", "english": "It is a thing.", "follow_ups": ["Why?"], "visual": "A box."}]}"#;

    fn vacancy() -> Brief {
        let mut brief = brief();
        brief.set_vacancy("Backend Engineer", "Acme", "Python and PostgreSQL.");
        brief
    }

    #[tokio::test]
    async fn research_ranks_the_topics_then_writes_terms_for_the_vital_few() {
        let llm = FakeLlm::new(vec![json(MAP), json(TERMS), json(TERMS), json(TERMS)]);
        let progress = ResearchProgress::default();
        let syllabus = research(llm.as_ref(), "m", &vacancy(), Lang::Ru, &progress).await.unwrap();
        assert_eq!(syllabus.topics.len(), 5);
        assert_eq!(syllabus.role, "Backend Engineer");
        assert_eq!(syllabus.company, "Acme");
        assert_eq!(syllabus.native, Lang::Ru);
        assert!(!syllabus.key.is_empty() && syllabus.made > 0);
        // OOP, REST and SQL are the vital three (24 of 27); the others wait until asked for
        for name in ["OOP", "REST", "SQL"] {
            let topic = &syllabus.topics[syllabus.topic_index(name).unwrap()];
            assert_eq!(topic.terms.len(), 1, "{name}");
        }
        assert!(syllabus.topics[syllabus.topic_index("Kubernetes").unwrap()].terms.is_empty());
        assert_eq!(llm.calls(), 4, "one map and one request per vital topic");
        assert_eq!(progress.done.load(Ordering::Relaxed), 4);
        assert_eq!(progress.total.load(Ordering::Relaxed), 4);
    }

    #[tokio::test]
    async fn research_needs_a_vacancy_and_costs_nothing_without_one() {
        let llm = FakeLlm::new(vec![]);
        let error = research(llm.as_ref(), "m", &brief(), Lang::En, &ResearchProgress::default()).await.unwrap_err();
        assert!(error.to_string().contains("Paste the vacancy"));
        assert_eq!(llm.calls(), 0);
    }

    #[tokio::test]
    async fn a_topic_that_fails_is_kept_without_terms_but_all_failing_is_an_error() {
        let one_fails = FakeLlm::new(vec![json(MAP), json(TERMS), json("nope"), json("still nope"), json(TERMS)]);
        let syllabus =
            research(one_fails.as_ref(), "m", &vacancy(), Lang::En, &ResearchProgress::default()).await.unwrap();
        let with_terms = syllabus.topics.iter().filter(|t| !t.terms.is_empty()).count();
        assert_eq!(with_terms, 2, "two of the three vital topics got terms");
        let all_fail = FakeLlm::new(vec![
            json(MAP),
            Reply::failing(LlmError::Unreachable("No AI gateway answers".into())),
            Reply::failing(LlmError::Unreachable("No AI gateway answers".into())),
            Reply::failing(LlmError::Unreachable("No AI gateway answers".into())),
        ]);
        let error =
            research(all_fail.as_ref(), "m", &vacancy(), Lang::En, &ResearchProgress::default()).await.unwrap_err();
        assert!(error.to_string().contains("No AI gateway"), "{error}");
    }

    #[tokio::test]
    async fn the_digest_is_made_from_the_session_and_refuses_an_empty_one() {
        let llm = FakeLlm::new(vec![json(
            r#"{"vital": ["Encapsulation hides state."], "next": "Do REST.", "hook": "Two left."}"#,
        )]);
        let made = digest(llm.as_ref(), "m", &session(), "OOP", Lang::Ru).await.unwrap();
        assert_eq!(made.vital, vec!["Encapsulation hides state."]);
        let request = llm.last_request();
        assert!(
            request[0].content.contains("in Russian")
                && request[1].content.contains("CANDIDATE: I build the order API.")
        );
        let empty = Session::new(Track::English, Kind::Live, "t");
        assert!(digest(llm.as_ref(), "m", &empty, "", Lang::En).await.is_err());
        assert_eq!(llm.calls(), 1);
    }

    #[tokio::test]
    async fn a_storyboard_is_asked_for_in_your_language_and_read() {
        let llm = FakeLlm::new(vec![json(
            r#"{"title": "Очередь", "check": "Кто выходит первым?", "scenes": [{"say": "Это как очередь в кассу.", "panels": [{"kind": "cells", "items": ["a", "b"]}]}]}"#,
        )]);
        let board = storyboard(llm.as_ref(), "m", "queue", "knows nothing", Lang::Ru, false).await.unwrap();
        assert_eq!((board.title.as_str(), board.scenes.len(), board.native), ("Очередь", 1, Lang::Ru));
        let request = llm.last_request();
        assert!(request[0].content.contains("in Russian") && request[1].content.contains("TOPIC: queue"));
        assert!(request[1].content.contains("knows nothing"));
        assert!(storyboard(llm.as_ref(), "m", "  ", "", Lang::En, false).await.is_err());
        assert_eq!(llm.calls(), 1, "an empty topic costs nothing");
    }

    #[tokio::test]
    async fn a_simpler_storyboard_says_so() {
        let llm =
            FakeLlm::new(vec![json(r#"{"scenes": [{"say": "One.", "panels": [{"kind": "text", "lines": ["x"]}]}]}"#)]);
        storyboard(llm.as_ref(), "m", "queue", "", Lang::En, true).await.unwrap();
        assert!(llm.last_request()[0].content.contains("did not understand the first version"));
    }

    fn services_with(texts: &[&str], frames: tokio::sync::mpsc::Receiver<Vec<i16>>) -> Services {
        let cell = std::sync::Mutex::new(Some(frames));
        Services {
            llm: FakeLlm::new(vec![]),
            stt: Some(FakeStt::new(texts)),
            tts: None,
            speaker: FakeSpeaker::new(),
            mic: Arc::new(move |_| {
                let receiver = cell.lock().unwrap().take().ok_or_else(|| anyhow::anyhow!("the microphone is busy"))?;
                Ok((receiver, Box::new(()) as Box<dyn std::any::Any + Send>))
            }),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn hearing_you_say_one_thing_returns_what_was_said() {
        let (talker, frames) = Talker::new();
        let services = services_with(&["I added an index."], frames);
        let speaking = tokio::spawn(async move {
            talker.silence(300).await;
            talker.speak(1500).await;
            talker.silence(1500).await;
        });
        let heard = hear(&services, "", SttHint::default(), Duration::from_secs(6)).await.unwrap();
        speaking.await.unwrap();
        assert_eq!(heard.text, "I added an index.");
    }

    #[tokio::test(start_paused = true)]
    async fn silence_means_nothing_was_heard() {
        let (talker, frames) = Talker::new();
        let services = services_with(&["x"], frames);
        let quiet = tokio::spawn(async move { talker.silence(8000).await });
        let error = hear(&services, "", SttHint::default(), Duration::from_secs(3)).await.unwrap_err();
        quiet.abort();
        assert_eq!(error.to_string(), "Nothing was heard.");
    }

    #[tokio::test(start_paused = true)]
    async fn without_recognition_it_says_how_to_fix_it() {
        let (_talker, frames) = Talker::new();
        let mut services = services_with(&[], frames);
        services.stt = None;
        let error = hear(&services, "", SttHint::default(), Duration::from_secs(1)).await.unwrap_err();
        assert!(error.to_string().contains("Settings"));
    }
}
