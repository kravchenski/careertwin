//! How long you wait: from the end of what you say to the first word of the answer, on the real
//! offline recogniser and voice and a real model. Ignored by default, because it needs the models
//! on disk and a model gateway.
//!
//! `PREP_MODELS_DIR=~/.local/share/careertwin-prep/models PREP_LLM_MODEL=auto \
//!  cargo test --release --features local --test latency -- --ignored --nocapture`
#![cfg(all(feature = "local-stt", feature = "local-tts"))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use careertwin_prep::audio::resample::{f32_to_i16, resample};
use careertwin_prep::audio::vad::FRAME_MS;
use careertwin_prep::brief::Brief;
use careertwin_prep::config::LlmSettings;
use careertwin_prep::conversation::{self, Command, Config, Deps, Event, Knobs, State};
use careertwin_prep::llm::HttpLlm;
use careertwin_prep::models::STT_MODELS;
use careertwin_prep::prompts::Lang;
use careertwin_prep::prompts::interviewer::InterviewerSpec;
use careertwin_prep::session::{Speaker as Who, Track};
use careertwin_prep::speech::stt_local::LocalStt;
use careertwin_prep::speech::tts_local::LocalTts;
use careertwin_prep::speech::{SttHint, Tts, Voice};
use careertwin_prep::testing::FakeSpeaker;
use tokio::sync::mpsc::UnboundedReceiver;

fn models_dir() -> PathBuf {
    std::env::var_os("PREP_MODELS_DIR")
        .map_or_else(|| std::env::temp_dir().join("careertwin-prep-test-models"), PathBuf::from)
}

/// Waits for an event that satisfies `wanted`, and says when it came.
async fn next(events: &mut UnboundedReceiver<Event>, started: Instant, wanted: impl Fn(&Event) -> bool) -> Duration {
    let limit = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let event = events.recv().await.expect("the call ended");
            if let Event::Caption { text, .. } = &event {
                println!("    {:.1}s: {text}", started.elapsed().as_secs_f32());
            }
            if wanted(&event) {
                return started.elapsed();
            }
        }
    });
    limit.await.expect("nothing happened for 90 s")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the offline models and a model gateway"]
async fn the_wait_between_your_last_word_and_the_first_word_of_the_answer() {
    let dir = models_dir();
    let tts = Arc::new(LocalTts::new(dir.join("kokoro")));
    let stt = Arc::new(LocalStt::new(STT_MODELS[2].path_in(&dir), Some("en".into())));
    let voice = Voice { name: "af_heart".into(), speed: 1.0, instructions: String::new(), language: Lang::En };

    // what the person says, made by the same voice
    let question = tts
        .synthesize(
            "Can you explain what a hash map is, and why looking things up in it is so fast?".into(),
            voice.clone(),
        )
        .await
        .expect("the voice speaks");
    let pcm = f32_to_i16(&resample(&question.pcm, question.sample_rate, 16_000));
    println!("the question lasts {:.1}s", question.seconds());

    let model = std::env::var("PREP_LLM_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = Arc::new(HttpLlm::new(&LlmSettings::default(), Vec::new()).expect("the gateway address is fine"));
    let (mic_tx, mic) = tokio::sync::mpsc::channel(4096);
    let (commands_tx, commands) = tokio::sync::mpsc::unbounded_channel();
    let (events_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let spec = InterviewerSpec::new(Track::Technical);
    let config = Config {
        spec,
        brief: Brief::default(),
        knobs: Knobs::default(),
        model: model.clone(),
        headphones: false,
        voice,
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm,
        stt: stt.clone(),
        tts: Some(tts.clone() as Arc<dyn Tts>),
        speaker: FakeSpeaker::new(),
        mic: Some(mic),
        wake: Arc::new(|| {}),
    };
    let started = Instant::now();
    tokio::spawn(conversation::run(deps, config, commands, events_tx));

    let greeting = next(&mut events, started, |e| matches!(e, Event::State(State::Speaking))).await;
    println!("model {model}: the greeting starts {:.1}s after the call opens", greeting.as_secs_f32());
    next(&mut events, started, |e| matches!(e, Event::State(State::Listening))).await;

    // you speak, in real time, and stop
    let frame = (16 * FRAME_MS) as usize;
    for chunk in pcm.chunks(frame) {
        let _ = mic_tx.send(chunk.to_vec()).await;
        tokio::time::sleep(Duration::from_millis(u64::from(FRAME_MS))).await;
    }
    let stopped = Instant::now();
    let quiet = vec![20i16; frame];
    let feeding = tokio::spawn(async move {
        for _ in 0..(8_000 / FRAME_MS) {
            if mic_tx.send(quiet.clone()).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(u64::from(FRAME_MS))).await;
        }
    });
    let heard = next(&mut events, stopped, |e| matches!(e, Event::Turn { who: Who::You, .. })).await;
    println!("model {model}: it has your words {:.1}s after you stopped", heard.as_secs_f32());
    let first = next(&mut events, stopped, |e| matches!(e, Event::State(State::Speaking))).await;
    println!("model {model}: the answer starts {:.1}s after you stopped", first.as_secs_f32());
    let _ = commands_tx.send(Command::Finish);
    feeding.abort();
}
