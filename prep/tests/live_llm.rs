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
