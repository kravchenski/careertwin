//! The interviewer on a real model. Ignored by default: it needs a running gateway.
//! `LIVE_URL=http://127.0.0.1:3260 LIVE_MODEL=auto cargo test --test live_llm -- --ignored --nocapture`
//!
//! It uses only the fictional demo profile, never your own ABOUT.md.

use std::sync::Arc;
use std::time::Duration;

use careertwin_prep::brief::Brief;
use careertwin_prep::config::LlmSettings;
use careertwin_prep::conversation::{self, Command, Config, Deps, Event, Knobs, State};
use careertwin_prep::llm::HttpLlm;
use careertwin_prep::prompts::Lang;
use careertwin_prep::prompts::interviewer::InterviewerSpec;
use careertwin_prep::secrets::Secret;
use careertwin_prep::session::{Speaker as Who, Track};
use careertwin_prep::speech::{SttHint, Voice};
use careertwin_prep::testing::{FakeSpeaker, FakeStt, FakeTts};

const DEMO: &str = include_str!("fixtures/ABOUT.demo.md");

async fn turn_text(events: &mut tokio::sync::mpsc::UnboundedReceiver<Event>, who: Who) -> String {
    let asked = std::time::Instant::now();
    let mut first = true;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(120), events.recv())
            .await
            .expect("no event in 2 minutes")
            .expect("the call ended");
        match event {
            Event::Caption { .. } if first => {
                first = false;
                println!("          (first sentence after {:.1}s)", asked.elapsed().as_secs_f32());
            }
            Event::Turn { who: w, text, .. } if w == who => return text,
            Event::Error(error) => panic!("error: {error}"),
            _ => {}
        }
    }
}

async fn listening(events: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) {
    loop {
        match tokio::time::timeout(Duration::from_secs(120), events.recv()).await.expect("timeout").expect("ended") {
            Event::State(State::Listening) => return,
            Event::Error(error) => panic!("error: {error}"),
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_interviews_like_a_person() {
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let track = match std::env::var("LIVE_TRACK").as_deref() {
        Ok("english") => Track::English,
        Ok("interview") => Track::Interview,
        _ => Track::Technical,
    };
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut spec = InterviewerSpec::new(track);
    spec.minutes = 20;
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "We build logistics APIs in Python with FastAPI and PostgreSQL, deployed in Docker. You will own services end to end.");
    let config = Config {
        spec,
        brief,
        knobs: Knobs::default(),
        model,
        headphones: false,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: Arc::new(llm),
        stt: FakeStt::new(&[]),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: None,
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));

    let technical = [
        "Hi Daniel, nice to meet you. I work on an order API in Python with FastAPI and PostgreSQL.",
        "Last year the latency doubled after a release. Um, I just added an index.",
        "Я не уверен как сказать... we looked at the slow query log and found the query on customer id.",
        "Honestly I have not used Kubernetes in production, only in a university course.",
    ];
    let english = [
        "Hello! I am fine, thank you. I have worked in Northwind since three years.",
        "I am building API for the orders, and yesterday I fixed one bug what was making the system slow.",
        "How to say... он был в базе данных... the query was not have the index.",
        "I want to improve my speaking because next month I have interview.",
    ];
    let answers = if track == Track::English { english } else { technical };
    let started = std::time::Instant::now();
    let first = turn_text(&mut events, Who::Interviewer).await;
    println!("\n[{:>4.1}s] INTERVIEWER: {first}", started.elapsed().as_secs_f32());
    for answer in answers {
        listening(&mut events).await;
        println!("[{:>4.1}s] YOU:         {answer}", started.elapsed().as_secs_f32());
        let sent = std::time::Instant::now();
        let _ = commands.send(Command::Say(answer.into()));
        let reply = turn_text(&mut events, Who::Interviewer).await;
        println!(
            "[{:>4.1}s] INTERVIEWER: {reply}   (reply took {:.1}s)",
            started.elapsed().as_secs_f32(),
            sent.elapsed().as_secs_f32()
        );
    }
    let _ = commands.send(Command::Finish);
}

/// The tutor on a real model: a terms lesson with a learner who knows one term, is unsure of the
/// next and says "I don't know" once. It prints the whole conversation and what the call
/// understood from the tags.
///
/// `LIVE_URL=http://127.0.0.1:3260 LIVE_MODEL=auto cargo test --test live_llm tutor -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_tutors_terms_in_your_language_then_in_english() {
    use careertwin_prep::prompts::tutor::{Lesson, Mode};
    use careertwin_prep::syllabus::Term;
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut spec = InterviewerSpec::tutoring(Track::Technical, Mode::Terms, Lang::Ru, 20);
    spec.lesson = Some(Lesson {
        topic: "Object-oriented programming".into(),
        terms: vec![
            Term {
                name: "Encapsulation".into(),
                plain: "Объект прячет своё внутреннее устройство и показывает наружу только то, что нужно.".into(),
                analogy: "Кофемашина: нажимаешь кнопку и не лезешь в бойлер.".into(),
                english: "Encapsulation means an object hides its internal state and exposes only what is needed."
                    .into(),
                follow_ups: vec!["Why is it useful?".into()],
                visual: String::new(),
            },
            Term {
                name: "Polymorphism".into(),
                plain: "Один и тот же вызов работает по-разному для разных объектов.".into(),
                analogy: "Кнопка «воспроизвести» в плеере: для музыки, видео и подкаста она делает своё.".into(),
                english:
                    "Polymorphism lets us call the same method on different objects and get behaviour that fits each."
                        .into(),
                follow_ups: vec!["Can you give an example?".into()],
                visual: String::new(),
            },
        ],
    });
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "Python, FastAPI and PostgreSQL.");
    let config = Config {
        spec,
        brief,
        knobs: Knobs::default(),
        model,
        headphones: false,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: Arc::new(llm),
        stt: FakeStt::new(&[]),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: None,
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let (tap_tx, mut tap) = tokio::sync::mpsc::unbounded_channel();
    // a relay that prints what the call understood and passes everything on
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match &event {
                Event::Tag(tag) => println!("          [tag] {tag:?}"),
                Event::TermProgress { name, half, score, .. } => {
                    println!("          [progress] {name} {half:?} {score}")
                }
                Event::Lesson(view) => println!(
                    "          [lesson] term {} of {}: {} ({:?})",
                    view.index + 1,
                    view.total,
                    view.name,
                    view.stage
                ),
                _ => {}
            }
            if tap_tx.send(event).is_err() {
                break;
            }
        }
    });
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));

    let answers = [
        "Это когда мы прячем внутренности объекта и даём доступ только через методы.",
        "Encapsulation is when an object hides its data and gives access only by methods. It helps to change the inside without breaking others.",
        "Хм, честно, не знаю, что это.",
        "Ну, наверное, когда у разных классов один метод, но они делают разное?",
        "Polymorphism means one method name, different behaviour in different classes. For example, the speak method for a dog and a cat.",
    ];
    let started = std::time::Instant::now();
    let first = turn_text(&mut tap, Who::Interviewer).await;
    println!("\n[{:>4.1}s] TUTOR: {first}", started.elapsed().as_secs_f32());
    for answer in answers {
        listening(&mut tap).await;
        println!("[{:>4.1}s] YOU:   {answer}", started.elapsed().as_secs_f32());
        let _ = commands.send(Command::Say(answer.into()));
        let reply = turn_text(&mut tap, Who::Interviewer).await;
        println!("[{:>4.1}s] TUTOR: {reply}", started.elapsed().as_secs_f32());
        assert!(!reply.contains("[["), "a tag leaked into the words: {reply}");
    }
    let _ = commands.send(Command::Finish);
}

/// A visual explanation on a real model: it prints every scene, what is said and what is shown.
///
/// `LIVE_TOPIC="How a hash map works" cargo test --test live_llm picture -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_draws_an_explanation_a_beginner_can_follow() {
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let topic = std::env::var("LIVE_TOPIC").unwrap_or_else(|_| "How a hash map works".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let started = std::time::Instant::now();
    let board = careertwin_prep::ops::storyboard(&llm, &model, &topic, "", Lang::Ru, false).await.expect("a story");
    println!("\n{} ({:.1}s) — check: {}", board.title, started.elapsed().as_secs_f32(), board.check);
    for (number, scene) in board.scenes.iter().enumerate() {
        println!("\n  scene {}: {}", number + 1, scene.say);
        for panel in &scene.panels {
            println!(
                "    [{:?} {}] {} | lines={:?} items={:?} highlight={:?} active={:?} note={:?} left={:?} right={:?}",
                panel.kind,
                panel.id,
                panel.title,
                panel.lines,
                panel.items,
                panel.highlight,
                panel.active,
                panel.note,
                panel.left.lines,
                panel.right.lines
            );
        }
    }
    assert!(board.scenes.len() >= 4, "a story of at least four scenes");
}
