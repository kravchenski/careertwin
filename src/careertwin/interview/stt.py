"""Local speech recognition with faster-whisper."""

from __future__ import annotations

import os
import re
from typing import Protocol

import numpy as np

GPU_LIBRARY_ERROR = re.compile(r"cublas|cudnn|cuda|libcu", re.I)


class Transcriber(Protocol):
    language: str | None
    last_language: str | None
    last_probability: float

    def transcribe(self, pcm: np.ndarray, prompt: str = "") -> str: ...


class WhisperTranscriber:
    """Loads the model lazily so the window can open while it downloads."""

    def __init__(
        self,
        model: str | None = None,
        *,
        language: str | None = None,
        device: str = "auto",
        compute_type: str = "int8",
    ):
        self.model_name = model or os.getenv("CAREERTWIN_WHISPER_MODEL", "small")
        self.language = None if language in (None, "", "auto") else language
        self.last_language: str | None = None
        self.last_probability = 0.0
        self.device = device
        self.compute_type = compute_type
        self._model = None

    def load(self) -> None:
        if self._model is not None:
            return
        try:
            from faster_whisper import WhisperModel
        except ImportError as exc:
            raise RuntimeError(
                "Speech recognition needs the interview extra: uv sync --extra interview"
            ) from exc
        self._model = WhisperModel(
            self.model_name, device=self.device, compute_type=self.compute_type
        )

    def transcribe(self, pcm: np.ndarray, prompt: str = "") -> str:
        self.load()
        audio = pcm.astype(np.float32) / 32768.0
        try:
            return self._run(audio, prompt)
        except RuntimeError as exc:
            if self.device == "cpu" or not GPU_LIBRARY_ERROR.search(str(exc)):
                raise
            self.device, self._model = "cpu", None
            self.load()
            return self._run(audio, prompt)

    def _run(self, audio: np.ndarray, prompt: str) -> str:
        segments, info = self._model.transcribe(
            audio,
            language=self.language,
            beam_size=1,
            vad_filter=False,
            condition_on_previous_text=False,
            initial_prompt=prompt or None,
        )
        text = " ".join(segment.text.strip() for segment in segments).strip()
        self.last_language = self.language or getattr(info, "language", None)
        self.last_probability = 1.0 if self.language else getattr(info, "language_probability", 0.0)
        return text
