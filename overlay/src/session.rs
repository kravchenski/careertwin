//! Orchestrates one interview: audio in, transcript and suggestions out.
//!
//! The session knows nothing about the window. A front end gets [`Event`]s from a channel and calls `start`, `ask`,
//! `ask_screenshot`, `hint_now`, `pause` and `stop`; everything runs on threads of its own.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use crate::audio::FrameSource;
use crate::brief::InterviewBrief;
use crate::coach::{Coach, CoachError, Reply, Request};
use crate::languages::LanguageTracker;
use crate::prompts::{self, SCREENSHOT_SYSTEM, SKIP, SYSTEM};
use crate::questions::{classify, is_question, mentions, worth_an_answer};
use crate::speech::Transcriber;
use crate::transcript::{INTERVIEWER, SCREENSHOT, TranscriptLog};
use crate::vad::{FRAME_MS, Options, SAMPLE_RATE, UtteranceSegmenter, display_level};

pub const MIN_WORDS_FOR_LANGUAGE: usize = 3;
pub const LEVEL_EVERY: usize = 3;
/// Frames of 20 ms: a live caption about every 1.5 s of speech.
pub const PARTIAL_EVERY: usize = 75;
/// A caption that covers all but 0.6 s of the speech is final enough.
pub const REUSE_SAMPLES: usize = SAMPLE_RATE as usize * 6 / 10;
/// Seconds before the same transcription failure is shown again.
pub const REPEAT_ERROR_AFTER: Duration = Duration::from_secs(10);
/// Seconds of the interviewer speaking before the answer starts on what is heard.
pub const EARLY_AFTER: Duration = Duration::from_secs(3);
/// Fewer words than this is not yet something to answer.
pub const EARLY_MIN_WORDS: usize = 5;
/// Without a sign of a question, this many words are shown to the model anyway.
pub const EARLY_LONG_WORDS: usize = 12;
/// Tries on one utterance: the first words may be only the set-up of the question.
pub const EARLY_ATTEMPTS: usize = 3;
/// Seconds between two tries, and the words that must have come in between.
pub const EARLY_RETRY: Duration = Duration::from_secs(2);
pub const EARLY_RETRY_WORDS: usize = 4;
/// An early answer older than this is not the answer to what was just asked.
pub const EARLY_STALE: Duration = Duration::from_secs(30);
/// How long to wait after a sentence that ends with a question mark.
pub const QUESTION_MARK_WAIT: Duration = Duration::from_millis(100);

fn words(text: &str) -> Vec<String> {
    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\w']+").expect("a fixed pattern"));
    WORD.find_iter(&text.to_lowercase()).map(|found| found.as_str().to_owned()).collect()
}

/// Whether the answer started on `early` already answers `final_text`, the question once it is over: nearly all the
/// early words are in the final text and the final adds little. When the interviewer went on to say the real
/// question, the answer starts again on all of it.
pub fn covers(early: &str, final_text: &str) -> bool {
    let (seen, heard) = (words(early), words(final_text));
    if seen.is_empty() || heard.is_empty() {
        return false;
    }
    let shared = seen.iter().filter(|word| heard.contains(word)).count();
    shared as f32 / seen.len() as f32 >= 0.75 && heard.len().saturating_sub(seen.len()) <= 5.max(seen.len() / 3)
}

/// Whether the model's reply so far says SKIP: `Some(true)`, `Some(false)`, or `None` while it cannot be told yet.
pub fn skip_verdict(text: &str, final_reply: bool) -> Option<bool> {
    let head = text.trim_start();
    if head.is_empty() {
        return None;
    }
    let upper = head.to_uppercase();
    if upper.starts_with(SKIP) {
        return Some(head.len() == SKIP.len() || !head[SKIP.len()..].chars().next().is_some_and(char::is_alphanumeric));
    }
    if SKIP.starts_with(&upper) {
        return if final_reply { Some(true) } else { None }; // "SK" so far: it may still become SKIP
    }
    Some(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Loading,
    Listening,
    Paused,
    Thinking,
    Error,
}

/// What the window is told.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Turn {
        speaker: String,
        text: String,
    },
    SuggestionStart {
        question: String,
    },
    Chunk(String),
    Done(String),
    /// The answer shown last was for part of a question that has since been said in full: drop it.
    Discard,
    Status(String),
    State(State),
    Level {
        speaker: String,
        level: f32,
    },
    Language(String),
    Source(String),
    Partial(String),
    Error(String),
}

/// What asks the model: streams a reply to `request` through `on_chunk`.
pub trait Model: Send + Sync {
    fn ask(
        &self,
        request: &Request,
        cancel: &CancellationToken,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<Reply, CoachError>;
    /// The model may say SKIP to words that are not a question for the candidate.
    fn supports_listening(&self) -> bool {
        true
    }
}

/// The real model: the coach, driven from the threads of the session.
pub struct CoachModel {
    pub coach: Coach,
    pub handle: Handle,
}

impl Model for CoachModel {
    fn ask(
        &self,
        request: &Request,
        cancel: &CancellationToken,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<Reply, CoachError> {
        self.handle.block_on(self.coach.ask(request, cancel, |piece| on_chunk(piece)))
    }
}

/// What the search of the candidate's project found for a question: code excerpts and the part of the dossier
/// that bears on it. Both are empty when the project has nothing to say about the question.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub code: String,
    pub dossier: String,
}

/// Finds what the candidate's project says about a question.
pub type Retrieve = Arc<dyn Fn(&str) -> Found + Send + Sync>;

/// What a request is made from, besides the question.
pub struct Context {
    pub brief: InterviewBrief,
    pub style: String,
    pub layout: String,
    /// The excerpts of the candidate's project that match a question, when there is a project.
    pub retrieve: Option<Retrieve>,
}

#[derive(Clone, Debug)]
pub struct Timing {
    pub debounce: Duration,
    /// Speak for this long and the answer starts on what is heard; `None` turns it off.
    pub early_after: Option<Duration>,
    pub early_retry: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self { debounce: Duration::from_millis(400), early_after: Some(EARLY_AFTER), early_retry: EARLY_RETRY }
    }
}

/// A queue the recogniser thread waits on, which can also say whether anything is waiting.
struct Queue<T> {
    items: Mutex<VecDeque<T>>,
    ready: Condvar,
}

impl<T> Queue<T> {
    fn new() -> Self {
        Self { items: Mutex::new(VecDeque::new()), ready: Condvar::new() }
    }

    fn push(&self, item: T) {
        if let Ok(mut items) = self.items.lock() {
            items.push_back(item);
        }
        self.ready.notify_one();
    }

    fn pop(&self) -> T {
        let mut items = self.items.lock().expect("the queue lock is never held across a panic");
        loop {
            if let Some(item) = items.pop_front() {
                return item;
            }
            items = self.ready.wait(items).expect("the queue lock is never held across a panic");
        }
    }

    fn is_empty(&self) -> bool {
        self.items.lock().is_ok_and(|items| items.is_empty())
    }
}

/// (speaker, the audio, whether it is a live caption of speech still going on); `None` ends the thread.
type Item = Option<(String, Vec<i16>, bool)>;

#[derive(Default)]
struct Early {
    /// The question an answer was started on while it was still being said, and when.
    question: Option<(String, Instant)>,
    /// Tries to answer the utterance now being heard before it ends.
    attempts: usize,
    /// When the last try began, and its words.
    last: Option<(Instant, usize)>,
    /// A try is still being judged or written.
    busy: bool,
    /// A try became a card on the screen.
    card: bool,
    /// Finished parts of this question, said before.
    parts: Option<Vec<String>>,
    /// When an early answer was put on the screen.
    shown: Option<Instant>,
}

#[derive(Default)]
struct Pending {
    parts: Vec<String>,
    /// Bumped to cancel the timer that is waiting.
    generation: u64,
}

pub struct Session {
    inner: Arc<Inner>,
}

struct Inner {
    context: Arc<Mutex<Context>>,
    model: Arc<dyn Model>,
    events: Sender<Event>,
    timing: Timing,
    log: Mutex<TranscriptLog>,
    language: Mutex<LanguageTracker>,
    transcriber: Mutex<Option<Arc<dyn Transcriber>>>,
    ready: AtomicBool,
    paused: AtomicBool,
    stopping: AtomicBool,
    listening: AtomicBool,
    queue: Queue<Item>,
    pending: Mutex<Pending>,
    early: Mutex<Early>,
    cancel: Mutex<CancellationToken>,
    suggest_lock: Mutex<()>,
    sources: Mutex<Vec<Arc<dyn FrameSource>>>,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
    stt_started: AtomicBool,
}

impl Session {
    pub fn new(
        context: Arc<Mutex<Context>>,
        model: Arc<dyn Model>,
        events: Sender<Event>,
        log: TranscriptLog,
        language: Option<&str>,
        timing: Timing,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                context,
                model,
                events,
                timing,
                log: Mutex::new(log),
                language: Mutex::new(LanguageTracker::new(language)),
                transcriber: Mutex::new(None),
                ready: AtomicBool::new(true),
                paused: AtomicBool::new(false),
                stopping: AtomicBool::new(false),
                listening: AtomicBool::new(false),
                queue: Queue::new(),
                pending: Mutex::new(Pending::default()),
                early: Mutex::new(Early::default()),
                cancel: Mutex::new(CancellationToken::new()),
                suggest_lock: Mutex::new(()),
                sources: Mutex::new(Vec::new()),
                threads: Mutex::new(Vec::new()),
                stt_started: AtomicBool::new(false),
            }),
        }
    }

    /// The transcript so far, for saving or for the export.
    pub fn with_log<R>(&self, work: impl FnOnce(&mut TranscriptLog) -> R) -> R {
        work(&mut self.inner.log.lock().expect("the log lock is never held across a panic"))
    }

    /// Listen to the sources; with none only `ask` works. `transcriber` loads in the background.
    pub fn start(&self, transcriber: Option<Arc<dyn Transcriber>>, sources: Vec<(String, Arc<dyn FrameSource>)>) {
        self.inner.listening.store(!sources.is_empty(), Ordering::SeqCst);
        match transcriber {
            Some(transcriber) => self.set_transcriber(Some(transcriber), true),
            None => self.inner.ready.store(true, Ordering::SeqCst),
        }
        if sources.is_empty() {
            self.inner.event(Event::State(State::Idle));
        } else {
            self.inner.ensure_stt();
            self.inner.start_listeners(sources);
        }
    }

    /// Replace the audio sources while the interview runs (none stops listening).
    pub fn set_sources(&self, sources: Vec<(String, Arc<dyn FrameSource>)>) {
        self.inner.stop_listeners();
        self.inner.listening.store(!sources.is_empty(), Ordering::SeqCst);
        if !sources.is_empty() {
            self.inner.ensure_stt();
            self.inner.start_listeners(sources);
        }
        self.inner.event(Event::State(self.inner.idle_state()));
    }

    /// Switch speech recognition; the old one keeps working until the new one is loaded.
    pub fn set_transcriber(&self, transcriber: Option<Arc<dyn Transcriber>>, first: bool) {
        let Some(transcriber) = transcriber else {
            *self.inner.transcriber.lock().expect("the lock is never held across a panic") = None;
            return;
        };
        {
            let language = self.inner.language.lock().expect("the lock is never held across a panic");
            transcriber.set_language(language.fixed.as_deref());
        }
        let inner = self.inner.clone();
        self.inner.spawn("load", move || inner.load(transcriber, first));
    }

    pub fn pause(&self, value: bool) {
        self.inner.paused.store(value, Ordering::SeqCst);
        self.inner.event(Event::State(if value { State::Paused } else { State::Listening }));
    }

    /// Fix the interview language (`None` returns to auto-detection).
    pub fn set_language(&self, value: Option<&str>) -> Option<String> {
        let code = self.inner.language.lock().expect("the lock is never held across a panic").set_fixed(value);
        if let Some(transcriber) =
            self.inner.transcriber.lock().expect("the lock is never held across a panic").as_ref()
        {
            transcriber.set_language(code.as_deref());
        }
        if let Some(code) = &code {
            self.inner.event(Event::Language(code.clone()));
        }
        code
    }

    pub fn language(&self) -> Option<String> {
        self.inner.language.lock().expect("the lock is never held across a panic").current.clone()
    }

    /// Stop the answer being written (the Stop button); listening goes on.
    pub fn cancel(&self) {
        self.inner.cancel_current();
        self.inner.event(Event::State(self.inner.idle_state()));
    }

    pub fn stop(&self) {
        let inner = &self.inner;
        inner.stopping.store(true, Ordering::SeqCst);
        inner.cancel_current();
        inner.pending.lock().expect("the lock is never held across a panic").generation += 1;
        inner.stop_listeners();
        inner.queue.push(None);
        let threads: Vec<_> = inner.threads.lock().expect("the lock is never held across a panic").drain(..).collect();
        for thread in threads {
            let _ = thread.join();
        }
    }

    /// Get a suggestion for a typed or pasted question right now.
    pub fn ask(&self, question: &str) {
        let question = question.split_whitespace().collect::<Vec<_>>().join(" ");
        if question.is_empty() {
            return;
        }
        self.inner.log.lock().expect("the lock is never held across a panic").add(INTERVIEWER, &question);
        self.inner.event(Event::Turn { speaker: INTERVIEWER.to_owned(), text: question.clone() });
        self.inner.clone().suggest(question, None, None, None, None);
    }

    /// Answer whatever the screenshot shows: a coding task, a slide, a question. `image` is a data URL.
    pub fn ask_screenshot(&self, image: &str, started_at: Option<Instant>) {
        self.inner.log.lock().expect("the lock is never held across a panic").add(INTERVIEWER, SCREENSHOT);
        self.inner.event(Event::Turn { speaker: INTERVIEWER.to_owned(), text: SCREENSHOT.to_owned() });
        self.inner.clone().suggest(
            SCREENSHOT.to_owned(),
            Some(image.to_owned()),
            Some(started_at.unwrap_or_else(Instant::now)),
            None,
            None,
        );
    }

    /// Suggest for the last thing the interviewer said, whether or not it sounded like a question.
    pub fn hint_now(&self) {
        let last = self
            .inner
            .log
            .lock()
            .expect("the lock is never held across a panic")
            .turns
            .iter()
            .rev()
            .find(|turn| turn.speaker == INTERVIEWER && turn.text != SCREENSHOT)
            .map(|turn| turn.text.clone());
        if let Some(text) = last {
            self.inner.clone().suggest(text, None, None, None, None);
        }
    }
}

impl Inner {
    fn event(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn spawn(&self, name: &str, work: impl FnOnce() + Send + 'static) {
        if let Ok(handle) = std::thread::Builder::new().name(name.to_owned()).spawn(work) {
            self.threads.lock().expect("the lock is never held across a panic").push(handle);
        }
    }

    fn idle_state(&self) -> State {
        if self.paused.load(Ordering::SeqCst) {
            State::Paused
        } else if self.listening.load(Ordering::SeqCst) {
            State::Listening
        } else {
            State::Idle
        }
    }

    fn cancel_current(&self) {
        self.cancel.lock().expect("the lock is never held across a panic").cancel();
    }

    fn ensure_stt(self: &Arc<Self>) {
        if self.stt_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let inner = self.clone();
        self.spawn("stt", move || inner.transcribe_loop());
    }

    fn start_listeners(self: &Arc<Self>, sources: Vec<(String, Arc<dyn FrameSource>)>) {
        for (speaker, source) in sources {
            self.sources.lock().expect("the lock is never held across a panic").push(source.clone());
            let inner = self.clone();
            self.spawn(&format!("listen-{speaker}"), move || inner.listen(&speaker, source.as_ref()));
        }
    }

    fn stop_listeners(&self) {
        let sources: Vec<_> = self.sources.lock().expect("the lock is never held across a panic").drain(..).collect();
        for source in sources {
            source.close();
        }
    }

    fn load(self: Arc<Self>, transcriber: Arc<dyn Transcriber>, first: bool) {
        if first {
            self.event(Event::State(State::Loading));
        }
        self.event(Event::Status("Loading speech model…".to_owned()));
        self.ready.store(false, Ordering::SeqCst);
        if let Err(error) = transcriber.load() {
            self.ready.store(
                self.transcriber.lock().expect("the lock is never held across a panic").is_some(),
                Ordering::SeqCst,
            );
            self.event(Event::State(State::Error));
            self.event(Event::Error(error));
            return;
        }
        *self.transcriber.lock().expect("the lock is never held across a panic") = Some(transcriber);
        self.ready.store(true, Ordering::SeqCst);
        self.event(Event::State(self.idle_state()));
        self.event(Event::Status("Listening".to_owned()));
    }

    fn listen(self: Arc<Self>, speaker: &str, source: &dyn FrameSource) {
        // A call monitor is digitally silent between words, so it can use a low threshold and still hear a quiet
        // voice; a microphone has room noise and needs a higher one.
        let mut segmenter = UtteranceSegmenter::new(Options {
            min_threshold: if speaker == INTERVIEWER { 120.0 } else { 250.0 },
            ..Options::default()
        });
        let (mut count, mut since_partial, mut early_taken) = (0usize, 0usize, false);
        let early_frames = self.timing.early_after.map_or(0, |after| after.as_millis() as usize / FRAME_MS);
        let result = source.run(&mut |frame| {
            if self.stopping.load(Ordering::SeqCst) {
                return false;
            }
            if self.paused.load(Ordering::SeqCst) {
                return true;
            }
            count += 1;
            if count % LEVEL_EVERY == 0 {
                self.event(Event::Level { speaker: speaker.to_owned(), level: display_level(frame) });
            }
            if let Some(utterance) = segmenter.push(frame) {
                since_partial = 0;
                early_taken = false;
                self.queue.push(Some((speaker.to_owned(), utterance, false)));
            } else if speaker == INTERVIEWER && segmenter.active() {
                since_partial += 1;
                // a caption is made when the speech reaches the early mark, not at the next beat
                let at_mark = early_frames > 0 && segmenter.frame_count() >= early_frames && !early_taken;
                if (at_mark || since_partial >= PARTIAL_EVERY)
                    && let Some(heard) = segmenter.snapshot()
                {
                    since_partial = 0;
                    early_taken = early_taken || at_mark;
                    self.queue.push(Some((speaker.to_owned(), heard, true)));
                }
            } else {
                since_partial = 0;
                early_taken = false;
            }
            true
        });
        if let Err(error) = result {
            self.event(Event::State(State::Error));
            self.event(Event::Error(format!("Audio from {} stopped: {error}", speaker.to_lowercase())));
        }
    }

    fn transcribe_loop(self: Arc<Self>) {
        let mut last_partial: Option<(String, usize, String)> = None;
        let mut last_failure: (String, Option<Instant>) = (String::new(), None);
        loop {
            let Some((speaker, pcm, partial)) = self.queue.pop() else { return };
            if self.stopping.load(Ordering::SeqCst) {
                return;
            }
            let transcriber = self.transcriber.lock().expect("the lock is never held across a panic").clone();
            let Some(transcriber) = transcriber.filter(|_| self.ready.load(Ordering::SeqCst)) else { continue };
            if partial && !self.queue.is_empty() {
                continue; // newer audio is already waiting; a caption of the old state is stale
            }
            let reuse = match (&last_partial, partial) {
                (Some((last_speaker, samples, text)), false) if *last_speaker == speaker => {
                    let missing = pcm.len() as i64 - *samples as i64;
                    (0..=REUSE_SAMPLES as i64).contains(&missing).then(|| text.clone())
                }
                _ => None,
            };
            let vocabulary = self.context.lock().expect("the lock is never held across a panic").brief.vocabulary();
            let heard = match reuse {
                Some(text) => Ok(crate::speech::Transcript { text, ..Default::default() }),
                None => transcriber.transcribe(&pcm, &vocabulary),
            };
            let transcript = match heard {
                Ok(transcript) => transcript,
                Err(error) => {
                    let message = format!("Transcription failed: {error}");
                    let now = Instant::now();
                    if message != last_failure.0
                        || last_failure.1.is_none_or(|at| now.duration_since(at) > REPEAT_ERROR_AFTER)
                    {
                        self.event(Event::Error(message.clone())); // a wrong key would otherwise repeat with every caption
                    }
                    last_failure = (message, Some(now));
                    continue;
                }
            };
            let text = transcript.text.trim().to_owned();
            if partial {
                if !text.is_empty() && self.queue.is_empty() {
                    last_partial = Some((speaker.clone(), pcm.len(), text.clone()));
                    self.event(Event::Partial(text.clone()));
                    self.answer_early(&speaker, &text, pcm.len() as f32 / SAMPLE_RATE as f32);
                }
            } else if !text.is_empty() {
                last_partial = None;
                self.end_of_utterance();
                self.event(Event::Partial(String::new()));
                self.observe_language(&text, &transcript);
                self.handle_text(&speaker, &text);
            } else {
                last_partial = None;
                self.end_of_utterance();
                self.event(Event::Partial(String::new()));
            }
        }
    }

    fn end_of_utterance(&self) {
        let mut early = self.early.lock().expect("the lock is never held across a panic");
        early.attempts = 0;
        early.last = None;
        early.card = false;
        early.parts = None;
    }

    /// The interviewer has been speaking for a while: start answering what was heard.
    ///
    /// Waiting for the end of a long question wastes the seconds the answer could already be written in. The first
    /// words may be only the set-up ("imagine a service that...") and the model then says SKIP, so it is tried again
    /// as more is said, a few times at most, until an answer is on the screen. When the question is over,
    /// `flush_pending` keeps that answer if it covers the question, or starts again on the whole of it.
    fn answer_early(self: &Arc<Self>, speaker: &str, text: &str, seconds: f32) {
        let Some(early_after) = self.timing.early_after else { return };
        if speaker != INTERVIEWER || self.paused.load(Ordering::SeqCst) {
            return;
        }
        {
            let early = self.early.lock().expect("the lock is never held across a panic");
            if early.card || early.busy {
                return;
            }
        }
        let count = words(text).len();
        if seconds < early_after.as_secs_f32() || count < EARLY_MIN_WORDS {
            return;
        }
        let language = self.language.lock().expect("the lock is never held across a panic").current.clone();
        let asks = is_question(text, language.as_deref());
        if !(asks || count >= EARLY_LONG_WORDS) {
            return; // a few words that sound like a statement: wait to hear more or the end of it
        }
        let now = Instant::now();
        let (question, parts) = {
            let mut early = self.early.lock().expect("the lock is never held across a panic");
            // a statement is shown to the model once: it is probably the set-up, and every try is a request
            if early.attempts >= if asks { EARLY_ATTEMPTS } else { 1 } {
                return;
            }
            if let Some((at, last_words)) = early.last
                && (now.duration_since(at) < self.timing.early_retry || count < last_words + EARLY_RETRY_WORDS)
            {
                return;
            }
            early.attempts += 1;
            early.last = Some((now, count));
            early.busy = true;
            let mut pending = self.pending.lock().expect("the lock is never held across a panic");
            pending.generation += 1; // a part of this question waiting to be sent goes with it
            if early.parts.is_none() {
                early.parts = Some(std::mem::take(&mut pending.parts));
            }
            let parts_taken = early.parts.clone().unwrap_or_default();
            let mut all = parts_taken.clone();
            all.push(text.to_owned());
            let question = all.join(" ");
            early.question = Some((question.clone(), now));
            (question, parts_taken.len())
        };
        // the parts of this question already said are turns in the log; they are in the question
        self.clone().suggest(question, None, None, Some(parts), Some("hearing"));
    }

    fn observe_language(&self, text: &str, transcript: &crate::speech::Transcript) {
        if text.split_whitespace().count() < MIN_WORDS_FOR_LANGUAGE {
            return;
        }
        let changed = self
            .language
            .lock()
            .expect("the lock is never held across a panic")
            .observe(transcript.language.as_deref(), transcript.probability);
        if let Some(code) = changed {
            self.event(Event::Language(code));
        }
    }

    fn handle_text(self: &Arc<Self>, speaker: &str, text: &str) {
        self.log.lock().expect("the lock is never held across a panic").add(speaker, text);
        self.event(Event::Turn { speaker: speaker.to_owned(), text: text.to_owned() });
        if speaker != INTERVIEWER {
            return;
        }
        let language = self.language.lock().expect("the lock is never held across a panic").current.clone();
        let vocabulary = self.context.lock().expect("the lock is never held across a panic").brief.vocabulary();
        if !(worth_an_answer(text, language.as_deref()) || mentions(text, &vocabulary)) {
            return;
        }
        let generation = {
            let mut pending = self.pending.lock().expect("the lock is never held across a panic");
            pending.parts.push(text.to_owned());
            pending.generation += 1;
            pending.generation
        };
        // A sentence that ends with a question mark is complete: do not wait for a second part.
        let wait = if text.trim_end().ends_with(['?', '？']) { QUESTION_MARK_WAIT } else { self.timing.debounce };
        let wait = wait.min(self.timing.debounce);
        let inner = self.clone();
        self.spawn("debounce", move || {
            std::thread::sleep(wait);
            if inner.pending.lock().is_ok_and(|pending| pending.generation == generation) {
                inner.flush_pending();
            }
        });
    }

    fn flush_pending(self: &Arc<Self>) {
        let question = {
            let mut pending = self.pending.lock().expect("the lock is never held across a panic");
            let question = pending.parts.join(" ");
            pending.parts.clear();
            question
        };
        let early = self.early.lock().expect("the lock is never held across a panic").question.take();
        if question.is_empty() || self.stopping.load(Ordering::SeqCst) {
            return;
        }
        if let Some((early_question, at)) = early
            && at.elapsed() < EARLY_STALE
            && covers(&early_question, &question)
        {
            return; // the answer started while the question was still being asked is for this
        }
        self.clone().suggest(question, None, None, None, Some("heard"));
    }

    /// Answer `question`. `early` is the number of finished turns of the log that are part of it while it is still
    /// being said and so is not a turn itself; for a finished question it is `None`. `listening` is set when the
    /// words came from the call audio, so the model may judge them not a question.
    fn suggest(
        self: Arc<Self>,
        question: String,
        image: Option<String>,
        started_at: Option<Instant>,
        early: Option<usize>,
        listening: Option<&'static str>,
    ) {
        let cancel = CancellationToken::new();
        {
            let mut current = self.cancel.lock().expect("the lock is never held across a panic");
            current.cancel();
            *current = cancel.clone();
        }
        let inner = self.clone();
        self.spawn("coach", move || {
            inner.write_suggestion(&question, &cancel, image.as_deref(), started_at, early, listening);
            if early.is_some() {
                inner.early.lock().expect("the lock is never held across a panic").busy = false;
            }
        });
    }

    /// The answer has its first words: put its place on the screen. An answer started on part of a question is
    /// replaced when the whole of it has been said; a typed question leaves it alone.
    fn announce(&self, question: &str, early: Option<usize>, listening: Option<&str>) {
        let replaced = {
            let mut state = self.early.lock().expect("the lock is never held across a panic");
            let replaced = state.shown;
            if early.is_some() {
                state.shown = Some(Instant::now());
                state.card = true;
            } else if listening.is_some() {
                state.shown = None;
            }
            replaced
        };
        if listening == Some("heard") && replaced.is_some_and(|at| at.elapsed() < EARLY_STALE) {
            self.event(Event::Discard);
        }
        self.event(Event::SuggestionStart { question: question.to_owned() });
    }

    fn build_request(
        &self,
        question: &str,
        image: Option<&str>,
        early: Option<usize>,
        listening: Option<&str>,
    ) -> Request {
        let context = self.context.lock().expect("the lock is never held across a panic");
        let language = self.language.lock().expect("the lock is never held across a panic").current.clone();
        if let Some(image) = image {
            let prompt = prompts::screenshot_prompt(&context.brief, language.as_deref(), &context.layout);
            return Request::screenshot(SCREENSHOT_SYSTEM, &prompt, image);
        }
        // the question is the last turn of the log, except what is still being said
        let history = self.log.lock().expect("the lock is never held across a panic").history(6);
        let keep = history.len().saturating_sub(early.unwrap_or(1));
        let found = context.retrieve.as_ref().map(|retrieve| retrieve(question)).unwrap_or_default();
        let (kind, history) = (classify(question), &history[..keep]);
        let (style, layout) = (&context.style, &context.layout);
        // a question the project answers gets the lean background: the dossier part and the code, not the whole CV
        let prompt = if found.code.is_empty() {
            prompts::user_prompt(&context.brief, question, kind, history, language.as_deref(), style, layout, "", listening)
        } else {
            prompts::project_prompt(
                &context.brief,
                question,
                kind,
                history,
                language.as_deref(),
                style,
                layout,
                &found.code,
                &found.dossier,
                listening,
            )
        };
        Request::text(SYSTEM, &prompt)
    }

    fn write_suggestion(
        self: &Arc<Self>,
        question: &str,
        cancel: &CancellationToken,
        image: Option<&str>,
        started_at: Option<Instant>,
        early: Option<usize>,
        listening: Option<&str>,
    ) {
        let _turn = self.suggest_lock.lock().expect("the lock is never held across a panic");
        if cancel.is_cancelled() {
            return;
        }
        if self.context.lock().expect("the lock is never held across a panic").brief.is_empty() && image.is_none() {
            // a task on the screen needs no CV
            self.event(Event::State(State::Error));
            self.event(Event::Error(
                "No context yet: add your CV or write about yourself in Settings → Interview".to_owned(),
            ));
            return;
        }
        let request = self.build_request(question, image, early, listening);
        self.event(Event::State(State::Thinking));
        // Words from the call audio may not be a question: the model says SKIP, and then nothing is shown. So the
        // answer is announced at its first words, not before them.
        let judging = listening.is_some() && self.model.supports_listening();
        if !judging {
            self.announce(question, early, listening);
        }
        let stream_cancel = cancel.child_token();
        let (mut text, mut announced, mut skipped) = (String::new(), !judging, false);
        let mut first_words: Option<Instant> = None;
        let result = self.model.ask(&request, &stream_cancel, &mut |chunk| {
            if cancel.is_cancelled() || skipped {
                return;
            }
            first_words.get_or_insert_with(Instant::now);
            text.push_str(chunk);
            if announced {
                self.event(Event::Chunk(chunk.to_owned()));
                return;
            }
            match skip_verdict(&text, false) {
                None => {} // too little to tell yet
                Some(true) => {
                    skipped = true;
                    stream_cancel.cancel();
                }
                Some(false) => {
                    announced = true;
                    self.announce(question, early, listening);
                    self.event(Event::Chunk(text.clone()));
                }
            }
        });
        if cancel.is_cancelled() {
            return;
        }
        if skipped {
            self.event(Event::State(self.idle_state()));
            self.event(Event::Status("That was not a question for you, so no answer".to_owned()));
            return;
        }
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => {
                self.event(Event::State(State::Error));
                self.event(Event::Error(error.message));
                return;
            }
        };
        if !announced && !text.trim().is_empty() && skip_verdict(&text, true) != Some(true) {
            self.announce(question, early, listening);
            self.event(Event::Chunk(text.clone()));
            announced = true;
        }
        if !announced {
            self.event(Event::State(self.idle_state()));
            if !text.trim().is_empty() {
                self.event(Event::Status("That was not a question for you, so no answer".to_owned()));
            }
            return;
        }
        self.event(Event::State(self.idle_state()));
        if let Some(model) = &reply.fell_back {
            self.event(Event::Status(format!("The first model is rate limited or down: answered by {model}")));
        }
        if reply.finish.as_deref() == Some("length") {
            self.event(Event::Status("Cut off: the model ran out of room before it finished. Ask again, or pick a model with longer answers in Settings → AI.".to_owned()));
        }
        if !text.trim().is_empty() {
            let answer = prompts::parse_suggestion(&text).sections.answer;
            self.log
                .lock()
                .expect("the lock is never held across a panic")
                .suggestions
                .push((question.to_owned(), answer));
            self.event(Event::Done(text.clone()));
            if image.is_some()
                && let (Some(started), Some(first)) = (started_at, first_words)
            {
                self.event(Event::Status(format!(
                    "Screenshot: first words after {:.1} s, whole answer after {:.1} s",
                    first.duration_since(started).as_secs_f32(),
                    started.elapsed().as_secs_f32()
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::{Scripted, Transcript};
    use crate::vad::FRAME_SAMPLES;
    use std::sync::mpsc::{Receiver, channel};

    #[test]
    fn an_early_answer_covers_the_question_when_little_was_added() {
        assert!(covers("what is a left join and", "What is a left join and when would you use it"));
        assert!(!covers(
            "imagine a service that",
            "Imagine a service that sends emails: how would you retry failed sends so that nothing is lost or sent twice?"
        ));
        assert!(!covers("", "anything") && !covers("anything", ""));
        assert!(!covers("tell me about kafka", "describe your experience with spark"));
    }

    #[test]
    fn a_reply_that_says_skip_is_told_early_and_a_partial_word_waits() {
        assert_eq!(skip_verdict("", false), None);
        assert_eq!(skip_verdict("SKIP", false), Some(true));
        assert_eq!(skip_verdict("  skip\n", false), Some(true));
        assert_eq!(skip_verdict("SKIPPING stones", false), Some(false)); // a word that only starts like it
        assert_eq!(skip_verdict("SK", false), None);
        assert_eq!(skip_verdict("SK", true), Some(true));
        assert_eq!(skip_verdict("POINTS:", false), Some(false));
        assert_eq!(skip_verdict("ANSWER: no", true), Some(false));
    }

    /// A model that answers from a script: (what it says in pieces, or an error).
    struct FakeModel {
        replies: Mutex<VecDeque<Result<Vec<String>, String>>>,
        requests: Mutex<Vec<Request>>,
        delay: Duration,
        listening: bool,
    }

    impl FakeModel {
        fn new(replies: Vec<Result<Vec<&str>, &str>>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(
                    replies
                        .into_iter()
                        .map(|reply| {
                            reply.map(|pieces| pieces.into_iter().map(str::to_owned).collect()).map_err(str::to_owned)
                        })
                        .collect(),
                ),
                requests: Mutex::new(Vec::new()),
                delay: Duration::ZERO,
                listening: true,
            })
        }

        fn asked(&self) -> Vec<String> {
            self.requests.lock().unwrap().iter().map(|request| request.prompt.clone()).collect()
        }
    }

    impl Model for FakeModel {
        fn ask(
            &self,
            request: &Request,
            cancel: &CancellationToken,
            on_chunk: &mut dyn FnMut(&str),
        ) -> Result<Reply, CoachError> {
            self.requests.lock().unwrap().push(request.clone());
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(vec!["ANSWER:\n".into(), "default".into()]));
            let pieces = reply.map_err(CoachError::new)?;
            let mut full = String::new();
            for piece in pieces {
                if cancel.is_cancelled() {
                    return Err(CoachError::new("cancelled"));
                }
                std::thread::sleep(self.delay);
                on_chunk(&piece);
                full.push_str(&piece);
            }
            Ok(Reply { text: full, finish: Some("stop".into()), fell_back: None })
        }

        fn supports_listening(&self) -> bool {
            self.listening
        }
    }

    /// Frames played to the session, one after the other, with a short wait so the threads keep up.
    struct Played {
        frames: Mutex<VecDeque<Vec<i16>>>,
        closed: AtomicBool,
        pace: Duration,
    }

    impl Played {
        fn new(pace: Duration) -> Arc<Self> {
            Arc::new(Self { frames: Mutex::new(VecDeque::new()), closed: AtomicBool::new(false), pace })
        }

        fn say(&self, frames: usize) {
            for _ in 0..frames {
                self.frames
                    .lock()
                    .unwrap()
                    .push_back((0..FRAME_SAMPLES).map(|n| if n % 2 == 0 { 3000 } else { -3000 }).collect());
            }
        }

        fn silence(&self, frames: usize) {
            for _ in 0..frames {
                self.frames.lock().unwrap().push_back(vec![0; FRAME_SAMPLES]);
            }
        }
    }

    impl FrameSource for Played {
        fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), crate::audio::AudioError> {
            while !self.closed.load(Ordering::SeqCst) {
                let frame = self.frames.lock().unwrap().pop_front();
                match frame {
                    Some(frame) => {
                        if !on_frame(&frame) {
                            return Ok(());
                        }
                        std::thread::sleep(self.pace);
                    }
                    None => std::thread::sleep(Duration::from_millis(5)),
                }
            }
            Ok(())
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    struct Fixture {
        session: Session,
        events: Receiver<Event>,
        model: Arc<FakeModel>,
    }

    fn fixture(model: Arc<FakeModel>, timing: Timing) -> Fixture {
        let (tx, rx) = channel();
        let context = Arc::new(Mutex::new(Context {
            brief: InterviewBrief::new("## Skills\n| Strong | Python, SQL |\n"),
            style: "standard".into(),
            layout: "lines".into(),
            retrieve: None,
        }));
        let dir = tempfile::tempdir().unwrap();
        let session =
            Session::new(context, model.clone(), tx, TranscriptLog::new(dir.path().join("t"), "test"), None, timing);
        Fixture { session, events: rx, model }
    }

    fn quick() -> Timing {
        Timing { debounce: Duration::from_millis(20), early_after: None, early_retry: Duration::from_millis(10) }
    }

    /// Everything that arrives until `done` says so, or the time is out.
    fn collect(events: &Receiver<Event>, done: impl Fn(&[Event]) -> bool) -> Vec<Event> {
        let mut seen = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !done(&seen) {
            if let Ok(event) = events.recv_timeout(Duration::from_millis(20)) {
                seen.push(event);
            }
        }
        seen
    }

    fn has(seen: &[Event], wanted: &Event) -> bool {
        seen.iter().any(|event| event == wanted)
    }

    fn chunks(seen: &[Event]) -> String {
        seen.iter()
            .filter_map(|event| if let Event::Chunk(text) = event { Some(text.as_str()) } else { None })
            .collect()
    }

    #[test]
    fn a_typed_question_is_logged_announced_streamed_and_finished() {
        let f = fixture(
            FakeModel::new(vec![Ok(vec!["POINTS:\n- A **join**\n", "ANSWER:\nIt keeps the left rows."])]),
            quick(),
        );
        f.session.ask("  What  is a left join? ");
        let seen = collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        assert!(has(&seen, &Event::Turn { speaker: INTERVIEWER.into(), text: "What is a left join?".into() }));
        assert!(has(&seen, &Event::SuggestionStart { question: "What is a left join?".into() }));
        assert_eq!(chunks(&seen), "POINTS:\n- A **join**\nANSWER:\nIt keeps the left rows.");
        assert!(has(&seen, &Event::State(State::Thinking)) && has(&seen, &Event::State(State::Idle)));
        f.session.with_log(|log| {
            assert_eq!(log.suggestions, [("What is a left join?".to_owned(), "It keeps the left rows.".to_owned())])
        });
        let prompt = &f.model.asked()[0];
        assert!(prompt.contains("INTERVIEWER'S QUESTION: What is a left join?") && prompt.contains("TYPE: definition"));
        f.session.stop();
    }

    #[test]
    fn a_question_the_project_answers_gets_the_lean_request_with_the_code_and_the_dossier_part() {
        let f = fixture(FakeModel::new(vec![Ok(vec!["ANSWER:\nThe middleware limits it."])]), quick());
        {
            let mut context = f.session.inner.context.lock().unwrap();
            context.brief.documents.push(crate::brief::Document::new("cv.pdf", "A LONG CV TEXT", "cv"));
            context.brief.project = Some(crate::brief::ProjectDossier { name: "bot".into(), dossier: "whole".into() });
            context.retrieve = Some(Arc::new(|_| Found { code: "--- throttle.py ---\nlimit()".into(), dossier: "## Overview\nA bot.".into() }));
        }
        f.session.ask("How does the bot stop flooding?");
        collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        let prompt = &f.model.asked()[0];
        assert!(prompt.contains("RELEVANT CODE FROM THE PROJECT (excerpts)\n--- throttle.py ---") && prompt.contains("## Overview\nA bot."));
        assert!(!prompt.contains("A LONG CV TEXT") && !prompt.contains("whole"));
        f.session.stop();
    }

    #[test]
    fn an_empty_background_says_what_to_add_and_asks_nothing() {
        let f = fixture(FakeModel::new(vec![]), quick());
        f.session.inner.context.lock().unwrap().brief = InterviewBrief::new("");
        f.session.ask("Tell me about yourself");
        let seen = collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Error(_))));
        assert!(seen.iter().any(|event| matches!(event, Event::Error(text) if text.contains("No context yet"))));
        assert!(f.model.asked().is_empty());
        f.session.stop();
    }

    #[test]
    fn a_screenshot_is_answered_even_with_no_context_and_reports_its_timing() {
        let f = fixture(
            FakeModel::new(vec![Ok(vec!["POINTS:\n- two pointers\n", "ANSWER:\n```python\npass\n```"])]),
            quick(),
        );
        f.session.inner.context.lock().unwrap().brief = InterviewBrief::new("");
        f.session.ask_screenshot("data:image/jpeg;base64,AAAA", Some(Instant::now()));
        let seen = collect(&f.events, |seen| {
            seen.iter().any(|event| matches!(event, Event::Status(text) if text.starts_with("Screenshot:")))
        });
        assert!(has(&seen, &Event::Turn { speaker: INTERVIEWER.into(), text: SCREENSHOT.into() }));
        assert!(seen.iter().any(|event| matches!(event, Event::Status(text) if text.contains("first words after"))));
        assert_eq!(f.model.requests.lock().unwrap()[0].image.as_deref(), Some("data:image/jpeg;base64,AAAA"));
        f.session.stop();
    }

    #[test]
    fn a_model_that_fails_is_reported_and_the_state_is_an_error() {
        let f = fixture(FakeModel::new(vec![Err("Groq returned 429: slow down")]), quick());
        f.session.ask("What is REST?");
        let seen = collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Error(_))));
        assert!(
            has(&seen, &Event::Error("Groq returned 429: slow down".into())) && has(&seen, &Event::State(State::Error))
        );
        f.session.stop();
    }

    fn listening(f: &Fixture, transcriber: Arc<Scripted>, source: Arc<Played>) {
        f.session.start(Some(transcriber), vec![(INTERVIEWER.to_owned(), source)]);
        // audio that comes while the model is still loading is dropped, so the tests wait for it
        collect(&f.events, |seen| has(seen, &Event::Status("Listening".into())));
    }

    #[test]
    fn a_question_heard_on_the_call_is_transcribed_logged_and_answered() {
        let f = fixture(FakeModel::new(vec![Ok(vec!["ANSWER:\n", "A left join keeps the left rows."])]), quick());
        let source = Played::new(Duration::ZERO);
        listening(&f, Scripted::saying(&["What is a left join?"]), source.clone());
        source.say(30);
        source.silence(40);
        let seen = collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        assert!(has(&seen, &Event::Turn { speaker: INTERVIEWER.into(), text: "What is a left join?".into() }));
        assert!(
            seen.iter().any(
                |event| matches!(event, Event::Level { speaker, level } if speaker == INTERVIEWER && *level > 0.0)
            )
        );
        assert_eq!(chunks(&seen), "ANSWER:\nA left join keeps the left rows.");
        assert!(f.model.asked()[0].contains("WHAT WAS HEARD: it came from the call audio"));
        f.session.stop();
    }

    #[test]
    fn small_talk_on_the_call_is_logged_but_not_answered() {
        let f = fixture(FakeModel::new(vec![]), quick());
        let source = Played::new(Duration::ZERO);
        listening(&f, Scripted::saying(&["Thanks"]), source.clone());
        source.say(30);
        source.silence(40);
        let seen = collect(&f.events, |seen| {
            seen.iter().any(|event| matches!(event, Event::Turn { text, .. } if text == "Thanks"))
        });
        std::thread::sleep(Duration::from_millis(150));
        assert!(f.model.asked().is_empty());
        assert!(!seen.iter().any(|event| matches!(event, Event::SuggestionStart { .. })));
        f.session.stop();
    }

    #[test]
    fn a_reply_of_skip_is_not_shown_and_the_window_says_why() {
        let f = fixture(FakeModel::new(vec![Ok(vec!["SK", "IP"])]), quick());
        let source = Played::new(Duration::ZERO);
        listening(&f, Scripted::saying(&["What is the capital of France?"]), source.clone());
        source.say(30);
        source.silence(40);
        let seen = collect(&f.events, |seen| {
            seen.iter().any(|event| matches!(event, Event::Status(text) if text.contains("not a question for you")))
        });
        assert!(!seen.iter().any(|event| matches!(event, Event::SuggestionStart { .. } | Event::Chunk(_))));
        f.session.stop();
    }

    #[test]
    fn a_typed_question_is_never_judged_so_a_reply_of_skip_would_still_show() {
        // typed questions are announced before the model speaks; only call audio is judged
        let f = fixture(FakeModel::new(vec![Ok(vec!["SKIP"])]), quick());
        f.session.ask("What is REST?");
        let seen = collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        assert!(seen.iter().any(|event| matches!(event, Event::SuggestionStart { .. })));
        f.session.stop();
    }

    #[test]
    fn the_language_follows_what_is_heard_and_is_announced_once() {
        let f = fixture(FakeModel::new(vec![]), quick());
        let russian =
            |text: &str| Ok(Transcript { text: text.to_owned(), language: Some("ru".into()), probability: 0.95 });
        let transcriber =
            Scripted::new(vec![russian("Расскажите о себе пожалуйста"), russian("Что такое left join в SQL")]);
        let source = Played::new(Duration::ZERO);
        listening(&f, transcriber, source.clone());
        for _ in 0..2 {
            source.say(30);
            source.silence(40);
        }
        let seen =
            collect(&f.events, |seen| seen.iter().filter(|event| matches!(event, Event::Turn { .. })).count() >= 2);
        assert_eq!(seen.iter().filter(|event| **event == Event::Language("ru".into())).count(), 1);
        assert_eq!(f.session.language().as_deref(), Some("ru"));
        f.session.stop();
    }

    #[test]
    fn a_paused_session_hears_nothing_and_the_state_follows() {
        let f = fixture(FakeModel::new(vec![]), quick());
        let source = Played::new(Duration::ZERO);
        listening(&f, Scripted::saying(&["What is a left join?"]), source.clone());
        f.session.pause(true);
        source.say(30);
        source.silence(40);
        std::thread::sleep(Duration::from_millis(300));
        let seen: Vec<Event> = std::iter::from_fn(|| f.events.try_recv().ok()).collect();
        assert!(has(&seen, &Event::State(State::Paused)));
        assert!(
            !seen.iter().any(|event| matches!(event, Event::Turn { .. } | Event::Level { .. })),
            "nothing is heard while paused"
        );
        assert!(f.model.asked().is_empty());
        f.session.pause(false);
        f.session.stop();
    }

    #[test]
    fn a_failing_recogniser_is_reported_once_not_with_every_caption() {
        let f = fixture(FakeModel::new(vec![]), quick());
        let transcriber = Scripted::new((0..3).map(|_| Err("Deepgram rejected the API key".to_owned())).collect());
        let source = Played::new(Duration::ZERO);
        listening(&f, transcriber, source.clone());
        for _ in 0..3 {
            source.say(30);
            source.silence(40);
        }
        std::thread::sleep(Duration::from_millis(500));
        let all: Vec<Event> = std::iter::from_fn(|| f.events.try_recv().ok()).collect();
        let errors = all
            .iter()
            .filter(|event| matches!(event, Event::Error(text) if text.contains("Transcription failed")))
            .count();
        assert!(errors <= 1, "{errors}");
        f.session.stop();
    }

    #[test]
    fn a_long_question_starts_its_answer_early_and_the_end_of_it_does_not_ask_again() {
        let timing = Timing {
            debounce: Duration::from_millis(40),
            early_after: Some(Duration::from_millis(400)),
            early_retry: Duration::from_millis(10),
        };
        let f = fixture(FakeModel::new(vec![Ok(vec!["ANSWER:\n", "A left join keeps every left row."])]), timing);
        let said = "What is a left join and when would you use it";
        let transcriber = Scripted::saying(&[said, said]); // the caption, then the finished sentence
        let source = Played::new(Duration::from_millis(2));
        listening(&f, transcriber, source.clone());
        source.say(40); // 0.8 s of speech: the caption is taken at the early mark
        source.silence(40);
        let seen = collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        std::thread::sleep(Duration::from_millis(300));
        let rest: Vec<Event> = std::iter::from_fn(|| f.events.try_recv().ok()).collect();
        assert!(seen.iter().any(|event| matches!(event, Event::Partial(text) if text == said)));
        assert_eq!(f.model.asked().len(), 1, "the answer begun on the caption already covers the question");
        assert!(!rest.iter().any(|event| matches!(event, Event::Discard | Event::SuggestionStart { .. })));
        assert!(f.model.asked()[0].contains("WHAT WAS HEARD: the interviewer is still speaking"));
        f.session.stop();
    }

    #[test]
    fn a_hint_for_the_last_thing_said_is_asked_whether_or_not_it_sounded_like_a_question() {
        let f = fixture(FakeModel::new(vec![Ok(vec!["ANSWER:\nok"])]), quick());
        f.session.with_log(|log| log.add(INTERVIEWER, "we use Airflow for most of the pipelines"));
        f.session.hint_now();
        collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        assert!(f.model.asked()[0].contains("we use Airflow for most of the pipelines"));
        f.session.stop();
    }

    #[test]
    fn the_history_in_a_prompt_leaves_out_the_question_itself() {
        let f = fixture(FakeModel::new(vec![Ok(vec!["ANSWER:\none"]), Ok(vec!["ANSWER:\ntwo"])]), quick());
        f.session.ask("What is a left join?");
        collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        f.session.ask("And a right join?");
        collect(&f.events, |seen| seen.iter().any(|event| matches!(event, Event::Done(_))));
        let second = &f.model.asked()[1];
        let history =
            second.split("RECENT CONVERSATION\n").nth(1).unwrap().split("\n\nINTERVIEWER'S QUESTION").next().unwrap();
        assert!(history.contains("What is a left join?") && !history.contains("And a right join?"), "{history}");
        f.session.stop();
    }

    #[test]
    fn a_new_question_cancels_the_answer_being_written() {
        let model = Arc::new(FakeModel {
            replies: Mutex::new(VecDeque::from([
                Ok(vec!["ANSWER:\n".into(), "slow".into(), " slow".into(), " slow".into(), " slow".into()]),
                Ok(vec!["ANSWER:\nfast".into()]),
            ])),
            requests: Mutex::new(Vec::new()),
            delay: Duration::from_millis(60),
            listening: true,
        });
        let f = fixture(model, quick());
        f.session.ask("First question about SQL");
        std::thread::sleep(Duration::from_millis(100));
        f.session.ask("Second question about Python");
        let seen = collect(&f.events, |seen| seen.iter().filter(|event| matches!(event, Event::Done(_))).count() >= 1);
        assert!(seen.iter().any(|event| matches!(event, Event::Done(text) if text.contains("fast"))));
        assert!(!seen.iter().any(|event| matches!(event, Event::Done(text) if text.contains("slow"))));
        f.session.stop();
    }
}
