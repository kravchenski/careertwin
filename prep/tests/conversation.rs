//! A whole call, run in the clock of the test with a scripted model, voice, recogniser and mic.

use std::sync::Arc;
use std::time::Duration;

use careertwin_prep::brief::Brief;
use careertwin_prep::conversation::{self, Command, Config, Deps, Event, Knobs, State};
use careertwin_prep::llm::{LlmError, Role};
use careertwin_prep::prompts::Lang;
use careertwin_prep::prompts::interviewer::InterviewerSpec;
use careertwin_prep::session::{Speaker as Who, Track};
use careertwin_prep::speech::{SpeechError, SttHint, Voice};
use careertwin_prep::testing::{FakeLlm, FakeSpeaker, FakeStt, FakeTts, Reply, Talker};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

const DEMO: &str = include_str!("fixtures/ABOUT.demo.md");

struct Setup {
    replies: Vec<Reply>,
    stt: Vec<&'static str>,
    headphones: bool,
    mic: bool,
    tts: bool,
    tts_fails: bool,
    stt_error: Option<SpeechError>,
    minutes: u32,
    knobs: Knobs,
    spec: Option<InterviewerSpec>,
    voice_alt: Option<Voice>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            replies: Vec::new(),
            stt: Vec::new(),
            headphones: false,
            mic: true,
            tts: true,
            tts_fails: false,
            stt_error: None,
            minutes: 20,
            knobs: Knobs::default(),
            spec: None,
            voice_alt: None,
        }
    }
}

struct Harness {
    events: UnboundedReceiver<Event>,
    commands: UnboundedSender<Command>,
    llm: Arc<FakeLlm>,
    tts: Arc<FakeTts>,
    speaker: Arc<FakeSpeaker>,
    stt: Arc<FakeStt>,
    talker: Talker,
    log: Vec<Event>,
}

fn start(setup: Setup) -> Harness {
    let llm = FakeLlm::new(setup.replies);
    let stt = match setup.stt_error {
        Some(error) => FakeStt::failing(error),
        None => FakeStt::new(&setup.stt),
    };
    let tts = if setup.tts_fails { FakeTts::failing() } else { FakeTts::new() };
    let speaker = FakeSpeaker::new();
    let (talker, mic) = Talker::new();
    let spec = setup.spec.unwrap_or_else(|| {
        let mut spec = InterviewerSpec::new(Track::Technical);
        spec.minutes = setup.minutes;
        spec
    });
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "Python and PostgreSQL.");
    let config = Config {
        spec,
        brief,
        knobs: setup.knobs,
        model: "test".into(),
        headphones: setup.headphones,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: setup.voice_alt,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: llm.clone(),
        stt: stt.clone(),
        tts: setup.tts.then(|| tts.clone() as Arc<dyn careertwin_prep::speech::Tts>),
        speaker: speaker.clone(),
        mic: setup.mic.then_some(mic),
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, events) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));
    Harness { events, commands, llm, tts, speaker, stt, talker, log: Vec::new() }
}

impl Harness {
    /// Collects events until `done` holds for the log, or `ms` of test time pass.
    async fn until(&mut self, ms: u64, done: impl Fn(&[Event]) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            self.drain();
            if done(&self.log) {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn drain(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.log.push(event);
        }
    }

    async fn listening(&mut self) {
        let before = self.log.iter().filter(|e| matches!(e, Event::State(State::Listening))).count();
        assert!(
            self.until(60_000, |log| log.iter().filter(|e| matches!(e, Event::State(State::Listening))).count()
                > before)
                .await,
            "never started listening: {:?}",
            self.states()
        );
    }

    fn states(&self) -> Vec<State> {
        self.log.iter().filter_map(|e| if let Event::State(s) = e { Some(*s) } else { None }).collect()
    }

    fn turns(&self) -> Vec<(Who, String, bool)> {
        self.log
            .iter()
            .filter_map(|e| {
                if let Event::Turn { who, text, append, .. } = e { Some((*who, text.clone(), *append)) } else { None }
            })
            .collect()
    }

    fn captions(&self) -> Vec<String> {
        self.log
            .iter()
            .filter_map(|e| if let Event::Caption { text, .. } = e { Some(text.clone()) } else { None })
            .collect()
    }

    fn errors(&self) -> Vec<String> {
        self.log.iter().filter_map(|e| if let Event::Error(m) = e { Some(m.clone()) } else { None }).collect()
    }

    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// You say something and stop: speech, then enough silence for the turn to end.
    async fn say_aloud(&self, speech_ms: u64) {
        self.talker.speak(speech_ms).await;
        self.talker.silence(900).await;
    }
}

/// No nudges for hours, so a long silence tests only the clock.
fn calm() -> Knobs {
    Knobs { nudge_first_s: 100_000, nudge_second_s: 200_000, move_on_s: 300_000, ..Knobs::default() }
}

fn greeting() -> Reply {
    Reply::text("Hi Alex, I'm Daniel from the platform team. What are you working on at the moment?")
}

#[tokio::test(start_paused = true)]
async fn the_interviewer_greets_sentence_by_sentence_then_listens() {
    let mut call = start(Setup { replies: vec![greeting()], ..Setup::default() });
    call.listening().await;
    assert_eq!(
        call.captions(),
        vec!["Hi Alex, I'm Daniel from the platform team.", "What are you working on at the moment?"]
    );
    assert_eq!(call.speaker.clips(), 2);
    let states = call.states();
    assert_eq!(states[..4], [State::Starting, State::Thinking, State::Speaking, State::Listening]);
    assert_eq!(call.turns().len(), 1);
    assert_eq!(call.turns()[0].0, Who::Interviewer);
    // the first request carries the persona, the facts and the opening instruction
    let request = &call.llm.requests.lock().unwrap()[0];
    assert_eq!(request[0].role, Role::System);
    assert!(request[0].content.contains("Northwind Logistics") && request[0].content.contains("Daniel"));
    assert!(request[1].content.contains("Alex"));
}

#[tokio::test(start_paused = true)]
async fn the_first_sentence_is_spoken_before_the_reply_is_finished() {
    let mut reply = greeting();
    reply.gap_ms = 400; // the model writes slowly, a word at a time
    let mut call = start(Setup { replies: vec![reply], ..Setup::default() });
    assert!(call.until(60_000, |log| log.iter().any(|e| matches!(e, Event::Caption { .. }))).await);
    let first_caption_at = tokio::time::Instant::now();
    call.listening().await;
    let done_at = tokio::time::Instant::now();
    assert!(done_at - first_caption_at > Duration::from_secs(2), "speech started well before the end of the reply");
}

#[tokio::test(start_paused = true)]
async fn your_answer_reaches_the_model_with_the_history_and_a_state_note() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Okay. And what happened after the release?")],
        stt: vec!["I build the order API with FastAPI."],
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1500).await;
    call.listening().await;
    let turns = call.turns();
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[1], (Who::You, "I build the order API with FastAPI.".to_owned(), false));
    assert_eq!(turns[2].0, Who::Interviewer);
    let second = &call.llm.requests.lock().unwrap()[1];
    let roles: Vec<Role> = second.iter().map(|m| m.role).collect();
    assert_eq!(roles, vec![Role::System, Role::User, Role::Assistant, Role::User]);
    assert!(second[3].content.starts_with("I build the order API with FastAPI."));
    assert!(second[3].content.contains("[STATE:") && second[3].content.contains("answered 1 question."));
}

#[tokio::test(start_paused = true)]
async fn an_unfinished_thought_waits_and_the_rest_is_merged_into_one_turn() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Right. Why an alert?")],
        stt: vec!["I added an index and,", "then I added an alert."],
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1500).await; // "...an index and," then a pause longer than the silence limit
    call.talker.speak(1200).await; // the rest of the thought, inside the wait
    call.talker.silence(900).await;
    call.listening().await;
    let you: Vec<_> = call.turns().into_iter().filter(|t| t.0 == Who::You).collect();
    assert_eq!(you.len(), 1, "{you:?}");
    assert_eq!(you[0].1, "I added an index and, then I added an alert.");
    assert_eq!(call.llm.calls(), 2, "the model was asked once, after the whole thought");
}

#[tokio::test(start_paused = true)]
async fn an_unfinished_thought_with_nothing_after_it_is_sent_after_the_wait() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Go on.")],
        stt: vec!["So we decided to"],
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1200).await;
    assert!(call.until(10_000, |log| log.iter().any(|e| matches!(e, Event::Turn { who: Who::You, .. }))).await);
    assert_eq!(call.turns()[1].1, "So we decided to");
}

#[tokio::test(start_paused = true)]
async fn a_slow_model_gets_an_acknowledgement_first() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Interesting. How did you find it?").slow(4000)],
        stt: vec!["The latency doubled after a release."],
        ..Setup::default()
    });
    call.listening().await;
    let clips_before = call.speaker.clips();
    call.say_aloud(1500).await;
    // before the model has said a word, a short acknowledgement is already playing
    let speaker = call.speaker.clone();
    assert!(call.until(3500, move |_| speaker.clips() > clips_before).await, "no acknowledgement");
    assert!(call.captions().len() == 2, "the model has not spoken yet: {:?}", call.captions());
    call.listening().await;
    assert!(call.captions().len() >= 3);
    let said = call.tts.said();
    assert!(said.iter().any(|s| ["Mm-hm.", "Okay.", "I see."].contains(&s.as_str())), "{said:?}");
}

#[tokio::test(start_paused = true)]
async fn a_quick_model_needs_no_acknowledgement() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Interesting. How did you find it?")],
        stt: vec!["The latency doubled after a release."],
        ..Setup::default()
    });
    call.listening().await;
    let clips_before = call.speaker.clips();
    call.say_aloud(1500).await;
    call.listening().await;
    // two sentences of the answer; nothing extra in front of them
    assert_eq!(call.speaker.clips() - clips_before, 2);
}

#[tokio::test(start_paused = true)]
async fn silence_brings_a_nudge_then_a_second_then_a_simpler_question() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("That's fine. Let's try something simpler: what is an index?")],
        ..Setup::default()
    });
    call.listening().await;
    call.talker.silence(15_000).await;
    call.drain();
    assert!(call.captions().iter().any(|c| c == "Take your time."), "{:?}", call.captions());
    call.talker.silence(27_000).await;
    call.drain();
    assert!(call.captions().iter().any(|c| c == "Would you like me to repeat the question?"), "{:?}", call.captions());
    assert_eq!(call.llm.calls(), 1, "the nudges are not model turns");
    call.talker.silence(40_000).await;
    call.drain();
    assert_eq!(call.llm.calls(), 2, "after a minute the interviewer moves on");
    let note = call.llm.last_request().last().unwrap().content.clone();
    assert!(note.contains("has not answered for over a minute"), "{note}");
}

#[tokio::test(start_paused = true)]
async fn speaking_to_a_person_resets_the_nudges() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Okay.")],
        stt: vec!["Well, I work on an order API."],
        ..Setup::default()
    });
    call.listening().await;
    call.talker.silence(10_000).await;
    call.say_aloud(1500).await;
    call.listening().await;
    call.talker.silence(10_000).await;
    call.drain();
    assert!(
        !call.captions().iter().any(|c| c == "Take your time."),
        "ten quiet seconds after an answer is not yet a nudge"
    );
}

#[tokio::test(start_paused = true)]
async fn on_speakers_the_microphone_is_shut_while_the_interviewer_talks() {
    let long = Reply::text(
        "Welcome. I'm Daniel. Let me tell you a little about how this call will go. We will start with your background. Then we will move on to the technical part.",
    );
    let mut call = start(Setup {
        replies: vec![long],
        stt: vec!["noise that must not be heard"],
        headphones: false,
        ..Setup::default()
    });
    assert!(call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::State(State::Speaking)))).await);
    call.talker.speak(1500).await; // you cough over the interviewer
    call.talker.silence(1000).await;
    assert_eq!(
        call.stt.calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the interviewer's own voice must not be recognised"
    );
    assert_eq!(call.speaker.stopped(), 0, "and it must not interrupt");
}

#[tokio::test(start_paused = true)]
async fn with_headphones_talking_over_the_interviewer_stops_it_and_keeps_what_was_said() {
    let long = Reply::text(
        "Welcome. I'm Daniel. Let me tell you a little about how this call will go. We will start with your background. Then we will move on to the technical part.",
    );
    let mut call = start(Setup {
        replies: vec![long, Reply::text("Sure. Go ahead.")],
        stt: vec!["Sorry, can I ask how long this takes?"],
        headphones: true,
        ..Setup::default()
    });
    assert!(call.until(30_000, |log| log.iter().filter(|e| matches!(e, Event::Caption { .. })).count() >= 2).await);
    tokio::time::sleep(Duration::from_millis(600)).await; // the first sentence is partly said
    call.talker.speak(1200).await;
    call.talker.silence(900).await;
    call.listening().await;
    assert!(call.speaker.stopped() >= 1, "it stopped talking");
    let turns = call.turns();
    let spoken = &turns[0];
    assert_eq!(spoken.0, Who::Interviewer);
    assert!(spoken.1.starts_with("Welcome."), "{spoken:?}");
    assert!(!spoken.1.contains("technical part"), "what was never said is not in the history: {spoken:?}");
    assert!(turns.iter().any(|t| t.0 == Who::You && t.1.contains("how long")), "{turns:?}");
}

#[tokio::test(start_paused = true)]
async fn a_hallucinated_thank_you_from_a_cough_is_not_an_answer() {
    let mut call = start(Setup { replies: vec![greeting()], stt: vec!["Thank you."], ..Setup::default() });
    call.listening().await;
    call.talker.speak(400).await;
    call.talker.silence(1500).await;
    call.drain();
    assert_eq!(call.llm.calls(), 1);
    assert_eq!(call.turns().iter().filter(|t| t.0 == Who::You).count(), 0);
    assert_eq!(*call.states().last().unwrap(), State::Listening);
}

#[tokio::test(start_paused = true)]
async fn a_typed_answer_works_without_a_microphone() {
    let mut call =
        start(Setup { replies: vec![greeting(), Reply::text("Good. Tell me more.")], mic: false, ..Setup::default() });
    call.listening().await;
    call.send(Command::Say("  I build APIs.  ".into()));
    call.listening().await;
    assert_eq!(call.turns()[1], (Who::You, "I build APIs.".to_owned(), false));
    assert_eq!(call.llm.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn typing_over_the_interviewer_interrupts_it() {
    let long = Reply::text(
        "Welcome. I'm Daniel. Let me tell you a little about how this call will go. We will start with your background.",
    );
    let mut call = start(Setup { replies: vec![long, Reply::text("Okay.")], mic: false, ..Setup::default() });
    assert!(call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::State(State::Speaking)))).await);
    call.send(Command::Say("Can we skip the intro?".into()));
    call.listening().await;
    assert!(call.speaker.stopped() >= 1);
    assert_eq!(call.llm.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn the_interrupt_button_silences_the_interviewer() {
    let long = Reply::text(
        "Welcome. I'm Daniel. Let me tell you a little about how this call will go. We will start with your background.",
    );
    let mut call = start(Setup { replies: vec![long], ..Setup::default() });
    assert!(call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::State(State::Speaking)))).await);
    call.send(Command::Interrupt);
    call.listening().await;
    assert!(call.speaker.stopped() >= 1);
}

#[tokio::test(start_paused = true)]
async fn a_muted_microphone_hears_nothing() {
    let mut call = start(Setup { replies: vec![greeting()], stt: vec!["must not be heard"], ..Setup::default() });
    call.listening().await;
    call.send(Command::Mute(true));
    call.talker.silence(100).await;
    call.say_aloud(1500).await;
    assert_eq!(call.stt.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    call.drain();
    assert_eq!(*call.states().last().unwrap(), State::Muted);
    call.send(Command::Mute(false));
    call.talker.silence(100).await;
    call.drain();
    assert_eq!(*call.states().last().unwrap(), State::Listening);
}

#[tokio::test(start_paused = true)]
async fn a_failed_model_is_reported_and_retry_asks_again() {
    let mut call = start(Setup {
        replies: vec![
            Reply::failing(LlmError::Unreachable("No AI gateway answers at http://127.0.0.1:3260".into())),
            greeting(),
        ],
        ..Setup::default()
    });
    call.listening().await;
    assert_eq!(call.errors().len(), 1);
    assert!(call.errors()[0].contains("No AI gateway"));
    assert!(call.turns().is_empty());
    call.send(Command::Retry);
    assert!(call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::Turn { who: Who::Interviewer, .. }))).await);
    assert_eq!(call.llm.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_failed_voice_leaves_the_interviewer_talking_in_writing() {
    let mut call = start(Setup { replies: vec![greeting()], tts_fails: true, ..Setup::default() });
    call.listening().await;
    assert_eq!(call.captions().len(), 2, "the words still appear");
    assert_eq!(call.errors().len(), 1, "one error, not one per sentence");
    assert!(call.errors()[0].contains("in writing"));
    assert_eq!(call.speaker.clips(), 0);
}

#[tokio::test(start_paused = true)]
async fn with_the_voice_off_the_interviewer_only_writes() {
    let mut call = start(Setup { replies: vec![greeting()], tts: false, ..Setup::default() });
    call.listening().await;
    assert_eq!(call.captions().len(), 2);
    assert_eq!(call.speaker.clips(), 0);
    assert!(call.errors().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_recognition_error_is_reported_and_the_call_goes_on() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Okay.")],
        stt_error: Some(SpeechError::Status("Speech recognition returned 401".into())),
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1500).await;
    call.drain();
    assert!(call.errors().iter().any(|e| e.contains("401")), "{:?}", call.errors());
    assert_eq!(*call.states().last().unwrap(), State::Listening);
    call.send(Command::Say("typed instead".into()));
    call.listening().await;
    assert_eq!(call.turns().iter().filter(|t| t.0 == Who::You).count(), 1);
}

#[tokio::test(start_paused = true)]
async fn when_the_time_is_up_the_answer_is_the_goodbye() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Thanks for your time, it was a pleasure. Goodbye!")],
        stt: vec!["I think that covers it."],
        minutes: 3,
        knobs: calm(),
        ..Setup::default()
    });
    call.listening().await;
    call.talker.silence(185_000).await; // past the end, but not yet into overtime
    call.say_aloud(1500).await;
    assert!(
        call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::Finished))).await,
        "the call did not end: {:?}",
        call.states()
    );
    let note = call.llm.last_request().last().unwrap().content.clone();
    assert!(note.contains("Time is up") && note.contains("say goodbye"), "{note}");
    assert_eq!(*call.states().last().unwrap(), State::Finished);
}

#[tokio::test(start_paused = true)]
async fn if_nobody_answers_after_the_time_the_interviewer_closes_the_call() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("We're out of time. Thank you, goodbye!")],
        minutes: 3,
        knobs: calm(),
        ..Setup::default()
    });
    call.listening().await;
    call.talker.silence(210_000).await;
    assert!(call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::Finished))).await, "{:?}", call.states());
    assert!(call.llm.last_request().last().unwrap().content.contains("Time is up"));
}

#[tokio::test(start_paused = true)]
async fn near_the_end_the_interviewer_is_told_to_ask_for_questions_once() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Do you have any questions for me?"), Reply::text("Good question.")],
        stt: vec!["Yes, I am ready.", "How big is the team?"],
        minutes: 3,
        knobs: calm(),
        ..Setup::default()
    });
    call.listening().await;
    call.talker.silence(165_000).await;
    call.say_aloud(1500).await;
    call.listening().await;
    let note = call.llm.requests.lock().unwrap()[1].last().unwrap().content.clone();
    assert!(note.contains("ask whether they have any questions"), "{note}");
    call.say_aloud(1500).await;
    call.listening().await;
    let note = call.llm.requests.lock().unwrap()[2].last().unwrap().content.clone();
    assert!(!note.contains("ask whether they have any questions"), "not asked twice: {note}");
}

#[tokio::test(start_paused = true)]
async fn finishing_ends_the_call_and_silences_the_speakers() {
    let mut call = start(Setup { replies: vec![greeting()], ..Setup::default() });
    assert!(call.until(30_000, |log| log.iter().any(|e| matches!(e, Event::State(State::Speaking)))).await);
    call.send(Command::Finish);
    assert!(call.until(5_000, |log| log.iter().any(|e| matches!(e, Event::Finished))).await);
    assert!(call.speaker.stopped() >= 1);
}

#[tokio::test(start_paused = true)]
async fn going_on_talking_before_the_answer_cancels_it_and_adds_to_your_turn() {
    let mut call = start(Setup {
        replies: vec![
            greeting(),
            Reply::text("This reply should never be heard.").slow(6000),
            Reply::text("Got it. And the alert?"),
        ],
        stt: vec!["I added an index.", "And I also added an alert."],
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1200).await; // answer one; the model is now slow to reply
    tokio::time::sleep(Duration::from_millis(500)).await;
    call.talker.speak(1200).await; // you remember something else
    call.talker.silence(900).await;
    call.listening().await;
    let you: Vec<_> = call.turns().into_iter().filter(|t| t.0 == Who::You).collect();
    assert_eq!(you.len(), 2, "{you:?}");
    assert!(you[1].2, "the second part continues the first turn");
    assert!(!call.captions().iter().any(|c| c.contains("never be heard")));
    let last = call.llm.last_request();
    let text = last.iter().filter(|m| m.role == Role::User).map(|m| m.content.clone()).collect::<Vec<_>>().join("|");
    assert!(text.contains("I added an index. And I also added an alert."), "{text}");
}

#[tokio::test(start_paused = true)]
async fn the_levels_are_reported_for_the_orb() {
    let mut call = start(Setup { replies: vec![greeting()], ..Setup::default() });
    call.listening().await;
    call.talker.speak(400).await;
    call.drain();
    let loudest_mic = call
        .log
        .iter()
        .filter_map(|e| if let Event::Level { mic, .. } = e { Some(*mic) } else { None })
        .fold(0.0f32, f32::max);
    let loudest_speaker = call
        .log
        .iter()
        .filter_map(|e| if let Event::Level { speaker, .. } = e { Some(*speaker) } else { None })
        .fold(0.0f32, f32::max);
    assert!(loudest_mic > 0.1, "{loudest_mic}");
    assert!(loudest_speaker > 0.1, "{loudest_speaker}");
}

#[tokio::test(start_paused = true)]
async fn an_empty_reply_is_asked_for_again_with_more_room() {
    let mut call = start(Setup { replies: vec![Reply::failing(LlmError::Empty), greeting()], ..Setup::default() });
    call.listening().await;
    assert!(call.errors().is_empty(), "the retry hid the hiccup: {:?}", call.errors());
    assert_eq!(call.captions().len(), 2);
    assert_eq!(*call.llm.allowances.lock().unwrap(), vec![400, 1500]);
}

#[tokio::test(start_paused = true)]
async fn two_empty_replies_in_a_row_are_reported() {
    let mut call = start(Setup {
        replies: vec![Reply::failing(LlmError::Empty), Reply::failing(LlmError::Empty)],
        ..Setup::default()
    });
    call.listening().await;
    assert_eq!(call.errors().len(), 1);
    assert!(call.captions().is_empty());
    assert_eq!(call.llm.calls(), 2);
}

// ---- the tutor ----

use careertwin_prep::prompts::interviewer::Persona;
use careertwin_prep::prompts::tutor::{Lesson, LessonView, Mode, Stage};
use careertwin_prep::syllabus::{Half, Term};
use careertwin_prep::tags::Tag;

fn term(name: &str) -> Term {
    Term {
        name: name.into(),
        plain: format!("{name} in plain words."),
        analogy: "Like a shop.".into(),
        english: format!("{name} is a thing."),
        follow_ups: vec!["Why?".into()],
        visual: String::new(),
    }
}

fn lesson_spec(mode: Mode) -> InterviewerSpec {
    let mut spec = InterviewerSpec::tutoring(Track::Technical, mode, Lang::Ru, 20);
    if mode == Mode::Terms {
        spec.lesson = Some(Lesson { topic: "OOP".into(), terms: vec![term("Encapsulation"), term("Polymorphism")] });
    }
    spec
}

impl Harness {
    fn tags(&self) -> Vec<Tag> {
        self.log.iter().filter_map(|e| if let Event::Tag(t) = e { Some(t.clone()) } else { None }).collect()
    }

    fn progress(&self) -> Vec<(String, Half, u8)> {
        self.log
            .iter()
            .filter_map(|e| match e {
                Event::TermProgress { name, half, score, .. } => Some((name.clone(), *half, *score)),
                _ => None,
            })
            .collect()
    }

    fn lessons(&self) -> Vec<LessonView> {
        self.log.iter().filter_map(|e| if let Event::Lesson(v) = e { Some(v.clone()) } else { None }).collect()
    }
}

#[tokio::test(start_paused = true)]
async fn a_terms_lesson_goes_from_your_words_to_english_and_on_to_the_next_term() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        replies: vec![
            Reply::text("Привет, Алекс. Сегодня ООП. Что такое инкапсуляция, своими словами?"),
            Reply::text("Да, верно. Теперь скажи то же по-английски, как на собеседовании. [[native: 3]]"),
            Reply::text("Good. Now, what is polymorphism, in your own words? [[english: 2]]"),
        ],
        stt: vec!["Объект прячет внутри своё состояние.", "Encapsulation means an object hides its state."],
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1500).await;
    call.listening().await;
    call.say_aloud(1500).await;
    call.listening().await;
    // the words that were said and shown are free of tags
    assert!(call.captions().iter().all(|c| !c.contains("[[")), "{:?}", call.captions());
    assert!(call.turns().iter().all(|t| !t.1.contains("[[")), "{:?}", call.turns());
    // and the call understood them
    assert_eq!(
        call.progress(),
        vec![("Encapsulation".to_owned(), Half::Native, 3), ("Encapsulation".to_owned(), Half::English, 2)]
    );
    let places: Vec<(usize, Stage)> = call.lessons().iter().map(|v| (v.index, v.stage)).collect();
    assert_eq!(places, vec![(0, Stage::Native), (0, Stage::English), (1, Stage::Native)]);
    // each request told the tutor where the lesson was
    let requests = call.llm.requests.lock().unwrap().clone();
    let note = |n: usize| requests[n].last().unwrap().content.clone();
    assert!(note(1).contains("term 1 of 2, \"Encapsulation\", stage one, in Russian"), "{}", note(1));
    assert!(note(2).contains("term 1 of 2, \"Encapsulation\", stage two, in English"), "{}", note(2));
    assert!(requests[0][0].content.contains("1. Encapsulation") && requests[0][0].content.contains("2. Polymorphism"));
}

#[tokio::test(start_paused = true)]
async fn a_tag_that_arrives_in_pieces_is_never_spoken() {
    let broken = Reply {
        pieces: ["Good. ", "Let's go on.", " [", "[nat", "ive: 2", "]", "]"].iter().map(|s| (*s).to_owned()).collect(),
        ..Reply::default()
    };
    let mut call = start(Setup { spec: Some(lesson_spec(Mode::Terms)), replies: vec![broken], ..Setup::default() });
    call.listening().await;
    assert_eq!(call.captions(), vec!["Good.", "Let's go on."]);
    assert!(call.tts.said().iter().all(|s| !s.contains('[')), "{:?}", call.tts.said());
    assert_eq!(call.tags(), vec![Tag::Native(2)]);
}

#[tokio::test(start_paused = true)]
async fn a_note_and_a_picture_are_passed_to_the_window() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        replies: vec![Reply::text(
            "Sure, watch this. [[show: how a queue works]][[note: A queue is first in, first out.]]",
        )],
        ..Setup::default()
    });
    call.listening().await;
    assert_eq!(
        call.tags(),
        vec![Tag::Show("how a queue works".into()), Tag::Note("A queue is first in, first out.".into())]
    );
    assert_eq!(call.captions(), vec!["Sure, watch this."]);
}

#[tokio::test(start_paused = true)]
async fn the_end_tag_closes_the_call_after_the_goodbye() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Intro)),
        replies: vec![Reply::text("Thanks, that is all for today. See you tomorrow. [[end]]")],
        ..Setup::default()
    });
    assert!(call.until(60_000, |log| log.iter().any(|e| matches!(e, Event::Finished))).await, "{:?}", call.states());
    assert_eq!(call.captions(), vec!["Thanks, that is all for today.", "See you tomorrow."]);
    assert!(call.speaker.clips() >= 2, "the goodbye was said before the call closed");
}

#[tokio::test(start_paused = true)]
async fn what_the_window_tells_the_tutor_arrives_with_your_next_words() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        replies: vec![Reply::text("Hello."), Reply::text("Okay.")],
        stt: vec!["I am ready."],
        ..Setup::default()
    });
    call.listening().await;
    call.send(Command::Context("the 25-minute focus block has just ended".into()));
    call.talker.silence(100).await;
    call.say_aloud(1500).await;
    call.listening().await;
    let note = call.llm.last_request().last().unwrap().content.clone();
    assert!(note.contains("[STATE: the 25-minute focus block has just ended]"), "{note}");
    assert!(note.contains("[LESSON:"), "{note}");
    // it is said once, not at every turn after
    let mut call2 = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        replies: vec![Reply::text("Hello."), Reply::text("Okay."), Reply::text("Fine.")],
        stt: vec!["One.", "Two."],
        ..Setup::default()
    });
    call2.listening().await;
    call2.send(Command::Context("a picture was shown".into()));
    call2.talker.silence(100).await;
    call2.say_aloud(1500).await;
    call2.listening().await;
    call2.say_aloud(1500).await;
    call2.listening().await;
    assert!(call2.llm.last_request().last().unwrap().content.contains("[LESSON:"));
    assert!(!call2.llm.last_request().last().unwrap().content.contains("a picture was shown"));
}

#[tokio::test(start_paused = true)]
async fn the_tutor_can_be_told_to_say_something_and_it_goes_into_the_history() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        replies: vec![Reply::text("Hello."), Reply::text("Right.")],
        stt: vec!["Yes."],
        ..Setup::default()
    });
    call.listening().await;
    call.send(Command::Speak("Take a look at line three.".into()));
    call.listening().await;
    assert!(call.captions().contains(&"Take a look at line three.".to_owned()));
    assert_eq!(call.llm.calls(), 1, "a line the program wrote is not a model turn");
    call.say_aloud(1200).await;
    call.listening().await;
    let history = call.llm.last_request();
    assert!(history.iter().any(|m| m.content.contains("Take a look at line three.")), "{history:?}");
}

#[tokio::test(start_paused = true)]
async fn while_a_picture_plays_the_tutor_does_not_nudge() {
    let mut call = start(Setup { replies: vec![greeting()], ..Setup::default() });
    call.listening().await;
    call.send(Command::Hold(true));
    call.talker.silence(60_000).await;
    call.drain();
    assert!(!call.captions().iter().any(|c| c == "Take your time."), "{:?}", call.captions());
    call.send(Command::Hold(false));
    call.talker.silence(20_000).await;
    call.drain();
    assert!(call.captions().iter().any(|c| c == "Take your time."), "the nudges come back when the hold ends");
}

#[tokio::test(start_paused = true)]
async fn a_chat_says_hello_without_asking_the_model_and_never_nudges() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Chat)),
        mic: false,
        tts: false,
        replies: vec![Reply::text("A queue is first in, first out.")],
        ..Setup::default()
    });
    call.listening().await;
    assert_eq!(call.llm.calls(), 0);
    assert_eq!(call.captions(), vec!["Я здесь. Пишите что угодно: вопрос, термин, строчку с лекции."]);
    call.talker.silence(5000).await;
    call.send(Command::Say("what is a queue?".into()));
    call.listening().await;
    assert_eq!(call.llm.calls(), 1);
    let request = call.llm.last_request();
    assert!(request[0].content.contains("This is a chat"), "the chat prompt is used");
    assert_eq!(request.last().unwrap().role, Role::User);
    assert!(request.iter().any(|m| m.content.contains("Я здесь")), "the greeting is part of the history");
    // a long quiet is not a reason to speak
    call.talker.silence(600_000).await;
    call.drain();
    assert_eq!(call.llm.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn turning_the_voice_off_stops_it_at_once_and_the_tutor_writes_on() {
    let mut call = start(Setup {
        replies: vec![greeting(), Reply::text("Okay. And then?")],
        stt: vec!["I added an index."],
        ..Setup::default()
    });
    call.listening().await;
    call.send(Command::Voice(false));
    call.talker.silence(100).await;
    let clips = call.speaker.clips();
    call.say_aloud(1500).await;
    call.listening().await;
    assert_eq!(call.speaker.clips(), clips, "nothing was spoken after the voice was turned off");
    assert!(call.captions().iter().any(|c| c.starts_with("Okay.")), "{:?}", call.captions());
}

#[tokio::test(start_paused = true)]
async fn each_sentence_is_spoken_in_the_voice_of_its_own_language() {
    let alt = Voice { name: "russian-voice".into(), speed: 1.0, instructions: String::new(), language: Lang::Ru };
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        voice_alt: Some(alt),
        replies: vec![Reply::text("Привет. Сегодня про ООП. Let's begin with the first term.")],
        ..Setup::default()
    });
    call.listening().await;
    let used: Vec<(String, String)> = call.tts.said().into_iter().zip(call.tts.voices_used()).collect();
    let voice_of = |text: &str| used.iter().find(|(said, _)| said.starts_with(text)).map(|(_, v)| v.clone());
    assert_eq!(voice_of("Привет").as_deref(), Some("russian-voice"));
    assert_eq!(voice_of("Сегодня").as_deref(), Some("russian-voice"));
    assert_eq!(voice_of("Let's begin").as_deref(), Some("alloy"));
}

#[tokio::test(start_paused = true)]
async fn when_the_other_language_cannot_be_spoken_it_is_said_once_and_then_only_written() {
    // the offline voice cannot do Russian: the first Russian sentence fails, the rest are not tried
    let alt = Voice { name: "russian-voice".into(), speed: 1.0, instructions: String::new(), language: Lang::Ru };
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Terms)),
        voice_alt: Some(alt),
        tts_fails: true,
        replies: vec![Reply::text("Привет. Как дела?"), Reply::text("Хорошо. Продолжим.")],
        stt: vec!["Нормально."],
        ..Setup::default()
    });
    call.listening().await;
    call.say_aloud(1200).await;
    call.listening().await;
    assert_eq!(call.errors().len(), 1, "{:?}", call.errors());
    assert!(call.errors()[0].contains("written only"), "{:?}", call.errors());
    assert!(call.captions().iter().any(|c| c == "Продолжим."), "the words still appear: {:?}", call.captions());
}

#[tokio::test(start_paused = true)]
async fn the_mentor_has_her_own_name_in_the_prompt() {
    let mut call = start(Setup {
        spec: Some(lesson_spec(Mode::Intro)),
        replies: vec![Reply::text("Hi Alex.")],
        ..Setup::default()
    });
    call.listening().await;
    let system = call.llm.requests.lock().unwrap()[0][0].content.clone();
    assert!(system.contains("You are Maya"));
    assert_eq!(lesson_spec(Mode::Intro).persona, Persona::Mentor);
}

#[tokio::test(start_paused = true)]
async fn while_code_is_written_the_tutor_waits_and_sees_the_latest_version_with_your_words() {
    let mut spec = lesson_spec(Mode::Coding);
    spec.task = Some(careertwin_prep::coding::Task {
        title: "Two sum".into(),
        statement: "Find two numbers.".into(),
        language: "python".into(),
        ..careertwin_prep::coding::Task::default()
    });
    let mut call = start(Setup {
        spec: Some(spec),
        replies: vec![
            Reply::text("Hi Alex. Here is the task. What approach do you have in mind?"),
            Reply::text("Okay. Try it by hand."),
        ],
        stt: vec!["I will use a dictionary."],
        ..Setup::default()
    });
    call.listening().await;
    // a long quiet while typing is not a reason to speak
    call.talker.silence(240_000).await;
    call.drain();
    assert_eq!(call.llm.calls(), 1, "no nudges while code is written");
    assert!(!call.captions().iter().any(|c| c == "Take your time."), "{:?}", call.captions());
    call.send(Command::Code("def f():\n    pass\n".into()));
    call.send(Command::Code("def f(nums):\n    seen = {}\n".into()));
    call.talker.silence(100).await;
    call.say_aloud(1500).await;
    call.listening().await;
    let note = call.llm.last_request().last().unwrap().content.clone();
    assert!(note.contains("[CODE the person has written so far"), "{note}");
    assert!(note.contains("seen = {}") && !note.contains("pass\n"), "only the latest version: {note}");
    let system = call.llm.requests.lock().unwrap()[0][0].content.clone();
    assert!(system.contains("PRIVATE NOTES FOR YOU") && system.contains("live-coding interview"));
}
