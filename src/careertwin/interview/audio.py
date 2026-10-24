"""Audio capture through PulseAudio/PipeWire (`parec`), no Python audio bindings needed."""

from __future__ import annotations

import shutil
import subprocess
from collections.abc import Iterator
from dataclasses import dataclass

import numpy as np

from careertwin.interview.vad import FRAME_BYTES, SAMPLE_RATE


class AudioError(RuntimeError):
    """Audio capture is unavailable or misconfigured."""


@dataclass(frozen=True)
class Sources:
    interviewer: str
    microphone: str


def _pactl(*args: str) -> str:
    if not shutil.which("pactl"):
        raise AudioError("`pactl` not found. Install pipewire-pulse (or pulseaudio-utils).")
    result = subprocess.run(["pactl", *args], capture_output=True, text=True, timeout=10)
    if result.returncode != 0:
        raise AudioError(f"pactl {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def list_sources() -> list[str]:
    return [line.split("\t")[1] for line in _pactl("list", "short", "sources").splitlines() if "\t" in line]


def default_sources() -> Sources:
    """The sink monitor carries the call audio; the default source is the microphone."""
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
