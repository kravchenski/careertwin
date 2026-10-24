"""The words of the window, one JSON file per language in `locales/`.

A language is a file named by its code (`uk.json`) with two sections, "overlay" (the interview
window) and "dialog" (Settings); every key of `en.json` should be there. A key that is missing
falls back to English, so a half-finished translation never shows a blank label. See
docs/TRANSLATING.md to add one.
"""

from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path
from typing import Any

LOCALES = Path(__file__).parent / "locales"
FALLBACK = "en"
# The order languages are offered in; anything else follows alphabetically.
ORDER = ("en", "ru", "pl", "uk", "de", "fr", "es")


@lru_cache(maxsize=None)
def catalog(code: str) -> dict[str, dict[str, str]]:
    """The strings of one language ({} when there is no such file or it is damaged)."""
    try:
        data = json.loads((LOCALES / f"{code}.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def available() -> tuple[str, ...]:
    """Codes of the languages the window speaks, English first."""
    found = {path.stem for path in LOCALES.glob("*.json")}
    known = [code for code in ORDER if code in found]
    return tuple(known + sorted(found - set(known)))


def has(code: str | None) -> bool:
    return bool(code) and code in available()


def text(code: str | None, section: str, key: str, **values: Any) -> str:
    """The string for `key` in `code`, in English when that language lacks it, with `values` filled in."""
    entry = catalog(code or FALLBACK).get(section, {}).get(key) or catalog(FALLBACK)[section][key]
    return entry.format(**values) if values else entry
