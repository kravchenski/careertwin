#![allow(clippy::field_reassign_with_default)]
//! The whole window, drawn headless: behaviour checked through what a screen reader sees, and
//! pictures written out (when `UI_SHOTS` names a folder) so the look can be judged.

use std::sync::Arc;

use careertwin_prep::config::{Paths, Settings};
use careertwin_prep::services::{Problems, Services};
use careertwin_prep::testing::{FakeLlm, FakeSpeaker, FakeStt, FakeTts};
use careertwin_prep::ui::{PrepApp, theme};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const DEMO: &str = include_str!("fixtures/ABOUT.demo.md");

pub struct Fixture {
    pub app: PrepApp,
    _dir: tempfile::TempDir,
    _runtime: tokio::runtime::Runtime,
}

pub fn fixture(language: &str) -> Fixture {
    fixture_with(language, vec![])
}

pub struct Options {
    pub replies: Vec<careertwin_prep::testing::Reply>,
    pub heard: Vec<&'static str>,
    pub mic: Option<tokio::sync::mpsc::Receiver<Vec<i16>>>,
}

pub fn fixture_with(language: &str, replies: Vec<careertwin_prep::testing::Reply>) -> Fixture {
    fixture_full(language, Options { replies, heard: vec![], mic: None })
}

pub fn fixture_full(language: &str, options: Options) -> Fixture {
    let Options { replies, heard, mic } = options;
    let mic_slot = std::sync::Mutex::new(mic);
    let dir = tempfile::tempdir().unwrap();
    let about = dir.path().join("ABOUT.md");
    std::fs::write(&about, DEMO).unwrap();
    let paths = Paths::under(dir.path().join("home"));
    let mut settings = Settings::default();
    settings.about_path = about.to_string_lossy().into_owned();
    settings.ui_language = if language == "ru" {
        careertwin_prep::config::UiLanguage::Ru
    } else {
        careertwin_prep::config::UiLanguage::En
    };
    let services = Services {
        llm: FakeLlm::new(replies),
        stt: Some(FakeStt::new(&heard)),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: Arc::new(move |_| match mic_slot.lock().unwrap().take() {
            Some(frames) => Ok((frames, Box::new(()) as Box<dyn std::any::Any + Send>)),
            None => Err(anyhow::anyhow!("no microphone in a test")),
        }),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let app = PrepApp::new(paths, settings, services, Problems::default(), runtime.handle().clone(), Arc::new(|| {}));
    Fixture { app, _dir: dir, _runtime: runtime }
}

pub fn harness(
    fixture: Fixture,
    width: f32,
    height: f32,
) -> (Harness<'static, PrepApp>, tempfile::TempDir, tokio::runtime::Runtime) {
    let Fixture { app, _dir, _runtime } = fixture;
    let harness = Harness::builder()
        .with_size(egui::vec2(width, height))
        .with_context_setup(theme::install)
        .build_ui_state(|ui, app: &mut PrepApp| app.show(ui), app);
    (harness, _dir, _runtime)
}

pub fn shot(harness: &mut Harness<'_, PrepApp>, name: &str) {
    if let Ok(dir) = std::env::var("UI_SHOTS") {
        harness.run();
        let image = harness.render().expect("the window renders headless");
        image.save(format!("{dir}/{name}.png")).unwrap();
    }
}

#[test]
fn the_three_tabs_are_there_and_the_technical_one_is_the_start() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    harness.run();
    harness.get_by_label("English");
    harness.get_by_label("Technical");
    harness.get_by_label("Interview");
    harness.get_by_label("Mock interview");
    shot(&mut harness, "home-technical-en");
}

use careertwin_prep::conversation::{Command, Event, Handle, State};
use careertwin_prep::prompts::interviewer::InterviewerSpec;
use careertwin_prep::session::{Kind, Session, Speaker as Who, Track};
use careertwin_prep::ui::Screen;
use careertwin_prep::ui::live::LiveView;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

struct Wired {
    events: UnboundedSender<Event>,
    commands: UnboundedReceiver<Command>,
}

fn live(harness: &mut Harness<'_, PrepApp>) -> Wired {
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = Handle::from_parts(command_tx, event_rx);
    let app = harness.state_mut();
    let spec = InterviewerSpec::new(Track::Technical);
    let session = Session::new(Track::Technical, Kind::Live, "Mock interview · Daniel · Tech lead");
    app.screen = Screen::Live(Box::new(LiveView::new(session, spec, handle, None)));
    Wired { events: event_tx, commands: command_rx }
}

#[test]
fn a_call_shows_what_is_said_and_who_is_speaking() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let wired = live(&mut harness);
    wired.events.send(Event::State(State::Speaking)).unwrap();
    wired
        .events
        .send(Event::Caption { text: "Hi Alex, I'm Daniel from the platform team.".into(), seconds: 3.0 })
        .unwrap();
    wired.events.send(Event::Caption { text: "What are you working on at the moment?".into(), seconds: 2.5 }).unwrap();
    harness.run();
    harness.get_by_label("Daniel is speaking");
    harness.get_by_label("End call");
    harness.get_by_label("Mute");
    shot(&mut harness, "live-speaking-en");

    wired
        .events
        .send(Event::Turn {
            who: Who::Interviewer,
            text: "Hi Alex, I'm Daniel from the platform team. What are you working on at the moment?".into(),
            language: None,
            append: false,
        })
        .unwrap();
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run();
    harness.get_by_label("Your turn. I'm listening.");
}

#[test]
fn what_you_type_goes_to_the_interviewer_and_hanging_up_ends_the_call() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let mut wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run();
    harness.get_by_role(egui::accesskit::Role::TextInput).focus();
    harness.run();
    harness.get_by_role(egui::accesskit::Role::TextInput).type_text("I build the order API.");
    harness.run();
    harness.get_by_label("Send").click();
    harness.run();
    match wired.commands.try_recv() {
        Ok(Command::Say(text)) => assert_eq!(text, "I build the order API."),
        other => panic!("expected the typed answer, got {other:?}"),
    }
    harness.get_by_label("End call").click();
    harness.run();
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Finish)));
    assert!(matches!(harness.state().screen, Screen::Home), "back to the start once the call is over");
}

#[test]
fn an_error_is_shown_with_a_way_to_try_again() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let mut wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Error("No AI gateway answers at http://127.0.0.1:3260 (connection refused).".into()))
        .unwrap();
    harness.run();
    harness.get_by_label_contains("No AI gateway answers");
    shot(&mut harness, "live-error-en");
    harness.get_by_label("Try again").click();
    harness.run();
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Retry)));
    assert!(harness.query_by_label_contains("No AI gateway answers").is_none(), "the banner goes away");
}

#[test]
fn the_transcript_opens_and_lists_both_sides() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Turn { who: Who::Interviewer, text: "What do you work on?".into(), language: None, append: false })
        .unwrap();
    wired
        .events
        .send(Event::Turn {
            who: Who::You,
            text: "An order API in FastAPI.".into(),
            language: Some("en".into()),
            append: false,
        })
        .unwrap();
    wired
        .events
        .send(Event::Turn { who: Who::You, text: "On PostgreSQL.".into(), language: None, append: true })
        .unwrap();
    harness.run();
    harness.get_by_label("Transcript").click();
    harness.run();
    harness.get_by_label("An order API in FastAPI. On PostgreSQL.");
    shot(&mut harness, "live-transcript-en");
}

#[test]
fn the_start_button_opens_a_call_on_the_real_engine_with_scripted_services() {
    let reply = careertwin_prep::testing::Reply::text("Hi Alex, I'm Daniel. What are you working on at the moment?");
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply]), 1120.0, 780.0);
    harness.run();
    harness.get_by_label("Mock interview").click();
    harness.run();
    harness.get_by_label("Start call").click();
    harness.run();
    assert!(matches!(harness.state().screen, Screen::Live(_)));
    // the interviewer's words arrive from the engine's own thread
    let mut seen = false;
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        harness.run();
        if harness.query_by_label_contains("What are you working on").is_some() {
            seen = true;
            break;
        }
    }
    assert!(seen, "the interviewer never spoke");
}

const REVIEW: &str = r#"{"summary": "A solid start: you named the cause and the fix. Add numbers and say what you would watch next time.",
 "scores": [{"name": "Accuracy", "value": 4, "note": "the cause was right"}, {"name": "Depth", "value": 3, "note": "no numbers"},
            {"name": "Structure", "value": 3, "note": ""}, {"name": "Communication", "value": 4, "note": ""}, {"name": "English", "value": 3, "note": "articles"}],
 "answers": [{"turn": 1, "good": ["You named the cause and the fix."], "issues": ["No p95 number."], "better_answer": "After the release p95 doubled. I traced it to a query on customer_id without an index, added the index and an alert, and latency was back within the hour.", "missed_points": ["Why an alert matters."]}],
 "language_notes": [{"said": "I have added a index", "better": "I added an index", "why": "Use the past simple, and an before a vowel sound."}],
 "priorities": ["Give one number for every story.", "Say what you would monitor.", "Use the past simple for finished events."],
 "phrases": [{"english": "p95 latency doubled", "meaning": "p95 задержка выросла вдвое"}, {"english": "traced it to a slow query", "meaning": "нашёл причину в медленном запросе"}]}"#;

fn finished_call(harness: &mut Harness<'_, PrepApp>) {
    let wired = live(harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Turn {
            who: Who::Interviewer,
            text: "What happened after the release?".into(),
            language: None,
            append: false,
        })
        .unwrap();
    wired
        .events
        .send(Event::Turn {
            who: Who::You,
            text: "The latency was doubled, I have added a index.".into(),
            language: Some("en".into()),
            append: false,
        })
        .unwrap();
    harness.run();
    harness.get_by_label("End call").click();
}

fn wait_for_label(harness: &mut Harness<'_, PrepApp>, text: &str) -> bool {
    for _ in 0..300 {
        harness.step(); // a spinner keeps repainting, so `run` would never settle
        if harness.query_by_label_contains(text).is_some() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[test]
fn after_the_call_the_review_is_made_and_shown() {
    let reply = careertwin_prep::testing::Reply { pieces: vec![REVIEW.to_owned()], ..Default::default() }.slow(600);
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply]), 1120.0, 1500.0);
    finished_call(&mut harness);
    harness.run_steps(3);
    harness.get_by_label("Preparing your review…");
    assert!(wait_for_label(&mut harness, "A solid start"), "the review never arrived");
    harness.get_by_label("Accuracy");
    harness.get_by_label_contains("I added an index");
    harness.get_by_label_contains("The latency was doubled");
    harness.get_by_label("Give one number for every story.");
    shot(&mut harness, "review-en");
    // and it was kept with the session
    let sessions = harness.state().core.store.list();
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].feedback.is_some());
    assert_eq!(sessions[0].status, careertwin_prep::session::Status::Done);
}

#[test]
fn a_failed_review_says_why_and_can_be_tried_again() {
    let replies = vec![
        careertwin_prep::testing::Reply::failing(careertwin_prep::llm::LlmError::Unreachable(
            "No AI gateway answers".into(),
        )),
        careertwin_prep::testing::Reply { pieces: vec![REVIEW.to_owned()], ..Default::default() },
    ];
    let (mut harness, _d, _r) = harness(fixture_with("en", replies), 1120.0, 900.0);
    finished_call(&mut harness);
    assert!(wait_for_label(&mut harness, "The review could not be made"));
    harness.get_by_label_contains("No AI gateway answers");
    harness.get_by_label("Try again").click();
    harness.run_steps(2);
    assert!(wait_for_label(&mut harness, "A solid start"));
}

#[test]
fn a_phrase_goes_into_the_review_deck_once() {
    let reply = careertwin_prep::testing::Reply { pieces: vec![REVIEW.to_owned()], ..Default::default() };
    // tall enough that the phrases are on screen: a button below the fold cannot be clicked
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply]), 1120.0, 2000.0);
    finished_call(&mut harness);
    assert!(wait_for_label(&mut harness, "A solid start"));
    let buttons = harness.get_all_by_label("Add to review").count();
    assert_eq!(buttons, 2);
    harness.get_all_by_label("Add to review").next().unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.get_all_by_label("Add to review").count(), 1);
    harness.get_by_label("Added");
    let deck = &harness.state().core.deck;
    assert_eq!(deck.cards.len(), 1);
    assert_eq!(deck.cards[0].front, "p95 latency doubled");
    assert!(
        careertwin_prep::srs::Deck::load(&harness.state().core.paths.deck_file()).cards.len() == 1,
        "and it was saved"
    );
}

#[test]
fn saying_the_better_answer_yourself_shows_which_words_went_missing() {
    let reply = careertwin_prep::testing::Reply { pieces: vec![REVIEW.to_owned()], ..Default::default() };
    let (talker, frames) = careertwin_prep::testing::Talker::new();
    let heard = "After the release p95 doubled. I traced it to a query without an index and added the index and latency was back within the hour.";
    let options = Options { replies: vec![reply], heard: vec![heard], mic: None };
    let mut fixture = fixture_full("en", options);
    // the microphone for the drill: hand the receiver to the services after the call has ended
    let slot = std::sync::Arc::new(std::sync::Mutex::new(Some(frames)));
    let taking = slot.clone();
    fixture.app.core.services.mic = Arc::new(move |_| match taking.lock().unwrap().take() {
        Some(frames) => Ok((frames, Box::new(()) as Box<dyn std::any::Any + Send>)),
        None => Err(anyhow::anyhow!("no microphone")),
    });
    let (mut harness, _d, runtime) = harness(fixture, 1120.0, 1500.0);
    finished_call(&mut harness);
    assert!(wait_for_label(&mut harness, "A solid start"));
    harness.get_by_label("Say it yourself").click();
    harness.run_steps(3);
    runtime.spawn(async move {
        talker.silence(300).await;
        talker.speak(1500).await;
        talker.silence(1500).await;
    });
    assert!(wait_for_label(&mut harness, "of the words matched"), "the drill never finished");
    harness.get_by_label_contains("Missed:");
    shot(&mut harness, "review-drill-en");
}

fn reply(text: &str) -> careertwin_prep::testing::Reply {
    careertwin_prep::testing::Reply { pieces: vec![text.to_owned()], ..Default::default() }
}

const CARDS: &str = r#"{"cards": [
 {"question": "A query on orders(customer_id) got slow after the table grew. What is the most likely fix?", "options": ["Add an index on customer_id", "Add more RAM to the web server", "Cache the page in the browser"], "answer_index": 0, "explanation": "Without an index the planner scans the whole table; a btree index lets it seek the matching rows.", "topic": "SQL and indexes", "difficulty": 2},
 {"question": "Why does an async endpoint stop scaling when it calls a blocking library?", "options": ["The event loop thread is blocked", "Python runs out of memory", "The database refuses connections"], "answer_index": 0, "explanation": "One blocked coroutine blocks the loop, so every other request waits.", "topic": "Async", "difficulty": 2}]}"#;

fn click_right_option(harness: &mut Harness<'_, PrepApp>, option: &str) {
    harness.get_by_label(option).click();
    harness.run_steps(2);
}

#[test]
fn a_concept_check_asks_how_sure_you_are_and_what_you_get_wrong_comes_back() {
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply(CARDS)]), 1120.0, 1500.0);
    harness.run();
    harness.get_by_label("Concept checks").click();
    harness.run_steps(2);
    harness.get_by_label("Make questions").click();
    assert!(wait_for_label(&mut harness, "most likely fix"), "no questions arrived");
    shot(&mut harness, "concepts-question-en");
    // the right answer is somewhere among the options
    let right = "Add an index on customer_id";
    click_right_option(&mut harness, right);
    harness.get_by_label("Submit answer").click();
    harness.run_steps(3);
    harness.get_by_label("Correct");
    harness.get_by_label_contains("Your confidence matched");
    shot(&mut harness, "concepts-answered-en");
    harness.get_by_label("Next").click();
    harness.run_steps(3);
    // the second one: sure and wrong
    harness.get_by_label("Python runs out of memory").click();
    harness.run_steps(2);
    {
        let Screen::Concepts(view) = &mut harness.state_mut().screen else { panic!("not on the concept checks") };
        view.confidence = 90.0;
    }
    harness.get_by_label("Submit answer").click();
    harness.run_steps(3);
    harness.get_by_label("Not quite");
    harness.get_by_label_contains("You were sure and wrong");
    harness.get_by_label("Finish").click();
    harness.run_steps(3);
    harness.get_by_label("1 of 2 right");
    harness.get_by_label_contains("Calibration");
    // the wrong answer was put into the review deck, due at once
    let deck = &harness.state().core.deck;
    assert_eq!(deck.cards.len(), 1);
    assert!(deck.cards[0].front.contains("blocking library"));
    assert!(deck.cards[0].back.contains("event loop"));
    assert!(deck.cards[0].is_due(careertwin_prep::session::now()));
    shot(&mut harness, "concepts-done-en");
}

const TRANSLATION: &str = r#"{"detected": "ru", "natural": "After the release the latency doubled, so I added an index.", "formal": "Following the release, latency doubled, and I resolved it by adding an index.", "short": "Latency doubled; I added an index.", "notes": ["'latency' is uncountable: no 'a latency'."]}"#;

fn type_into(harness: &mut Harness<'_, PrepApp>, text: &str) {
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
    harness.run_steps(2);
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput).type_text(text);
    harness.run_steps(2);
}

#[test]
fn a_thought_in_russian_comes_back_in_three_english_versions() {
    let (mut harness, _d, _r) = harness(fixture_with("ru", vec![reply(TRANSLATION)]), 1120.0, 1200.0);
    harness.run();
    harness.get_by_label("Английский").click();
    harness.run_steps(2);
    harness.get_by_label("Скажи по-английски").click();
    harness.run_steps(2);
    type_into(&mut harness, "После релиза задержка выросла вдвое, и я добавил индекс.");
    harness.get_by_label("Сказать по-английски").click();
    assert!(wait_for_label(&mut harness, "Latency doubled; I added an index."), "no translation");
    // the names of the three versions are small capitals
    harness.get_by_label("ЕСТЕСТВЕННО");
    harness.get_by_label("ОФИЦИАЛЬНО");
    harness.get_by_label("КОРОТКО");
    shot(&mut harness, "translate-ru");
    // it is kept with the sessions
    let kept = harness.state().core.store.list();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].kind, Kind::Translate);
    harness.get_all_by_label("В повторение").next().unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.state().core.deck.cards.len(), 1);
    assert!(harness.state().core.deck.cards[0].back.contains("После релиза"));
}

const ANALYSIS: &str = r#"{"summary": "The CV fits the backend work well; Kubernetes is the one real gap.", "strengths": ["Python, FastAPI and PostgreSQL in production"], "gaps": [{"gap": "Kubernetes", "how_to_address": "Say it was a university lab and name what you built."}], "likely_questions": ["How have you used Kubernetes?"], "cv_fixes": ["Move the latency story to the top."]}"#;
const QUESTIONS: &str = r#"{"questions": [{"question": "Tell me about a production incident you fixed.", "kind": "behavioral", "outline": ["After a release p95 doubled.", "I traced it to a missing index."]}]}"#;

#[test]
fn a_vacancy_is_compared_with_the_cv_and_a_question_can_be_practised() {
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply(ANALYSIS), reply(QUESTIONS)]), 1120.0, 1800.0);
    harness.run();
    harness.get_by_label("Prepare for a vacancy").click();
    harness.run_steps(2);
    {
        let core = &mut harness.state_mut().core;
        core.vacancy.title = "Backend Engineer".into();
        core.vacancy.company = "Acme".into();
        core.vacancy.description = "Python, FastAPI, PostgreSQL and some Kubernetes.".into();
    }
    harness.run_steps(2);
    harness.get_by_label("Compare my CV").click();
    assert!(wait_for_label(&mut harness, "Kubernetes is the one real gap"), "no analysis");
    harness.get_by_label("Likely questions").click();
    assert!(wait_for_label(&mut harness, "Tell me about a production incident"), "no questions");
    shot(&mut harness, "prepare-en");
    harness.get_by_label("Practise this").click();
    harness.run_steps(3);
    let Screen::Setup(view) = &harness.state().screen else { panic!("not on the call setup") };
    assert!(view.spec.focus.contains("Tell me about a production incident"));
}

#[test]
fn the_vacancy_is_kept_between_runs_and_reaches_the_interviewer() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    {
        let core = &mut harness.state_mut().core;
        core.vacancy.title = "Backend Engineer".into();
        core.vacancy.company = "Acme".into();
        core.vacancy.description = "Python and PostgreSQL.".into();
        core.save_vacancy();
        assert!(core.working_brief().render().contains("VACANCY: Backend Engineer at Acme"));
    }
    let loaded = careertwin_prep::ui::Vacancy::load(&harness.state().core.paths);
    assert_eq!(loaded.company, "Acme");
}

#[test]
fn the_cards_that_are_due_come_back_and_can_be_graded() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    {
        let core = &mut harness.state_mut().core;
        core.track = Track::English;
        core.deck.add(careertwin_prep::srs::Card::new(
            careertwin_prep::srs::CardKind::Phrase,
            "I'd like to walk you through it",
            "Я хотел бы показать вам",
            "",
            0,
        ));
    }
    harness.run();
    harness.get_by_label_contains("1 to review today");
    harness.get_by_label("Phrases to review").click();
    harness.run_steps(2);
    harness.get_by_label("I'd like to walk you through it");
    assert!(harness.query_by_label("Good").is_none(), "the answer is hidden until asked for");
    harness.get_by_label("Show the answer").click();
    harness.run_steps(2);
    harness.get_by_label("Я хотел бы показать вам");
    harness.get_by_label("Good").click();
    harness.run_steps(3);
    let card = &harness.state().core.deck.cards[0];
    assert_eq!(card.reps, 1);
    assert!(!card.is_due(careertwin_prep::session::now()), "a good answer pushes it into the future");
    harness.get_by_label("Nothing here yet.");
}

#[test]
fn the_history_lists_sessions_filters_opens_and_deletes_them() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    {
        let store = &harness.state().core.store;
        let mut live = Session::new(Track::Technical, Kind::Live, "Mock interview · Daniel · Tech lead");
        live.add_turn(Who::Interviewer, "What do you work on?", None);
        live.add_turn(Who::You, "An order API.", None);
        live.finish();
        store.save(&live).unwrap();
        store.save(&Session::new(Track::English, Kind::Live, "Conversation · Olivia")).unwrap();
    }
    harness.run();
    harness.get_by_label("SESSIONS").click();
    harness.run_steps(3);
    // the list is made of buttons; the start page also names the technical session under "Recent"
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Mock interview · Daniel · Tech lead");
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Conversation · Olivia");
    shot(&mut harness, "history-en");
    // typing in the search narrows the list
    harness.get_by_role(egui::accesskit::Role::TextInput).focus();
    harness.run_steps(2);
    harness.get_by_role(egui::accesskit::Role::TextInput).type_text("olivia");
    harness.run_steps(3);
    assert!(
        harness.query_by_role_and_label(egui::accesskit::Role::Button, "Mock interview · Daniel · Tech lead").is_none()
    );
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Conversation · Olivia");
    // delete it
    harness.get_by_label("Delete Conversation · Olivia").click();
    harness.run_steps(3);
    assert_eq!(harness.state().core.store.list().len(), 1);
    // opening the other one shows its review and closes the list
    harness.get_by_role(egui::accesskit::Role::TextInput).focus();
    harness.run_steps(1);
    {
        harness.state_mut().history.query.clear();
    }
    harness.run_steps(3);
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Mock interview · Daniel · Tech lead").click();
    harness.run_steps(3);
    assert!(matches!(harness.state().screen, Screen::Review(_)));
    assert!(!harness.state().core.history_open);
}

#[test]
fn settings_are_checked_saved_and_applied() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 1000.0);
    harness.run();
    harness.get_by_label("Settings").click();
    harness.run_steps(3);
    harness.get_by_label("Language model");
    harness.get_by_label("Speech recognition");
    harness.get_by_label("Interviewer's voice");
    shot(&mut harness, "settings-en");
    // an address that would send a key in the clear is refused with the reason
    {
        let draft = harness_draft(&mut harness);
        draft.settings.llm.url = "http://example.com".into();
    }
    harness.get_by_label("Save").click();
    harness.run_steps(3);
    harness.get_by_label_contains("plain http");
    assert!(harness.state().core.settings_open, "it stays open to be corrected");
    // a good one is saved, applied and written to a private file
    {
        let draft = harness_draft(&mut harness);
        draft.settings.llm.url = "http://127.0.0.1:3260".into();
        draft.settings.ui_language = careertwin_prep::config::UiLanguage::Ru;
        draft.llm_key = "sk-test-1234567890".into();
    }
    harness.get_by_label("Save").click();
    harness.run_steps(3);
    assert!(!harness.state().core.settings_open);
    let saved = careertwin_prep::config::Settings::load(&harness.state().core.paths);
    assert_eq!(saved.llm.key.expose(), "sk-test-1234567890");
    assert_eq!(saved.ui_language, careertwin_prep::config::UiLanguage::Ru);
    harness.run_steps(2);
    harness.get_by_label("Пробное собеседование");
}

fn harness_draft<'a>(harness: &'a mut Harness<'_, PrepApp>) -> &'a mut careertwin_prep::ui::settings::Draft {
    harness.state_mut().draft.as_mut().expect("the settings are open")
}

#[test]
fn the_other_tabs_have_their_own_cards() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.run();
    harness.get_by_label("English").click();
    harness.run_steps(2);
    harness.get_by_label("English practice");
    harness.get_by_label("Conversation");
    harness.get_by_label("Say it in English");
    harness.get_by_label("Phrases to review");
    shot(&mut harness, "home-english-en");
    harness.get_by_label("Interview").click();
    harness.run_steps(2);
    harness.get_by_label("Job interview");
    harness.get_by_label("Mock interview");
    harness.get_by_label("Prepare for a vacancy");
    harness.get_by_label("Technical").click();
    harness.run_steps(2);
    harness.get_by_label("Technical interview");
    harness.get_by_label("Concept checks");
}

#[test]
fn the_whole_interface_speaks_russian() {
    let (mut harness, _d, _r) = harness(fixture("ru"), 1120.0, 900.0);
    harness.run();
    harness.get_by_label("Английский");
    harness.get_by_label("Техническое");
    harness.get_by_label("Собеседование");
    harness.get_by_label("Техническое собеседование");
    harness.get_by_label("Пробное собеседование");
    harness.get_by_label("Проверка понимания");
    shot(&mut harness, "home-technical-ru");
    harness.get_by_label("Пробное собеседование").click();
    harness.run_steps(2);
    harness.get_by_label("Начать звонок");
    harness.get_by_label("ИНТЕРВЬЮЕР");
    shot(&mut harness, "setup-ru");
}

#[test]
fn without_a_profile_the_technical_tab_says_so_and_will_not_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = fixture("en");
    fixture.app.core.brief = None;
    let _ = dir;
    let (mut harness, _d, _r) = harness(fixture, 1120.0, 900.0);
    harness.run();
    harness.get_by_label_contains("No profile found");
    harness.get_by_label("Mock interview").click();
    harness.run_steps(2);
    harness.get_by_label("Start call").click();
    harness.run_steps(2);
    assert!(matches!(harness.state().screen, Screen::Setup(_)), "the call does not start without a profile");
}

#[test]
fn settings_say_which_models_fit_this_computer_and_can_apply_the_interviewer_suggestion() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 1000.0);
    harness.run();
    harness.get_by_label("Settings").click();
    harness.run_steps(3);
    harness.get_by_label("Offline on this computer");
    harness.get_by_label_contains("GB of memory");
    harness.get_by_label_contains("Hearing: Whisper");
    harness.get_by_label_contains("ollama pull");
    if !cfg!(feature = "local-stt") && !cfg!(feature = "local-tts") {
        harness.get_by_label_contains("This build has no offline engines");
    }
    let plan = careertwin_prep::models::recommend(&careertwin_prep::models::Hardware::detect());
    // the section is far down the scrolling list of settings
    harness.get_by_label(&format!("Use {}", plan.llm.tag)).scroll_to_me();
    harness.run_steps(3);
    harness.get_by_label(&format!("Use {}", plan.llm.tag)).click();
    harness.run_steps(2);
    let draft = harness_draft(&mut harness);
    assert_eq!(draft.settings.llm.model, plan.llm.tag);
    assert_eq!(draft.settings.llm.url, "http://127.0.0.1:11434");
    shot(&mut harness, "settings-offline-en");
}

#[test]
fn a_model_being_downloaded_is_shown_and_the_call_waits_for_it() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    let progress = std::sync::Arc::new(careertwin_prep::speech::download::Progress::default());
    progress.total.store(200, std::sync::atomic::Ordering::Relaxed);
    progress.done.store(84, std::sync::atomic::Ordering::Relaxed);
    harness.state_mut().core.downloads.push(careertwin_prep::ui::Download {
        name: "Whisper small".into(),
        progress,
        failed: None,
        finished: false,
    });
    harness.run();
    harness.get_by_label("Downloading Whisper small… 42%");
    harness.get_by_label("Mock interview").click();
    harness.run_steps(2);
    harness.get_by_label("The offline models are still downloading.");
    harness.get_by_label("Start call").click();
    harness.run_steps(2);
    assert!(
        matches!(harness.state().screen, Screen::Setup(_)),
        "the call does not start while its models are on the way"
    );
    // once the download is done the call can start
    harness.state_mut().core.downloads[0].finished = true;
    harness.run_steps(2);
    harness.get_by_label("Start call").click();
    harness.run_steps(2);
    assert!(matches!(harness.state().screen, Screen::Live(_)));
}

#[test]
fn a_failed_download_is_explained_in_settings() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 1000.0);
    let progress = std::sync::Arc::new(careertwin_prep::speech::download::Progress::default());
    harness.state_mut().core.downloads.push(careertwin_prep::ui::Download {
        name: "Kokoro".into(),
        progress,
        failed: Some("could not reach huggingface.co".into()),
        finished: false,
    });
    harness.run();
    harness.get_by_label("Settings").click();
    harness.run_steps(3);
    harness.get_by_label_contains("Could not download Kokoro: could not reach huggingface.co");
}

#[test]
fn the_sphere_is_not_redrawn_on_every_frame() {
    // a sphere that only breathes must be cheap: thirty frames in a row make one picture, not thirty
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    for _ in 0..30 {
        harness.step();
    }
    let renders = harness.state().core.orb_renders;
    assert!(renders >= 1, "the sphere is on the home page");
    assert!(renders <= 3, "{renders} pictures for thirty frames: the processor would be doing the work for nothing");
}

#[test]
fn a_call_redraws_the_sphere_a_few_times_a_second_not_every_frame() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    for _ in 0..40 {
        harness.step();
    }
    let renders = harness.state().core.orb_renders;
    assert!((1..=4).contains(&renders), "{renders} pictures for forty frames");
}
