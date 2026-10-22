"""The call audio follows the output that plays the call; quiet voices still count."""

from __future__ import annotations

import json
import threading
import time

import numpy as np

from careertwin.interview import audio
from careertwin.interview.audio import FollowingSource, audio_state, next_monitor
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.session import INTERVIEWER, YOU, Callbacks, InterviewSession
from careertwin.interview.transcript import TranscriptLog

SPEAKER, JABRA = "speaker.monitor", "jabra.monitor"


def test_next_monitor_prefers_what_is_playing_and_sticks_to_it():
    assert next_monitor(None, JABRA, [], None) == JABRA
    assert next_monitor(SPEAKER, JABRA, [JABRA], JABRA) == JABRA
    assert next_monitor(JABRA, JABRA, [JABRA, SPEAKER], JABRA) == JABRA
    assert next_monitor(SPEAKER, JABRA, [JABRA, SPEAKER], JABRA) == SPEAKER
    assert next_monitor(None, JABRA, [SPEAKER, JABRA], None) == JABRA
    assert next_monitor(None, JABRA, [SPEAKER], None) == SPEAKER
    assert next_monitor(SPEAKER, SPEAKER, [], SPEAKER) == SPEAKER
    assert next_monitor(SPEAKER, JABRA, [], SPEAKER) == JABRA


def test_audio_state_reads_default_sink_and_playing_streams(monkeypatch):
    sinks = [
        {"index": 1, "name": "speaker", "monitor_source": SPEAKER},
        {"index": 2, "name": "jabra", "monitor_source": JABRA},
    ]
    streams = [
        {"sink": 2, "corked": False}, {"sink": 1, "corked": True}, {"sink": 2, "corked": False},
        {"sink": 99, "corked": False},
    ]

    def fake(*args):
        if args == ("get-default-sink",):
            return "jabra"
        return json.dumps(sinks if args[-1] == "sinks" else streams)

    monkeypatch.setattr(audio, "_pactl", fake)
    assert audio_state() == (JABRA, [JABRA])

    def broken(*args):
        if args == ("get-default-sink",):
            return "speaker"
        raise audio.AudioError("no json")

    monkeypatch.setattr(audio, "_pactl", broken)
    assert audio_state() == (SPEAKER, [])


class FakeParec:
    def __init__(self, name, log):
        self.source, self.log, self.closed = name, log, threading.Event()
        log.append(("open", name))

    def frames(self):
        frame = np.zeros(320, dtype=np.int16)
        while not self.closed.is_set():
            time.sleep(0.005)
            yield frame

    def close(self):
        self.closed.set()
        self.log.append(("close", self.source))


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def run(source, results):
    for frame in source.frames():
        results.append(frame)


def test_the_source_moves_to_the_output_that_starts_playing():
    world = {"default": SPEAKER, "playing": []}
    log, names = [], []
    source = FollowingSource(
        poll=0.03,
        on_change=names.append,
        state=lambda: (world["default"], list(world["playing"])),
        spawn=lambda name: FakeParec(name, log),
    )
    frames: list = []
    thread = threading.Thread(target=run, args=(source, frames), daemon=True)
    thread.start()
    assert wait_for(lambda: names == [SPEAKER] and frames)
    world["playing"] = [JABRA]
    assert wait_for(lambda: names == [SPEAKER, JABRA])
    assert ("close", SPEAKER) in log and ("open", JABRA) in log
    before = len(frames)
    assert wait_for(lambda: len(frames) > before + 3)
    source.close()
    thread.join(timeout=3)
    assert not thread.is_alive()


def test_a_short_sound_elsewhere_does_not_make_it_jump():
    world = {"default": JABRA, "playing": [JABRA]}
    log, names = [], []
    source = FollowingSource(
        poll=0.03, on_change=names.append,
        state=lambda: (world["default"], list(world["playing"])), spawn=lambda name: FakeParec(name, log),
    )
    thread = threading.Thread(target=run, args=(source, []), daemon=True)
    thread.start()
    assert wait_for(lambda: names == [JABRA])
    world["playing"] = [JABRA, SPEAKER]
    time.sleep(0.2)
    world["playing"] = [JABRA]
    time.sleep(0.2)
    assert names == [JABRA] and ("open", SPEAKER) not in log
    source.close()
    thread.join(timeout=3)


def test_a_new_default_output_is_followed_when_nothing_plays():
    world = {"default": SPEAKER, "playing": []}
    names = []
    source = FollowingSource(
        poll=0.03, on_change=names.append,
        state=lambda: (world["default"], list(world["playing"])), spawn=lambda name: FakeParec(name, []),
    )
    thread = threading.Thread(target=run, args=(source, []), daemon=True)
    thread.start()
    assert wait_for(lambda: names == [SPEAKER])
    world["default"] = JABRA
    assert wait_for(lambda: names == [SPEAKER, JABRA])
    source.close()
    thread.join(timeout=3)


def quiet_voice(amplitude):
    t = np.arange(16000) / 16000
    tone = (amplitude * np.sin(2 * np.pi * 220 * t)).astype(np.int16)
    silence = np.zeros(16000, dtype=np.int16)
    return np.concatenate([silence, tone, silence])


class OneShot:
    def __init__(self, audio):
        self.audio = audio

    def frames(self):
        for start in range(0, len(self.audio) - 319, 320):
            yield self.audio[start : start + 320]

    def close(self):
        pass


class Heard:
    last_language, last_probability, language = None, 0.0, None

    def __init__(self):
        self.count = 0

    def transcribe(self, pcm, prompt=""):
        self.count += 1
        return "Tell me about yourself please"


class Coach:
    def stream(self, *args, **kwargs):
        return iter(())


def heard_count(speaker, amplitude, tmp_path, monkeypatch):
    about = tmp_path / "ABOUT.md"
    about.write_text("# C\n\n## Experience\n- Built FastAPI services.\n")
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(about))
    transcriber = Heard()
    session = InterviewSession(
        InterviewBrief.load(), transcriber, Coach(), Callbacks(), debounce=0.01, log=TranscriptLog(tmp_path)
    )
    session.start({speaker: OneShot(quiet_voice(amplitude))})
    wait_for(lambda: transcriber.count, timeout=1.5)
    session.stop()
    return transcriber.count


def test_a_quiet_call_is_heard_but_microphone_noise_is_not(tmp_path, monkeypatch):
    # RMS of a sine is amplitude / 1.41: a quiet voice at about 140 RMS.
    assert heard_count(INTERVIEWER, 200, tmp_path, monkeypatch) == 1
    assert heard_count(YOU, 200, tmp_path, monkeypatch) == 0
    assert heard_count(YOU, 1500, tmp_path, monkeypatch) == 1
