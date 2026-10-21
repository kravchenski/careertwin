"""Placeholder-aware profile name resolution."""

from __future__ import annotations

from typing import Any

_PLACEHOLDER_NAMES = frozenset({
    "unknown", "linkedin user", "candidate", "applicant",
    "your name", "your twin", "professional",
})


def is_real_profile_name(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    normalized = " ".join(value.split()).lower()
    return len(normalized) >= 2 and normalized not in _PLACEHOLDER_NAMES


def resolve_profile_name(
    profile_name: Any,
    first_name: Any = "",
    last_name: Any = "",
) -> str:
    if is_real_profile_name(profile_name):
        return " ".join(str(profile_name).split())
    composed = " ".join(
        str(part).strip()
        for part in (first_name, last_name)
        if isinstance(part, str) and part.strip()
    )
    return composed if is_real_profile_name(composed) else ""
