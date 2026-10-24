"""Audio capture: PulseAudio/PipeWire (`parec`) on Linux, WASAPI loopback on Windows."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import threading
import time
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import TYPE_CHECKING

import numpy as np

from careertwin.interview.vad import FRAME_BYTES, SAMPLE_RATE

if TYPE_CHECKING:
    from careertwin.interview.audio_windows import WasapiSource


class AudioError(RuntimeError):
    """Audio capture is unavailable or misconfigured."""


@dataclass(frozen=True)
class Sources:
    interviewer: str
    microphone: str


def _pactl(*args: str) -> str:
    if sys.platform == "darwin":
        raise AudioError(
            "Call audio is not available on macOS yet. Type a question, or use the Screenshot button."
        )
    if not shutil.which("pactl"):
        raise AudioError("`pactl` not found. Install pipewire-pulse (or pulseaudio-utils).")
    result = subprocess.run(["pactl", *args], capture_output=True, text=True, timeout=10)
    if result.returncode != 0:
        raise AudioError(f"pactl {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def _windows() -> bool:
    return sys.platform == "win32"


def list_sources() -> list[str]:
    if _windows():
        from careertwin.interview import audio_windows

        return audio_windows.list_sources()
    return [line.split("\t")[1] for line in _pactl("list", "short", "sources").splitlines() if "\t" in line]


def default_sources() -> Sources:
    """The sink monitor carries the call audio; the default source is the microphone."""
    if _windows():
        from careertwin.interview import audio_windows

        return audio_windows.default_sources()
    return Sources(
        interviewer=_pactl("get-default-sink") + ".monitor",
        microphone=_pactl("get-default-source"),
    )


class ParecSource:
    """Reads 20 ms frames of mono 16 kHz PCM16 from a Pulse source."""

    def __init__(self, source: str):
        self.source = source
        self.process: subprocess.Popen[bytes] | None = None

    def frames(self) -> Iterator[np.ndarray]:
        if not shutil.which("parec"):
            raise AudioError("`parec` not found. Install pipewire-pulse (or pulseaudio-utils).")
        self.process = subprocess.Popen(
            [
                "parec", f"--device={self.source}", f"--rate={SAMPLE_RATE}", "--channels=1",
                "--format=s16le", "--latency-msec=20",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        assert self.process.stdout is not None
        while True:
            data = self.process.stdout.read(FRAME_BYTES)
            if len(data) < FRAME_BYTES:
                return
            yield np.frombuffer(data, dtype=np.int16)

    def close(self) -> None:
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()


def open_source(name: str, *, microphone: bool = False) -> ParecSource | WasapiSource:
    """A source for a device name: a Pulse source on Linux, an output or microphone on Windows."""
    if _windows():
        from careertwin.interview.audio_windows import WasapiSource

        return WasapiSource(name, loopback=not microphone)
    return ParecSource(name)


def listen_briefly(name: str = "", seconds: float = 0.6) -> int:
    """Open the call audio for a moment and return the loudest sample heard: proof that capture works."""
    source = open_source(name)
    peak = 0
    deadline = time.monotonic() + seconds
    try:
        for frame in source.frames():
            peak = max(peak, int(np.abs(frame.astype(np.int32)).max()))
            if time.monotonic() >= deadline:
                break
    finally:
        source.close()
    return peak


def audio_state() -> tuple[str, list[str]]:
    """(monitor of the default output, monitors of the outputs that are playing something now)."""
    if _windows():
        from careertwin.interview import audio_windows

        return audio_windows.audio_state()
    default = _pactl("get-default-sink") + ".monitor"
    try:
        sinks = json.loads(_pactl("--format=json", "list", "sinks"))
        inputs = json.loads(_pactl("--format=json", "list", "sink-inputs"))
    except (AudioError, ValueError):
        return default, []
    by_index = {sink["index"]: sink for sink in sinks}
    playing: list[str] = []
    for stream in inputs:
        sink = by_index.get(stream.get("sink"))
        if sink and not stream.get("corked"):
            monitor = sink.get("monitor_source") or sink["name"] + ".monitor"
            if monitor not in playing:
                playing.append(monitor)
    return default, playing


def next_monitor(current: str | None, default: str, playing: list[str], last_default: str | None) -> str:
    """Which monitor to listen to: stay on one that is playing, else follow what plays, else the default."""
    if current in playing:
        return current
    if playing:
        return default if default in playing else playing[0]
    if current is None or default != last_default:
        return default
    return current


class FollowingSource:
    """Call audio that follows the output actually playing the call.

    A laptop with a headset has two outputs. The call plays on whichever Zoom or Meet picked, and
    the system default can change when a headset is plugged in, so a monitor fixed at start can be
    silent. This source re-checks every `poll` seconds and moves to the right monitor; it moves only
    after two checks agree, so a notification sound on another output does not make it jump."""

    def __init__(
        self,
        *,
        poll: float = 2.0,
        on_change: Callable[[str], None] | None = None,
        state: Callable[[], tuple[str, list[str]]] | None = None,
        spawn: Callable[[str], ParecSource | WasapiSource] | None = None,
    ):
        self.poll, self.on_change = poll, on_change
        self._state = state or audio_state
        self._spawn = spawn or open_source
        self.current: str | None = None
        self._target: str | None = None
        self._pending: str | None = None
        self._last_default: str | None = None
        self._stop = threading.Event()
        self._switching = threading.Event()
        self._source: ParecSource | WasapiSource | None = None

    def frames(self) -> Iterator[np.ndarray]:
        self._stop.clear()
        default, playing = self._state()
        self._last_default = default
        self.current = next_monitor(None, default, playing, None)
        self._announce()
        threading.Thread(target=self._watch, name="follow-audio", daemon=True).start()
        while not self._stop.is_set():
            source = self._source = self._spawn(self.current)
            try:
                for frame in source.frames():
                    if self._stop.is_set() or self._switching.is_set():
                        break
                    yield frame
            finally:
                source.close()
            if self._stop.is_set():
                return
            if self._switching.is_set():
                self._switching.clear()
                self.current = self._target
                self._announce()
            else:
                time.sleep(0.5)

    def close(self) -> None:
        self._stop.set()
        if self._source is not None:
            self._source.close()

    def _announce(self) -> None:
        if self.on_change and self.current:
            self.on_change(self.current)

    def _watch(self) -> None:
        while not self._stop.wait(self.poll):
            try:
                default, playing = self._state()
            except AudioError:
                continue
            wanted = next_monitor(self.current, default, playing, self._last_default)
            if wanted == self.current:
                self._pending, self._last_default = None, default
            elif wanted == self._pending:
                self._pending, self._target, self._last_default = None, wanted, default
                self._switching.set()
                if self._source is not None:
                    self._source.close()
            else:
                self._pending = wanted
