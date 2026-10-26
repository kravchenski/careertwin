"""Local speech recognition with faster-whisper."""

from __future__ import annotations

import ctypes
import importlib.util
import os
import re
from pathlib import Path
from typing import Protocol

import numpy as np

GPU_LIBRARY_ERROR = re.compile(r"cublas|cudnn|cuda|libcu", re.I)


CUDA_LIBRARIES = (
    ("nvidia.cublas", "libcublasLt.so.12"),
    ("nvidia.cublas", "libcublas.so.12"),
    ("nvidia.cudnn", "libcudnn.so.9"),
)


def preload_cuda_libraries() -> int:
    """Make the CUDA 12 libraries of the `gpu` extra visible to CTranslate2; return how many loaded.

    pip's nvidia-* wheels put them in site-packages/nvidia/*/lib, a folder the dynamic loader does
    not search, so they are loaded by path before the model is created."""
    loaded = 0
    for package, library in CUDA_LIBRARIES:
        try:
            spec = importlib.util.find_spec(package)
        except (ImportError, ValueError):
            continue
        for base in (spec.submodule_search_locations or []) if spec else []:
            candidate = Path(base) / "lib" / library
            if candidate.exists():
                try:
                    ctypes.CDLL(str(candidate), mode=ctypes.RTLD_GLOBAL)
                    loaded += 1
                except OSError:
                    pass
                break
    return loaded


def cuda_device_count() -> int:
    try:
        import ctranslate2

        return ctranslate2.get_cuda_device_count()
    except Exception:
        return 0


MIN_LOGPROB = -1.5  # below this Whisper is guessing
SILENCE_PROB, SILENCE_LOGPROB = 0.6, -1.0  # it thinks nobody speaks and is not sure of what it wrote
MAX_COMPRESSION = 2.4  # text that repeats itself ("REST, REST, REST") is a loop, not speech
RETRY_LOGPROB = -0.5  # a fixed language that reads this badly is looked at again in the language Whisper hears
SWITCH_PROBABILITY = 0.8
ECHO_WORDS = 6  # a reply this short made only of the words of the prompt is the prompt repeated


def is_speech(segment) -> bool:
    """Whether Whisper believes its own segment. On music and noise it writes words anyway, and with a
    prompt of terms it writes the terms; its own confidence figures tell those from speech."""
    logprob = getattr(segment, "avg_logprob", 0.0)
    if logprob < MIN_LOGPROB:
        return False
    if getattr(segment, "no_speech_prob", 0.0) > SILENCE_PROB and logprob < SILENCE_LOGPROB:
        return False
    return getattr(segment, "compression_ratio", 1.0) <= MAX_COMPRESSION


def echoes_prompt(text: str, prompt: str) -> bool:
    """A few words that are all in the prompt and no question: Whisper read its prompt back."""
    word = r"[\w+#]+(?:\.[\w+#]+)*"
    words = re.findall(word, text.lower())
    known = set(re.findall(word, prompt.lower()))
    return bool(words) and len(words) <= ECHO_WORDS and "?" not in text and all(word in known for word in words)


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
        compute_type: str = "auto",
    ):
        self.model_name = model or os.getenv("CAREERTWIN_WHISPER_MODEL", "small")
        self.language = None if language in (None, "", "auto") else language
        self.last_language: str | None = None
        self.last_probability = 0.0
        self.device = device
        self.compute_type = compute_type
        self.active_device = ""
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
        use_gpu = self.device == "cuda" or (self.device == "auto" and cuda_device_count() > 0)
        if use_gpu:
            preload_cuda_libraries()
            try:
                self._model = WhisperModel(
                    self.model_name,
                    device="cuda",
                    compute_type="float16" if self.compute_type == "auto" else self.compute_type,
                )
                self.active_device = "cuda"
            except (RuntimeError, ValueError) as exc:
                if self.device == "cuda" and not GPU_LIBRARY_ERROR.search(str(exc)):
                    raise
                self.device = "cpu"
        if self._model is None:
            self._model = WhisperModel(
                self.model_name,
                device="cpu",
                compute_type="int8" if self.compute_type in ("auto", "float16") else self.compute_type,
            )
            self.active_device = "cpu"
        self._warm_up()

    def _warm_up(self) -> None:
        """The first pass pays for kernel setup; do it now, while the window says it is loading."""
        try:
            self._run(np.zeros(16_000, dtype=np.float32), "")
        except RuntimeError as exc:
            if self.active_device == "cuda" and GPU_LIBRARY_ERROR.search(str(exc)):
                self.device, self._model = "cpu", None
                self.load()

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
        text, logprob, info = self._decode(audio, prompt, self.language)
        language, probability = self.language or getattr(info, "language", None), 1.0 if self.language else getattr(info, "language_probability", 0.0)
        if self.language and (not text or logprob < RETRY_LOGPROB):
            # a fixed language is wrong for a speaker of another one (a Russian question read as English
            # gives "levoe s'iedininev"): look again without it and keep that reading if it is surely another
            # language and the better one
            other, other_logprob, other_info = self._decode(audio, prompt, None)
            detected = getattr(other_info, "language", None)
            sure = getattr(other_info, "language_probability", 0.0)
            if other and detected and detected != self.language and sure >= SWITCH_PROBABILITY and (not text or other_logprob > logprob):
                text, language, probability = other, detected, sure
        self.last_language, self.last_probability = language, probability
        return "" if echoes_prompt(text, prompt) else text

    def _decode(self, audio: np.ndarray, prompt: str, language: str | None):
        """(text of the segments Whisper believes, the lowest confidence among all of them, info)."""
        segments, info = self._model.transcribe(
            audio,
            language=language,
            beam_size=1,
            vad_filter=False,
            without_timestamps=True,
            condition_on_previous_text=False,
            initial_prompt=prompt or None,
        )
        heard = list(segments)
        text = " ".join(segment.text.strip() for segment in heard if is_speech(segment)).strip()
        return text, min((getattr(segment, "avg_logprob", 0.0) for segment in heard), default=0.0), info
