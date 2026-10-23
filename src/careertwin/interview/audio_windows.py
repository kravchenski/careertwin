"""Call audio and microphone on Windows through WASAPI, using the `soundcard` package.

The call is heard by recording an output device in loopback mode: Windows hands over exactly what
that device plays, so no virtual cable is needed. Windows converts the stream to 16 kHz mono for
us. A loopback stream sends nothing while the device is silent; `soundcard` fills those gaps with
zeros, which the speech detector needs to see to know that a sentence has ended.
"""

from __future__ import annotations

import contextlib
import ctypes
import sys
import threading
from collections.abc import Iterator
from types import ModuleType

import numpy as np

from careertwin.interview.audio import AudioError, Sources
from careertwin.interview.vad import FRAME_SAMPLES, SAMPLE_RATE

COINIT_MULTITHREADED = 0
S_OK, S_FALSE = 0, 1


def _soundcard() -> ModuleType:
    try:
        import soundcard
    except ImportError as exc:
        raise AudioError("Listening on Windows needs the `soundcard` package: uv sync --extra interview") from exc
    return soundcard


@contextlib.contextmanager
def com_apartment() -> Iterator[None]:
    """COM must be initialised on every thread that talks to WASAPI.

    The window's thread already is (single-threaded apartment) and answers "changed mode", which
    is fine; a worker thread joins the multithreaded apartment and leaves it again afterwards."""
    if sys.platform != "win32" or not hasattr(ctypes, "windll"):
        yield
        return
    joined = ctypes.windll.ole32.CoInitializeEx(None, COINIT_MULTITHREADED) in (S_OK, S_FALSE)
    try:
        yield
    finally:
        if joined:
            ctypes.windll.ole32.CoUninitialize()


def to_pcm16(block: np.ndarray) -> np.ndarray:
    """Float samples in [-1, 1], one channel (any shape) to 16-bit PCM."""
    return np.clip(np.asarray(block, dtype=np.float32).reshape(-1) * 32767.0, -32768, 32767).astype(np.int16)


def find_device(sc: ModuleType, name: str, *, loopback: bool):
    """The output (loopback) or the microphone called `name`; an empty name means the default one."""
    if not name:
        if not loopback:
            return sc.default_microphone()
        return sc.get_microphone(sc.default_speaker().id, include_loopback=True)
    devices = [device for device in sc.all_microphones(include_loopback=True) if bool(device.isloopback) == loopback]
    for device in devices:
        if name in (device.id, device.name):
            return device
    wanted = name.casefold()
    for device in devices:
        if wanted in device.name.casefold():
            return device
    raise AudioError(f"No {'output' if loopback else 'microphone'} called {name!r}. Pick one in Settings → Audio.")


def list_sources() -> list[str]:
    """Names of the outputs (heard in loopback) and then the microphones."""
    sc = _soundcard()
    with com_apartment():
        names = [speaker.name for speaker in sc.all_speakers()]
        names += [mic.name for mic in sc.all_microphones() if mic.name not in names]
    return names


def default_sources() -> Sources:
    sc = _soundcard()
    with com_apartment():
        try:
            return Sources(interviewer=sc.default_speaker().name, microphone=sc.default_microphone().name)
        except Exception as exc:  # no output or no microphone
            raise AudioError(f"Windows audio devices are not available: {exc}") from exc


def audio_state() -> tuple[str, list[str]]:
    """(the default output, outputs known to be playing). Windows does not say which one plays."""
    sc = _soundcard()
    with com_apartment():
        try:
            return sc.default_speaker().name, []
        except Exception as exc:
            raise AudioError(f"No audio output: {exc}") from exc


class WasapiSource:
    """Reads 20 ms frames of mono 16 kHz PCM16 from an output (loopback) or a microphone."""

    def __init__(self, name: str = "", *, loopback: bool = True):
        self.name, self.loopback = name, loopback
        self._stop = threading.Event()

    def frames(self) -> Iterator[np.ndarray]:
        sc = _soundcard()
        self._stop.clear()
        with com_apartment():
            try:
                device = find_device(sc, self.name, loopback=self.loopback)
                recorder = device.recorder(samplerate=SAMPLE_RATE, channels=1)
                recorder.__enter__()
            except AudioError:
                raise
            except Exception as exc:
                raise AudioError(f"Could not open {self.name or 'the default device'}: {exc}") from exc
            try:
                while not self._stop.is_set():
                    yield to_pcm16(recorder.record(numframes=FRAME_SAMPLES))
            except RuntimeError:
                return  # the device went away (unplugged, default changed): the follower reopens
            finally:
                with contextlib.suppress(RuntimeError):
                    recorder.__exit__(None, None, None)

    def close(self) -> None:
        self._stop.set()
