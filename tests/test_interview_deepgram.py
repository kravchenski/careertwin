"""Deepgram as the speech engine: the request it sends, what it makes of the answer, the errors."""

from __future__ import annotations

import io
import threading
import time
import wave

import httpx
import numpy as np
import pytest

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.runtime import make_transcriber
from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
from careertwin.interview.settings import InterviewSettings
from careertwin.interview.stt import WhisperTranscriber
from careertwin.interview.stt_deepgram import (
    DeepgramError,
    DeepgramTranscriber,
    check_key,
    key_terms,
    wav_bytes,
)
from careertwin.interview.transcript import TranscriptLog


def pcm(seconds: float = 1.0) -> np.ndarray:
    return (np.arange(int(16_000 * seconds)) % 200).astype(np.int16)


def answer(text="hello there", language="en", confidence=0.93):
    channel = {"alternatives": [{"transcript": text}], "detected_language": language, "language_confidence": confidence}
    return httpx.Response(200, json={"results": {"channels": [channel]}})


def make(handler, **kwargs):
    return DeepgramTranscriber("dg-key", transport=httpx.MockTransport(handler), **kwargs)


def test_samples_travel_as_a_16_khz_mono_wav():
    samples = pcm(0.5)
    with wave.open(io.BytesIO(wav_bytes(samples))) as wav:
        assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) == (1, 2, 16_000)
        assert np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2").tolist() == samples.tolist()


def test_the_vocabulary_becomes_a_short_list_of_key_terms():
    assert key_terms("Grid Dynamics, Java, , " + "x" * 41 + ", Spring Boot") == ["Grid Dynamics", "Java", "Spring Boot"]
    assert len(key_terms(", ".join(f"term{i}" for i in range(50)))) == 20


def test_an_utterance_is_posted_with_the_key_and_language_detection():
    seen = {}

    def handler(request):
        seen.update(url=request.url, auth=request.headers["authorization"], kind=request.headers["content-type"], body=request.content)
        return answer("what is python", "en", 0.9)

    transcriber = make(handler)
    assert transcriber.transcribe(pcm(), "Grid Dynamics, Java") == "what is python"
    assert seen["url"].host == "api.deepgram.com" and seen["url"].path == "/v1/listen"
    params = seen["url"].params
    assert params["model"] == "nova-3" and params["smart_format"] == "true" and params["detect_language"] == "true"
    assert "language" not in params and params.get_list("keyterm") == ["Grid Dynamics", "Java"]
    assert seen["auth"] == "Token dg-key" and seen["kind"] == "audio/wav" and seen["body"][:4] == b"RIFF"
    assert (transcriber.last_language, transcriber.last_probability) == ("en", 0.9)


def test_a_fixed_language_is_sent_and_trusted():
    seen = {}
    transcriber = make(lambda request: seen.update(params=request.url.params) or answer("cześć", "pl", 0.2), language="pl")
    assert transcriber.transcribe(pcm()) == "cześć"
    assert seen["params"]["language"] == "pl" and "detect_language" not in seen["params"]
    assert (transcriber.last_language, transcriber.last_probability) == ("pl", 1.0)


def test_the_session_can_switch_the_language_between_calls():
    languages = []
    transcriber = make(lambda request: languages.append(dict(request.url.params)) or answer())
    transcriber.transcribe(pcm())
    transcriber.language = "ru"
    transcriber.transcribe(pcm())
    assert "detect_language" in languages[0] and languages[1]["language"] == "ru"


def test_key_terms_are_for_nova_3_only_and_a_rejected_list_is_retried_without_it():
    calls = []

    def handler(request):
        calls.append(request.url.params.get_list("keyterm"))
        return httpx.Response(400, text="bad keyterm") if request.url.params.get_list("keyterm") else answer("ok")

    assert make(handler).transcribe(pcm(), "Java, Spring") == "ok"
    assert calls == [["Java", "Spring"], []]
    calls.clear()
    assert make(handler, ).transcribe(pcm(), "") == "ok" and calls == [[]]
    calls.clear()
    older = DeepgramTranscriber("k", "nova-2", transport=httpx.MockTransport(lambda r: calls.append(r.url.params.get_list("keyterm")) or answer()))
    older.transcribe(pcm(), "Java, Spring")
    assert calls == [[]]


def test_a_very_short_sound_is_not_worth_a_request():
    def handler(request):
        raise AssertionError("no request expected")

    assert make(handler).transcribe(np.zeros(800, dtype=np.int16)) == ""


@pytest.mark.parametrize(
    ("response", "expected"),
    [
        (httpx.Response(401, json={"err_code": "INVALID_AUTH", "err_msg": "Invalid credentials."}), "rejected the API key"),
        (httpx.Response(402, text="pay"), "no credit left"),
        (httpx.Response(429, text="slow"), "too many requests"),
        (httpx.Response(500, json={"err_msg": "boom"}), "Deepgram returned 500: boom"),
        (httpx.Response(503, json={"message": "maintenance", "category": "x"}), "Deepgram returned 503: maintenance"),
        (httpx.Response(502, text="plain text error"), "Deepgram returned 502: plain text error"),
        (httpx.Response(200, json={"results": {}}), "unreadable"),
        (httpx.Response(200, text="not json"), "unreadable"),
    ],
)
def test_failures_say_what_to_change(response, expected):
    with pytest.raises(DeepgramError, match=expected):
        make(lambda request: response).transcribe(pcm())


def test_an_unreachable_deepgram_is_explained():
    def refuse(request):
        raise httpx.ConnectError("refused")

    with pytest.raises(DeepgramError, match=r"Deepgram is not reachable \(ConnectError\)"):
        make(refuse).transcribe(pcm())


def test_without_a_key_loading_says_where_to_enter_it(monkeypatch):
    monkeypatch.delenv("DEEPGRAM_API_KEY", raising=False)
    with pytest.raises(RuntimeError, match="Settings → Audio"):
        DeepgramTranscriber("").load()
    monkeypatch.setenv("DEEPGRAM_API_KEY", "from-env")
    assert DeepgramTranscriber("").api_key == "from-env"


def test_the_key_check_asks_for_the_projects():
    seen = {}

    def ok(request):
        seen.update(url=str(request.url), auth=request.headers["authorization"])
        return httpx.Response(200, json={"projects": []})

    check_key("dg-key", httpx.MockTransport(ok))
    assert seen == {"url": "https://api.deepgram.com/v1/projects", "auth": "Token dg-key"}
    with pytest.raises(DeepgramError, match="rejected"):
        check_key("bad", httpx.MockTransport(lambda r: httpx.Response(401, text="no")))
    with pytest.raises(DeepgramError, match="Enter the Deepgram API key"):
        check_key("")


def test_the_settings_choose_the_engine():
    settings = InterviewSettings()
    assert settings.stt_engine == "whisper" and settings.deepgram_model == "nova-3"
    assert isinstance(make_transcriber(settings, "en"), WhisperTranscriber)
    settings.stt_engine, settings.deepgram_key, settings.deepgram_model = "deepgram", "k", "nova-2"
    chosen = make_transcriber(settings, "pl")
    assert isinstance(chosen, DeepgramTranscriber) and (chosen.api_key, chosen.model, chosen.language) == ("k", "nova-2", "pl")
    with pytest.raises(ValueError):
        settings.set("stt_engine", "azure", save=False)


class Failing:
    last_language, last_probability, language = None, 0.0, None

    def transcribe(self, pcm, prompt=""):
        raise DeepgramError("Deepgram rejected the API key: check it in Settings → Audio")


def test_a_wrong_key_is_reported_once_not_with_every_caption(tmp_path, monkeypatch):
    about = tmp_path / "ABOUT.md"
    about.write_text("# C\n\n## Experience\n- Built FastAPI services.\n")
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(about))
    errors = []
    session = InterviewSession(
        InterviewBrief.load(), Failing(), None, Callbacks(on_error=errors.append),
        debounce=0.01, log=TranscriptLog(tmp_path),
    )
    session.transcriber, session._ready = Failing(), threading.Event()
    session._ready.set()
    for _ in range(4):
        session._utterances.put((INTERVIEWER, pcm(2), False))
    thread = threading.Thread(target=session._transcribe_loop, daemon=True)
    thread.start()
    end = time.monotonic() + 3
    while time.monotonic() < end and not session._utterances.empty():
        time.sleep(0.01)
    time.sleep(0.1)
    session.stop()
    assert len(errors) == 1 and "rejected the API key" in errors[0]
