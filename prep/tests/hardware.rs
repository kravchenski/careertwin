//! Checks against the real sound card. They need a microphone and speakers, so they are ignored by
//! default: run them with `cargo test --test hardware -- --ignored --nocapture`.

use std::time::Duration;

use careertwin_prep::audio::capture::input_devices;
use careertwin_prep::audio::playback::output_devices;
use careertwin_prep::audio::vad::FRAME_SAMPLES;
use careertwin_prep::audio::{Mic, Player, Speaker};

#[test]
#[ignore = "needs a sound card"]
fn the_devices_can_be_listed() {
    println!("microphones: {:?}", input_devices());
    println!("speakers: {:?}", output_devices());
}

#[tokio::test]
#[ignore = "needs a microphone"]
async fn the_microphone_delivers_whole_frames_of_16k_audio() {
    let mut mic = Mic::open("").expect("the default microphone opens");
    println!("microphone: {}", mic.device);
    let mut frames = 0usize;
    let mut loudest = 0.0f32;
    let end = tokio::time::Instant::now() + Duration::from_millis(1200);
    while let Ok(Some(frame)) = tokio::time::timeout_at(end, mic.frames.recv()).await {
        assert_eq!(frame.len(), FRAME_SAMPLES);
        loudest = loudest.max(careertwin_prep::audio::vad::rms(&frame));
        frames += 1;
    }
    println!("{frames} frames in 1.2 s (the first 0.3 s are skipped), loudest rms {loudest:.1}");
    assert!((30..=50).contains(&frames), "{frames} frames of 20 ms in 0.9 s of audio");
    assert!(loudest < 20_000.0, "the start-up click must not get through: rms {loudest}");
}

#[tokio::test]
#[ignore = "needs speakers"]
async fn the_speakers_take_audio_and_drain_it() {
    let player = Player::open("").expect("the default speakers open");
    // 200 ms of silence: nothing is heard, but the queue must be consumed in real time
    player.enqueue(vec![0.0; 4800], 24_000);
    assert!(player.queued() > Duration::from_millis(100));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(player.queued().is_zero(), "{:?} still queued", player.queued());
    player.enqueue(vec![0.0; 48_000], 24_000);
    player.stop();
    assert!(player.queued().is_zero());
}
