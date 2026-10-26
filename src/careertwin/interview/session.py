"""Orchestrates one interview: audio in, transcript and suggestions out.

The session knows nothing about Qt. A front end passes callbacks and calls
`start`, `ask`, `ask_screenshot`, `pause` and `stop`; every callback runs on a
worker thread.
"""

from __future__ import annotations

import queue
import re
import threading
import time
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import Protocol

import numpy as np

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import CoachError
from careertwin.interview.languages import LanguageTracker
from careertwin.interview.prompts import parse_suggestion
from careertwin.interview.prompts import SKIP, parse_suggestion
from careertwin.interview.questions import is_question, worth_an_answer
from careertwin.interview.stt import Transcriber
from careertwin.interview.transcript import TranscriptLog
from careertwin.interview.vad import FRAME_MS, SAMPLE_RATE, UtteranceSegmenter

INTERVIEWER = "Interviewer"
YOU = "You"
MIN_WORDS_FOR_LANGUAGE = 3
LEVEL_EVERY = 3
PARTIAL_EVERY = 75  # frames of 20 ms: a live caption about every 1.5 s of speech
QUESTION_MARK_WAIT = 0.1  # seconds to wait after a sentence that ends with a question mark
REUSE_SAMPLES = 16_000 * 6 // 10  # a caption that covers all but 0.6 s of the speech is final enough
REPEAT_ERROR_AFTER = 10.0  # seconds before the same transcription failure is shown again
EARLY_AFTER = 3.0  # seconds of the interviewer speaking before the answer starts on what is heard
EARLY_MIN_WORDS = 5  # fewer words than this is not yet something to answer
EARLY_LONG_WORDS = 12  # without a sign of a question, this many words are shown to the model anyway
EARLY_ATTEMPTS = 3  # tries on one utterance: the first words may be only the set-up of the question
EARLY_RETRY = 2.0  # seconds between two tries
EARLY_RETRY_WORDS = 4  # and the words that must have come in between
EARLY_STALE = 30.0  # an early answer older than this is not the answer to what was just asked


def _words(text: str) -> list[str]:
    return re.findall(r"[\w']+", text.lower())


def covers(early: str, final: str) -> bool:
    """Whether the answer started on `early` already answers `final`, the question once it is over.

    It does when nearly all the early words are in the final text and the final adds little. When
    the interviewer went on to say the real question, the answer starts again on all of it.
    """
    seen, heard = _words(early), _words(final)
    if not seen or not heard:
        return False
    known = set(heard)
    shared = sum(1 for word in seen if word in known)
    return shared / len(seen) >= 0.75 and len(heard) - len(seen) <= max(5, len(seen) // 3)


def _skip_verdict(text: str, *, final: bool = False) -> bool | None:
    """Whether the model's reply so far says SKIP: True, False, or None while it cannot be told yet."""
    head = text.lstrip()
    if not head:
        return None
    upper = head.upper()
    if upper.startswith(SKIP):
        return len(head) == len(SKIP) or not head[len(SKIP)].isalnum()
    if SKIP.startswith(upper):
        return True if final else None  # "SK" so far: it may still become SKIP
    return False


class FrameSource(Protocol):
    def frames(self) -> Iterator[np.ndarray]: ...
    def close(self) -> None: ...


class Coach(Protocol):
    def stream(
        self,
        question: str,
        history: list[str],
        cancel: threading.Event,
        language: str | None = None,
        image: str | None = None,
        listening: str | None = None,
    ) -> Iterator[str]: ...


@dataclass
class Callbacks:
    on_turn: Callable[[str, str], None] = lambda speaker, text: None
    on_suggestion_start: Callable[[str], None] = lambda question: None
    on_suggestion_chunk: Callable[[str], None] = lambda text: None
    on_suggestion_done: Callable[[str], None] = lambda text: None
    # the answer shown last was for part of a question that has since been said in full: drop it
    on_suggestion_discard: Callable[[], None] = lambda: None
    on_status: Callable[[str], None] = lambda text: None
    on_state: Callable[[str], None] = lambda kind: None
    on_level: Callable[[str, float], None] = lambda speaker, level: None
    on_language: Callable[[str], None] = lambda code: None
    on_source: Callable[[str], None] = lambda name: None
    on_partial: Callable[[str], None] = lambda text: None
    on_error: Callable[[str], None] = lambda text: None


class InterviewSession:
    def __init__(
        self,
        brief: InterviewBrief,
        transcriber: Transcriber | None,
        coach: Coach,
        callbacks: Callbacks | None = None,
        *,
        language: str | None = None,
        debounce: float = 0.4,
        log: TranscriptLog | None = None,
        early_after: float | None = EARLY_AFTER,
    ):
        self.brief = brief
        self.transcriber = transcriber
        self.coach = coach
        self.cb = callbacks or Callbacks()
        self.debounce = debounce
        # seconds of speech before answering what was heard so far; None turns it off
        self.early_after = early_after
        self.log = log or TranscriptLog()
        self.language = LanguageTracker(language)
        if transcriber is not None and self.language.fixed:
            transcriber.language = self.language.fixed
        self.paused = threading.Event()
        self.stopping = threading.Event()
        self._utterances: queue.Queue[tuple[str, np.ndarray, bool] | None] = queue.Queue()
        self._threads: list[threading.Thread] = []
        self._listeners: list[threading.Thread] = []
        self._stt: threading.Thread | None = None
        self._ready = threading.Event()
        self._sources: list[FrameSource] = []
        self._pending: list[str] = []
        self._timer: threading.Timer | None = None
        self._lock = threading.Lock()
        self._cancel = threading.Event()
        self._suggest_lock = threading.Lock()
        self._listening = False
        # the question an answer was started on while it was still being said, and when
        self._early: tuple[str, float] | None = None
        # tries to answer the utterance now being heard before it ends
        self._early_attempts = 0
        self._early_last: tuple[float, int] | None = None  # when the last try began, and its words
        self._early_busy = False  # a try is still being judged or written
        self._early_card = False  # a try became a card on the screen
        self._early_parts: list[str] | None = None  # finished parts of this question, said before
        self._early_shown: float | None = None  # when an early answer was put on the screen

    def start(self, sources: dict[str, FrameSource]) -> None:
        """Listen to `{speaker: source}`; with no sources only `ask` works."""
        self._listening = bool(sources)
        if self.transcriber is not None:
            self._spawn(lambda: self._load(self.transcriber, first=True), "load")
        else:
            self._ready.set()
        if sources:
            self._ensure_stt()
            self._start_listeners(sources)
        else:
            self.cb.on_state("idle")

    def set_sources(self, sources: dict[str, FrameSource]) -> None:
        """Replace the audio sources while the interview runs (`{}` stops listening)."""
        self._stop_listeners()
        self._listening = bool(sources)
        if sources:
            self._ensure_stt()
            self._start_listeners(sources)
        self.cb.on_state(self._idle_state())

    def set_transcriber(self, transcriber: Transcriber | None) -> None:
        """Switch speech recognition; the old model keeps working until the new one is loaded."""
        if transcriber is None:
            self.transcriber = None
            return
        if self.language.current and not self.language.fixed:
            transcriber.language = None
        elif self.language.fixed:
            transcriber.language = self.language.fixed
        self._spawn(lambda: self._load(transcriber, first=self.transcriber is None), "load")

    def _ensure_stt(self) -> None:
        if self._stt is None or not self._stt.is_alive():
            self._stt = threading.Thread(target=self._transcribe_loop, name="stt", daemon=True)
            self._stt.start()
            self._threads.append(self._stt)

    def _start_listeners(self, sources: dict[str, FrameSource]) -> None:
        for speaker, source in sources.items():
            self._sources.append(source)
            thread = threading.Thread(
                target=lambda s=speaker, src=source: self._listen(s, src),
                name=f"listen-{speaker}",
                daemon=True,
            )
            thread.start()
            self._listeners.append(thread)

    def _stop_listeners(self) -> None:
        for source in self._sources:
            source.close()
        for thread in self._listeners:
            thread.join(timeout=3)
        self._sources, self._listeners = [], []

    def pause(self, value: bool = True) -> None:
        self.paused.set() if value else self.paused.clear()
        self.cb.on_state("paused" if value else "listening")

    def set_language(self, value: str | None) -> str | None:
        """Fix the interview language (`None` returns to auto-detection)."""
        code = self.language.set_fixed(value)
        if self.transcriber is not None:
            self.transcriber.language = code
        if code:
            self.cb.on_language(code)
        return code

    def stop(self) -> None:
        self.stopping.set()
        self._cancel.set()
        if self._timer:
            self._timer.cancel()
        for source in self._sources:
            source.close()
        self._utterances.put(None)
        for thread in [*self._listeners, *self._threads]:
            thread.join(timeout=3)

    def ask(self, question: str) -> None:
        """Get a suggestion for a typed or pasted question right now."""
        question = " ".join(question.split())
        if not question:
            return
        self.log.add(INTERVIEWER, question)
        self.cb.on_turn(INTERVIEWER, question)
        self._suggest(question)

    def ask_screenshot(self, image: str, started_at: float | None = None) -> None:
        """Answer whatever the screenshot shows: a coding task, a slide, a question.

        `image` is a data URL (or bare base64 PNG); `started_at` is when the key was pressed,
        so the time to the first words and to the whole answer can be reported."""
        self.log.add(INTERVIEWER, "[screenshot]")
        self.cb.on_turn(INTERVIEWER, "[screenshot]")
        self._suggest("[screenshot]", image=image, started_at=started_at or time.perf_counter())

    def hint_now(self) -> None:
        """Suggest for the last thing the interviewer said, whether or not it sounded like a question."""
        for turn in reversed(self.log.turns):
            if turn.speaker == INTERVIEWER and turn.text != "[screenshot]":
                self._suggest(turn.text)
                return

    def _spawn(self, target: Callable[[], None], name: str) -> None:
        thread = threading.Thread(target=target, name=name, daemon=True)
        thread.start()
        self._threads.append(thread)

    def _listen(self, speaker: str, source: FrameSource) -> None:
        # A call monitor is digitally silent between words, so it can use a low threshold and
        # still hear a quiet voice; a microphone has room noise and needs a higher one.
        segmenter = UtteranceSegmenter(min_threshold=120.0 if speaker == INTERVIEWER else 250.0)
        count = 0
        since_partial = 0
        early_frames = int(self.early_after * 1000) // FRAME_MS if self.early_after else 0
        early_taken = False
        try:
            for frame in source.frames():
                if self.stopping.is_set():
                    return
                if self.paused.is_set():
                    continue
                count += 1
                if count % LEVEL_EVERY == 0:
                    rms = float(np.sqrt(np.mean(frame.astype(np.float32) ** 2)))
                    self.cb.on_level(speaker, min(1.0, (rms / 6000.0) ** 0.5))
                if (utterance := segmenter.push(frame)) is not None:
                    since_partial = 0
                    early_taken = False
                    self._utterances.put((speaker, utterance, False))
                elif speaker == INTERVIEWER and segmenter.active:
                    since_partial += 1
                    # a caption is made when the speech reaches the early mark, not at the next beat
                    reached = early_frames and len(segmenter.frames) >= early_frames
                    at_mark = bool(reached) and not early_taken
                    due = at_mark or since_partial >= PARTIAL_EVERY
                    if due and (heard := segmenter.snapshot()) is not None:
                        since_partial = 0
                        early_taken = early_taken or at_mark
                        self._utterances.put((speaker, heard, True))
                else:
                    since_partial = 0
                    early_taken = False
        except Exception as exc:
            self.cb.on_state("error")
            self.cb.on_error(f"Audio from {speaker.lower()} stopped: {exc}")

    def _load(self, transcriber: Transcriber, *, first: bool) -> None:
        """Load a speech model (slow the first time) and make it the active one."""
        if first:
            self.cb.on_state("loading")
        self.cb.on_status("Loading speech model…")
        try:
            load = getattr(transcriber, "load", None)
            if load:
                load()
        except Exception as exc:
            self.cb.on_state("error")
            self.cb.on_error(str(exc))
            return
        self.transcriber = transcriber
        self._ready.set()
        self.cb.on_state(self._idle_state())
        self.cb.on_status("Listening")

    def _transcribe_loop(self) -> None:
        last_partial: tuple[str, int, str] | None = None
        last_failure = ("", 0.0)
        while not self.stopping.is_set():
            item = self._utterances.get()
            if item is None:
                return
            transcriber = self.transcriber
            if transcriber is None or not self._ready.is_set():
                continue
            speaker, pcm, partial = item
            if partial and not self._utterances.empty():
                continue  # newer audio is already waiting; a caption of the old state is stale
            reuse = None
            if not partial and last_partial and last_partial[0] == speaker:
                missing = len(pcm) - last_partial[1]
                reuse = last_partial[2] if 0 <= missing <= REUSE_SAMPLES else None
            try:
                text = reuse or transcriber.transcribe(pcm, self.brief.vocabulary())
            except Exception as exc:
                message, now = f"Transcription failed: {exc}", time.monotonic()
                if message != last_failure[0] or now - last_failure[1] > REPEAT_ERROR_AFTER:
                    self.cb.on_error(message)  # a wrong key would otherwise repeat with every caption
                last_failure = (message, now)
                continue
            if partial:
                if text and self._utterances.empty():
                    last_partial = (speaker, len(pcm), text)
                    self.cb.on_partial(text)
                    self._answer_early(speaker, text, len(pcm) / SAMPLE_RATE)
            elif text:
                last_partial = None
                self._end_of_utterance()
                self.cb.on_partial("")
                self._observe_language(text)
                self.handle_text(speaker, text)
            else:
                last_partial = None
                self._end_of_utterance()
                self.cb.on_partial("")

    def _end_of_utterance(self) -> None:
        self._early_attempts, self._early_last = 0, None
        self._early_card, self._early_parts = False, None

    def _answer_early(self, speaker: str, text: str, seconds: float) -> None:
        """The interviewer has been speaking for a while: start answering what was heard.

        Waiting for the end of a long question wastes the seconds the answer could already be
        written in. The first words may be only the set-up ("imagine a service that...") and the
        model then says SKIP, so it is tried again as more is said, a few times at most, until an
        answer is on the screen. When the question is over `_flush_pending` keeps that answer if it
        covers the question, or starts again on the whole of it.
        """
        if speaker != INTERVIEWER or not self.early_after or self.paused.is_set():
            return
        if self._early_card or self._early_busy or self._early_attempts >= EARLY_ATTEMPTS:
            return
        words = len(_words(text))
        if seconds < self.early_after or words < EARLY_MIN_WORDS:
            return
        if not (is_question(text, self.language.current) or words >= EARLY_LONG_WORDS):
            return  # a few words that sound like a statement: wait to hear more or the end of it
        now = time.monotonic()
        if self._early_last and (
            now - self._early_last[0] < EARLY_RETRY or words < self._early_last[1] + EARLY_RETRY_WORDS
        ):
            return
        self._early_attempts += 1
        self._early_last = (now, words)
        self._early_busy = True
        with self._lock:
            if self._timer:
                self._timer.cancel()  # a part of this question waiting to be sent goes with it
            if self._early_parts is None:
                self._early_parts = list(self._pending)
                self._pending.clear()
            question = " ".join([*self._early_parts, text])
            self._early = (question, now)
            parts = len(self._early_parts)
        # the parts of this question already said are turns in the log; they are in the question
        self._suggest(question, early=parts, listening="hearing")

    def _observe_language(self, text: str) -> None:
        if len(text.split()) < MIN_WORDS_FOR_LANGUAGE or self.transcriber is None:
            return
        changed = self.language.observe(
            getattr(self.transcriber, "last_language", None),
            getattr(self.transcriber, "last_probability", 0.0),
        )
        if changed:
            self.cb.on_language(changed)

    def handle_text(self, speaker: str, text: str) -> None:
        self.log.add(speaker, text)
        self.cb.on_turn(speaker, text)
        if speaker != INTERVIEWER or not worth_an_answer(text, self.language.current):
            return
        with self._lock:
            self._pending.append(text)
            if self._timer:
                self._timer.cancel()
            # A sentence that ends with a question mark is complete: do not wait for a second part.
            wait = QUESTION_MARK_WAIT if text.rstrip().endswith(("?", "？")) else self.debounce
            self._timer = threading.Timer(min(wait, self.debounce), self._flush_pending)
            self._timer.daemon = True
            self._timer.start()

    def _flush_pending(self) -> None:
        with self._lock:
            question = " ".join(self._pending)
            self._pending.clear()
            early, self._early = self._early, None
        if not question or self.stopping.is_set():
            return
        if early and time.monotonic() - early[1] < EARLY_STALE and covers(early[0], question):
            return  # the answer started while the question was still being asked is for this
        self._suggest(question, listening="heard")

    def _suggest(
        self,
        question: str,
        image: str | None = None,
        started_at: float | None = None,
        *,
        early: int | None = None,
        listening: str | None = None,
    ) -> None:
        """Answer `question`.

        `early` is the number of finished turns of the log that are part of it while it is still
        being said and so is not a turn itself; for a finished question it is None. `listening` is
        set when the words came from the call audio, so the model may judge them not a question.
        """
        self._cancel.set()
        cancel = threading.Event()
        self._cancel = cancel
        threading.Thread(
            target=self._run_suggestion,
            args=(question, cancel, image, started_at, early, listening),
            name="coach",
            daemon=True,
        ).start()

    def _announce(self, question: str, early: int | None, listening: str | None) -> None:
        """The answer has its first words: put its place on the screen. An answer started on part of a
        question is replaced when the whole of it has been said; a typed question leaves it alone."""
        with self._lock:
            replaced = self._early_shown
            if early is not None:
                self._early_shown = time.monotonic()
                self._early_card = True
            elif listening is not None:
                self._early_shown = None
        if replaced is not None and listening == "heard" and time.monotonic() - replaced < EARLY_STALE:
            self.cb.on_suggestion_discard()
        self.cb.on_suggestion_start(question)

    def _idle_state(self) -> str:
        return "paused" if self.paused.is_set() else "listening" if self._listening else "idle"

    def _run_suggestion(
        self,
        question: str,
        cancel: threading.Event,
        image: str | None,
        started_at: float | None = None,
        early: int | None = None,
        listening: str | None = None,
    ) -> None:
        try:
            self._write_suggestion(question, cancel, image, started_at, early, listening)
        finally:
            if early is not None:
                self._early_busy = False

    def _write_suggestion(
        self,
        question: str,
        cancel: threading.Event,
        image: str | None,
        started_at: float | None,
        early: int | None,
        listening: str | None,
    ) -> None:
        with self._suggest_lock:
            if cancel.is_set():
                return
            if self.brief.is_empty() and not image:  # a task on the screen needs no CV
                self.cb.on_state("error")
                self.cb.on_error("No context yet: add your CV or write about yourself in Settings → Interview")
                return
            # the question is the last turn of the log, except what is still being said
            history = self.log.history()
            history = history[: max(0, len(history) - (1 if early is None else early))]
            self.cb.on_state("thinking")
            # Words from the call audio may not be a question: the model says SKIP, and then nothing is
            # shown. So the answer is announced at its first words, not before them.
            judging = listening is not None and bool(getattr(self.coach, "supports_listening", False))
            if not judging:
                self._announce(question, early, listening)
            announced = not judging
            text = ""
            first_words: float | None = None
            try:
                stream = (
                    self.coach.stream(question, history, cancel, self.language.current, image, listening)
                    if judging
                    else self.coach.stream(question, history, cancel, self.language.current, image)
                )
                for chunk in stream:
                    if cancel.is_set():
                        return
                    first_words = first_words or time.perf_counter()
                    text += chunk
                    if announced:
                        self.cb.on_suggestion_chunk(chunk)
                        continue
                    verdict = _skip_verdict(text)
                    if verdict is None:
                        continue  # too little to tell yet
                    if verdict:
                        self.cb.on_state(self._idle_state())
                        self.cb.on_status("That was not a question for you, so no answer")
                        return
                    announced = True
                    self._announce(question, early, listening)
                    self.cb.on_suggestion_chunk(text)
            except CoachError as exc:
                self.cb.on_state("error")
                self.cb.on_error(str(exc))
                return
            if not announced and text.strip() and not _skip_verdict(text, final=True):
                self._announce(question, early, listening)
                self.cb.on_suggestion_chunk(text)
            elif not announced:
                self.cb.on_state(self._idle_state())
                if text.strip():
                    self.cb.on_status("That was not a question for you, so no answer")
                return
            self.cb.on_state(self._idle_state())
            if getattr(self.coach, "last_finish", None) == "length" and not cancel.is_set():
                self.cb.on_status(
                    "Cut off: the model ran out of room before it finished. "
                    "Ask again, or pick a model with longer answers in Settings → AI."
                )
            if text.strip():
                self.log.suggestions.append((question, parse_suggestion(text).answer))
                self.cb.on_suggestion_done(text)
                if image and started_at and first_words:
                    now = time.perf_counter()
                    self.cb.on_status(
                        f"Screenshot: first words after {first_words - started_at:.1f} s, "
                        f"whole answer after {now - started_at:.1f} s"
                    )
