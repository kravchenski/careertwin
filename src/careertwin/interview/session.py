"""Orchestrates one interview: audio in, transcript and suggestions out.

The session knows nothing about Qt. A front end passes callbacks and calls
`start`, `ask`, `pause` and `stop`; every callback runs on a worker thread.
"""

from __future__ import annotations

import queue
import threading
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import Protocol

import numpy as np

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import CoachError
from careertwin.interview.prompts import parse_suggestion
from careertwin.interview.questions import is_question
from careertwin.interview.stt import Transcriber
from careertwin.interview.transcript import TranscriptLog
from careertwin.interview.vad import UtteranceSegmenter

INTERVIEWER = "Interviewer"
YOU = "You"


class FrameSource(Protocol):
    def frames(self) -> Iterator[np.ndarray]: ...
    def close(self) -> None: ...


class Coach(Protocol):
    def stream(
        self, question: str, history: list[str], cancel: threading.Event
    ) -> Iterator[str]: ...


@dataclass
class Callbacks:
    on_turn: Callable[[str, str], None] = lambda speaker, text: None
    on_suggestion_start: Callable[[str], None] = lambda question: None
    on_suggestion_chunk: Callable[[str], None] = lambda text: None
    on_suggestion_done: Callable[[str], None] = lambda text: None
    on_status: Callable[[str], None] = lambda text: None
    on_error: Callable[[str], None] = lambda text: None


class InterviewSession:
    def __init__(
        self,
        brief: InterviewBrief,
        transcriber: Transcriber | None,
        coach: Coach,
        callbacks: Callbacks | None = None,
        *,
        debounce: float = 1.0,
        log: TranscriptLog | None = None,
    ):
        self.brief = brief
        self.transcriber = transcriber
        self.coach = coach
        self.cb = callbacks or Callbacks()
        self.debounce = debounce
        self.log = log or TranscriptLog()
        self.paused = threading.Event()
        self.stopping = threading.Event()
        self._utterances: queue.Queue[tuple[str, np.ndarray] | None] = queue.Queue()
        self._threads: list[threading.Thread] = []
        self._sources: list[FrameSource] = []
        self._pending: list[str] = []
        self._timer: threading.Timer | None = None
        self._lock = threading.Lock()
        self._cancel = threading.Event()
        self._suggest_lock = threading.Lock()

    def start(self, sources: dict[str, FrameSource]) -> None:
        """Listen to `{speaker: source}`; with no sources only `ask` works."""
        if sources:
            self._spawn(self._transcribe_loop, "stt")
        for speaker, source in sources.items():
            self._sources.append(source)
            self._spawn(lambda s=speaker, src=source: self._listen(s, src), f"listen-{speaker}")

    def pause(self, value: bool = True) -> None:
        self.paused.set() if value else self.paused.clear()
        self.cb.on_status("Paused" if value else "Listening")

    def stop(self) -> None:
        self.stopping.set()
        self._cancel.set()
        if self._timer:
            self._timer.cancel()
        for source in self._sources:
            source.close()
        self._utterances.put(None)
        for thread in self._threads:
            thread.join(timeout=3)

    def ask(self, question: str) -> None:
        """Get a suggestion for a typed or pasted question right now."""
        question = " ".join(question.split())
        if not question:
            return
        self.log.add(INTERVIEWER, question)
        self.cb.on_turn(INTERVIEWER, question)
        self._suggest(question)

    def hint_now(self) -> None:
        """Suggest for the last thing the interviewer said, whether or not it sounded like a question."""
        for turn in reversed(self.log.turns):
            if turn.speaker == INTERVIEWER:
                self._suggest(turn.text)
                return

    def _spawn(self, target: Callable[[], None], name: str) -> None:
        thread = threading.Thread(target=target, name=name, daemon=True)
        thread.start()
        self._threads.append(thread)

    def _listen(self, speaker: str, source: FrameSource) -> None:
        segmenter = UtteranceSegmenter()
        try:
            for frame in source.frames():
                if self.stopping.is_set():
                    return
                if self.paused.is_set():
                    continue
                if (utterance := segmenter.push(frame)) is not None:
                    self._utterances.put((speaker, utterance))
        except Exception as exc:
            self.cb.on_error(f"Audio from {speaker.lower()} stopped: {exc}")

    def _transcribe_loop(self) -> None:
        assert self.transcriber is not None
        self.cb.on_status("Loading speech model…")
        try:
            load = getattr(self.transcriber, "load", None)
            if load:
                load()
        except Exception as exc:
            self.cb.on_error(str(exc))
            return
        self.cb.on_status("Listening")
        vocabulary = self.brief.vocabulary()
        while not self.stopping.is_set():
            item = self._utterances.get()
            if item is None:
                return
            speaker, pcm = item
            try:
                text = self.transcriber.transcribe(pcm, vocabulary)
            except Exception as exc:
                self.cb.on_error(f"Transcription failed: {exc}")
                continue
            if text:
                self.handle_text(speaker, text)

    def handle_text(self, speaker: str, text: str) -> None:
        self.log.add(speaker, text)
        self.cb.on_turn(speaker, text)
        if speaker != INTERVIEWER or not is_question(text):
            return
        with self._lock:
            self._pending.append(text)
            if self._timer:
                self._timer.cancel()
            self._timer = threading.Timer(self.debounce, self._flush_pending)
            self._timer.daemon = True
            self._timer.start()

    def _flush_pending(self) -> None:
        with self._lock:
            question = " ".join(self._pending)
            self._pending.clear()
        if question and not self.stopping.is_set():
            self._suggest(question)

    def _suggest(self, question: str) -> None:
        self._cancel.set()
        cancel = threading.Event()
        self._cancel = cancel
        threading.Thread(
            target=self._run_suggestion, args=(question, cancel), name="coach", daemon=True
        ).start()

    def _run_suggestion(self, question: str, cancel: threading.Event) -> None:
        with self._suggest_lock:
            if cancel.is_set():
                return
            history = self.log.history()[:-1]
            self.cb.on_suggestion_start(question)
            text = ""
            try:
                for chunk in self.coach.stream(question, history, cancel):
                    if cancel.is_set():
                        return
                    text += chunk
                    self.cb.on_suggestion_chunk(chunk)
            except CoachError as exc:
                self.cb.on_error(str(exc))
                return
            if text.strip():
                self.log.suggestions.append((question, parse_suggestion(text).answer))
                self.cb.on_suggestion_done(text)
