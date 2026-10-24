"""Speech-to-text latency: quicker end of speech, live captions, GPU with a safe fallback."""

from __future__ import annotations

import inspect
import sys
import threading
import time
import types

import numpy as np
import pytest

from careertwin.interview import stt
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.session import (
    INTERVIEWER,
    REUSE_SAMPLES,
    YOU,
    Callbacks,
    InterviewSession,
)
from careertwin.interview.stt import WhisperTranscriber
from careertwin.interview.transcript import TranscriptLog
from careertwin.interview.vad import FRAME_SAMPLES, UtteranceSegmenter


def tone(ms, amplitude=4000):
    t = np.arange(FRAME_SAMPLES * (ms // 20)) / 16000
    return (amplitude * np.sin(2 * np.pi * 220 * t)).astype(np.int16)


def silence(ms):
    return np.zeros(FRAME_SAMPLES * (ms // 20), dtype=np.int16)


def frames(*parts):
    audio = np.concatenate(parts)
    return [audio[i : i + FRAME_SAMPLES] for i in range(0, len(audio) - FRAME_SAMPLES + 1, FRAME_SAMPLES)]


def test_speech_ends_after_half_a_second_of_silence_and_chunks_are_short():
    done = []
    segmenter = UtteranceSegmenter()
    for frame in frames(silence(200), tone(1000), silence(600)):
        if (utterance := segmenter.push(frame)) is not None:
            done.append(utterance)
    assert len(done) == 1
    pause = UtteranceSegmenter()
    kept = [u for f in frames(tone(600), silence(400), tone(600), silence(600)) if (u := pause.push(f)) is not None]
    assert len(kept) == 1
    assert segmenter.max_frames * 20 == 12_000


def test_snapshot_gives_the_speech_so_far():
    segmenter = UtteranceSegmenter()
    assert segmenter.snapshot() is None
    for frame in frames(silence(200), tone(100)):
        segmenter.push(frame)
    assert segmenter.snapshot() is None
    for frame in frames(tone(1000)):
        segmenter.push(frame)
    heard = segmenter.snapshot()
    assert heard is not None and len(heard) >= 16_000


def test_questions_are_sent_to_the_coach_after_a_short_debounce():
    assert inspect.signature(InterviewSession.__init__).parameters["debounce"].default == 0.4


ABOUT = "# C\n\n## Experience\n- Built FastAPI services.\n"


@pytest.fixture
def about(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))


class Counting:
    last_language, last_probability, language = None, 0.0, None

    def __init__(self, text="the final text"):
        self.calls, self.text = 0, text

    def transcribe(self, pcm, prompt=""):
        self.calls += 1
        return self.text


class NoCoach:
    def stream(self, *args, **kwargs):
        return iter(())


def make(tmp_path, transcriber):
    captions, turns = [], []
    session = InterviewSession(
        InterviewBrief.load(), transcriber, NoCoach(),
        Callbacks(on_partial=captions.append, on_turn=lambda s, t: turns.append((s, t))),
        debounce=0.01, log=TranscriptLog(tmp_path),
    )
    session.transcriber, session._ready = transcriber, threading.Event()
    session._ready.set()
    return session, captions, turns


def run_loop(session):
    thread = threading.Thread(target=session._transcribe_loop, daemon=True)
    thread.start()
    return thread


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def pcm(seconds):
    return np.zeros(int(16_000 * seconds), dtype=np.int16)


def test_stale_captions_are_skipped_when_newer_audio_is_waiting(about, tmp_path):
    transcriber = Counting()
    session, captions, turns = make(tmp_path, transcriber)
    for item in ((INTERVIEWER, pcm(2), True), (INTERVIEWER, pcm(3), True), (INTERVIEWER, pcm(4), False)):
        session._utterances.put(item)
    thread = run_loop(session)
    assert wait_for(lambda: turns)
    assert transcriber.calls == 1 and turns == [(INTERVIEWER, "the final text")] and captions == [""]
    session.stop()
    thread.join(timeout=3)


def test_a_caption_that_covers_the_speech_becomes_the_final_text(about, tmp_path):
    transcriber = Counting("first caption")
    session, captions, turns = make(tmp_path, transcriber)
    thread = run_loop(session)
    session._utterances.put((INTERVIEWER, pcm(4), True))
    assert wait_for(lambda: captions == ["first caption"])
    close = 4 + REUSE_SAMPLES / 16_000 - 0.05
    session._utterances.put((INTERVIEWER, pcm(close), False))
    assert wait_for(lambda: turns)
    assert transcriber.calls == 1 and turns[-1] == (INTERVIEWER, "first caption") and captions[-1] == ""

    transcriber.text = "second text"
    session._utterances.put((INTERVIEWER, pcm(4), True))
    assert wait_for(lambda: captions[-1] == "second text")
    session._utterances.put((INTERVIEWER, pcm(4 + REUSE_SAMPLES / 16_000 + 0.5), False))
    assert wait_for(lambda: len(turns) == 2)
    assert transcriber.calls == 3
    session.stop()
    thread.join(timeout=3)


def test_a_final_without_a_caption_or_from_another_speaker_is_transcribed(about, tmp_path):
    transcriber = Counting("fresh")
    session, captions, turns = make(tmp_path, transcriber)
    thread = run_loop(session)
    session._utterances.put((INTERVIEWER, pcm(2), False))
    assert wait_for(lambda: len(turns) == 1) and transcriber.calls == 1
    session._utterances.put((INTERVIEWER, pcm(3), True))
    assert wait_for(lambda: captions and captions[-1] == "fresh")
    session._utterances.put((YOU, pcm(3), False))
    assert wait_for(lambda: len(turns) == 2) and transcriber.calls == 3
    session.stop()
    thread.join(timeout=3)


class OneShot:
    def __init__(self, audio):
        self.audio = audio

    def frames(self):
        for start in range(0, len(self.audio) - 319, 320):
            yield self.audio[start : start + 320]

    def close(self):
        pass


def collected(speaker, tmp_path):
    session = InterviewSession(
        InterviewBrief.load(), None, NoCoach(), Callbacks(), log=TranscriptLog(tmp_path)
    )
    items = []
    session._utterances.put = items.append
    session._listen(speaker, OneShot(np.concatenate([silence(200), tone(4200), silence(1000)])))
    return items


def test_only_the_interviewer_gets_live_captions(about, tmp_path):
    heard = collected(INTERVIEWER, tmp_path)
    assert [kind for _s, _p, kind in heard].count(True) >= 2 and heard[-1][2] is False
    assert [len(p) for _s, p, kind in heard if kind] == sorted(len(p) for _s, p, kind in heard if kind)
    assert all(kind is False for _s, _p, kind in collected(YOU, tmp_path))


# ---- GPU --------------------------------------------------------------------------------


class FakeModel:
    created: list[dict] = []

    def __init__(self, name, device="cpu", compute_type="int8"):
        if device == "cuda" and FakeModel.fail_cuda:
            raise RuntimeError("Library libcublas.so.12 is not found or cannot be loaded")
        FakeModel.created.append({"name": name, "device": device, "compute_type": compute_type})
        self.device, self.runs = device, 0

    def transcribe(self, audio, **kwargs):
        self.runs += 1
        if self.device == "cuda" and FakeModel.fail_run:
            raise RuntimeError("cudnn failed")
        return [types.SimpleNamespace(text=" hi ")], types.SimpleNamespace(language="en", language_probability=1.0)

    fail_cuda = False
    fail_run = False


@pytest.fixture
def fake_whisper(monkeypatch):
    FakeModel.created, FakeModel.fail_cuda, FakeModel.fail_run = [], False, False
    stand_in = types.ModuleType("faster_whisper")
    stand_in.WhisperModel = FakeModel
    monkeypatch.setitem(sys.modules, "faster_whisper", stand_in)
    monkeypatch.setattr(stt, "preload_cuda_libraries", lambda: 3)
    return FakeModel


def test_the_gpu_is_used_with_half_precision_when_there_is_one(fake_whisper, monkeypatch):
    monkeypatch.setattr(stt, "cuda_device_count", lambda: 1)
    transcriber = WhisperTranscriber("small", language="en")
    transcriber.load()
    assert fake_whisper.created == [{"name": "small", "device": "cuda", "compute_type": "float16"}]
    assert transcriber.active_device == "cuda" and transcriber._model.runs == 1
    assert transcriber.transcribe(np.zeros(1600, dtype=np.int16)) == "hi"


def test_without_a_gpu_or_with_missing_libraries_it_runs_on_the_cpu(fake_whisper, monkeypatch):
    monkeypatch.setattr(stt, "cuda_device_count", lambda: 0)
    cpu = WhisperTranscriber("small")
    cpu.load()
    assert fake_whisper.created[-1] == {"name": "small", "device": "cpu", "compute_type": "int8"}

    monkeypatch.setattr(stt, "cuda_device_count", lambda: 1)
    fake_whisper.fail_cuda = True
    broken = WhisperTranscriber("small")
    broken.load()
    assert fake_whisper.created[-1]["device"] == "cpu" and broken.device == "cpu"
    assert broken.active_device == "cpu"

    forced_cpu = WhisperTranscriber("small", device="cpu")
    forced_cpu.load()
    assert forced_cpu.active_device == "cpu"


def test_a_gpu_that_fails_on_the_first_pass_falls_back_during_warm_up(fake_whisper, monkeypatch):
    monkeypatch.setattr(stt, "cuda_device_count", lambda: 1)
    fake_whisper.fail_run = True
    transcriber = WhisperTranscriber("small")
    transcriber.load()
    assert transcriber.active_device == "cpu" and fake_whisper.created[-1]["device"] == "cpu"
    assert transcriber.transcribe(np.zeros(1600, dtype=np.int16)) == "hi"


def test_preloading_cuda_libraries_finds_the_nvidia_wheels(tmp_path, monkeypatch):
    loaded = []
    monkeypatch.setattr(stt.ctypes, "CDLL", lambda path, mode=0: loaded.append(path))
    monkeypatch.setattr(stt.importlib.util, "find_spec", lambda name: None)
    assert stt.preload_cuda_libraries() == 0
    (tmp_path / "cublas" / "lib").mkdir(parents=True)
    (tmp_path / "cublas" / "lib" / "libcublas.so.12").write_text("")
    (tmp_path / "cublas" / "lib" / "libcublasLt.so.12").write_text("")

    def spec(name):
        base = tmp_path / name.split(".")[1]
        return types.SimpleNamespace(submodule_search_locations=[str(base)]) if base.exists() else None

    monkeypatch.setattr(stt.importlib.util, "find_spec", spec)
    assert stt.preload_cuda_libraries() == 2
    assert [p.rsplit("/", 1)[1] for p in loaded] == ["libcublasLt.so.12", "libcublas.so.12"]


def test_a_sentence_ending_with_a_question_mark_is_sent_without_the_full_debounce(about, tmp_path):
    asked = []

    class Recording:
        def stream(self, question, *args, **kwargs):
            asked.append((question, time.monotonic()))
            return iter(())

    session = InterviewSession(
        InterviewBrief.load(), None, Recording(), Callbacks(), debounce=0.6, log=TranscriptLog(tmp_path)
    )
    started = time.monotonic()
    session.handle_text(INTERVIEWER, "Could you tell me about yourself?")
    assert wait_for(lambda: asked, timeout=2)
    assert asked[0][1] - started < 0.45
    asked.clear()
    started = time.monotonic()
    session.handle_text(INTERVIEWER, "Tell me about your last project")
    assert wait_for(lambda: asked, timeout=2)
    assert asked[0][1] - started >= 0.55
    session.stop()
