"""Seniority detection — hard-excludes mid/senior+ postings for a student-focused search.

Signals come from the full job text (title + description), not just the title,
since many listings only reveal the real experience/degree bar in the body.
"""

from __future__ import annotations

import re

_ROLE_WORD = r"(?:engineer|developer|designer|architect|scientist|manager|analyst|product|consultant|specialist)"

_SENIOR_TITLE_PATTERNS = [
    r"(?<!rising )\bsenior\b(?!\s+(year|student|undergraduate|undergrad|standing|preferred|class))",
    rf"\bsr\.?\s+(?:\w+\s+){{0,3}}{_ROLE_WORD}\b",
    rf"\bstaff\b\s+(?:\w+\s+){{0,3}}{_ROLE_WORD}\b",
    r"\bprincipal\b",
    rf"\blead\b\s+(?:\w+\s+){{0,3}}{_ROLE_WORD}\b",
    r"\b\w+\s+lead\b",
    r"\bdirector\b",
    r"\bhead of\b",
    r"\bvice president\b",
    r"\b(?<!\w)vp\b",
    r"\bchief\b",
]
_MID_TITLE_PATTERNS = [
    rf"\bmid\b\s+(?:\w+\s+){{0,2}}{_ROLE_WORD}\b",
    r"\bmid[\s-]?level\b",
    r"\bmiddle\b",
    r"\bmid[\s-]?senior\b",
]

_YEARS_EXPERIENCE_RE = re.compile(
    r"(\d+)\s*\+?\s*(?:-\s*\d+\s*)?(?:years?|yrs?)\s*(?:of\s+)?(?:relevant\s+|professional\s+|work(?:ing)?\s+)?experience",
    re.I,
)
_DEGREE_REQUIRED_RE = re.compile(
    r"\b(bachelor'?s?|master'?s?|phd|doctorate)\s+degree\s+(is\s+)?(required|preferred)\b",
    re.I,
)
_STUDENT_IN_PROGRESS_RE = re.compile(
    r"\b(pursuing|currently enrolled|working toward|candidate for|expected to graduate|enrolled in)\b",
    re.I,
)


def requires_experience_or_degree(text: str) -> bool:
    """True if the text clearly demands 2+ years of prior work experience or an
    already-completed degree (as opposed to a degree-in-progress, which is normal
    for internships)."""
    lower = text.lower()
    if _STUDENT_IN_PROGRESS_RE.search(lower):
        return False
    for match in _YEARS_EXPERIENCE_RE.finditer(lower):
        if int(match.group(1)) >= 2:
            return True
    return bool(_DEGREE_REQUIRED_RE.search(lower))


def is_senior_or_mid(text: str) -> bool:
    """True if the job text signals a mid/senior+ position that should be
    excluded from a student/entry-level search entirely."""
    lower = text.lower()
    if any(re.search(p, lower) for p in _SENIOR_TITLE_PATTERNS):
        return True
    if any(re.search(p, lower) for p in _MID_TITLE_PATTERNS):
        return True
    return requires_experience_or_degree(lower)
