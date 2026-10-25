//! Stand-ins for the microphone, the speakers, the model and the voice, so a whole conversation
//! can be run in a test (or a screenshot) without sound or network.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::{FutureExt, StreamExt};
use tokio::time::Instant;

use crate::audio::Speaker;
use crate::audio::vad::{FRAME_MS, FRAME_SAMPLES};
use crate::llm::{ChatOptions, Llm, LlmError, Message};
use crate::speech::{Speech, SpeechError, Stt, SttHint, Transcript, Tts, Voice};

/// One scripted reply of the fake model.
#[derive(Clone, Default)]
pub struct Reply {
    pub pieces: Vec<String>,
    /// Silence before the first piece, to model a slow model.
    pub delay_ms: u64,
    /// Silence between pieces.
    pub gap_ms: u64,
    pub error: Option<LlmError>,
}

impl Reply {
    pub fn text(text: &str) -> Self {
        // split on spaces the way tokens arrive
        let pieces = text.split_inclusive(' ').map(str::to_owned).collect();
        Self { pieces, ..Self::default() }
    }

    pub fn slow(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub fn failing(error: LlmError) -> Self {
        Self { error: Some(error), ..Self::default() }
    }
}

#[derive(Default)]
pub struct FakeLlm {
    replies: Mutex<VecDeque<Reply>>,
    /// Every request the program made, oldest first.
    pub requests: Mutex<Vec<Vec<Message>>>,
    /// The `max_tokens` of every request.
    pub allowances: Mutex<Vec<u32>>,
}

impl FakeLlm {
    pub fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self { replies: Mutex::new(replies.into()), requests: Mutex::default(), allowances: Mutex::default() })
    }

    pub fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn last_request(&self) -> Vec<Message> {
        self.requests.lock().unwrap().last().cloned().unwrap_or_default()
    }
}

impl Llm for FakeLlm {
    fn complete(&self, messages: Vec<Message>, _: ChatOptions) -> BoxFuture<'static, Result<String, LlmError>> {
        self.requests.lock().unwrap().push(messages);
        let reply = self.replies.lock().unwrap().pop_front();
        async move {
            if let Some(reply) = &reply
                && reply.delay_ms > 0
            {
                tokio::time::sleep(Duration::from_millis(reply.delay_ms)).await;
            }
            match reply {
                Some(Reply { error: Some(error), .. }) => Err(error),
                Some(reply) => Ok(reply.pieces.concat()),
                None => Err(LlmError::Empty),
            }
        }
        .boxed()
    }

    fn stream(&self, messages: Vec<Message>, options: ChatOptions) -> BoxStream<'static, Result<String, LlmError>> {
        self.requests.lock().unwrap().push(messages);
        self.allowances.lock().unwrap().push(options.max_tokens);
        let reply = self.replies.lock().unwrap().pop_front();
        futures_util::stream::unfold((reply, 0usize), |(reply, index)| async move {
            let reply = reply?;
            if index == 0 && reply.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(reply.delay_ms)).await;
            } else if index > 0 && reply.gap_ms > 0 {
                tokio::time::sleep(Duration::from_millis(reply.gap_ms)).await;
            }
            if index == 0 {
                if let Some(error) = reply.error.clone() {
                    return Some((Err(error), (None, 0)));
                }
                if reply.pieces.is_empty() {
                    return Some((Err(LlmError::Empty), (None, 0)));
                }
            }
            let piece = reply.pieces.get(index)?.clone();
            Some((Ok(piece), (Some(reply), index + 1)))
        })
        .boxed()
    }
}

pub struct FakeStt {
    transcripts: Mutex<VecDeque<Result<Transcript, SpeechError>>>,
    pub calls: AtomicUsize,
    pub delay_ms: u64,
}

impl FakeStt {
    pub fn new(texts: &[&str]) -> Arc<Self> {
        let transcripts =
            texts.iter().map(|t| Ok(Transcript { text: (*t).to_owned(), language: Some("en".into()) })).collect();
        Arc::new(Self { transcripts: Mutex::new(transcripts), calls: AtomicUsize::new(0), delay_ms: 150 })
    }

    pub fn failing(error: SpeechError) -> Arc<Self> {
        Arc::new(Self {
            transcripts: Mutex::new(VecDeque::from([Err(error)])),
            calls: AtomicUsize::new(0),
            delay_ms: 50,
        })
    }
}

impl Stt for FakeStt {
    fn transcribe(&self, _: Vec<i16>, _: SttHint) -> BoxFuture<'static, Result<Transcript, SpeechError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = self.transcripts.lock().unwrap().pop_front();
        let delay = self.delay_ms;
        async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            next.unwrap_or_else(|| Ok(Transcript { text: String::new(), language: None }))
        }
        .boxed()
    }
}

/// A voice that makes silence as long as the sentence would take to say.
pub struct FakeTts {
    pub spoken: Mutex<Vec<String>>,
    /// The voice each sentence was asked for in, in the same order.
    pub voices: Mutex<Vec<String>>,
    pub fail: bool,
}

impl FakeTts {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { spoken: Mutex::default(), voices: Mutex::default(), fail: false })
    }

    pub fn failing() -> Arc<Self> {
        Arc::new(Self { spoken: Mutex::default(), voices: Mutex::default(), fail: true })
    }

    pub fn said(&self) -> Vec<String> {
        self.spoken.lock().unwrap().clone()
    }

    pub fn voices_used(&self) -> Vec<String> {
        self.voices.lock().unwrap().clone()
    }
}

/// About fifteen characters a second, like speech.
pub fn spoken_seconds(text: &str) -> f32 {
    (text.chars().count() as f32 / 15.0).max(0.3)
}

impl Tts for FakeTts {
    fn id(&self) -> String {
        "fake|voice".into()
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        self.spoken.lock().unwrap().push(text.clone());
        self.voices.lock().unwrap().push(voice.name);
        let fail = self.fail;
        async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            if fail {
                return Err(SpeechError::Status("the voice engine is down".into()));
            }
            let samples = (spoken_seconds(&text) * 24_000.0) as usize;
            Ok(Speech { pcm: vec![0.05; samples], sample_rate: 24_000 })
        }
        .boxed()
    }
}

/// Speakers that play in the clock of the test.
pub struct FakeSpeaker {
    until: Mutex<Instant>,
    pub enqueued: Mutex<Vec<f32>>,
    pub stops: AtomicUsize,
}

impl FakeSpeaker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { until: Mutex::new(Instant::now()), enqueued: Mutex::default(), stops: AtomicUsize::new(0) })
    }

    pub fn stopped(&self) -> usize {
        self.stops.load(Ordering::SeqCst)
    }

    pub fn clips(&self) -> usize {
        self.enqueued.lock().unwrap().len()
    }
}

impl Speaker for FakeSpeaker {
    fn enqueue(&self, pcm: Vec<f32>, sample_rate: u32) {
        let seconds = pcm.len() as f32 / sample_rate as f32;
        self.enqueued.lock().unwrap().push(seconds);
        let mut until = self.until.lock().unwrap();
        let start = (*until).max(Instant::now());
        *until = start + Duration::from_secs_f32(seconds);
    }

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
        *self.until.lock().unwrap() = Instant::now();
    }

    fn queued(&self) -> Duration {
        self.until.lock().unwrap().saturating_duration_since(Instant::now())
    }

    fn level(&self) -> f32 {
        if self.queued().is_zero() { 0.0 } else { 0.5 }
    }
}

/// Loud and quiet 20 ms frames for a scripted microphone.
pub fn frame(amplitude: i16) -> Vec<i16> {
    (0..FRAME_SAMPLES).map(|i| if i % 2 == 0 { amplitude } else { -amplitude }).collect()
}

/// A person who talks into the microphone in real (test) time.
pub struct Talker {
    tx: tokio::sync::mpsc::Sender<Vec<i16>>,
}

impl Talker {
    pub fn new() -> (Self, tokio::sync::mpsc::Receiver<Vec<i16>>) {
        let (tx, rx) = tokio::sync::mpsc::channel(4096);
        (Self { tx }, rx)
    }

    async fn play(&self, amplitude: i16, ms: u64) {
        for _ in 0..ms / u64::from(FRAME_MS) {
            let _ = self.tx.send(frame(amplitude)).await;
            tokio::time::sleep(Duration::from_millis(u64::from(FRAME_MS))).await;
        }
    }

    pub async fn speak(&self, ms: u64) {
        self.play(3000, ms).await;
    }

    pub async fn silence(&self, ms: u64) {
        self.play(30, ms).await;
    }
}
