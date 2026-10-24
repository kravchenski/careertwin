"""A technical question gets a technical answer: a shape that fits, no invented experience, the next question."""

from __future__ import annotations

import pytest

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.prompts import (
    GENERAL_KNOWLEDGE,
    KIND_HINTS,
    NEXT_HINT,
    SCREENSHOT_SYSTEM,
    SYSTEM,
    parse_suggestion,
    sections,
    split_partial,
    user_prompt,
)
from careertwin.interview.questions import Kind

REPLY = (
    "POINTS:\n- Use a **stack** for the brackets.\n- Time **O(n)**, space **O(n)**.\n"
    "ANSWER:\n```python\ndef balanced(s):\n    return True\n```\nIt pushes and pops.\n"
    "KEY:\n- **Stack**: last in, first out.\n"
    "CONTEXT:\nParsers use the same idea.\n"
    "NEXT:\n- What about other characters? - They are ignored.\n- Can it return the position? - Yes, track the index.\n"
)


def test_the_next_questions_are_a_section_of_their_own():
    parts = sections(REPLY)
    assert parts.follow_ups == [
        "What about other characters? - They are ignored.",
        "Can it return the position? - Yes, track the index.",
    ]
    assert parts.context == "Parsers use the same idea."  # NEXT does not leak into the section before it
    assert parts.has_details
    assert parse_suggestion(REPLY).as_dict()["follow_ups"] == parts.follow_ups


def test_a_reply_without_next_has_no_follow_ups():
    assert sections("POINTS:\n- a\nANSWER:\nb").follow_ups == []
    assert parse_suggestion("POINTS:\n- a\nANSWER:\nb").as_dict()["follow_ups"] == []


def test_only_the_capital_heading_starts_the_section():
    """"Next: we deploy" inside an answer is a sentence, not a heading."""
    parts = sections("ANSWER:\nFirst we test.\nNext: we deploy it.\nThen we watch.")
    assert parts.follow_ups == [] and "Next: we deploy it." in parts.answer


def test_a_heading_that_is_still_streaming_is_not_shown():
    assert split_partial("POINTS:\n- one\nANSWER:\ntext\nNE")[1] == "text"
    assert sections("POINTS:\n- one\nANSWER:\ntext\nNEX").answer == "text"
    assert sections("ANSWER:\ntext\nNEXT:\n- q - a").follow_ups == ["q - a"]


def prompt(kind, **options):
    return user_prompt(InterviewBrief(candidate="Alex: Python, FastAPI"), "How would you cache it?", kind, [], "en", **options)


def test_technical_questions_get_a_playbook_and_the_rule_about_experience():
    for kind in (Kind.TECHNICAL, Kind.DEFINITION):
        text = prompt(kind)
        assert GENERAL_KNOWLEDGE in text and NEXT_HINT in text
    technical = prompt(Kind.TECHNICAL)
    for shape in ("Concept or difference", "Design", "Troubleshooting", "Coding", "Choice"):
        assert shape in technical
    assert "complexity" in technical and "instead of guessing" in technical


def test_other_questions_do_not_carry_the_technical_text():
    for kind in set(Kind) - {Kind.TECHNICAL, Kind.DEFINITION}:
        assert "NEXT" not in prompt(kind) and "general knowledge" not in prompt(kind), kind
    assert set(KIND_HINTS) == set(Kind)


def test_the_hint_does_not_let_the_answer_describe_the_candidates_work():
    for banned in ("built", "used", "debugged", "monitored"):
        assert banned in GENERAL_KNOWLEDGE  # named as what must not be claimed
    assert "never what the candidate did" in GENERAL_KNOWLEDGE
    assert "NEXT" not in SYSTEM  # the system prompt stays short for the other kinds


@pytest.mark.parametrize("kind", ["Multiple choice", "What does this print or return", "SQL", "A bug to find", "Design or theory"])
def test_a_screenshot_is_answered_by_the_kind_of_task_it_shows(kind):
    assert kind in SCREENSHOT_SYSTEM
    assert "Never think aloud" in SCREENSHOT_SYSTEM


def test_a_trace_of_the_code_is_asked_for_before_the_output():
    assert "statement by statement" in SCREENSHOT_SYSTEM and "shared by reference" in SCREENSHOT_SYSTEM
