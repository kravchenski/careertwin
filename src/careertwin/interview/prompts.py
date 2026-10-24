"""Prompt text and output parsing for the answer coach."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.questions import Kind, detect_language

LANGUAGES = {"en": "English", "ru": "Russian", "pl": "Polish"}

SYSTEM = """\
You help a candidate during a live job interview. You see the interviewer's question and the \
candidate's own background. Reply fast and in this exact shape:

POINTS:
- three to four short bullets (at most 12 words each) the candidate can glance at
ANSWER:
a natural spoken answer of 3 to 6 sentences, about 30 seconds aloud, first person

Rules
- Use ONLY facts from CANDIDATE CONTEXT. Never invent employers, dates, numbers, tools, \
degrees or results.
- If the context does not support the question, start POINTS with "No direct experience on \
record" and give an honest answer: the closest real experience, how the candidate learns, \
and no bluff.
- Answer in the language of the question.
- Behavioral questions: use situation, task, action, result, with the real details.
- Technical questions: answer directly, name the trade-off, offer to go deeper.
- Salary: follow SALARY POLICY. Never state a number that is not in the context.
- Questions for the interviewer: suggest two specific, non-generic questions about the \
vacancy.
- No filler such as "great question". No markdown other than the two headings and bullets.
"""

KIND_HINTS = {
    Kind.INTRO: "Self-introduction: present, past, why this role. Under a minute.",
    Kind.BEHAVIORAL: "Behavioral: structure the answer as situation, task, action, result.",
    Kind.MOTIVATION: "Motivation: connect real interests to the vacancy and company.",
    Kind.SALARY: "Salary: follow SALARY POLICY; do not name an unsupported figure.",
    Kind.LOGISTICS: "Logistics: answer from the context only; say what is unknown.",
    Kind.CANDIDATE_QUESTIONS: "Suggest two sharp questions the candidate can ask.",
    Kind.TECHNICAL: "Technical: be precise and concrete, mention a trade-off.",
    Kind.SMALL_TALK: "Small talk: a one-line natural reply.",
}


def user_prompt(brief: InterviewBrief, question: str, kind: Kind, history: list[str]) -> str:
    language = LANGUAGES[detect_language(question)]
    recent = "\n".join(history[-6:]) or "(start of the interview)"
    return (
        f"{brief.render()}\n\nRECENT CONVERSATION\n{recent}\n\n"
        f"INTERVIEWER'S QUESTION: {question}\nTYPE: {kind.value}. {KIND_HINTS[kind]}\n"
        f"Answer in {language}."
    )


@dataclass
class Suggestion:
    points: list[str] = field(default_factory=list)
    answer: str = ""
    raw: str = ""

    def as_dict(self) -> dict:
        return {"points": self.points, "answer": self.answer}


def parse_suggestion(text: str) -> Suggestion:
    raw = text.strip()
    match = re.search(r"POINTS:\s*(.*?)\s*ANSWER:\s*(.*)", raw, re.S | re.I)
    if not match:
        return Suggestion(answer=raw, raw=raw)
    points = [
        re.sub(r"^[-•*\d.)\s]+", "", line).strip()
        for line in match[1].splitlines()
        if line.strip()
    ]
    return Suggestion(points=points, answer=match[2].strip(), raw=raw)


def split_partial(text: str) -> tuple[list[str], str]:
    """Points and answer so far from a half-streamed reply."""
    head, _, tail = text.partition("ANSWER:")
    if "ANSWER:" not in text and "POINTS:" not in text:
        return [], text.strip()
    points = [
        re.sub(r"^[-•*\d.)\s]+", "", line).strip()
        for line in head.replace("POINTS:", "").splitlines()
        if line.strip()
    ]
    return points, tail.strip()
