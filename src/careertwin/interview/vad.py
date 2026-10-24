"""Energy-based utterance segmentation for 16 kHz mono PCM16 audio."""

from __future__ import annotations

from collections import deque
from collections.abc import Iterable, Iterator

import numpy as np

SAMPLE_RATE = 16_000
FRAME_MS = 20
FRAME_SAMPLES = SAMPLE_RATE * FRAME_MS // 1000
FRAME_BYTES = FRAME_SAMPLES * 2


class UtteranceSegmenter:
    """Groups frames into utterances: speech, then enough silence to call it finished."""

    def __init__(
        self,
        *,
        end_silence_ms: int = 500,
        min_speech_ms: int = 300,
        max_utterance_ms: int = 12_000,
        preroll_ms: int = 200,
        tail_ms: int = 200,
        min_threshold: float = 250.0,
        noise_multiplier: float = 3.0,
    ):
        self.end_frames = end_silence_ms // FRAME_MS
        self.min_frames = min_speech_ms // FRAME_MS
        self.max_frames = max_utterance_ms // FRAME_MS
        self.tail_frames = tail_ms // FRAME_MS
        self.preroll: deque[np.ndarray] = deque(maxlen=preroll_ms // FRAME_MS)
        self.min_threshold = min_threshold
        self.noise_multiplier = noise_multiplier
        self.noise = min_threshold / noise_multiplier
        self.frames: list[np.ndarray] = []
        self.speech_frames = 0
        self.silent_frames = 0
        self.active = False

    def threshold(self) -> float:
        return max(self.min_threshold, self.noise * self.noise_multiplier)

    def push(self, frame: np.ndarray) -> np.ndarray | None:
        """Feed one frame; return a finished utterance (int16) or None."""
        energy = float(np.sqrt(np.mean(frame.astype(np.float32) ** 2))) if frame.size else 0.0
        voiced = energy >= self.threshold()
        if not self.active:
            if not voiced:
                self.noise = 0.95 * self.noise + 0.05 * energy
                self.preroll.append(frame)
                return None
            self.active = True
            self.frames = [*self.preroll]
            self.preroll.clear()
            self.speech_frames = 0
            self.silent_frames = 0
        self.frames.append(frame)
        if voiced:
            self.speech_frames += 1
            self.silent_frames = 0
        else:
            self.silent_frames += 1
        if self.silent_frames >= self.end_frames or len(self.frames) >= self.max_frames:
            return self._finish()
        return None

    def snapshot(self) -> np.ndarray | None:
        """The speech heard so far in the current utterance, or None while idle or too short."""
        if not self.active or self.speech_frames < self.min_frames:
            return None
        return np.concatenate(self.frames)

    def flush(self) -> np.ndarray | None:
        return self._finish() if self.active else None

    def _finish(self) -> np.ndarray | None:
        frames, enough = self.frames, self.speech_frames >= self.min_frames
        surplus = self.silent_frames - self.tail_frames
        if surplus > 0:
            frames = frames[:-surplus]
        self.frames, self.active = [], False
        self.speech_frames = self.silent_frames = 0
        return np.concatenate(frames) if enough and frames else None


def utterances(frames: Iterable[np.ndarray], **options: float) -> Iterator[np.ndarray]:
    segmenter = UtteranceSegmenter(**options)
    for frame in frames:
        if (utterance := segmenter.push(frame)) is not None:
            yield utterance
    if (tail := segmenter.flush()) is not None:
        yield tail
