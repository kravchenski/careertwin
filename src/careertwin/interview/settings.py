"""Settings and saved context of the interview window, kept in `data/interview/`."""

from __future__ import annotations

import json
import os
from dataclasses import asdict, dataclass, fields
from pathlib import Path
from typing import Any

from careertwin.settings import DATA_DIR

WHISPER_MODELS = ("tiny", "base", "small", "medium", "large-v3-turbo")
ANSWER_STYLES = ("concise", "standard", "detailed")
ANSWER_LAYOUTS = ("main", "lines")
DEVICES = ("auto", "cpu", "cuda")
STT_ENGINES = ("whisper", "deepgram")


def interview_dir() -> Path:
    return DATA_DIR / "interview"


@dataclass
class InterviewSettings:
    """Everything adjustable in the window. Empty strings mean "use the default"."""

    language: str = "auto"
    ui_language: str = "auto"
    listen: bool = True
    call_source: str = ""
    use_mic: bool = False
    mic_source: str = ""
    stt_engine: str = "whisper"
    whisper_model: str = "small"
    whisper_device: str = "auto"
    deepgram_key: str = ""
    deepgram_model: str = "nova-3"
    gateway_url: str = ""
    gateway_key: str = ""
    model: str = ""
    answer_style: str = "standard"
    answer_layout: str = "lines"
    show_full_answer: bool = True
    screenshot_enabled: bool = True
    screenshot_tool: str = "auto"
    screenshot_delay_ms: int = 250
    pinned: bool = True
    frameless: bool = True
    opacity: int = 100
    font_scale: int = 100
    save_transcripts: bool = True

    @classmethod
    def path(cls) -> Path:
        return interview_dir() / "settings.json"

    @classmethod
    def load(cls) -> InterviewSettings:
        """Read the saved settings; unknown keys and bad values fall back to defaults."""
        settings = cls()
        try:
            data = json.loads(cls.path().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return settings
        if isinstance(data, dict):
            for field in fields(cls):
                if field.name in data:
                    settings.set(field.name, data[field.name], save=False, strict=False)
        return settings

    def set(self, name: str, value: Any, *, save: bool = True, strict: bool = True) -> None:
        """Validate one value, store it and (by default) save the file."""
        by_name = {field.name: field for field in fields(self)}
        if name not in by_name:
            if strict:
                raise KeyError(f"Unknown setting {name!r}")
            return
        current = getattr(self, name)
        try:
            value = _coerce(name, value, current)
        except ValueError:
            if strict:
                raise
            return
        setattr(self, name, value)
        if save:
            self.save()

    def save(self) -> None:
        path = self.path()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(asdict(self), indent=2, ensure_ascii=False), encoding="utf-8")
        os.chmod(path, 0o600)


def _coerce(name: str, value: Any, current: Any) -> Any:
    if isinstance(current, bool):
        if not isinstance(value, bool):
            raise ValueError(f"{name} must be true or false")
        return value
    if isinstance(current, int):
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise ValueError(f"{name} must be a number")
        value = int(value)
        limits = {"opacity": (35, 100), "font_scale": (70, 180), "screenshot_delay_ms": (0, 5000)}
        low, high = limits.get(name, (0, 10**9))
        if not low <= value <= high:
            raise ValueError(f"{name} must be between {low} and {high}")
        return value
    if not isinstance(value, str):
        raise ValueError(f"{name} must be text")
    value = value.strip()
    choices = {
        "whisper_model": WHISPER_MODELS,
        "answer_style": ANSWER_STYLES,
        "answer_layout": ANSWER_LAYOUTS,
        "whisper_device": DEVICES,
        "stt_engine": STT_ENGINES,
    }.get(name)
    if choices and value not in choices:
        raise ValueError(f"{name} must be one of {', '.join(choices)}")
    if name == "gateway_url" and value and not value.startswith(("http://", "https://")):
        raise ValueError("The gateway URL must start with http:// or https://")
    return value
