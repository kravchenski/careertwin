"""Cloud speech recognition with Deepgram: every utterance goes to the API as a small WAV file.

Unlike the local Whisper model nothing is installed or downloaded, and it is fast on a computer
without a graphics card. The cost is that the voices of the call leave this computer, and that the
live captions send the growing sentence again every 1.5 s, so the audio Deepgram bills is about
three to four times the speech.
"""

from __future__ import annotations

import io
import json
import os
import wave

import httpx
import numpy as np

from careertwin.interview.vad import SAMPLE_RATE

LISTEN_URL = "https://api.deepgram.com/v1/listen"
PROJECTS_URL = "https://api.deepgram.com/v1/projects"
MODELS = ("nova-3", "nova-2")
MAX_KEY_TERMS = 20
MIN_SAMPLES = SAMPLE_RATE // 10  # a tenth of a second is not worth a request


class DeepgramError(RuntimeError):
    """Deepgram refused the request or could not be reached; the text says what to change."""


def wav_bytes(pcm: np.ndarray) -> bytes:
    """16 kHz mono PCM16 samples in a WAV container, which Deepgram recognises by itself."""
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(SAMPLE_RATE)
        wav.writeframes(np.asarray(pcm, dtype="<i2").tobytes())
    return buffer.getvalue()


def key_terms(vocabulary: str) -> list[str]:
    """The comma-separated names worth recognising (company, skills) as a short list."""
    terms = (term.strip() for term in vocabulary.split(","))
    return [term for term in terms if 0 < len(term) <= 40][:MAX_KEY_TERMS]


def explain(status: int, body: str) -> str:
    """A failed response as one line; Deepgram answers with `err_msg`, `message` or plain text."""
    try:
        data = json.loads(body)
        detail = (data.get("err_msg") or data.get("message") or "") if isinstance(data, dict) else ""
    except ValueError:
        detail = ""
    detail = detail or body.strip()[:200]
    if status in (401, 403):
        return "Deepgram rejected the API key: check it in Settings → Audio"
    if status == 402:
        return "Deepgram says the account has no credit left"
    if status == 429:
        return "Deepgram says there are too many requests; try again in a moment"
    return f"Deepgram returned {status}: {detail}"


def check_key(api_key: str, transport: httpx.BaseTransport | None = None) -> None:
    """Ask Deepgram for the account's projects: succeeds when the key works, else raises."""
    if not api_key:
        raise DeepgramError("Enter the Deepgram API key first")
    try:
        with httpx.Client(timeout=10, transport=transport) as client:
            response = client.get(PROJECTS_URL, headers={"Authorization": f"Token {api_key}"})
    except httpx.HTTPError as exc:
        raise DeepgramError(f"Deepgram is not reachable ({type(exc).__name__})") from exc
    if response.status_code >= 400:
        raise DeepgramError(explain(response.status_code, response.text))


class DeepgramTranscriber:
    """The same interface as the Whisper transcriber, so the session does not know the difference."""

    active_device = "deepgram"

    def __init__(
        self,
        api_key: str = "",
        model: str = "nova-3",
        *,
        language: str | None = None,
        transport: httpx.BaseTransport | None = None,
        timeout: float = 20.0,
    ):
        self.api_key = api_key or os.getenv("DEEPGRAM_API_KEY", "")
        self.model = model or MODELS[0]
        self.language = None if language in (None, "", "auto") else language
        self.last_language: str | None = None
        self.last_probability = 0.0
        self.transport = transport
        self.timeout = timeout
        self._client: httpx.Client | None = None

    def load(self) -> None:
        if not self.api_key:
            raise RuntimeError(
                "Deepgram needs an API key: Settings → Audio (or DEEPGRAM_API_KEY in .env)"
            )

    def transcribe(self, pcm: np.ndarray, prompt: str = "") -> str:
        self.load()
        if len(pcm) < MIN_SAMPLES:
            return ""
        terms = key_terms(prompt) if self.model.startswith("nova-3") else []
        response = self._post(pcm, terms)
        if response.status_code == 400 and terms:  # a key term the API dislikes must not cost a sentence
            response = self._post(pcm, [])
        if response.status_code >= 400:
            raise DeepgramError(explain(response.status_code, response.text))
        try:
            channel = response.json()["results"]["channels"][0]
            text = channel["alternatives"][0]["transcript"]
        except (ValueError, KeyError, IndexError, TypeError) as exc:
            raise DeepgramError("Deepgram returned an unreadable answer") from exc
        self.last_language = self.language or channel.get("detected_language")
        self.last_probability = 1.0 if self.language else float(channel.get("language_confidence") or 0.0)
        return text.strip()

    def _post(self, pcm: np.ndarray, terms: list[str]) -> httpx.Response:
        params: list[tuple[str, str]] = [("model", self.model), ("smart_format", "true")]
        params.append(("language", self.language) if self.language else ("detect_language", "true"))
        params += [("keyterm", term) for term in terms]
        if self._client is None:  # one connection for the whole call: no new handshake per sentence
            self._client = httpx.Client(timeout=self.timeout, transport=self.transport)
        try:
            return self._client.post(
                LISTEN_URL,
                params=params,
                content=wav_bytes(pcm),
                headers={"Authorization": f"Token {self.api_key}", "Content-Type": "audio/wav"},
            )
        except httpx.HTTPError as exc:
            raise DeepgramError(f"Deepgram is not reachable ({type(exc).__name__})") from exc
