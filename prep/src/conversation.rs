//! The call itself: a live, spoken conversation with the interviewer.
//!
//! Everything a real conversation needs happens here. The interviewer starts speaking as soon as
//! its first sentence is written, waits when your thought is not finished, says "mm-hm" when it
//! needs a moment, nudges you when you go quiet, stops when you talk over it (with headphones),
//! and watches the clock. The window only shows the [`Event`]s and sends [`Command`]s.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::stream::FuturesOrdered;
use futures_util::{FutureExt, StreamExt};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::audio::Speaker;
use crate::audio::vad::{self, FRAME_MS, Segmenter};
use crate::brief::Brief;
use crate::llm::{ChatOptions, Llm, LlmError, Message};
use crate::prompts::Lang;
use crate::prompts::interviewer::{self, Clock, InterviewerSpec};
use crate::prompts::tutor::{self, LessonState, LessonView, Mode};
use crate::session::Speaker as Who;
use crate::speech::{self, SpeechError, Stt, SttHint, Transcript, Tts, Voice};
use crate::syllabus::Half;
use crate::tags::{Tag, TagFilter};

/// How the call is paced. The defaults feel like a person; the tests shrink them.
#[derive(Debug, Clone)]
pub struct Knobs {
    /// Silence that ends what you are saying.
    pub end_silence_ms: u32,
    /// How long to wait for the rest of a thought that sounds unfinished.
    pub extend_ms: u64,
    /// The most it waits after a pause once you have been talking for a while: someone telling a story
    /// stops to think, and gets more room the longer it has gone on. A short answer gets none.
    pub patience_ms: u64,
    /// If the interviewer has not started speaking after this long, it acknowledges you first.
    pub filler_after_ms: u64,
    /// The microphone stays shut this long after the speakers stop, so it does not hear the tail.
    pub speaker_tail_ms: u64,
    /// With headphones, this much speech over the interviewer interrupts it.
    pub barge_in_ms: u32,
    /// Silence before the first and second nudge, and before the interviewer moves on.
    pub nudge_first_s: u64,
    pub nudge_second_s: u64,
    pub move_on_s: u64,
    /// After the time is up and nobody answers, the interviewer closes the call this much later.
    pub overtime_s: u64,
}

impl Default for Knobs {
    fn default() -> Self {
        Self {
            end_silence_ms: 700,
            extend_ms: 1400,
            patience_ms: 2400,
            filler_after_ms: 1200,
            speaker_tail_ms: 350,
            barge_in_ms: 300,
            nudge_first_s: 14,
            nudge_second_s: 40,
            move_on_s: 75,
            overtime_s: 20,
        }
    }
}

pub struct Config {
    pub spec: InterviewerSpec,
    pub brief: Brief,
    pub knobs: Knobs,
    pub model: String,
    pub headphones: bool,
    pub voice: Voice,
    /// The voice for sentences in the other language, for lessons that switch between two.
    pub voice_alt: Option<Voice>,
    pub stt_hint: SttHint,
}

pub struct Deps {
    pub llm: Arc<dyn Llm>,
    pub stt: Arc<dyn Stt>,
    /// `None`: the interviewer writes and does not speak.
    pub tts: Option<Arc<dyn Tts>>,
    pub speaker: Arc<dyn Speaker>,
    /// `None`: answers are typed.
    pub mic: Option<Receiver<Vec<i16>>>,
    /// Called after every event, so a window can redraw.
    pub wake: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Connecting to the model.
    Starting,
    /// The interviewer is working out what to say.
    Thinking,
    Speaking,
    /// Your turn.
    Listening,
    /// Your microphone is off.
    Muted,
    Finished,
}

#[derive(Debug, Clone)]
pub enum Event {
    State(State),
    /// A sentence the interviewer is about to say, and how long it takes.
    Caption {
        text: String,
        seconds: f32,
    },
    /// A finished turn. `append` means it continues your previous one.
    Turn {
        who: Who,
        text: String,
        language: Option<String>,
        append: bool,
    },
    Level {
        mic: f32,
        speaker: f32,
    },
    Error(String),
    /// Something was said but no words came out of it: too quiet, too far, or only noise.
    Unclear,
    /// The tutor wrote a tag that the window should act on: a note, a picture to show.
    Tag(Tag),
    /// Where the terms lesson is.
    Lesson(LessonView),
    /// A term, or half of it, was finished with this score from 0 to 3.
    TermProgress {
        index: usize,
        name: String,
        half: Half,
        score: u8,
    },
    Finished,
}

#[derive(Debug, Clone)]
pub enum Command {
    Mute(bool),
    /// Stop the interviewer talking.
    Interrupt,
    /// Answer in writing.
    Say(String),
    /// Try the last request again after an error.
    Retry,
    /// End the call now.
    Finish,
    /// Something the tutor should know at its next turn (a focus block ended, a picture was shown).
    Context(String),
    /// The tutor says exactly this, if it is not busy.
    Speak(String),
    /// While held, the tutor does not nudge you for being quiet: a picture is playing.
    Hold(bool),
    /// Turns the tutor's voice on or off; off, it writes only.
    Voice(bool),
    /// The code in the editor now; the tutor sees the latest one with your next words.
    Code(String),
}

enum Internal {
    Transcribed { result: Result<Transcript, SpeechError>, speech_ms: u32 },
    Done(TurnResult),
}

/// Speech at least this long that turns out to be no words is reported as not understood.
const UNCLEAR_AFTER_MS: u32 = 1000;

struct TurnResult {
    id: u64,
    full_text: String,
    error: Option<String>,
    tags: Vec<Tag>,
}

/// Decides who enqueues first, so a filler never lands after the real sentence.
#[derive(Default)]
struct Gate(Mutex<bool>);

impl Gate {
    fn started(&self) -> bool {
        *self.0.lock().unwrap()
    }
    /// The turn begins to speak; from now on fillers are late.
    fn open_for_turn(&self, speak: impl FnOnce()) {
        let mut started = self.0.lock().unwrap();
        *started = true;
        speak();
    }
    /// A filler may speak only if the turn has not.
    fn try_filler(&self, speak: impl FnOnce()) {
        let started = self.0.lock().unwrap();
        if !*started {
            speak();
        }
    }
}

struct Turn {
    id: u64,
    cancel: CancellationToken,
    gate: Arc<Gate>,
    /// What was handed to the speakers: the sentence and how long it lasts.
    enqueued: Arc<Mutex<Vec<(String, f32)>>>,
    done: bool,
    full_text: String,
    canned: bool,
    started_at: Instant,
    filler_played: bool,
}

struct Pending {
    text: String,
    language: Option<String>,
    deadline: Instant,
    /// How much of this turn has been said so far, across the pieces that were merged.
    spoken_ms: u32,
}

/// How long to wait after a pause before taking it that you are done: nothing for the first few
/// seconds, then a sixth of what has been said, up to `most`. Ten seconds earn about a second, and
/// a story of a quarter of a minute or more the most.
fn patience(spoken_ms: u32, most: u64) -> u64 {
    (spoken_ms.saturating_sub(3_000) as u64 / 6).min(most)
}

struct Call {
    deps: Deps,
    config: Config,
    events: UnboundedSender<Event>,
    internal: UnboundedSender<Internal>,
    state: State,
    started: Instant,
    history: Vec<(bool, String)>,
    answered: usize,
    segmenter: Segmenter,
    muted: bool,
    turn: Option<Turn>,
    turn_counter: u64,
    last_request: Option<Vec<Message>>,
    pending: Option<Pending>,
    stt_inflight: usize,
    gate_until: Instant,
    silent_since: Option<Instant>,
    nudges: u8,
    asked_for_questions: bool,
    final_sent: bool,
    mic_level: f32,
    fillers: Arc<Mutex<Vec<crate::speech::Speech>>>,
    last_error: Option<(String, Instant)>,
    barged: bool,
    cancel_all: CancellationToken,
    lesson: Option<LessonState>,
    /// What the call wants the tutor to know at its next turn.
    context: Vec<String>,
    holding: bool,
    voice_on: bool,
    /// The code in the editor now, for a live-coding call.
    code: Option<String>,
    /// Set once a sentence in the other language could not be spoken, so it is said once and not again.
    alt_failed: Arc<AtomicBool>,
}

/// A handle to a running call.
pub struct Handle {
    commands: UnboundedSender<Command>,
    pub events: UnboundedReceiver<Event>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Handle {
    /// A handle that is not tied to a running call: the caller feeds the events and reads the
    /// commands. Used by the window under test.
    pub fn from_parts(commands: UnboundedSender<Command>, events: UnboundedReceiver<Event>) -> Self {
        Self { commands, events, thread: None }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Ends the call and waits for it to wind down.
    pub fn close(mut self) {
        self.send(Command::Finish);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Finish);
    }
}

/// Starts a call on its own thread.
pub fn start(deps: Deps, config: Config) -> Handle {
    let (commands, command_rx) = unbounded_channel();
    let (event_tx, events) = unbounded_channel();
    let thread = std::thread::Builder::new()
        .name("prep-call".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build();
            if let Ok(runtime) = runtime {
                runtime.block_on(run(deps, config, command_rx, event_tx));
            }
        })
        .ok();
    Handle { commands, events, thread }
}

fn canned(language: Lang, which: usize) -> &'static str {
    const EN: [&str; 3] = ["Take your time.", "Would you like me to repeat the question?", "No rush, I'm here."];
    const RU: [&str; 3] = ["Не торопитесь.", "Повторить вопрос?", "Всё в порядке, я слушаю."];
    match language {
        Lang::En => EN[which % 3],
        Lang::Ru => RU[which % 3],
    }
}

fn filler_phrases(language: Lang) -> &'static [&'static str] {
    match language {
        Lang::En => &["Mm-hm.", "Okay.", "I see.", "Right."],
        Lang::Ru => &["Ага.", "Понятно.", "Хорошо.", "Угу."],
    }
}

async fn next_frame(mic: &mut Option<Receiver<Vec<i16>>>) -> Option<Vec<i16>> {
    match mic {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

/// Runs the call until it ends. Tests call this directly with a paused clock.
pub async fn run(deps: Deps, config: Config, mut commands: UnboundedReceiver<Command>, events: UnboundedSender<Event>) {
    let (internal_tx, mut internal_rx) = unbounded_channel();
    let segmenter =
        Segmenter::new(vad::Options { end_silence_ms: config.knobs.end_silence_ms, ..vad::Options::default() });
    let now = Instant::now();
    let mut deps = deps;
    let mut mic = deps.mic.take();
    // the voice loads while the model thinks of its first words, not after them
    if let Some(tts) = deps.tts.clone() {
        tokio::spawn(tts.warm_up());
    }
    let mut call = Call {
        deps,
        config,
        events,
        internal: internal_tx,
        state: State::Starting,
        started: now,
        history: Vec::new(),
        answered: 0,
        segmenter,
        muted: false,
        turn: None,
        turn_counter: 0,
        last_request: None,
        pending: None,
        stt_inflight: 0,
        gate_until: now,
        silent_since: None,
        nudges: 0,
        asked_for_questions: false,
        final_sent: false,
        mic_level: 0.0,
        fillers: Arc::new(Mutex::new(Vec::new())),
        last_error: None,
        barged: false,
        cancel_all: CancellationToken::new(),
        lesson: None,
        context: Vec::new(),
        holding: false,
        voice_on: true,
        code: None,
        alt_failed: Arc::new(AtomicBool::new(false)),
    };
    call.emit(Event::State(State::Starting));
    call.start_lesson();
    call.prepare_fillers();
    if call.config.spec.mode == Mode::Chat {
        // nobody waits for a model to say hello in a chat
        let greeting = tutor::greeting(call.config.spec.native).to_owned();
        call.begin_turn(Vec::new(), Some(greeting));
    } else {
        let opening = interviewer::opening(&call.config.spec, &call.config.brief);
        call.begin_turn(opening, None);
    }

    let mut ticker = tokio::time::interval(Duration::from_millis(u64::from(FRAME_MS)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            frame = next_frame(&mut mic) => match frame {
                Some(frame) => call.on_frame(&frame),
                None => mic = None,
            },
            command = commands.recv() => match command {
                Some(command) => {
                    if call.on_command(command) {
                        break;
                    }
                }
                None => break,
            },
            Some(message) = internal_rx.recv() => call.on_internal(message),
            _ = ticker.tick() => {
                if call.on_tick() {
                    break;
                }
            }
        }
    }
    call.shutdown();
}

impl Call {
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        (self.deps.wake)();
    }

    fn set_state(&mut self, state: State) {
        if self.state != state {
            self.state = state;
            self.emit(Event::State(state));
        }
    }

    fn elapsed(&self) -> u32 {
        Instant::now().duration_since(self.started).as_secs().min(u64::from(u32::MAX)) as u32
    }

    fn report(&mut self, message: String) {
        let now = Instant::now();
        let repeat = self
            .last_error
            .as_ref()
            .is_some_and(|(last, at)| *last == message && now.duration_since(*at) < Duration::from_secs(10));
        if !repeat {
            self.emit(Event::Error(message.clone()));
        }
        self.last_error = Some((message, now));
    }

    /// Makes the short acknowledgements ready in the background.
    fn prepare_fillers(&self) {
        let spec = &self.config.spec;
        // an acknowledgement in the wrong language of a mixed lesson would be odd
        if !self.voice_on || spec.native != spec.language && spec.mode.teaches() {
            return;
        }
        let Some(tts) = self.deps.tts.clone() else { return };
        let voice = self.config.voice.clone();
        let phrases = filler_phrases(self.config.spec.language);
        let store = Arc::clone(&self.fillers);
        let cancel = self.cancel_all.clone();
        tokio::spawn(async move {
            for phrase in phrases.iter().take(3) {
                let made = tokio::select! {
                    _ = cancel.cancelled() => return,
                    made = tts.synthesize((*phrase).to_owned(), voice.clone()) => made,
                };
                match made {
                    Ok(speech) => store.lock().unwrap().push(speech),
                    Err(_) => return,
                }
            }
        });
    }

    // ---- interviewer turns ----

    fn begin_turn(&mut self, messages: Vec<Message>, canned_text: Option<String>) {
        self.cancel_turn();
        self.turn_counter += 1;
        let id = self.turn_counter;
        let cancel = CancellationToken::new();
        let gate = Arc::new(Gate::default());
        let enqueued = Arc::new(Mutex::new(Vec::new()));
        let is_canned = canned_text.is_some();
        if canned_text.is_none() {
            self.last_request = Some(messages.clone());
        }
        self.turn = Some(Turn {
            id,
            cancel: cancel.clone(),
            gate: Arc::clone(&gate),
            enqueued: Arc::clone(&enqueued),
            done: false,
            full_text: String::new(),
            canned: is_canned,
            started_at: Instant::now(),
            filler_played: false,
        });
        self.set_state(State::Thinking);
        if !is_canned {
            // a nudge does not restart the silence it is about
            self.silent_since = None;
        }
        let job = TurnJob {
            id,
            llm: Arc::clone(&self.deps.llm),
            tts: if self.voice_on { self.deps.tts.clone() } else { None },
            speaker: Arc::clone(&self.deps.speaker),
            voice: self.config.voice.clone(),
            voice_alt: self.config.voice_alt.clone(),
            alt_failed: Arc::clone(&self.alt_failed),
            model: self.config.model.clone(),
            messages,
            canned: canned_text,
            cancel,
            gate,
            enqueued,
            events: self.events.clone(),
            wake: Arc::clone(&self.deps.wake),
            internal: self.internal.clone(),
        };
        tokio::spawn(job.run());
    }

    fn cancel_turn(&mut self) {
        if let Some(turn) = self.turn.take() {
            turn.cancel.cancel();
            self.deps.speaker.stop();
        }
    }

    /// What of an interrupted turn was really spoken: the sentences whose audio had started.
    fn spoken_part(&self, turn: &Turn) -> String {
        let enqueued = turn.enqueued.lock().unwrap().clone();
        let mut remaining = self.deps.speaker.queued().as_secs_f32();
        let mut keep = enqueued.len();
        // sentences at the end whose audio has not started were never said
        while keep > 0 {
            let seconds = enqueued[keep - 1].1;
            if remaining >= seconds - 0.05 {
                remaining -= seconds;
                keep -= 1;
            } else {
                break;
            }
        }
        enqueued.iter().take(keep).map(|(text, _)| text.as_str()).collect::<Vec<_>>().join(" ")
    }

    /// The interviewer was cut off: keep what was said, drop what was not.
    fn interrupt_interviewer(&mut self) {
        let Some(turn) = self.turn.take() else { return };
        let spoken = self.spoken_part(&turn);
        turn.cancel.cancel();
        self.deps.speaker.stop();
        if !turn.canned {
            self.commit_interviewer(spoken);
        }
        self.segmenter.reset();
    }

    fn on_internal(&mut self, message: Internal) {
        match message {
            Internal::Transcribed { result, speech_ms } => self.on_transcribed(result, speech_ms),
            Internal::Done(result) => self.on_turn_done(result),
        }
    }

    fn on_turn_done(&mut self, result: TurnResult) {
        let Some(turn) = self.turn.as_mut() else { return };
        if turn.id != result.id {
            return; // a turn that was cancelled
        }
        turn.done = true;
        turn.full_text = result.full_text.trim().to_owned();
        let nothing_said = turn.enqueued.lock().unwrap().is_empty() && turn.full_text.is_empty();
        if let Some(error) = result.error {
            self.report(error);
        }
        self.apply_tags(result.tags);
        if nothing_said && !self.turn.as_ref().is_some_and(|t| t.canned) {
            // nothing came back: let the person try again
            self.turn = None;
            self.set_state(State::Listening);
            self.silent_since = Some(Instant::now());
        }
    }

    fn start_lesson(&mut self) {
        let spec = &self.config.spec;
        if spec.mode != Mode::Terms {
            return;
        }
        let Some(lesson) = spec.lesson.clone().filter(|l| !l.terms.is_empty()) else { return };
        let state = LessonState::new(lesson.terms.len());
        if let Some(view) = state.view(&lesson) {
            self.emit(Event::Lesson(view));
        }
        self.lesson = Some(state);
    }

    /// Acts on the tags the tutor wrote at the end of a message.
    fn apply_tags(&mut self, tags: Vec<Tag>) {
        for tag in tags {
            let lesson = self.config.spec.lesson.clone();
            match &tag {
                Tag::Native(score) | Tag::English(score) => {
                    let half = if matches!(tag, Tag::Native(_)) { Half::Native } else { Half::English };
                    if let (Some(state), Some(lesson)) = (self.lesson.as_mut(), lesson) {
                        let index = state.index;
                        let moved = if half == Half::Native { state.native_done() } else { state.english_done() };
                        if moved {
                            let name = lesson.terms.get(index).map(|t| t.name.clone()).unwrap_or_default();
                            let view = state.view(&lesson);
                            self.emit(Event::TermProgress { index, name, half, score: *score });
                            if let Some(view) = view {
                                self.emit(Event::Lesson(view));
                            }
                        }
                    }
                }
                Tag::End => self.final_sent = true,
                Tag::Note(_) | Tag::Show(_) => {}
            }
            self.emit(Event::Tag(tag));
        }
    }

    /// The interviewer's turn is over: it goes into the history and onto the screen.
    fn commit_interviewer(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        self.history.push((true, text.clone()));
        self.emit(Event::Turn { who: Who::Interviewer, text, language: None, append: false });
    }

    // ---- microphone ----

    fn mic_open(&self) -> bool {
        if self.muted {
            return false;
        }
        let speaking = self.turn.as_ref().is_some_and(|t| t.gate.started() || !self.deps.speaker.queued().is_zero());
        if self.config.headphones {
            return true;
        }
        !speaking && self.deps.speaker.queued().is_zero() && Instant::now() >= self.gate_until
    }

    fn on_frame(&mut self, frame: &[i16]) {
        self.mic_level = if self.muted { 0.0 } else { vad::level(frame) };
        if !self.mic_open() {
            if self.segmenter.active() {
                self.segmenter.reset();
            }
            return;
        }
        let speaking = self.state == State::Speaking;
        match self.segmenter.push(frame) {
            vad::Event::Started => self.silent_since = None,
            vad::Event::Finished(pcm) => self.on_utterance(pcm),
            vad::Event::None => {}
        }
        // talking over the interviewer
        if self.segmenter.active() && self.segmenter.speech_ms() >= self.config.knobs.barge_in_ms {
            if speaking && self.config.headphones {
                self.interrupt_interviewer();
                self.barged = true;
                self.set_state(State::Listening);
            } else if self.state == State::Thinking
                && self.turn.as_ref().is_some_and(|t| !t.gate.started() && !t.canned)
            {
                // you went on talking before it answered: it stops and listens
                self.cancel_turn();
                self.barged = true;
                self.set_state(State::Listening);
            }
        }
    }

    fn on_utterance(&mut self, pcm: Vec<i16>) {
        let speech_ms = (pcm.len() as u64 * 1000 / 16_000) as u32;
        self.set_state(State::Thinking);
        self.stt_inflight += 1;
        let stt = Arc::clone(&self.deps.stt);
        let hint = self.config.stt_hint.clone();
        let internal = self.internal.clone();
        tokio::spawn(async move {
            let result = stt.transcribe(pcm, hint).await;
            let _ = internal.send(Internal::Transcribed { result, speech_ms });
        });
    }

    fn on_transcribed(&mut self, result: Result<Transcript, SpeechError>, speech_ms: u32) {
        self.stt_inflight = self.stt_inflight.saturating_sub(1);
        // words that were still being recognised when the call was put on hold are not an answer
        if self.holding {
            self.back_to_listening_if_idle();
            return;
        }
        match result {
            Err(error) => {
                self.report(error.to_string());
                self.back_to_listening_if_idle();
            }
            Ok(transcript) if speech::is_noise(&transcript.text, speech_ms) => {
                // a real stretch of speech that gave nothing is said aloud on the screen, not swallowed
                if speech_ms >= UNCLEAR_AFTER_MS {
                    self.emit(Event::Unclear);
                }
                self.back_to_listening_if_idle();
            }
            Ok(transcript) => {
                let (previous_text, previous_ms) = match self.pending.take() {
                    Some(previous) => (Some(previous.text), previous.spoken_ms),
                    None => (None, 0),
                };
                let text = match previous_text {
                    Some(previous) => format!("{previous} {}", transcript.text.trim()),
                    None => transcript.text.trim().to_owned(),
                };
                let spoken_ms = previous_ms + speech_ms;
                let patient = patience(spoken_ms, self.config.knobs.patience_ms);
                let wait =
                    if speech::looks_unfinished(&text) { self.config.knobs.extend_ms.max(patient) } else { patient };
                self.pending = Some(Pending {
                    text,
                    language: transcript.language,
                    deadline: Instant::now() + Duration::from_millis(wait),
                    spoken_ms,
                });
            }
        }
    }

    fn back_to_listening_if_idle(&mut self) {
        if self.pending.is_none() && self.stt_inflight == 0 && !self.segmenter.active() && self.turn.is_none() {
            self.set_state(if self.muted { State::Muted } else { State::Listening });
            self.silent_since.get_or_insert_with(Instant::now);
        }
    }

    // ---- your turn ----

    fn commit_user(&mut self, text: String, language: Option<String>) {
        let append = self.barged
            && self.turn.is_none()
            && self.history.last().is_some_and(|(is_interviewer, _)| !*is_interviewer);
        self.barged = false;
        if append {
            if let Some((_, previous)) = self.history.last_mut() {
                previous.push(' ');
                previous.push_str(&text);
            }
        } else {
            self.history.push((false, text.clone()));
            self.answered += 1;
        }
        self.emit(Event::Turn { who: Who::You, text, language, append });
        self.nudges = 0;
        let clock = interviewer::clock(self.elapsed(), self.config.spec.total_seconds(), self.asked_for_questions);
        if clock == Clock::AskForQuestions {
            self.asked_for_questions = true;
        }
        if clock == Clock::Final {
            self.final_sent = true;
        }
        if let Some(state) = self.lesson.as_mut() {
            state.answered();
        }
        let note = self.make_note(clock);
        let messages = interviewer::turn(&self.config.spec, &self.config.brief, &self.history, Some(note));
        self.begin_turn(messages, None);
    }

    /// What the call tells the interviewer or tutor along with your words: the clock, the lesson
    /// and whatever the window asked it to pass on.
    fn make_note(&mut self, clock: Clock) -> String {
        let spec = &self.config.spec;
        let mut note = if spec.mode.teaches() {
            tutor::state_note(spec, self.elapsed(), self.answered, clock, self.lesson.as_ref())
        } else {
            interviewer::state_note(spec, self.elapsed(), self.answered, clock)
        };
        for extra in self.context.drain(..) {
            note.push('\n');
            note.push_str(&extra);
        }
        if let Some(code) = &self.code {
            note.push_str(&format!(
                "\n[CODE the person has written so far, live; never read it aloud:\n```\n{}\n```]",
                crate::brief::truncate_chars(code, 6000)
            ));
        }
        note
    }

    fn on_command(&mut self, command: Command) -> bool {
        match command {
            Command::Finish => return true,
            Command::Mute(on) => {
                self.muted = on;
                self.segmenter.reset();
                if on {
                    self.set_state(State::Muted);
                } else if self.state == State::Muted {
                    self.set_state(State::Listening);
                }
            }
            Command::Interrupt => {
                if self.turn.is_some() {
                    self.interrupt_interviewer();
                    self.set_state(State::Listening);
                    self.silent_since = Some(Instant::now());
                }
            }
            Command::Say(text) => {
                let text = text.trim().to_owned();
                if !text.is_empty() {
                    if self.turn.as_ref().is_some_and(|t| t.gate.started()) {
                        self.interrupt_interviewer();
                    } else {
                        self.cancel_turn();
                    }
                    self.pending = None;
                    self.barged = false;
                    self.commit_user(text, None);
                }
            }
            Command::Retry => {
                if let (None, Some(messages)) = (&self.turn, self.last_request.clone()) {
                    self.begin_turn(messages, None);
                }
            }
            Command::Context(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    self.context.push(format!("[STATE: {text}]"));
                    let extra = self.context.len().saturating_sub(4);
                    self.context.drain(..extra);
                }
            }
            Command::Speak(text) => {
                let text = text.trim().to_owned();
                let idle = self.turn.is_none() && self.pending.is_none() && !self.segmenter.active();
                if idle && !text.is_empty() && self.state != State::Finished {
                    self.begin_turn(Vec::new(), Some(text));
                }
            }
            Command::Hold(on) => {
                self.holding = on;
                if on {
                    // a half-said thought waiting to be sent would be answered during the break
                    self.pending = None;
                    self.barged = false;
                } else {
                    self.silent_since = Some(Instant::now());
                    self.nudges = 0;
                }
            }
            Command::Code(code) => self.code = Some(code),
            Command::Voice(on) => {
                self.voice_on = on;
                if !on {
                    self.deps.speaker.stop();
                    if self.turn.as_ref().is_some_and(|t| t.gate.started()) {
                        self.interrupt_interviewer();
                        self.set_state(State::Listening);
                    }
                }
            }
        }
        false
    }

    // ---- the clock ----

    /// Returns true when the call is over.
    fn on_tick(&mut self) -> bool {
        let now = Instant::now();
        self.emit(Event::Level { mic: self.mic_level, speaker: self.deps.speaker.level() });
        self.mic_level *= 0.6;

        let ready = self.pending.as_ref().is_some_and(|p| now >= p.deadline)
            && self.stt_inflight == 0
            && !self.segmenter.active();
        if let (true, Some(pending)) = (ready, self.pending.take_if(|_| ready)) {
            self.commit_user(pending.text, pending.language);
        }

        if let Some(turn) = self.turn.as_mut() {
            if self.state == State::Thinking && turn.gate.started() {
                self.state = State::Speaking;
                let _ = self.events.send(Event::State(State::Speaking));
            }
            // a person acknowledges you when they need a moment
            let wait = Duration::from_millis(self.config.knobs.filler_after_ms);
            if self.state == State::Thinking
                && !turn.canned
                && !turn.filler_played
                && now.duration_since(turn.started_at) >= wait
                && !self.history.is_empty()
            {
                turn.filler_played = true;
                let pick = {
                    let fillers = self.fillers.lock().unwrap();
                    (!fillers.is_empty()).then(|| fillers[self.answered % fillers.len()].clone())
                };
                if let Some(speech) = pick {
                    let speaker = Arc::clone(&self.deps.speaker);
                    turn.gate.try_filler(|| speaker.enqueue(speech.pcm, speech.sample_rate));
                }
            }
            if turn.done && self.deps.speaker.queued().is_zero() {
                let spoken = turn.enqueued.lock().unwrap().iter().map(|(t, _)| t.clone()).collect::<Vec<_>>().join(" ");
                let text = if spoken.trim().is_empty() { turn.full_text.clone() } else { spoken };
                self.turn = None;
                self.commit_interviewer(text);
                self.gate_until = now + Duration::from_millis(self.config.knobs.speaker_tail_ms);
                if self.final_sent {
                    self.set_state(State::Finished);
                    self.emit(Event::Finished);
                    return true;
                }
                self.set_state(if self.muted { State::Muted } else { State::Listening });
                self.silent_since.get_or_insert(now);
            }
        }

        self.check_silence(now);
        false
    }

    fn check_silence(&mut self, now: Instant) {
        // a chat has no clock, and while code is being written a quiet person is a thinking one
        let quiet_mode = matches!(self.config.spec.mode, Mode::Chat | Mode::Coding);
        if self.holding || self.config.spec.mode == Mode::Chat {
            return;
        }
        if self.state != State::Listening
            || self.turn.is_some()
            || self.pending.is_some()
            || self.stt_inflight > 0
            || self.segmenter.active()
            || self.history.is_empty()
            || self.muted
        {
            return;
        }
        let Some(since) = self.silent_since else { return };
        let quiet = now.duration_since(since).as_secs();
        let knobs = self.config.knobs.clone();
        let language = self.config.spec.language;
        // time is up: close the call even if nobody is talking
        let total = u64::from(self.config.spec.total_seconds());
        if !self.final_sent && u64::from(self.elapsed()) >= total + knobs.overtime_s {
            self.final_sent = true;
            let note = self.make_note(Clock::Final);
            let messages = interviewer::turn(&self.config.spec, &self.config.brief, &self.history, Some(note));
            self.begin_turn(messages, None);
            return;
        }
        if quiet_mode {
            return;
        }
        match self.nudges {
            0 if quiet >= knobs.nudge_first_s => {
                self.nudges = 1;
                self.begin_turn(Vec::new(), Some(canned(language, 0).to_owned()));
            }
            1 if quiet >= knobs.nudge_second_s => {
                self.nudges = 2;
                self.begin_turn(Vec::new(), Some(canned(language, 1).to_owned()));
            }
            2 if quiet >= knobs.move_on_s => {
                self.nudges = 0;
                let note = "[STATE: the candidate has not answered for over a minute. Say that it is fine, then ask a simpler, related question.]".to_owned();
                let messages = interviewer::turn(&self.config.spec, &self.config.brief, &self.history, Some(note));
                self.begin_turn(messages, None);
            }
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        self.cancel_all.cancel();
        if let Some(turn) = self.turn.take() {
            turn.cancel.cancel();
        }
        self.deps.speaker.stop();
        if self.state != State::Finished {
            self.set_state(State::Finished);
            self.emit(Event::Finished);
        }
    }
}

/// One turn of the interviewer: ask the model, cut the reply into sentences, give each a voice
/// and queue it, all while the model is still writing.
struct TurnJob {
    id: u64,
    llm: Arc<dyn Llm>,
    tts: Option<Arc<dyn Tts>>,
    speaker: Arc<dyn Speaker>,
    voice: Voice,
    voice_alt: Option<Voice>,
    alt_failed: Arc<AtomicBool>,
    model: String,
    messages: Vec<Message>,
    canned: Option<String>,
    cancel: CancellationToken,
    gate: Arc<Gate>,
    enqueued: Arc<Mutex<Vec<(String, f32)>>>,
    events: UnboundedSender<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
    internal: UnboundedSender<Internal>,
}

type Synthesis = futures_util::future::BoxFuture<'static, (String, Option<Result<crate::speech::Speech, SpeechError>>)>;

/// Which of the two languages a sentence is written in: mostly Cyrillic letters is Russian.
fn language_of(sentence: &str) -> Lang {
    let letters = sentence.chars().filter(|c| c.is_alphabetic()).count();
    let cyrillic = sentence.chars().filter(|c| ('\u{400}'..='\u{4FF}').contains(c)).count();
    if letters > 0 && cyrillic * 2 > letters { Lang::Ru } else { Lang::En }
}

impl TurnJob {
    /// The voice for a sentence, and whether it is the voice of the other language.
    fn voice_for(&self, sentence: &str) -> (Voice, bool) {
        match &self.voice_alt {
            Some(alt) if language_of(sentence) != self.voice.language => (alt.clone(), true),
            _ => (self.voice.clone(), false),
        }
    }

    fn synth(&self, sentence: String) -> Synthesis {
        let (voice, alternative) = self.voice_for(&sentence);
        // once the other language has failed to be spoken, it is only written
        if alternative && self.alt_failed.load(Ordering::Relaxed) {
            return async move { (sentence, None) }.boxed();
        }
        match self.tts.clone() {
            Some(tts) => async move {
                let made = tts.synthesize(sentence.clone(), voice).await;
                (sentence, Some(made))
            }
            .boxed(),
            None => async move { (sentence, None) }.boxed(),
        }
    }

    fn send(&self, event: Event) {
        let _ = self.events.send(event);
        (self.wake)();
    }

    fn open(&self, max_tokens: u32) -> futures_util::stream::BoxStream<'static, Result<String, LlmError>> {
        self.llm
            .stream(self.messages.clone(), ChatOptions::new(self.model.clone()).max_tokens(max_tokens).temperature(0.7))
    }

    async fn run(self) {
        let mut full = String::new();
        let mut error: Option<String> = None;
        let mut pending: FuturesOrdered<Synthesis> = FuturesOrdered::new();
        let mut splitter = crate::sentences::SentenceSplitter::default();
        let mut voice_failed = false;

        if let Some(text) = &self.canned {
            pending.push_back(self.synth(text.clone()));
            full = text.clone();
        }
        // A model that reasons first can spend a small allowance on thoughts and say nothing, so an
        // empty reply is asked for once more with more room.
        let allowances = [400u32, 1500];
        let mut attempt = 0;
        let mut stream = match self.canned {
            Some(_) => futures_util::stream::empty().boxed(),
            None => self.open(allowances[0]),
        };
        let mut stream_done = self.canned.is_some();
        let mut tags = TagFilter::default();
        loop {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return,
                piece = stream.next(), if !stream_done => match piece {
                    Some(Ok(piece)) => {
                        let words = tags.push(&piece);
                        full.push_str(&words);
                        for sentence in splitter.push(&words) {
                            pending.push_back(self.synth(sentence));
                        }
                    }
                    Some(Err(LlmError::Empty)) if full.is_empty() && attempt + 1 < allowances.len() => {
                        attempt += 1;
                        stream = self.open(allowances[attempt]);
                    }
                    Some(Err(failure)) => {
                        error = Some(describe(&failure));
                        stream_done = true;
                        full.push_str(&tags.finish());
                        if let Some(rest) = splitter.finish() {
                            pending.push_back(self.synth(rest));
                        }
                    }
                    None => {
                        stream_done = true;
                        let held = tags.finish();
                        full.push_str(&held);
                        for sentence in splitter.push(&held) {
                            pending.push_back(self.synth(sentence));
                        }
                        if let Some(rest) = splitter.finish() {
                            pending.push_back(self.synth(rest));
                        }
                    }
                },
                Some((sentence, made)) = pending.next(), if !pending.is_empty() => {
                    let seconds = match made {
                        Some(Ok(speech)) => {
                            let seconds = speech.seconds();
                            self.gate.open_for_turn(|| self.speaker.enqueue(speech.pcm, speech.sample_rate));
                            seconds
                        }
                        Some(Err(failure)) => {
                            let other = self.voice_for(&sentence).1;
                            if other {
                                // said once for the whole call, not at every turn
                                if !self.alt_failed.swap(true, Ordering::Relaxed) {
                                    error.get_or_insert(format!("{failure} The rest of that language is written only."));
                                }
                            } else if !voice_failed {
                                voice_failed = true;
                                error.get_or_insert(format!("{failure} The interviewer continues in writing."));
                            }
                            self.gate.open_for_turn(|| {});
                            estimate_seconds(&sentence)
                        }
                        None => {
                            self.gate.open_for_turn(|| {});
                            0.0
                        }
                    };
                    self.enqueued.lock().unwrap().push((sentence.clone(), seconds));
                    self.send(Event::Caption { text: sentence, seconds });
                }
            }
            if stream_done && pending.is_empty() {
                break;
            }
        }
        let _ =
            self.internal.send(Internal::Done(TurnResult { id: self.id, full_text: full, error, tags: tags.take() }));
    }
}

fn describe(error: &LlmError) -> String {
    error.to_string()
}

/// How long a sentence takes to say, when there is no audio to measure.
fn estimate_seconds(text: &str) -> f32 {
    (text.split_whitespace().count() as f32 / 2.6).max(0.5)
}
