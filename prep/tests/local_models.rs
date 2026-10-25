//! The offline engines on the real models: the voice says a sentence and the recogniser hears it
//! back. Ignored by default because it downloads about 520 MB (checked against pinned hashes).
//!
//! `cargo test --release --features local --test local_models -- --ignored --nocapture`
#![cfg(all(feature = "local-stt", feature = "local-tts"))]

use std::path::PathBuf;
use std::time::Instant;

use careertwin_prep::audio::resample::{f32_to_i16, resample};
use careertwin_prep::models::{self, STT_MODELS};
use careertwin_prep::prompts::Lang;
use careertwin_prep::speech::download::{self, Progress};
use careertwin_prep::speech::stt_local::LocalStt;
use careertwin_prep::speech::tts_local::LocalTts;
use careertwin_prep::speech::{Stt, SttHint, Tts, Voice};
use careertwin_prep::textdiff;

fn models_dir() -> PathBuf {
    std::env::var_os("PREP_MODELS_DIR")
        .map_or_else(|| std::env::temp_dir().join("careertwin-prep-test-models"), PathBuf::from)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads about 520 MB"]
async fn the_offline_voice_speaks_and_the_offline_ear_hears_it() {
    let dir = models_dir();
    let progress = Progress::default();
    let started = Instant::now();
    let whisper = &STT_MODELS[2]; // small
    download::ensure(&[models::stt_asset(whisper)], &dir, &progress).await.expect("whisper downloads and verifies");
    let kokoro = dir.join("kokoro");
    download::ensure(&models::kokoro_assets("af_heart").unwrap(), &kokoro, &progress)
        .await
        .expect("kokoro downloads and verifies");
    println!("models ready in {:.1}s", started.elapsed().as_secs_f32());

    let sentence = "Tell me about a time you fixed a slow endpoint in production.";
    let tts = LocalTts::new(kokoro);
    let voice = Voice { name: "af_heart".into(), speed: 1.0, instructions: String::new(), language: Lang::En };
    let started = Instant::now();
    let first = tts.synthesize(sentence.into(), voice.clone()).await.expect("the voice speaks");
    println!(
        "first call (loads the model): {:.1}s for {:.1}s of speech",
        started.elapsed().as_secs_f32(),
        first.seconds()
    );
    let started = Instant::now();
    let speech = tts.synthesize(sentence.into(), voice).await.expect("the voice speaks again");
    let took = started.elapsed().as_secs_f32();
    println!(
        "second call: {took:.2}s for {:.1}s of speech, real-time factor {:.2}",
        speech.seconds(),
        took / speech.seconds()
    );
    assert!(speech.seconds() > 2.0 && speech.seconds() < 12.0, "{} s", speech.seconds());
    let loudest = speech.pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(loudest > 0.1, "the voice is silent: {loudest}");

    let pcm = f32_to_i16(&resample(&speech.pcm, speech.sample_rate, 16_000));
    let stt = LocalStt::new(whisper.path_in(&dir), Some("en".into()));
    let started = Instant::now();
    let heard = stt.transcribe(pcm.clone(), SttHint::default()).await.expect("the ear hears");
    println!("first hearing (loads the model): {:.1}s -> {:?}", started.elapsed().as_secs_f32(), heard.text);
    let started = Instant::now();
    let heard = stt.transcribe(pcm, SttHint::default()).await.expect("the ear hears again");
    println!("second hearing: {:.2}s -> {:?}", started.elapsed().as_secs_f32(), heard.text);
    let comparison = textdiff::compare(sentence, &heard.text);
    println!("{:.0}% of the words matched; missed: {:?}", comparison.accuracy * 100.0, comparison.missed());
    assert!(comparison.accuracy >= 0.8, "heard {:?}", heard.text);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads about 90 MB"]
async fn the_offline_russian_voice_speaks_and_the_offline_ear_hears_it() {
    use careertwin_prep::speech::tts_piper::PiperTts;

    let dir = models_dir();
    let progress = Progress::default();
    let whisper = &STT_MODELS[2]; // small
    download::ensure(&[models::stt_asset(whisper)], &dir, &progress).await.expect("whisper downloads and verifies");
    let folder = dir.join("piper");
    download::ensure(&models::piper_assets("ru_RU-irina-medium").unwrap(), &folder, &progress)
        .await
        .expect("the Russian voice downloads and verifies against the hashes written in the program");

    let sentence = "Давай разберём, что такое хеш-таблица, и почему поиск в ней такой быстрый.";
    let tts = PiperTts::new(folder);
    let voice =
        Voice { name: "ru_RU-irina-medium".into(), speed: 1.0, instructions: String::new(), language: Lang::Ru };
    let started = Instant::now();
    let first = tts.synthesize(sentence.into(), voice.clone()).await.expect("the voice speaks Russian");
    println!(
        "first call (unpacks the program): {:.2}s for {:.1}s of speech",
        started.elapsed().as_secs_f32(),
        first.seconds()
    );
    let started = Instant::now();
    let speech = tts.synthesize(sentence.into(), voice).await.expect("the voice speaks again");
    let took = started.elapsed().as_secs_f32();
    println!(
        "second call: {took:.2}s for {:.1}s of speech, real-time factor {:.2}",
        speech.seconds(),
        took / speech.seconds()
    );
    assert!(speech.seconds() > 3.0 && speech.seconds() < 15.0, "{} s", speech.seconds());
    let loudest = speech.pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(loudest > 0.1, "the voice is silent: {loudest}");

    let pcm = f32_to_i16(&resample(&speech.pcm, speech.sample_rate, 16_000));
    let stt = LocalStt::new(whisper.path_in(&dir), Some("ru".into()));
    let heard = stt.transcribe(pcm, SttHint::default()).await.expect("the ear hears");
    println!("heard: {:?}", heard.text);
    let comparison = textdiff::compare(sentence, &heard.text);
    println!("{:.0}% of the words matched; missed: {:?}", comparison.accuracy * 100.0, comparison.missed());
    assert!(comparison.accuracy >= 0.7, "heard {:?}", heard.text);
}
