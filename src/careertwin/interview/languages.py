"""Interview language: which language answers, hints and the window speak."""

from __future__ import annotations

from collections import Counter, deque

NAMES = {
    "en": "English", "ru": "Russian", "pl": "Polish", "uk": "Ukrainian", "be": "Belarusian",
    "de": "German", "fr": "French", "es": "Spanish", "it": "Italian", "pt": "Portuguese",
    "nl": "Dutch", "cs": "Czech", "sk": "Slovak", "sv": "Swedish", "da": "Danish",
    "no": "Norwegian", "fi": "Finnish", "tr": "Turkish", "ro": "Romanian", "hu": "Hungarian",
    "bg": "Bulgarian", "lt": "Lithuanian", "lv": "Latvian", "et": "Estonian", "el": "Greek",
    "he": "Hebrew", "ar": "Arabic", "zh": "Chinese", "ja": "Japanese", "ko": "Korean",
    "hi": "Hindi", "vi": "Vietnamese", "id": "Indonesian",
}
NATIVE = {
    "en": "English", "ru": "Русский", "pl": "Polski", "uk": "Українська", "de": "Deutsch",
    "fr": "Français", "es": "Español", "it": "Italiano", "pt": "Português", "cs": "Čeština",
}
UI_LANGUAGES = ("en", "ru", "pl")


def name(code: str | None) -> str:
    """English name for a language code, used inside prompts."""
    return NAMES.get((code or "").lower(), (code or "English").title() if code else "English")


def normalize(value: str | None) -> str | None:
    """`ru-RU`, `RU`, `Russian` -> `ru`; `auto` or empty -> None."""
    text = (value or "").strip().lower()
    if text in {"", "auto"}:
        return None
    if text in NAMES:
        return text
    base = text.replace("_", "-").split("-")[0]
    if base in NAMES:
        return base
    for code, label in NAMES.items():
        if label.lower() == text:
            return code
    return base if base.isalpha() and len(base) in (2, 3) else None


class LanguageTracker:
    """Settles on the interview language and does not flip on one odd utterance.

    A fixed language always wins. In auto mode the language is the majority of the
    last few confident detections of the interviewer's speech.
    """

    def __init__(self, fixed: str | None = None, window: int = 5, min_probability: float = 0.6):
        self.fixed = normalize(fixed)
        self.min_probability = min_probability
        self.recent: deque[str] = deque(maxlen=window)
        self.current: str | None = self.fixed

    def set_fixed(self, value: str | None) -> str | None:
        self.fixed = normalize(value)
        self.recent.clear()
        self.current = self.fixed
        return self.current

    def observe(self, code: str | None, probability: float = 1.0) -> str | None:
        """Feed one detection; return the language if it changed, else None."""
        if self.fixed or not code or probability < self.min_probability:
            return None
        self.recent.append(code)
        winner = Counter(self.recent).most_common(1)[0][0]
        if winner != self.current:
            self.current = winner
            return winner
        return None
