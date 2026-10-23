"""Call audio and microphone on Windows through WASAPI, with a fake `soundcard`."""

from __future__ import annotations

import numpy as np
import pytest

from careertwin.interview import audio, audio_windows
from careertwin.interview.audio import AudioError, FollowingSource, ParecSource
from careertwin.interview.audio_windows import WasapiSource, find_device, to_pcm16
from careertwin.interview.vad import FRAME_SAMPLES, SAMPLE_RATE


class FakeRecorder:
    def __init__(self, level: float = 0.5, fail_after: int | None = None):
        self.level, self.fail_after, self.blocks = level, fail_after, 0
        self.entered = self.exited = False

    def __enter__(self):
        self.entered = True
        return self

    def __exit__(self, *exc):
        self.exited = True

    def record(self, numframes):
        if self.fail_after is not None and self.blocks >= self.fail_after:
            raise RuntimeError("Error 0x88890004")  # AUDCLNT_E_DEVICE_INVALIDATED
        self.blocks += 1
        return np.full((numframes, 1), self.level, dtype=np.float32)


class FakeDevice:
    def __init__(self, name: str, id: str, isloopback: bool = False, recorder: FakeRecorder | None = None):
        self.name, self.id, self.isloopback = name, id, isloopback
        self.fake_recorder = recorder or FakeRecorder()
        self.opened_with: dict | None = None

    def recorder(self, samplerate, channels=None, blocksize=None):
        self.opened_with = {"samplerate": samplerate, "channels": channels}
        return self.fake_recorder


class FakeSoundcard:
    def __init__(self):
        self.speakers = [FakeDevice("Speakers (Realtek)", "spk-1"), FakeDevice("Headset (Jabra)", "spk-2")]
        self.loopbacks = [FakeDevice(s.name, s.id, isloopback=True) for s in self.speakers]
        self.mics = [FakeDevice("Microphone (Realtek)", "mic-1"), FakeDevice("Headset (Jabra)", "mic-2")]
        self.default = self.speakers[1]

    def default_speaker(self):
        return self.default

    def default_microphone(self):
        return self.mics[0]

    def all_speakers(self):
        return self.speakers

    def all_microphones(self, include_loopback=False):
        return (self.loopbacks if include_loopback else []) + self.mics

    def get_microphone(self, id, include_loopback=False):
        return next(d for d in self.all_microphones(include_loopback) if d.id == id)


@pytest.fixture
def sc(monkeypatch):
    fake = FakeSoundcard()
    monkeypatch.setattr(audio_windows, "_soundcard", lambda: fake)
    return fake


def test_samples_become_clipped_16_bit_pcm():
    block = np.array([[0.0], [0.5], [-1.0], [2.0], [-2.0]], dtype=np.float32)
    pcm = to_pcm16(block)
    assert pcm.dtype == np.int16 and pcm.shape == (5,)
    assert pcm.tolist() == [0, 16383, -32767, 32767, -32768]


def test_the_default_output_is_heard_through_its_loopback(sc):
    device = find_device(sc, "", loopback=True)
    assert device.isloopback and device.id == "spk-2"
    assert find_device(sc, "", loopback=False).id == "mic-1"


def test_a_device_is_found_by_name_or_part_of_it_and_loopback_is_kept_apart_from_microphones(sc):
    assert find_device(sc, "Speakers (Realtek)", loopback=True).id == "spk-1"
    assert find_device(sc, "realtek", loopback=True).id == "spk-1"
    assert find_device(sc, "Headset (Jabra)", loopback=True).isloopback
    assert not find_device(sc, "Headset (Jabra)", loopback=False).isloopback
    with pytest.raises(AudioError, match="No output called"):
        find_device(sc, "Nothing", loopback=True)
    with pytest.raises(AudioError, match="No microphone called"):
        find_device(sc, "Speakers (Realtek)", loopback=False)


def test_frames_are_20_ms_of_mono_16_khz_pcm(sc):
    source = WasapiSource("")
    frames = source.frames()
    first, second = next(frames), next(frames)
    device = sc.loopbacks[1]
    assert device.opened_with == {"samplerate": SAMPLE_RATE, "channels": 1}
    assert first.dtype == np.int16 and len(first) == FRAME_SAMPLES == 320
    assert int(first[0]) == 16383 and len(second) == 320
    assert device.fake_recorder.entered and not device.fake_recorder.exited
    source.close()
    assert list(frames) == []
    assert device.fake_recorder.exited


def test_a_microphone_source_opens_a_microphone(sc):
    next(WasapiSource("Microphone (Realtek)", loopback=False).frames())
    assert sc.mics[0].opened_with == {"samplerate": SAMPLE_RATE, "channels": 1}
    assert sc.loopbacks[0].opened_with is None


def test_an_unplugged_device_ends_the_stream_so_the_follower_can_reopen(sc):
    sc.loopbacks[1].fake_recorder = FakeRecorder(fail_after=2)
    frames = list(WasapiSource("").frames())
    assert len(frames) == 2 and sc.loopbacks[1].fake_recorder.exited


def test_a_device_that_cannot_be_opened_is_an_audio_error(sc):
    def refuse(*args, **kwargs):
        raise RuntimeError("unsupported format")

    sc.loopbacks[1].recorder = refuse
    with pytest.raises(AudioError, match="Could not open the default device: unsupported format"):
        next(WasapiSource("").frames())


def test_missing_soundcard_package_is_explained(monkeypatch):
    monkeypatch.setitem(__import__("sys").modules, "soundcard", None)
    with pytest.raises(AudioError, match="soundcard"):
        audio_windows._soundcard()


def test_listing_and_defaults_come_from_the_windows_devices(monkeypatch, sc):
    monkeypatch.setattr("sys.platform", "win32")
    assert audio.list_sources() == ["Speakers (Realtek)", "Headset (Jabra)", "Microphone (Realtek)"]
    assert audio.default_sources() == audio.Sources(interviewer="Headset (Jabra)", microphone="Microphone (Realtek)")
    assert audio.audio_state() == ("Headset (Jabra)", [])


def test_a_name_opens_the_kind_of_source_the_platform_has(monkeypatch):
    monkeypatch.setattr("sys.platform", "win32")
    call, mic = audio.open_source("Headset"), audio.open_source("Headset", microphone=True)
    assert isinstance(call, WasapiSource) and call.loopback and not mic.loopback
    monkeypatch.setattr("sys.platform", "linux")
    assert isinstance(audio.open_source("x.monitor"), ParecSource)


def test_the_automatic_source_reopens_through_the_platform_default(monkeypatch):
    monkeypatch.setattr("sys.platform", "win32")
    assert isinstance(FollowingSource()._spawn("Headset (Jabra)"), WasapiSource)


def test_listening_briefly_reports_the_loudest_sample(monkeypatch):
    class Fake:
        def frames(self):
            yield np.array([1, -900, 20], dtype=np.int16)
            yield np.array([5, 7, 3], dtype=np.int16)

        def close(self):
            self.closed = True

    fake = Fake()
    monkeypatch.setattr(audio, "open_source", lambda name: fake)
    assert audio.listen_briefly("x", seconds=10) == 900 and fake.closed
