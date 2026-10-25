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
    // the real program starts the timer by itself; most tests want to start it themselves
    settings.pomodoro.auto_start = false;
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
        // the fonts of the machine come in only when screenshots are taken, so the tests are the same everywhere
        .with_context_setup(|ctx| {
            theme::install_with(ctx, std::env::var("UI_SHOTS").ok().and_then(|_| theme::find_display_font()))
        })
        .build_ui_state(|ui, app: &mut PrepApp| app.show(ui), app);
    (harness, _dir, _runtime)
}

/// Settings are opened from the menu over the panel at the bottom left.
pub fn open_settings(harness: &mut Harness<'_, PrepApp>) {
    harness.get_by_label("Account menu").click();
    harness.run_steps(2);
    harness.get_by_label("Settings").click();
    harness.run_steps(2);
}

/// The ways to practise are in the drawer that the first button at the top left opens.
pub fn open_ways(harness: &mut Harness<'_, PrepApp>) {
    if !harness.state().core.history_open {
        let label = harness.state().core.tr.get(careertwin_prep::i18n::K::Sessions);
        harness.get_by_label(label).click();
        harness.run_steps(3);
    }
}

/// Opens one of the screens as a click on it would. They are not listed anywhere in the window any
/// more, because the tutor leads and opens the one it needs; the table of them is still there.
pub fn click_card(harness: &mut Harness<'_, PrepApp>, label: &str) {
    let tr = harness.state().core.tr;
    let nav = [Track::English, Track::Technical, Track::Interview]
        .into_iter()
        .flat_map(careertwin_prep::ui::home::ways)
        .find(|card| tr.get(card.title) == label)
        .unwrap_or_else(|| panic!("there is no screen called {label}"))
        .go;
    harness.state_mut().navigate(nav);
    harness.run_steps(3);
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
    shot(&mut harness, "home-technical-en");
    // the menu at the left lists what you did last and nothing else: the tutor leads
    open_ways(&mut harness);
    harness.get_by_label("RECENT");
    for way in ["Mock interview", "Live coding", "Your plan", "Explain with pictures", "Write to the tutor"] {
        assert!(harness.query_by_label(way).is_none(), "{way} is not listed in the menu");
    }
    shot(&mut harness, "ways-en");
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
fn in_a_call_there_is_no_panel_at_the_bottom_and_no_timer() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    harness.run();
    harness.get_by_label("Account menu");
    harness.get_by_label("Start the timer");
    let _wired = live(&mut harness);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.has_microphone = true;
    }
    harness.run_steps(3);
    assert!(harness.query_by_label("Account menu").is_none(), "the panel is not shown in a call");
    assert!(harness.query_by_label("Start the timer").is_none(), "nor is the timer");
    harness.get_by_label("Mute");
    harness.get_by_label("End call");
}

#[test]
fn a_click_outside_your_activity_closes_it_but_the_click_that_opened_it_does_not() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.run();
    harness.get_by_label("0 of 25 min today").click();
    harness.run_steps(3);
    assert!(harness.state().core.usage_open, "the click that opened it did not close it");
    // a click on empty ground, far from the window
    let away = egui::pos2(60.0, 500.0);
    harness.hover_at(away);
    harness.run_steps(1);
    harness.event(egui::Event::PointerButton {
        pos: away,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: away,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(3);
    assert!(!harness.state().core.usage_open, "a click anywhere else closes it");
}

#[test]
fn a_call_shows_what_is_said_and_who_is_speaking() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let wired = live(&mut harness);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.has_microphone = true;
    }
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
    // nothing else is on the screen: no clock, no LIVE, no transcript, no level bars, no second line
    for junk in ["Transcript", "Skip", "Mute the tutor", "Unmute the tutor"] {
        assert!(harness.query_by_label(junk).is_none(), "{junk} should not be on a call");
    }
    assert!(harness.query_by_label_contains("LIVE").is_none() && harness.query_by_label_contains("YOU:").is_none());
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
fn the_start_button_opens_a_call_on_the_real_engine_with_scripted_services() {
    let reply = careertwin_prep::testing::Reply::text("Hi Alex, I'm Daniel. What are you working on at the moment?");
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply]), 1120.0, 780.0);
    harness.run();
    click_card(&mut harness, "Mock interview");
    harness.run();
    harness.get_by_label("Start call").scroll_to_me();
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
    click_card(&mut harness, "Concept checks");
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
    click_card(&mut harness, "Скажи по-английски");
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
    click_card(&mut harness, "Prepare for a vacancy");
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
    click_card(&mut harness, "Phrases to review");
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
    open_ways(&mut harness);
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
    harness.get_by_label("Delete Conversation · Olivia").scroll_to_me();
    harness.run_steps(12);
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
    let session = "Mock interview · Daniel · Tech lead";
    harness.get_by_role_and_label(egui::accesskit::Role::Button, session).scroll_to_me();
    harness.run_steps(12);
    harness.get_by_role_and_label(egui::accesskit::Role::Button, session).click();
    harness.run_steps(3);
    assert!(matches!(harness.state().screen, Screen::Review(_)));
    assert!(!harness.state().core.history_open);
}

#[test]
fn settings_are_checked_saved_and_applied() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 1000.0);
    harness.run();
    open_settings(&mut harness);
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
    harness.get_by_label("Техническое собеседование");
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
    // there is no grid of cards on the page, and no list of them anywhere: the tutor leads
    assert!(harness.query_by_label("Conversation").is_none());
    shot(&mut harness, "home-english-en");
    harness.get_by_label("Interview").click();
    harness.run_steps(2);
    harness.get_by_label("Job interview");
    harness.get_by_label("Technical").click();
    harness.run_steps(2);
    harness.get_by_label("Technical interview");
    assert!(harness.query_by_label("Concept checks").is_none());
}

#[test]
fn the_whole_interface_speaks_russian() {
    let (mut harness, _d, _r) = harness(fixture("ru"), 1120.0, 900.0);
    harness.run();
    harness.get_by_label("Английский");
    harness.get_by_label("Техническое");
    harness.get_by_label("Собеседование");
    harness.get_by_label("Техническое собеседование");
    shot(&mut harness, "home-technical-ru");
    click_card(&mut harness, "Пробное собеседование");
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
    click_card(&mut harness, "Mock interview");
    harness.run_steps(2);
    harness.get_by_label("Start call").click();
    harness.run_steps(2);
    assert!(matches!(harness.state().screen, Screen::Setup(_)), "the call does not start without a profile");
}

#[test]
fn settings_say_which_models_fit_this_computer_and_can_apply_the_interviewer_suggestion() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 1000.0);
    harness.run();
    open_settings(&mut harness);
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
    click_card(&mut harness, "Mock interview");
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
    open_settings(&mut harness);
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

#[test]
fn the_panel_at_the_bottom_opens_a_menu_without_plans_or_downloads() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    harness.run();
    assert!(harness.query_by_label("Your activity").is_none(), "the menu is closed at first");
    harness.get_by_label("Account menu").click();
    harness.run_steps(2);
    // the address comes from the ABOUT.md of the demo profile
    harness.get_by_label("alex@example.com");
    for item in ["Settings", "Your activity", "Language", "Help", "Quit"] {
        harness.get_by_label(item);
    }
    for gone in ["Upgrade", "Download", "More options", "Learn more"] {
        assert!(harness.query_by_label_contains(gone).is_none(), "{gone} does not belong in a free program");
    }
    shot(&mut harness, "account-menu-en");
    harness.get_by_label("Quit").click();
    harness.run_steps(2);
    assert!(harness.state().core.quit, "quit asks the window to close");
}

#[test]
fn the_menu_speaks_russian_and_changes_the_language() {
    let (mut harness, _d, _r) = harness(fixture("ru"), 1120.0, 780.0);
    harness.run();
    harness.get_by_label("Меню профиля").click();
    harness.run_steps(2);
    for item in ["Настройки", "Ваша активность", "Язык", "Помощь", "Выйти"] {
        harness.get_by_label(item);
    }
    harness.get_by_label("Язык").click();
    harness.run_steps(2);
    harness.get_by_label("English").click();
    harness.run_steps(3);
    harness.get_by_label("Technical");
    assert!(!harness.state().core.tr.is_russian());
}

#[test]
fn the_progress_in_the_bar_opens_your_activity_and_the_home_page_has_no_numbers() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.run();
    assert!(harness.query_by_label("Practice calendar").is_none(), "the calendar is not on the home page");
    assert!(harness.query_by_label("Day streak").is_none());
    harness.get_by_label("0 of 25 min today").click();
    harness.run_steps(3);
    harness.get_by_label("Day streak");
    harness.get_by_label("Practice calendar");
}

#[test]
fn usage_and_help_open_from_the_menu_and_close_again() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.state_mut().core.stats.add_practice(careertwin_prep::session::now(), 12 * 60, 140);
    harness.run();
    harness.get_by_label("Account menu").click();
    harness.run_steps(2);
    harness.get_by_label("Your activity").click();
    harness.run();
    harness.get_by_label("Day streak");
    assert_eq!(harness.query_all_by_label("Practice calendar").count(), 1, "the calendar lives in the window only");
    assert!(harness.query_all_by_label("12 min").count() >= 1, "today and the total both show the twelve minutes");
    shot(&mut harness, "usage-en");
    harness.get_by_label("Close").click();
    harness.run_steps(3);
    assert!(harness.query_by_label("Day streak").is_none());
    harness.get_by_label("Account menu").click();
    harness.run_steps(2);
    harness.get_by_label("Help").click();
    harness.run_steps(3);
    harness.get_by_label("KEYS");
    harness.get_by_label("Ctrl+Shift+,");
    shot(&mut harness, "help-en");
}

#[test]
fn the_shortcut_opens_settings_from_anywhere() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Comma);
    harness.run_steps(3);
    harness.get_by_label("Language model");
    // and about you is in them
    harness.get_by_label("YOUR NAME");
    harness.get_by_label("Focus timer and goal");
}

#[test]
fn the_focus_timer_starts_pauses_and_skips() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    harness.run();
    assert!(!harness.state().core.pomodoro.running());
    harness.get_by_label("Start the timer").click();
    harness.run_steps(2);
    assert!(harness.state().core.pomodoro.running());
    harness.get_by_label("Pause the timer");
    shot(&mut harness, "timer-running-en");
    harness.get_by_label("Skip to the next phase").click();
    harness.run_steps(2);
    assert_eq!(harness.state().core.pomodoro.phase(), careertwin_prep::pomodoro::Phase::ShortBreak);
    assert_eq!(harness.state().core.pomodoro.blocks, 0, "a skipped block is not counted");
    harness.get_by_label("Pause the timer").click();
    harness.run_steps(2);
    assert!(!harness.state().core.pomodoro.running());
}

#[test]
fn a_finished_block_is_counted_announced_and_chimed() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    harness.run();
    harness
        .state_mut()
        .core
        .on_timer(careertwin_prep::pomodoro::Signal::FocusDone { next: careertwin_prep::pomodoro::Phase::ShortBreak });
    harness.run_steps(2);
    harness.get_by_label_contains("Focus block done. Rest for 5 minutes");
    assert_eq!(harness.state().core.stats.today(careertwin_prep::session::now()).focus_blocks, 1);
    harness.get_by_label("Dismiss").click();
    harness.run_steps(2);
    assert!(harness.query_by_label_contains("Focus block done").is_none());
}

#[test]
fn a_call_ending_is_counted_in_the_practice_log() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Turn {
            who: Who::You,
            text: "I build the order API in Python.".into(),
            language: None,
            append: false,
        })
        .unwrap();
    harness.run();
    harness.get_by_label("End call").click();
    harness.run_steps(3);
    let today = harness.state().core.stats.today(careertwin_prep::session::now());
    assert_eq!((today.sessions, today.words), (1, 7));
}

const MAP: &str = r#"{"role": "Backend Engineer", "summary": "A talk about the web and data.", "topics": [
  {"name": "OOP", "kind": "concept", "why": "Every back end is object oriented.", "likelihood": 9, "effort": 2},
  {"name": "REST", "kind": "concept", "why": "The vacancy builds APIs.", "likelihood": 8, "effort": 3},
  {"name": "SQL", "kind": "concept", "why": "PostgreSQL is named.", "likelihood": 7, "effort": 4},
  {"name": "Kubernetes", "kind": "framework", "why": "Mentioned once.", "likelihood": 2, "effort": 9}]}"#;
const TERMS: &str = r#"{"terms": [{"name": "Encapsulation", "plain": "The object keeps its insides to itself.",
  "analogy": "A coffee machine: you press a button, you do not touch the boiler.",
  "english": "Encapsulation means an object hides its state and exposes only what is needed.",
  "follow_ups": ["Why is it useful?"], "visual": "A box with a hidden core."}]}"#;

fn wait_until(harness: &mut Harness<'_, PrepApp>, what: &str, done: impl Fn(&Harness<'_, PrepApp>) -> bool) {
    for _ in 0..600 {
        // a spinner asks for frames for as long as a job runs, so the window is stepped, not settled
        harness.run_steps(2);
        if done(harness) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!(
        "never happened: {what} (notice: {:?}, researching: {})",
        harness.state().core.notice,
        harness.state().core.research.is_some()
    );
}

#[test]
fn the_plan_is_researched_from_the_vacancy_and_the_few_that_matter_come_first() {
    use careertwin_prep::testing::Reply;
    let replies = vec![Reply::text(MAP), Reply::text(TERMS), Reply::text(TERMS), Reply::text(TERMS)];
    let (mut harness, _d, _r) = harness(fixture_with("en", replies), 1120.0, 1000.0);
    harness.state_mut().core.vacancy.description = "Python, FastAPI and PostgreSQL for an order API.".into();
    harness.run();
    click_card(&mut harness, "Your plan");
    harness.run_steps(2);
    harness.get_by_label_contains("There is no plan yet");
    harness.get_by_label("Research the vacancy").click();
    wait_until(&mut harness, "the plan appears", |h| h.state().core.syllabus.is_some());
    harness.run_steps(2);
    harness.get_by_label("0% of the likely questions covered");
    harness.get_by_label("THE 20% THAT MATTERS");
    harness.get_by_label("LATER");
    assert!(harness.query_all_by_label("OOP").count() >= 1);
    harness.get_by_label("Kubernetes");
    harness.get_by_label_contains("3 of 4 topics hold 80% of the questions");
    shot(&mut harness, "plan-en");
    // the name appears in "start here" and on its card; the card is the one to open
    harness.get_all_by_label("OOP").last().unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("Encapsulation");
    harness.get_by_label_contains("coffee machine");
    shot(&mut harness, "plan-open-en");
    // the plan is kept for the next run
    assert!(harness.state().core.paths.syllabus_file().exists());
}

#[test]
fn a_plan_made_for_another_vacancy_says_so() {
    use careertwin_prep::testing::Reply;
    let replies = vec![Reply::text(MAP), Reply::text(TERMS), Reply::text(TERMS), Reply::text(TERMS)];
    let (mut harness, _d, _r) = harness(fixture_with("en", replies), 1120.0, 1000.0);
    harness.state_mut().core.vacancy.description = "Python and PostgreSQL.".into();
    harness.state_mut().core.start_research();
    wait_until(&mut harness, "the plan appears", |h| h.state().core.syllabus.is_some());
    harness.state_mut().screen = Screen::Plan(Box::default());
    harness.run_steps(2);
    assert!(harness.query_by_label("The vacancy has changed since this plan was made.").is_none());
    harness.state_mut().core.vacancy.description = "Go and Kafka instead.".into();
    harness.run_steps(2);
    harness.get_by_label("The vacancy has changed since this plan was made.");
    harness.get_by_label("Research again");
}

#[test]
fn research_without_a_vacancy_is_refused_politely() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.state_mut().screen = Screen::Plan(Box::default());
    harness.run_steps(2);
    harness.get_by_label("Research the vacancy").click();
    harness.run_steps(2);
    assert!(harness.state().core.research.is_none(), "nothing starts without a vacancy text");
}

#[test]
fn notes_are_the_tutors_to_write_and_yours_to_star_and_delete() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.run();
    // there is no notebook button and no box to write a note in: the tutor takes the notes after a call
    assert!(harness.query_by_label("Notes").is_none(), "no notebook button in the top bar");
    harness.state_mut().screen = Screen::Notes(Box::default());
    harness.run_steps(3);
    harness.get_by_label("No notes yet. The tutor writes them after a call.");
    assert_eq!(harness.query_all_by_role(egui::accesskit::Role::TextInput).count(), 0, "nothing to type into");
    assert!(harness.query_by_label("Add").is_none());
    harness.state_mut().core.add_note(careertwin_prep::notes::NoteKind::Vital, "OOP", "Ask about the project defence.");
    harness.run_steps(3);
    harness.get_by_label("Ask about the project defence.");
    assert!(harness.state().core.paths.notes_file().exists());
    harness.get_by_label("Star this note").click();
    harness.run_steps(2);
    harness.get_by_label("Remove the star");
    assert!(harness.state().core.notebook.notes[0].starred);
    shot(&mut harness, "notes-en");
    harness.get_by_label("Delete this note").click();
    harness.run_steps(2);
    assert!(harness.state().core.notebook.notes.is_empty());
}

#[test]
fn after_a_call_the_review_is_made_and_then_the_notes_that_matter() {
    use careertwin_prep::testing::Reply;
    const DIGEST: &str = r#"{"vital": ["Add the index on the column you filter by.", "Give one number for every story."],
      "mistakes": ["Say 'I added an index', not 'I have added a index'."], "next": "Practise the second answer out loud.",
      "hook": "Two of the five answers are ready."}"#;
    let (mut harness, _d, _r) =
        harness(fixture_with("en", vec![Reply::text(REVIEW), Reply::text(DIGEST)]), 1120.0, 1000.0);
    let wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    for text in ["I added an index.", "p95 went back to normal."] {
        wired.events.send(Event::Turn { who: Who::You, text: text.into(), language: None, append: false }).unwrap();
    }
    harness.run();
    harness.get_by_label("End call").click();
    wait_until(&mut harness, "the notes appear", |h| h.state().core.notebook.notes.len() == 3);
    harness.run_steps(2);
    harness.get_by_label("THE 20% TO KEEP");
    harness.get_by_label("Add the index on the column you filter by.");
    harness.get_by_label_contains("Practise the second answer out loud.");
    shot(&mut harness, "review-notes-en");
    harness.get_by_label("Open notes").click();
    harness.run_steps(2);
    harness.get_by_label("Give one number for every story.");
}

// ---- the tutor ----

use careertwin_prep::prompts::Lang;
use careertwin_prep::prompts::tutor::{LessonView, Mode, Stage};
use careertwin_prep::syllabus::{Half, Syllabus, Term, Topic, TopicKind};

fn plan_of_two() -> Syllabus {
    let term = |name: &str| Term {
        name: name.into(),
        plain: format!("{name} in plain words."),
        analogy: "Like a shop.".into(),
        english: format!("{name} means something."),
        follow_ups: vec!["Why?".into()],
        visual: String::new(),
    };
    Syllabus {
        role: "Backend Engineer".into(),
        topics: vec![
            Topic {
                name: "OOP".into(),
                kind: TopicKind::Concept,
                likelihood: 9,
                effort: 2,
                terms: vec![term("Encapsulation"), term("Polymorphism")],
                ..Topic::default()
            },
            Topic {
                name: "REST".into(),
                likelihood: 8,
                effort: 3,
                terms: vec![term("Idempotency")],
                ..Topic::default()
            },
        ],
        native: Lang::En,
        ..Syllabus::default()
    }
}

/// The tutor ends the part that is on: you have answered, and it says goodbye.
fn the_tutor_ends_this_part(harness: &mut Harness<'_, PrepApp>) {
    let Screen::Live(view) = &mut harness.state_mut().screen else { panic!("a part should be on") };
    view.session.add_turn(Who::You, "an answer", None);
    view.finished = true;
    harness.run_steps(4);
}

#[test]
fn the_english_tab_makes_a_lesson_of_what_you_bring_and_starts_it() {
    use careertwin_prep::testing::Reply;
    let lesson = r#"{"topic": "Phrasal verbs", "terms": [{"name": "give up", "plain": "Сдаться, перестать пытаться.", "analogy": "Как бросить бег на полпути.", "english": "I will not give up.", "follow_ups": ["When did you last give up?"], "visual": "A runner stops."}, {"name": "look after", "plain": "Заботиться о ком-то.", "analogy": "Как няня.", "english": "I look after my cat.", "follow_ups": ["Who do you look after?"], "visual": "A person feeds a cat."}]}"#;
    let replies = vec![Reply::text(lesson), Reply::text("Hi Alex. In your own words, what does give up mean?")];
    let (mut harness, _d, _r) = harness(fixture_with("en", replies), 1120.0, 900.0);
    harness.run();
    harness.get_by_label("English").click();
    harness.run_steps(2);
    harness.get_by_label("Study something together").click();
    harness.run_steps(3);
    harness.get_by_label("Study together");
    // nothing to study yet, so nothing to start
    assert!(harness.query_by_label("Start the lesson").is_some());
    harness.get_all_by_role(egui::accesskit::Role::MultilineTextInput).next().unwrap().focus();
    harness.run();
    harness.get_all_by_role(egui::accesskit::Role::MultilineTextInput).next().unwrap().type_text("give up, look after");
    harness.run();
    harness.get_by_label("Start the lesson").click();
    wait_until(&mut harness, "the lesson starts", |h| matches!(h.state().screen, Screen::Live(_)));
    let Screen::Live(view) = &harness.state().screen else { panic!("the lesson should have started") };
    assert_eq!(view.spec.mode, Mode::Terms);
    let lesson = view.spec.lesson.as_ref().unwrap();
    assert_eq!((lesson.topic.as_str(), lesson.terms.len()), ("Phrasal verbs", 2));
}

#[test]
fn the_interview_goes_from_topic_to_topic_then_to_code_and_the_cross_ends_it() {
    use careertwin_prep::testing::Reply;
    let replies = vec![Reply::text("Let us start."), Reply::text("Next topic."), Reply::text("And more.")];
    let (mut harness, _d, _r) = harness(fixture_with("en", replies), 1120.0, 900.0);
    harness.state_mut().core.syllabus = Some(plan_of_two());
    harness.run();
    harness.get_by_label("Start the interview").click();
    harness.run_steps(3);
    let topic = |harness: &Harness<'_, PrepApp>| match &harness.state().screen {
        Screen::Live(view) => view.spec.lesson.as_ref().map(|lesson| lesson.topic.clone()),
        _ => None,
    };
    assert_eq!(topic(&harness).as_deref(), Some("OOP"));
    assert!(harness.state().core.run.is_some());
    // the tutor ends the first topic by itself: the second one follows at once, with nobody pressing anything
    the_tutor_ends_this_part(&mut harness);
    assert_eq!(topic(&harness).as_deref(), Some("REST"));
    assert!(harness.state().core.run.is_some());
    // and after the last topic comes the code
    the_tutor_ends_this_part(&mut harness);
    assert!(matches!(harness.state().screen, Screen::Code(_)), "the coding task comes after the topics");
    assert!(harness.state().core.run.as_ref().is_some_and(|run| run.coded));
}

#[test]
fn ending_a_part_with_the_cross_ends_the_whole_interview() {
    use careertwin_prep::testing::Reply;
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![Reply::text("Let us start.")]), 1120.0, 900.0);
    harness.state_mut().core.syllabus = Some(plan_of_two());
    harness.run();
    harness.get_by_label("Start the interview").click();
    harness.run_steps(3);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.session.add_turn(Who::You, "an answer", None);
    }
    harness.get_by_label("End call").click();
    harness.run_steps(3);
    assert!(harness.state().core.run.is_none(), "the cross stops the interview");
    assert!(matches!(harness.state().screen, Screen::Review(_)));
}

#[test]
fn with_a_plan_the_main_button_starts_the_next_topic_as_a_lesson_and_the_timer() {
    use careertwin_prep::testing::Reply;
    let reply = Reply::text("Hi Alex. In your own words, what is encapsulation?");
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![reply]), 1120.0, 900.0);
    harness.state_mut().core.syllabus = Some(plan_of_two());
    harness.run();
    harness.get_by_label_contains("We begin with OOP");
    harness.get_by_label("Start the interview").click();
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!("a lesson should have started") };
    assert_eq!(view.spec.mode, Mode::Terms);
    assert_eq!(view.spec.lesson.as_ref().unwrap().terms.len(), 2);
    assert_eq!(view.spec.persona.first_name(), "Maya");
    assert!(harness.state().core.pomodoro.running(), "a lesson is a focus block");
    wait_until(&mut harness, "the tutor speaks", |h| h.query_by_label_contains("what is encapsulation").is_some());
    harness.get_by_label("Term 1 of 2");
    harness.get_by_label("Encapsulation");
    harness.get_by_label("IN YOUR WORDS");
    shot(&mut harness, "lesson-en");
}

/// A tutoring call wired to the test, like `live` is for an interview.
fn tutoring(harness: &mut Harness<'_, PrepApp>, mode: Mode) -> Wired {
    use careertwin_prep::prompts::interviewer::InterviewerSpec;
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = Handle::from_parts(command_tx, event_rx);
    let app = harness.state_mut();
    let mut spec = InterviewerSpec::tutoring(Track::Technical, mode, Lang::En, 20);
    if mode == Mode::Terms {
        spec.lesson = plan_of_two()
            .topics
            .into_iter()
            .next()
            .map(|topic| careertwin_prep::prompts::tutor::Lesson { topic: topic.name, terms: topic.terms });
    }
    let session = Session::new(Track::Technical, Kind::Live, "OOP");
    app.screen = Screen::Live(Box::new(LiveView::new(session, spec, handle, None)));
    Wired { events: event_tx, commands: command_rx }
}

#[test]
fn a_lesson_shows_the_term_the_stage_and_remembers_how_it_went() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    let wired = tutoring(&mut harness, Mode::Terms);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Lesson(LessonView { index: 0, total: 2, name: "Encapsulation".into(), stage: Stage::Native }))
        .unwrap();
    harness.run();
    harness.get_by_label("Term 1 of 2");
    harness.get_by_label("Encapsulation");
    harness.get_by_label("IN YOUR WORDS");
    // your own words went well, then English
    wired
        .events
        .send(Event::TermProgress { index: 0, name: "Encapsulation".into(), half: Half::Native, score: 3 })
        .unwrap();
    wired
        .events
        .send(Event::Lesson(LessonView { index: 0, total: 2, name: "Encapsulation".into(), stage: Stage::English }))
        .unwrap();
    harness.run();
    harness.get_by_label("IN ENGLISH");
    let progress = &harness.state().core.progress;
    assert_eq!(progress.get("OOP", "Encapsulation").native, 3);
    assert!(harness.state().core.paths.progress_file().exists(), "what you know is kept for the next run");
    // the tutor's note goes into the notebook under the topic
    wired.events.send(Event::Tag(careertwin_prep::tags::Tag::Note("Encapsulation hides state.".into()))).unwrap();
    harness.run();
    let notes = &harness.state().core.notebook.notes;
    assert_eq!((notes[0].topic.as_str(), notes[0].text.as_str()), ("OOP", "Encapsulation hides state."));
    shot(&mut harness, "lesson-stage-en");
}

#[test]
fn the_chat_is_quiet_has_no_microphone_and_keeps_what_you_write() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    let mut wired = tutoring(&mut harness, Mode::Chat);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Turn {
            who: Who::Interviewer,
            text: "I'm here. Type anything.".into(),
            language: None,
            append: false,
        })
        .unwrap();
    wired
        .events
        .send(Event::Turn { who: Who::You, text: "What is a closure?".into(), language: None, append: false })
        .unwrap();
    wired
        .events
        .send(Event::Turn {
            who: Who::Interviewer,
            text: "A function that remembers the variables around it.".into(),
            language: None,
            append: false,
        })
        .unwrap();
    harness.run();
    harness.get_by_label("Write to me. I'm here.");
    harness.get_by_label("A function that remembers the variables around it.");
    harness.get_by_label("Finish chat");
    assert!(
        harness.query_by_label("Mute").is_none() && harness.query_by_label("Unmute the tutor").is_none(),
        "no microphone, no voice"
    );
    assert!(harness.query_by_label("Transcript").is_none());
    harness.get_by_role(egui::accesskit::Role::TextInput).focus();
    harness.run();
    harness.get_by_role(egui::accesskit::Role::TextInput).type_text("and a decorator?");
    harness.run();
    harness.get_by_label("Send").click();
    harness.run();
    match wired.commands.try_recv() {
        Ok(Command::Say(text)) => assert_eq!(text, "and a decorator?"),
        other => panic!("expected the typed question, got {other:?}"),
    }
    // a note the tutor writes in a chat is a lecture note
    wired.events.send(Event::Tag(careertwin_prep::tags::Tag::Note("A closure keeps its scope.".into()))).unwrap();
    harness.run();
    assert_eq!(harness.state().core.notebook.notes[0].kind, careertwin_prep::notes::NoteKind::Lecture);
    shot(&mut harness, "chat-en");
}

#[test]
fn the_interview_tab_starts_the_introduction_coach_and_the_english_tab_a_conversation() {
    use careertwin_prep::testing::Reply;
    let (mut harness, _d, _r) = harness(
        fixture_with(
            "en",
            vec![Reply::text("Hi Alex. Tell me about yourself."), Reply::text("Hello. How was your day?")],
        ),
        1120.0,
        900.0,
    );
    harness.run();
    harness.get_by_label("Interview").click();
    harness.run_steps(3);
    harness.get_by_label("Practise my introduction").click();
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!("the coach should have started") };
    assert_eq!(view.spec.mode, Mode::Intro);
    wait_until(&mut harness, "the coach asks", |h| h.query_by_label_contains("Tell me about yourself").is_some());
    harness.get_by_label("End call").click();
    harness.run_steps(3);
    harness.get_by_label("English").click();
    harness.run_steps(3);
    assert!(harness.state().core.pomodoro.running(), "the first conversation started the timer");
    harness.state_mut().core.pomodoro.pause(std::time::Instant::now());
    assert!(!harness.state().core.pomodoro.running());
    harness.get_by_label("Study something together").click();
    harness.run_steps(3);
    harness.get_by_label("Just talk, no material").click();
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!("the conversation should have started") };
    assert_eq!(view.spec.mode, Mode::Interview);
    assert_eq!(view.spec.track, Track::English);
    assert!(harness.state().core.pomodoro.running(), "the timer starts with the conversation");
}

#[test]
fn writing_to_the_tutor_opens_a_quiet_chat_from_the_menu_of_any_tab() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.run();
    assert!(harness.query_by_label("Write instead").is_none(), "the home page has one button");
    click_card(&mut harness, "Write to the tutor");
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!("the chat should have opened") };
    assert_eq!(view.spec.mode, Mode::Chat);
    assert!(!harness.state().core.pomodoro.running(), "a chat in a lecture does not start the timer");
    wait_until(&mut harness, "the greeting", |h| h.query_by_label_contains("Type anything").is_some());
}

// ---- pictures ----

const STORY: &str = r#"{"title": "How a queue works", "check": "Who leaves the queue first?",
 "scenes": [
  {"say": "Think of a queue at a shop till.",
   "panels": [{"id": "line", "kind": "cells", "title": "People in a line", "items": ["Ann", "Bob", "Cy"], "highlight": [0], "note": "first in, first out"}]},
  {"say": "The first person to arrive is served first.",
   "panels": [
     {"id": "line", "kind": "cells", "title": "People in a line", "items": ["Ann", "Bob", "Cy", "Di"], "highlight": [0]},
     {"id": "path", "kind": "flow", "title": "Journey", "items": ["arrive", "wait", "served"], "active": 2},
     {"id": "code", "kind": "code", "title": "Python", "lines": ["q = deque()", "q.append('Ann')", "q.popleft()"], "highlight": [2]}]},
  {"say": "A stack is the opposite.",
   "panels": [
     {"kind": "compare", "title": "Queue vs stack", "left": {"title": "Queue", "lines": ["first in", "first out"]}, "right": {"title": "Stack", "lines": ["last in", "first out"]}},
     {"kind": "analogy", "title": "Like", "lines": ["A plate pile", "A stack"]},
     {"kind": "steps", "title": "Steps", "lines": ["push", "push", "pop"], "active": 2},
     {"kind": "layers", "title": "Layers", "lines": ["top", "middle", "bottom"]}]}
 ]}"#;

fn picture_fixture() -> Fixture {
    fixture_with("en", vec![careertwin_prep::testing::Reply::text(STORY)])
}

#[test]
fn the_picture_page_asks_for_a_topic_then_draws_and_tells_it() {
    let (mut harness, _d, _r) = harness(picture_fixture(), 1120.0, 860.0);
    harness.run();
    click_card(&mut harness, "Explain with pictures");
    harness.run_steps(3);
    harness.get_by_label("Explain it with pictures");
    harness.get_by_label("Show me"); // not ready until a topic is typed
    harness.get_all_by_role(egui::accesskit::Role::TextInput).next().unwrap().focus();
    harness.run();
    harness.get_all_by_role(egui::accesskit::Role::TextInput).next().unwrap().type_text("how a queue works");
    harness.run();
    harness.get_by_label("Show me").click();
    harness.run_steps(2);
    harness.get_by_label("Drawing it…");
    wait_until(
        &mut harness,
        "the story is ready",
        |h| matches!(&h.state().screen, Screen::Show(v) if v.stage.as_ref().is_some_and(|s| s.player().is_some())),
    );
    harness.run_steps(2);
    harness.get_by_label("How a queue works");
    for control in
        ["Pause", "Next scene", "Previous scene", "Play it again", "Explain more simply", "Close the picture"]
    {
        harness.get_by_label(control);
    }
    harness.get_by_label_contains("Think of a queue");
    shot(&mut harness, "picture-1-en");
    // the second scene grows the first picture instead of drawing it again
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        let player = view.stage.as_mut().unwrap().player_mut().unwrap();
        for i in 0..3 {
            player.set_voice(i, None);
        }
    }
    harness.get_by_label("Next scene").click();
    harness.run_steps(3);
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        view.stage.as_mut().unwrap().player_mut().unwrap().seek(0.8, std::time::Instant::now());
    }
    harness.run_steps(2);
    shot(&mut harness, "picture-2-en");
    harness.get_by_label("Next scene").click();
    harness.run_steps(3);
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        view.stage.as_mut().unwrap().player_mut().unwrap().seek(0.9, std::time::Instant::now());
    }
    harness.run_steps(2);
    shot(&mut harness, "picture-3-en");
    // closing it goes back to the form
    harness.get_by_label("Close the picture").click();
    harness.run_steps(3);
    harness.get_by_label("Show me");
}

#[test]
fn a_story_can_be_paused_and_played_from_the_controls() {
    let (mut harness, _d, _r) = harness(picture_fixture(), 1120.0, 860.0);
    harness.state_mut().screen = Screen::Show(Box::default());
    harness.run();
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        view.stage = None;
    }
    let stage = { harness.state_mut().core.begin_stage("queue", "", false) };
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        view.stage = Some(stage);
    }
    wait_until(
        &mut harness,
        "the story is ready",
        |h| matches!(&h.state().screen, Screen::Show(v) if v.stage.as_ref().is_some_and(|s| s.player().is_some())),
    );
    harness.run_steps(2);
    harness.get_by_label("Pause").click();
    harness.run_steps(2);
    let Screen::Show(view) = &harness.state().screen else { panic!() };
    assert_eq!(view.stage.as_ref().unwrap().player().unwrap().phase(), careertwin_prep::explain::Phase::Paused);
    harness.get_by_label("Play").click();
    // it begins as soon as its voice is there
    wait_until(&mut harness, "the scene plays", |h| {
        matches!(&h.state().screen, Screen::Show(v)
            if v.stage.as_ref().and_then(|s| s.player()).is_some_and(|p| p.phase() == careertwin_prep::explain::Phase::Playing))
    });
}

#[test]
fn a_story_that_cannot_be_written_says_so_and_offers_another_try() {
    use careertwin_prep::testing::Reply;
    let (mut harness, _d, _r) =
        harness(fixture_with("en", vec![Reply::text("no json"), Reply::text("still none")]), 1120.0, 860.0);
    let stage = harness.state_mut().core.begin_stage("queue", "", false);
    harness.state_mut().screen =
        Screen::Show(Box::new(careertwin_prep::ui::show::ShowView { stage: Some(stage), ..Default::default() }));
    wait_until(&mut harness, "the failure shows", |h| h.query_by_label("The picture could not be made").is_some());
    harness.get_by_label("Try again, more simply");
    shot(&mut harness, "picture-failed-en");
}

#[test]
fn when_the_tutor_says_show_the_call_holds_draws_and_asks_the_question_after() {
    let (mut harness, _d, _r) = harness(picture_fixture(), 1120.0, 900.0);
    let mut wired = tutoring(&mut harness, Mode::Terms);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired.events.send(Event::Tag(careertwin_prep::tags::Tag::Show("how a queue works".into()))).unwrap();
    harness.run_steps(2);
    assert!(
        matches!(wired.commands.try_recv(), Ok(Command::Hold(true))),
        "the tutor does not nudge while a picture plays"
    );
    assert!(matches!(&harness.state().screen, Screen::Live(v) if v.stage.is_some()), "the picture is on its way");
    wait_until(
        &mut harness,
        "the story is ready",
        |h| matches!(&h.state().screen, Screen::Live(v) if v.stage.as_ref().is_some_and(|s| s.player().is_some())),
    );
    harness.run_steps(2);
    harness.get_by_label("How a queue works");
    shot(&mut harness, "picture-in-call-en");
    // you speak: the picture waits for the answer
    wired
        .events
        .send(Event::Turn { who: Who::You, text: "wait, why first in?".into(), language: None, append: false })
        .unwrap();
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    assert_eq!(view.stage.as_ref().unwrap().player().unwrap().phase(), careertwin_prep::explain::Phase::Paused);
    // it is watched to the end: the tutor is released and asks its question
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        let player = view.stage.as_mut().unwrap().player_mut().unwrap();
        for i in 0..3 {
            player.set_voice(i, None);
        }
        let now = std::time::Instant::now();
        player.resume(now);
        player.go_to(2, now);
        player.seek(0.99, now);
    }
    harness.run_steps(2);
    wait_until(
        &mut harness,
        "the end of the story",
        |h| matches!(&h.state().screen, Screen::Live(v) if v.stage.is_none()),
    );
    let mut commands = Vec::new();
    while let Ok(command) = wired.commands.try_recv() {
        commands.push(command);
    }
    assert!(commands.iter().any(|c| matches!(c, Command::Hold(false))), "{commands:?}");
    assert!(
        commands.iter().any(|c| matches!(c, Command::Context(t) if t.contains("watched all of it"))),
        "{commands:?}"
    );
    assert!(
        commands.iter().any(|c| matches!(c, Command::Speak(t) if t == "Who leaves the queue first?")),
        "{commands:?}"
    );
}

#[test]
fn closing_a_picture_early_releases_the_tutor_and_says_so() {
    let (mut harness, _d, _r) = harness(picture_fixture(), 1120.0, 900.0);
    let mut wired = tutoring(&mut harness, Mode::Terms);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired.events.send(Event::Tag(careertwin_prep::tags::Tag::Show("queues".into()))).unwrap();
    wait_until(
        &mut harness,
        "the story is ready",
        |h| matches!(&h.state().screen, Screen::Live(v) if v.stage.as_ref().is_some_and(|s| s.player().is_some())),
    );
    harness.run_steps(2);
    harness.get_by_label("Close the picture").click();
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    assert!(view.stage.is_none());
    let mut commands = Vec::new();
    while let Ok(command) = wired.commands.try_recv() {
        commands.push(command);
    }
    assert!(commands.iter().any(|c| matches!(c, Command::Hold(false))));
    assert!(
        commands.iter().any(|c| matches!(c, Command::Context(t) if t.contains("closed it before the end"))),
        "{commands:?}"
    );
    assert!(!commands.iter().any(|c| matches!(c, Command::Speak(_))), "no question after a closed picture");
}

// ---- live coding ----

const TASK: &str = r#"{"title": "Two sum", "statement": "Find two numbers in the list that add up to the target and return their positions.",
 "starter": "def two_sum(nums, target):\n    # return the two positions", "examples": [{"input": "[2, 7, 11], 9", "output": "[0, 1]"}],
 "outline": "One pass with a dictionary of numbers seen so far.", "pitfalls": ["the same element used twice"],
 "hints": ["What do you need to remember about the numbers you have already seen?"], "complexity": "O(n) time"}"#;

fn task() -> careertwin_prep::coding::Task {
    careertwin_prep::prompts::coding::parse_task(TASK, "python").unwrap()
}

/// A live-coding call wired to the test.
fn coding(harness: &mut Harness<'_, PrepApp>) -> Wired {
    use careertwin_prep::prompts::interviewer::InterviewerSpec;
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = Handle::from_parts(command_tx, event_rx);
    let app = harness.state_mut();
    let mut spec = InterviewerSpec::tutoring(Track::Technical, Mode::Coding, Lang::En, 30);
    spec.task = Some(task());
    spec.focus = "Two sum".into();
    let session = Session::new(Track::Technical, Kind::Live, "Live coding · Two sum");
    let mut view = LiveView::new(session, spec, handle, None);
    view.coding = Some(careertwin_prep::ui::code::CodeSession::new(task()));
    app.screen = Screen::Live(Box::new(view));
    Wired { events: event_tx, commands: command_rx }
}

#[test]
fn the_coding_page_chooses_a_language_and_starts_a_call_with_the_task() {
    use careertwin_prep::testing::Reply;
    let (mut harness, _d, _r) = harness(
        fixture_with("en", vec![Reply::text(TASK), Reply::text("Hi Alex. Here is a small task.")]),
        1280.0,
        900.0,
    );
    harness.state_mut().core.vacancy.description = "We use TypeScript and React.".into();
    harness.run();
    click_card(&mut harness, "Live coding");
    harness.run_steps(3);
    harness.get_by_label("LANGUAGE");
    harness.get_by_label_contains("The tutor sees only this window");
    // the vacancy names TypeScript, so that is the choice
    let Screen::Code(view) = &harness.state().screen else { panic!("the page should be open") };
    assert_eq!(view.language, "typescript");
    harness.get_by_label("Rust").click();
    harness.run_steps(2);
    harness.get_by_label("Get a task").click();
    wait_until(&mut harness, "the call starts", |h| matches!(h.state().screen, Screen::Live(_)));
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    assert_eq!(view.spec.mode, Mode::Coding);
    assert_eq!(view.spec.task.as_ref().unwrap().language, "rust", "the language that was picked");
    assert!(view.coding.is_some());
    harness.get_by_label("Two sum");
    harness.get_by_label_contains("Find two numbers in the list");
    harness.get_by_label("I'm done");
    harness.get_by_label("I don't know");
    shot(&mut harness, "coding-en");
}

#[test]
fn code_is_written_in_the_editor_and_the_tutor_is_given_it() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1280.0, 900.0);
    let mut wired = coding(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(2);
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
    harness.run();
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput).type_text("\n    seen = {}");
    harness.run();
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    assert!(view.coding.as_ref().unwrap().text.contains("seen = {}"));
    // once it has been still for a moment the tutor is sent the code
    let mut sent = None;
    for _ in 0..400 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        harness.run_steps(2);
        while let Ok(command) = wired.commands.try_recv() {
            if let Command::Code(code) = command {
                sent = Some(code);
            }
        }
        if sent.is_some() {
            break;
        }
    }
    assert!(sent.expect("the code was sent").contains("seen = {}"));
}

#[test]
fn the_buttons_say_what_a_person_would_say() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1280.0, 900.0);
    let mut wired = coding(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(2);
    harness.get_by_label("I don't know").click();
    harness.run_steps(2);
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Say(text)) if text == "I don't know how to continue."));
    harness.get_by_label("I'm done").click();
    harness.run_steps(2);
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Say(text)) if text.contains("I think I'm done")));
}

#[test]
fn a_block_that_comes_in_all_at_once_is_noticed_and_the_tutor_asks_about_it_without_accusing() {
    use careertwin_prep::testing::Reply;
    const EXPLAIN: &str = r#"{"status": "explain", "say": "That block came in all at once. Can you walk me through it line by line?", "level": 1}"#;
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![Reply::text(EXPLAIN)]), 1280.0, 900.0);
    let mut wired = coding(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(2);
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
    harness.run_steps(2);
    let block = "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        if target - n in seen:\n            return [seen[target - n], i]\n        seen[n] = i\n";
    harness.event(egui::Event::Paste(block.into()));
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    let pastes = &view.coding.as_ref().unwrap().watcher.pastes;
    assert_eq!(pastes.len(), 1, "the paste was seen");
    assert!(pastes[0].chars >= 100);
    let mut said = None;
    for _ in 0..600 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        harness.run_steps(2);
        while let Ok(command) = wired.commands.try_recv() {
            if let Command::Speak(text) = command {
                said = Some(text);
            }
        }
        if said.is_some() {
            break;
        }
    }
    let said = said.expect("the tutor said something");
    assert!(said.contains("walk me through it"), "{said}");
    assert!(!said.to_lowercase().contains("cheat"), "no accusation");
}

#[test]
fn a_quiet_look_at_good_code_says_nothing() {
    use careertwin_prep::testing::Reply;
    const QUIET: &str = r#"{"status": "on_track", "say": "", "level": 0}"#;
    let (mut harness, _d, _r) = harness(fixture_with("en", vec![Reply::text(QUIET)]), 1280.0, 900.0);
    let mut wired = coding(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(2);
    // pretend the person is stuck for a minute
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        let coding = view.coding.as_mut().unwrap();
        coding.watcher = careertwin_prep::watch::Watcher::new(
            &coding.text,
            std::time::Instant::now() - std::time::Duration::from_secs(120),
        );
    }
    // wait for the look to be made and finished, however busy the machine is
    for _ in 0..600 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        harness.run_steps(2);
        let Screen::Live(view) = &harness.state().screen else { panic!() };
        let watcher = &view.coding.as_ref().unwrap().watcher;
        if watcher.looks > 0 && !watcher.looking {
            break;
        }
    }
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    assert!(view.coding.as_ref().unwrap().watcher.looks > 0, "the tutor looked at the code");
    let mut spoke = false;
    while let Ok(command) = wired.commands.try_recv() {
        spoke |= matches!(command, Command::Speak(_));
    }
    assert!(!spoke, "an on-track person is left alone");
    let Screen::Live(view) = &harness.state().screen else { panic!() };
    assert!(!view.coding.as_ref().unwrap().watcher.looking, "the look is over");
}

#[test]
fn the_finished_code_and_what_the_editor_saw_go_into_the_review() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1280.0, 900.0);
    let wired = coding(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Turn { who: Who::You, text: "I will use a dictionary.".into(), language: None, append: false })
        .unwrap();
    harness.run_steps(2);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.coding.as_mut().unwrap().text = "def two_sum(nums, target):\n    return [0, 1]\n".into();
    }
    harness.get_by_label("End call").click();
    harness.run_steps(3);
    let Screen::Review(review) = &harness.state().screen else { panic!("the review should open") };
    let session = harness.state().core.store.load(&review.session.id).unwrap();
    assert!(session.payload["code"].as_str().unwrap().contains("return [0, 1]"));
    assert_eq!(session.payload["signals"], "nothing unusual");
    let prompt = careertwin_prep::prompts::feedback::prompt(&session, &careertwin_prep::brief::Brief::default());
    assert!(prompt.contains("THE CODE AT THE END") && prompt.contains("return [0, 1]"), "{prompt}");
    assert!(prompt.contains("WHAT THE EDITOR SAW: nothing unusual."));
}

// ---- the pictures used in the documentation (written only when UI_SHOTS names a folder) ----

#[test]
fn docs_a_picture_in_the_middle_of_its_story() {
    if std::env::var("UI_SHOTS").is_err() {
        return;
    }
    let (mut harness, _d, _r) = harness(picture_fixture(), 1120.0, 780.0);
    let stage = harness.state_mut().core.begin_stage("queue", "", false);
    harness.state_mut().screen =
        Screen::Show(Box::new(careertwin_prep::ui::show::ShowView { stage: Some(stage), ..Default::default() }));
    wait_until(
        &mut harness,
        "the story is ready",
        |h| matches!(&h.state().screen, Screen::Show(v) if v.stage.as_ref().is_some_and(|s| s.player().is_some())),
    );
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        let player = view.stage.as_mut().unwrap().player_mut().unwrap();
        for i in 0..3 {
            player.set_voice(i, None);
        }
    }
    harness.run_steps(2);
    harness.get_by_label("Next scene").click();
    harness.run_steps(3);
    // let the panels settle in, then stand at the moment the sentence is half said
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        harness.run_steps(1);
    }
    if let Screen::Show(view) = &mut harness.state_mut().screen {
        view.stage.as_mut().unwrap().player_mut().unwrap().seek(0.7, std::time::Instant::now());
    }
    harness.run_steps(2);
    shot(&mut harness, "docs-picture");
}

#[test]
fn docs_a_lesson_with_the_tutor_speaking() {
    if std::env::var("UI_SHOTS").is_err() {
        return;
    }
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    let wired = tutoring(&mut harness, Mode::Terms);
    wired.events.send(Event::State(State::Speaking)).unwrap();
    wired
        .events
        .send(Event::Lesson(LessonView { index: 1, total: 4, name: "Polymorphism".into(), stage: Stage::English }))
        .unwrap();
    wired
        .events
        .send(Event::Caption {
            text: "That's the idea: one method name, different behaviour in each class.".into(),
            seconds: 6.0,
        })
        .unwrap();
    wired
        .events
        .send(Event::Caption { text: "Now say it the way you would in the interview.".into(), seconds: 4.0 })
        .unwrap();
    harness.run();
    shot(&mut harness, "docs-lesson");
}

#[test]
fn docs_the_home_with_a_plan_and_a_streak() {
    if std::env::var("UI_SHOTS").is_err() {
        return;
    }
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    harness.state_mut().core.syllabus = Some(plan_of_two());
    let now = careertwin_prep::session::now();
    for back in 1..=4u64 {
        harness.state_mut().core.stats.add_practice(now - back * 86_400, 15 * 60, 200);
    }
    harness.state_mut().core.stats.add_practice(now, 9 * 60, 120);
    harness.state_mut().core.progress.record("OOP", "Encapsulation", careertwin_prep::syllabus::Half::Native, 3, now);
    harness.run();
    shot(&mut harness, "docs-home");
}

#[test]
fn docs_live_coding_with_a_question_from_the_tutor() {
    if std::env::var("UI_SHOTS").is_err() {
        return;
    }
    let (mut harness, _d, _r) = harness(fixture("en"), 1280.0, 780.0);
    let wired = coding(&mut harness);
    wired.events.send(Event::State(State::Speaking)).unwrap();
    wired
        .events
        .send(Event::Caption {
            text: "Take [2, 7, 11] with the target 9 and follow the loop. What does it return?".into(),
            seconds: 5.0,
        })
        .unwrap();
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.coding.as_mut().unwrap().text = "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        seen[n] = i\n        if target - n in seen:\n            return [target - n, n]\n    return []\n".into();
    }
    harness.run_steps(3);
    shot(&mut harness, "docs-coding");
}

#[test]
fn the_smallest_window_still_fits_the_bar_in_both_languages() {
    for language in ["en", "ru"] {
        let (mut harness, _d, _r) = harness(fixture(language), 760.0, 560.0);
        harness.run();
        shot(&mut harness, &format!("smallest-{language}"));
        // the sessions button, the tabs and the timer all fit without sitting on each other
        let tabs: Vec<_> = harness.get_all_by_role(egui::accesskit::Role::Tab).map(|node| node.rect()).collect();
        assert_eq!(tabs.len(), 3);
        let tabs_left = tabs.iter().map(|r| r.left()).fold(f32::MAX, f32::min);
        let tabs_right = tabs.iter().map(|r| r.right()).fold(0.0, f32::max);
        let timer = harness.get_by_label(if language == "en" { "Start the timer" } else { "Запустить таймер" }).rect();
        let list = harness.get_by_label(if language == "en" { "SESSIONS" } else { "СЕССИИ" }).rect();
        assert!(list.right() < tabs_left, "{language}: the sessions button {list:?} meets the tabs at {tabs_left}");
        assert!(
            tabs_right < timer.left(),
            "{language}: the tabs end at {tabs_right}, the timer starts at {}",
            timer.left()
        );
    }
}

#[test]
fn russian_screens_for_a_look() {
    if std::env::var("UI_SHOTS").is_err() {
        return;
    }
    let (mut harness, _d, _r) = harness(fixture("ru"), 1120.0, 860.0);
    harness.state_mut().core.syllabus = Some(plan_of_two());
    harness.state_mut().core.notebook.add(
        careertwin_prep::notes::NoteKind::Vital,
        "OOP",
        "Инкапсуляция прячет состояние: наружу торчат только методы.",
        careertwin_prep::session::now(),
    );
    harness.run();
    shot(&mut harness, "ru-home");
    harness.state_mut().screen = Screen::Plan(Box::default());
    harness.run_steps(3);
    shot(&mut harness, "ru-plan");
    harness.state_mut().screen = Screen::Notes(Box::default());
    harness.run_steps(3);
    shot(&mut harness, "ru-notes");
    harness.state_mut().screen = Screen::Show(Box::default());
    harness.run_steps(3);
    shot(&mut harness, "ru-show");
    harness.state_mut().screen =
        Screen::Code(Box::new(careertwin_prep::ui::code::SetupView::new(&harness.state().core)));
    harness.run_steps(3);
    shot(&mut harness, "ru-code-setup");
    harness.state_mut().core.usage_open = true;
    harness.run_steps(3);
    shot(&mut harness, "ru-usage");
    harness.state_mut().core.usage_open = false;
    harness.state_mut().core.help_open = true;
    harness.run_steps(3);
    shot(&mut harness, "ru-help");
}

#[test]
fn another_practised_language_is_chosen_in_settings_and_shows_in_the_lesson() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 1000.0);
    harness.state_mut().core.syllabus = Some(plan_of_two());
    harness.run();
    harness.get_by_label_contains("in English with your grammar");
    open_settings(&mut harness);
    // the choice sits low in the settings window, below what a headless window shows
    harness_draft(&mut harness).settings.practice_language = "German".into();
    harness.run_steps(2);
    harness.get_by_label("Save").click();
    harness.run_steps(3);
    assert_eq!(harness.state().core.settings.practice_name(), "German");
    harness.get_by_label_contains("in German with your grammar");
    // a lesson started now practises German
    let wired = tutoring(&mut harness, Mode::Terms);
    let _ = wired;
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.lesson = Some(LessonView { index: 0, total: 2, name: "Encapsulation".into(), stage: Stage::English });
    }
    harness.run_steps(2);
    harness.get_by_label("IN GERMAN");
}

#[test]
fn when_every_term_is_learned_the_main_button_is_live_coding_and_a_plan_without_terms_says_so() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 900.0);
    let mut plan = plan_of_two();
    for topic in &mut plan.topics {
        topic.terms.clear();
    }
    harness.state_mut().core.syllabus = Some(plan);
    harness.run();
    harness.get_by_label_contains("The plan has no terms yet");
    // now everything is learned
    harness.state_mut().core.syllabus = Some(plan_of_two());
    let now = careertwin_prep::session::now();
    let plan = plan_of_two();
    for topic in &plan.topics {
        for term in &topic.terms {
            harness.state_mut().core.progress.record(&topic.name, &term.name, Half::Native, 3, now);
            harness.state_mut().core.progress.record(&topic.name, &term.name, Half::English, 3, now);
        }
    }
    harness.run_steps(2);
    harness.get_by_label_contains("Every term of the plan is learned");
    harness.get_by_label("Start the interview").click();
    harness.run_steps(3);
    let Screen::Live(view) = &harness.state().screen else { panic!("the interview should have started") };
    assert_eq!(view.spec.mode, Mode::Terms, "what is known is asked again: an interview tests everything");
}

#[test]
fn the_timer_starts_by_itself_when_the_program_opens_unless_it_is_turned_off() {
    // the real default
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 780.0);
    harness.run();
    assert!(!harness.state().core.pomodoro.running(), "the test fixture keeps it still");
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path().join("home"));
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let make = |auto_start: bool| {
        let mut settings = Settings::default();
        settings.pomodoro.auto_start = auto_start;
        let services = Services {
            llm: FakeLlm::new(vec![]),
            stt: None,
            tts: None,
            speaker: FakeSpeaker::new(),
            mic: Arc::new(|_| Err(anyhow::anyhow!("no microphone in a test"))),
        };
        PrepApp::new(paths.clone(), settings, services, Problems::default(), runtime.handle().clone(), Arc::new(|| {}))
    };
    assert!(make(true).core.pomodoro.running(), "it is on by default and starts with the program");
    assert!(!make(false).core.pomodoro.running());
    assert!(Settings::default().pomodoro.auto_start);
}

#[test]
fn a_call_with_a_microphone_has_two_buttons_and_a_text_box_comes_when_you_type() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 860.0);
    let mut wired = live(&mut harness);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.has_microphone = true;
    }
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(3);
    harness.get_by_label("Mute");
    harness.get_by_label("End call");
    assert!(harness.query_by_role(egui::accesskit::Role::TextInput).is_none(), "no text box until you type");
    shot(&mut harness, "call-en");
    // starting to type brings the box, with what was typed in it
    harness.event(egui::Event::Text("I build the order API.".into()));
    harness.run_steps(4);
    harness.get_by_role(egui::accesskit::Role::TextInput);
    harness.get_by_label("Send").click();
    harness.run_steps(3);
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Say(text)) if text == "I build the order API."));
    // the microphone button turns the microphone off and on
    harness.get_by_label("Mute").click();
    harness.run_steps(2);
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Mute(true))));
    harness.get_by_label("Talk").click();
    harness.run_steps(2);
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Mute(false))));
    // and the cross ends the call
    harness.get_by_label("End call").click();
    harness.run_steps(3);
    assert!(matches!(wired.commands.try_recv(), Ok(Command::Finish)));
}

#[test]
fn an_error_in_a_call_stays_until_the_tutor_speaks_again() {
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 860.0);
    let wired = live(&mut harness);
    wired.events.send(Event::State(State::Listening)).unwrap();
    wired
        .events
        .send(Event::Error("No AI gateway answers at http://127.0.0.1:3260 (connection refused).".into()))
        .unwrap();
    // your words arrive, and the error must not vanish with them
    wired.events.send(Event::Turn { who: Who::You, text: "Hello.".into(), language: None, append: false }).unwrap();
    harness.run_steps(3);
    harness.get_by_label_contains("No AI gateway answers");
    wired
        .events
        .send(Event::Turn { who: Who::Interviewer, text: "Hi, I'm Daniel.".into(), language: None, append: false })
        .unwrap();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("No AI gateway answers").is_none(), "gone once the tutor answers");
}

// ---- breaks ----

fn drain(wired: &mut Wired) -> Vec<Command> {
    let mut commands = Vec::new();
    while let Ok(command) = wired.commands.try_recv() {
        commands.push(command);
    }
    commands
}

#[test]
fn when_the_focus_block_ends_the_call_pauses_and_the_tutor_says_to_take_a_break() {
    use careertwin_prep::pomodoro::{Phase, Signal};
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 860.0);
    let mut wired = live(&mut harness);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.has_microphone = true;
    }
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(2);
    drain(&mut wired);
    harness.state_mut().on_timer_signal(Signal::FocusDone { next: Phase::ShortBreak });
    harness.run_steps(3);
    let commands = drain(&mut wired);
    assert!(commands.iter().any(|c| matches!(c, Command::Mute(true))), "the microphone is shut: {commands:?}");
    assert!(commands.iter().any(|c| matches!(c, Command::Hold(true))), "no nudges while away: {commands:?}");
    let said = commands.iter().find_map(|c| if let Command::Speak(t) = c { Some(t.clone()) } else { None });
    assert_eq!(
        said.as_deref(),
        Some("Time for a break. Stand up, walk around, drink some water. I'll be here in 5 minutes.")
    );
    harness.get_by_label("Break");
    harness.get_by_label_contains("The call is paused");
    harness.get_by_label("Back to the call");
    shot(&mut harness, "break-en");
    // a second finished block does not stack a second pause
    harness.state_mut().on_timer_signal(Signal::FocusDone { next: Phase::ShortBreak });
    harness.run_steps(2);
    assert!(drain(&mut wired).iter().all(|c| !matches!(c, Command::Speak(_))));
    // when the break is over the call goes on by itself
    harness.state_mut().on_timer_signal(Signal::BreakDone);
    harness.run_steps(3);
    let commands = drain(&mut wired);
    assert!(commands.iter().any(|c| matches!(c, Command::Hold(false))), "{commands:?}");
    assert!(commands.iter().any(|c| matches!(c, Command::Mute(false))), "{commands:?}");
    assert!(commands.iter().any(|c| matches!(c, Command::Speak(t) if t.starts_with("Welcome back"))), "{commands:?}");
    assert!(harness.query_by_label("Back to the call").is_none());
    // and a break can be cut short; the timer really is on a break then
    harness.state_mut().core.pomodoro.skip(std::time::Instant::now());
    assert_eq!(harness.state().core.pomodoro.phase(), Phase::ShortBreak);
    harness.state_mut().on_timer_signal(Signal::FocusDone { next: Phase::ShortBreak });
    harness.run_steps(3);
    drain(&mut wired);
    harness.get_by_label("Back to the call").click();
    harness.run_steps(3);
    let commands = drain(&mut wired);
    assert!(commands.iter().any(|c| matches!(c, Command::Hold(false))), "{commands:?}");
    assert_eq!(harness.state().core.pomodoro.phase(), Phase::Focus, "back to work");
}

#[test]
fn a_microphone_that_was_off_stays_off_after_the_break_and_a_chat_is_not_paused() {
    use careertwin_prep::pomodoro::{Phase, Signal};
    let (mut harness, _d, _r) = harness(fixture("en"), 1120.0, 860.0);
    let mut wired = live(&mut harness);
    if let Screen::Live(view) = &mut harness.state_mut().screen {
        view.has_microphone = true;
        view.muted = true;
    }
    wired.events.send(Event::State(State::Muted)).unwrap();
    harness.run_steps(2);
    drain(&mut wired);
    harness.state_mut().on_timer_signal(Signal::FocusDone { next: Phase::LongBreak });
    harness.run_steps(2);
    let said =
        drain(&mut wired).into_iter().find_map(|c| if let Command::Speak(t) = c { Some(t) } else { None }).unwrap();
    assert!(said.contains("15 minutes"), "a long break names its length: {said}");
    harness.state_mut().on_timer_signal(Signal::BreakDone);
    harness.run_steps(2);
    assert!(drain(&mut wired).iter().all(|c| !matches!(c, Command::Mute(false))), "it was off before and stays off");
    // a chat in a lecture is left alone
    let (mut harness, _d, _r) = harness_chat();
    let mut wired = tutoring(&mut harness, Mode::Chat);
    wired.events.send(Event::State(State::Listening)).unwrap();
    harness.run_steps(2);
    harness.state_mut().on_timer_signal(Signal::FocusDone { next: Phase::ShortBreak });
    harness.run_steps(2);
    assert!(drain(&mut wired).is_empty(), "nothing is said in a chat");
    assert!(harness.query_by_label("Back to the call").is_none());
}

fn harness_chat() -> (Harness<'static, PrepApp>, tempfile::TempDir, tokio::runtime::Runtime) {
    harness(fixture("en"), 1120.0, 860.0)
}
