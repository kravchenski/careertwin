"""Connects the saved settings to the live interview: audio, speech model, gateway, screenshots.

The window calls `update()` for every change; it is applied at once and saved. Nothing here
imports Qt, so the whole behaviour is testable without a display.
"""

from __future__ import annotations

import sys
import threading
from collections.abc import Callable
from typing import Any

import httpx

from careertwin.interview import screenshot
from careertwin.interview.audio import (
    AudioError,
    FollowingSource,
    default_sources,
    list_sources,
    open_source,
)
from careertwin.interview.coach import StreamingCoach, gateway_url
from careertwin.interview.languages import normalize
from careertwin.interview.library import ContextController
from careertwin.interview.prepare import (
    ANALYSIS_SYSTEM,
    QUESTIONS_SYSTEM,
    PrepareError,
    analysis_prompt,
    parse_analysis,
    parse_questions,
    questions_prompt,
)
from careertwin.interview.providers import base_url
from careertwin.interview.session import INTERVIEWER, YOU, FrameSource, InterviewSession
from careertwin.interview.settings import InterviewSettings
from careertwin.interview.stt import Transcriber, WhisperTranscriber
from careertwin.interview.stt_deepgram import DeepgramTranscriber

AUDIO_KEYS = {"listen", "call_source", "use_mic", "mic_source"}
SPEECH_KEYS = {"stt_engine", "whisper_model", "whisper_device", "deepgram_key", "deepgram_model"}
COACH_KEYS = {"gateway_url", "gateway_key", "model", "vision_model", "answer_style", "answer_layout"}
WINDOW_KEYS = {"pinned", "frameless", "opacity", "font_scale", "show_full_answer", "ui_language"}


def make_transcriber(settings: InterviewSettings, language: str | None) -> Transcriber:
    """The speech recognition the settings ask for: Deepgram in the cloud, or Whisper on this computer."""
    if settings.stt_engine == "deepgram":
        return DeepgramTranscriber(settings.deepgram_key, settings.deepgram_model, language=language)
    return WhisperTranscriber(settings.whisper_model, language=language, device=settings.whisper_device)


class Runtime:
    def __init__(
        self,
        settings: InterviewSettings,
        context: ContextController,
        coach: StreamingCoach,
        session: InterviewSession,
    ):
        self.settings = settings
        self.context = context
        self.coach = coach
        self.session = session
        self.current_source = ""
        self.on_window: Callable[[str, Any], None] = lambda name, value: None
        self.on_error: Callable[[str], None] = lambda message: None

    # ---- applying settings -------------------------------------------------

    def apply_all(self) -> None:
        """Push every saved setting into the live objects (audio is started by `start_audio`)."""
        self._apply_coach()
        self.session.set_language(self._language())

    def update(self, name: str, value: Any) -> None:
        """Change one setting, save it and apply it. Raises ValueError for a bad value."""
        self.settings.set(name, value)
        if name == "language":
            self.session.set_language(self._language())
        elif name in COACH_KEYS:
            self._apply_coach()
            if name == "answer_layout":
                self.on_window(name, self.settings.answer_layout)
        elif name in SPEECH_KEYS:
            self.reload_speech()
        elif name in AUDIO_KEYS:
            self.restart_audio()
            if name == "listen":
                self.on_window(name, self.settings.listen)
        elif name in WINDOW_KEYS:
            self.on_window(name, getattr(self.settings, name))

    def reset(self) -> None:
        """Restore the defaults for every setting."""
        defaults = InterviewSettings()
        for name in vars(defaults):
            self.update(name, getattr(defaults, name))

    def _language(self) -> str | None:
        return normalize(self.settings.language)

    def _apply_coach(self) -> None:
        self.coach.url = self.settings.gateway_url or None
        self.coach.api_key = self.settings.gateway_key or None
        self.coach.model = self.settings.model or None
        self.coach.vision_model = self.settings.vision_model or None
        self.coach.style = self.settings.answer_style
        self.coach.layout = self.settings.answer_layout

    # ---- audio and speech --------------------------------------------------

    def build_sources(self) -> dict[str, FrameSource]:
        """The audio sources the settings ask for: call audio and optionally the microphone."""
        if not self.settings.listen:
            return {}
        sources: dict[str, FrameSource] = {}
        if self.settings.call_source:
            sources[INTERVIEWER] = open_source(self.settings.call_source)
            self._source_changed(self.settings.call_source)
        else:
            sources[INTERVIEWER] = FollowingSource(on_change=self._source_changed)
        if self.settings.use_mic:
            sources[YOU] = open_source(self.settings.mic_source or default_sources().microphone, microphone=True)
        return sources

    def _source_changed(self, name: str) -> None:
        self.current_source = name
        self.session.cb.on_source(name)

    def start_audio(self) -> None:
        self.session.start(self._safe_sources())

    def restart_audio(self) -> None:
        if self.settings.listen and self.session.transcriber is None:
            self.reload_speech()
        self.session.set_sources(self._safe_sources())

    def _safe_sources(self) -> dict[str, FrameSource]:
        try:
            return self.build_sources()
        except AudioError as exc:
            self.on_error(str(exc))
            return {}

    def reload_speech(self) -> None:
        if not self.settings.listen and self.session.transcriber is None:
            return
        self.session.set_transcriber(make_transcriber(self.settings, self._language()))

    @classmethod
    def audio_choices(cls) -> tuple[list[str], list[str]]:
        """(outputs whose sound can be heard, microphones): what the two pickers offer."""
        if sys.platform == "win32":
            try:
                from careertwin.interview import audio_windows

                return audio_windows.list_outputs(), audio_windows.list_microphones()
            except AudioError:
                return [], []
        names = cls.audio_sources()
        return (
            sorted(name for name in names if name.endswith(".monitor")),
            sorted(name for name in names if not name.endswith(".monitor")),
        )

    @staticmethod
    def audio_sources() -> list[str]:
        try:
            return list_sources()
        except AudioError:
            return []

    # ---- gateway -----------------------------------------------------------

    def warm_up(self) -> None:
        """Open the connection to the AI provider in the background, before the first question."""
        threading.Thread(target=self.coach.warm_up, name="warm-up", daemon=True).start()

    def gateway(self) -> str:
        return base_url(self.settings.gateway_url or gateway_url())

    def models(self) -> list[str]:
        """Model ids offered by the gateway; raises RuntimeError with a readable message."""
        headers = {}
        if self.settings.gateway_key:
            headers["Authorization"] = f"Bearer {self.settings.gateway_key}"
        try:
            response = httpx.get(f"{self.gateway()}/v1/models", headers=headers, timeout=8)
            response.raise_for_status()
            return [item["id"] for item in response.json().get("data", []) if "id" in item]
        except httpx.HTTPError as exc:
            raise RuntimeError(f"Cannot reach {self.gateway()} ({type(exc).__name__})") from exc
        except (ValueError, KeyError, TypeError) as exc:
            raise RuntimeError(f"{self.gateway()} did not return a model list") from exc

    # ---- preparing ---------------------------------------------------------

    def prepare_language(self) -> str | None:
        """The language to prepare in: the fixed one, else what the interview settled on."""
        return self._language() or self.session.language.current

    def question_bank(self, count: int = 10, focus: str = "") -> list[dict]:
        """Likely questions for the active interview, each with first-person lines to say."""
        brief = self.context.brief
        prompt = questions_prompt(brief, count, focus, self.prepare_language())
        return self._retry(
            lambda: parse_questions(
                self.coach.complete(QUESTIONS_SYSTEM, prompt, max_tokens=3000, temperature=0.4), count
            )
        )

    def analyze_cv(self) -> dict:
        """Review the attached CV against the vacancy of the active interview."""
        prompt = analysis_prompt(self.context.brief, self.prepare_language())
        return self._retry(
            lambda: parse_analysis(self.coach.complete(ANALYSIS_SYSTEM, prompt, max_tokens=3000))
        )

    @staticmethod
    def _retry(work, attempts: int = 2):
        """Free models sometimes return broken JSON; try once more before giving up."""
        for attempt in range(attempts):
            try:
                return work()
            except PrepareError:
                if attempt == attempts - 1:
                    raise

    # ---- screenshots -------------------------------------------------------

    def screenshot_tools(self) -> list[str]:
        return screenshot.available_tools()

    def screenshot_ready(self) -> bool:
        return self.settings.screenshot_enabled and bool(self.screenshot_tools())

    def capture(self, output: str | None = None) -> str:
        """Capture the screen (or one monitor) with the configured tool; return it as a data URL."""
        return screenshot.capture(self.settings.screenshot_tool, output).data_url()
