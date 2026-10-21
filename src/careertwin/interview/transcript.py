"""Plain-text record of an interview, kept on this machine only."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

from careertwin.settings import DATA_DIR


@dataclass
class Turn:
    speaker: str
    text: str
    at: datetime = field(default_factory=datetime.now)

    def line(self) -> str:
        return f"{self.speaker}: {self.text}"


class TranscriptLog:
    def __init__(self, directory: Path | None = None):
        self.turns: list[Turn] = []
        self.suggestions: list[tuple[str, str]] = []
        self.directory = directory or DATA_DIR / "interviews"
        self.started = datetime.now()

    def add(self, speaker: str, text: str) -> Turn:
        turn = Turn(speaker, text)
        self.turns.append(turn)
        return turn

    def history(self, limit: int = 6) -> list[str]:
        return [turn.line() for turn in self.turns[-limit:]]

    def save(self) -> Path | None:
        if not self.turns:
            return None
        self.directory.mkdir(parents=True, exist_ok=True)
        path = self.directory / f"{self.started:%Y-%m-%d_%H%M}.md"
        lines = [f"# Interview {self.started:%Y-%m-%d %H:%M}", ""]
        lines += [f"**{turn.at:%H:%M:%S} {turn.speaker}:** {turn.text}" for turn in self.turns]
        if self.suggestions:
            lines += ["", "## Suggestions shown", ""]
            lines += [f"- *{question}*\n\n  {answer}" for question, answer in self.suggestions]
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        return path
