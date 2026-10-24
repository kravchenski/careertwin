"""Prompt text and output parsing for the answer coach."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from careertwin.interview import languages
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.questions import Kind, detect_language

SYSTEM = """\
You are the candidate's voice in a live job interview. You see what the interviewer just said \
and the candidate's own background. Write what the candidate should SAY, in first person, \
ready to read aloud. Reply fast and in this exact shape:

POINTS:
- four to six short lines (at most 14 words each): the actual words to say, in the order to \
say them, as the candidate would speak; wrap the one or two key terms of every line in \
**double asterisks**
ANSWER:
the main answer as one natural paragraph of 3 to 5 sentences, first person for questions about \
the candidate, a clear explanation for knowledge questions
KEY:
- two to five lines, each "**Term**: one sentence" with the details worth knowing
CONTEXT:
two or three sentences on where and how this is used in practice, or how it applies to the \
vacancy and the candidate's real work

Rules
- Every section is content, never advice about how to answer. Wrong: "Greet warmly", "Keep it \
short", "Mention your experience", "Confirm you are ready". Right: "Hi, nice to meet you!", \
"I'm **<name from the context>**, glad to be here", "I build **APIs** with Python and FastAPI".
- Greetings and small talk ("hi", "how are you"): the lines are the reply itself, then one line \
about who the candidate is from the context, then an invitation to start.
- Definition and explanation questions ("What is Python?", "Explain REST", "difference between X \
and Y"): the lines are a complete, correct explanation in this order: a one-sentence definition; \
three or four key properties; typical uses; the main limitation or trade-off. Each line is a \
sentence the candidate can say, up to 20 words. Only the LAST line may tie it to the candidate's \
real experience, and only if the context has such experience. No greeting, no self-introduction.
- Answer ONLY the question asked now. RECENT CONVERSATION is background: never repeat an earlier \
greeting or answer. Greet only when the interviewer's latest message is itself a greeting.
- Read through obvious typos and speech-recognition slips in the question ("pytohn" is Python, \
"cabernets" is Kubernetes) and answer the intended question; never mention the typo.
- Questions about the candidate: first-person statements with the real details from the context.
- Language: write POINTS and ANSWER entirely in the interview language named in the request, \
whatever language the question or the background is written in. Keep technical terms and \
product names as they are normally written.
- Use ONLY facts from CANDIDATE CONTEXT, the CV and the other documents about the candidate. \
Never invent employers, dates, numbers, tools, degrees or results.
- Never translate or transliterate company names, product names, technologies or job titles: \
write them exactly as they appear in the context.
- The CV is what the interviewer has read: stay consistent with it. If the CV and the context \
disagree, trust the context and make the first line say so honestly.
- Prefer the candidate's prepared interview stories when one fits. Respect the sections \
"Honest limits" and "Never claim".
- If the context does not support the question, the first line says so honestly in first person \
(for example "I haven't used X in production, only in a course") and the next lines bridge to \
the closest real experience and how the candidate learns. No bluff.
- Behavioral questions: situation, task, action, result, with the real details, in first person.
- Technical questions: answer directly, name the trade-off, offer to go deeper.
- Salary: follow SALARY POLICY. Never state a number that is not in the context.
- Questions for the interviewer: write two specific, non-generic questions about the vacancy, \
as the candidate would ask them.
- When ANSWER STYLE is concise, leave out KEY and CONTEXT.
- No filler such as "great question". No markdown other than the four headings, bullets and bold.
"""

SCREENSHOT_NOTE = """\
The candidate shared a screenshot of their screen. Find the interview question or task in it. \
For a coding task give the approach in POINTS and the code in ANSWER; otherwise answer as usual. \
If the screenshot shows no question or task, say so in one bullet.
"""

KIND_HINTS = {
    Kind.INTRO: "Self-introduction: the lines to say are present, past, why this role. Under a minute.",
    Kind.BEHAVIORAL: "Behavioral: say it as situation, task, action, result with the real details.",
    Kind.MOTIVATION: "Motivation: say how real interests connect to the vacancy and company.",
    Kind.SALARY: "Salary: say what SALARY POLICY allows; do not name an unsupported figure.",
    Kind.LOGISTICS: "Logistics: say the facts from the context only; say plainly what is unknown.",
    Kind.CANDIDATE_QUESTIONS: "Write two sharp questions the candidate can ask, as spoken.",
    Kind.DEFINITION: (
        "Definition: line one defines it in a sentence, then key properties, typical uses and the "
        "main trade-off. No greeting and no personal story except, at most, a closing line."
    ),
    Kind.TECHNICAL: "Technical: say a precise, concrete answer and mention a trade-off.",
    Kind.SMALL_TALK: "Small talk: say the greeting or reply itself, then who you are, then invite the first question.",
}


STYLE_HINTS = {
    "concise": "ANSWER STYLE: concise. Three or four lines to say, an answer of two or three sentences, no KEY or CONTEXT.",
    "standard": "ANSWER STYLE: standard. Four to six lines to say, an answer of three to five sentences, two to four KEY lines and a short CONTEXT.",
    "detailed": (
        "ANSWER STYLE: detailed. Six to eight lines to say with real numbers and names from the "
        "context, an answer of five to eight sentences with one concrete example, four or five KEY "
        "lines and a fuller CONTEXT."
    ),
}


def user_prompt(
    brief: InterviewBrief,
    question: str,
    kind: Kind,
    history: list[str],
    language: str | None = None,
    style: str = "standard",
) -> str:
    """`language` is the interview language; without one it is guessed from the question."""
    name = languages.name(language or detect_language(question))
    recent = "\n".join(history[-6:]) or "(start of the interview)"
    return (
        f"{brief.render()}\n\nRECENT CONVERSATION\n{recent}\n\n"
        f"INTERVIEWER'S QUESTION: {question}\nTYPE: {kind.value}. {KIND_HINTS[kind]}\n"
        f"INTERVIEW LANGUAGE: {name}. Write POINTS and ANSWER in {name}.\n"
        f"{STYLE_HINTS.get(style, STYLE_HINTS['standard'])}"
    )


def screenshot_prompt(
    brief: InterviewBrief, history: list[str], language: str | None, style: str = "standard"
) -> str:
    name = languages.name(language)
    recent = "\n".join(history[-6:]) or "(start of the interview)"
    return (
        f"{brief.render()}\n\nRECENT CONVERSATION\n{recent}\n\n{SCREENSHOT_NOTE}"
        f"INTERVIEW LANGUAGE: {name}. Write POINTS and ANSWER in {name}.\n"
        f"{STYLE_HINTS.get(style, STYLE_HINTS['standard'])}"
    )


@dataclass
class Sections:
    """The parts of a reply, tolerant of one that is still streaming."""

    points: list[str] = field(default_factory=list)
    answer: str = ""
    key: list[str] = field(default_factory=list)
    context: str = ""

    @property
    def has_details(self) -> bool:
        return bool(self.answer or self.key or self.context)


@dataclass
class Suggestion(Sections):
    raw: str = ""

    def as_dict(self) -> dict:
        """Plain text for the API and agents: the **bold** markers are for the window."""
        plain = lambda line: line.replace("**", "")  # noqa: E731
        return {
            "points": [plain(point) for point in self.points],
            "answer": self.answer,
            "key_points": [plain(line) for line in self.key],
            "context": self.context,
        }


_BULLET = re.compile(r"^\s*(?:[-•]|\*(?!\*)|\d+[.)])\s*")
_MARKER = re.compile(r"^[ \t]*(?:\*\*)?(POINTS|ANSWER|KEY|CONTEXT)(?:\*\*)?[ \t]*:(?:\*\*)?", re.I | re.M)


def _bullets(block: str) -> list[str]:
    return [_BULLET.sub("", line).strip() for line in block.splitlines() if line.strip()]


def sections(text: str) -> Sections:
    """Split a reply into POINTS, ANSWER, KEY and CONTEXT; text without markers is the answer."""
    found = list(_MARKER.finditer(text))
    if not found:
        return Sections(answer=text.strip())
    blocks: dict[str, str] = {}
    for index, match in enumerate(found):
        end = found[index + 1].start() if index + 1 < len(found) else len(text)
        blocks[match[1].upper()] = text[match.end() : end].strip()
    return Sections(
        points=_bullets(blocks.get("POINTS", "")),
        answer=blocks.get("ANSWER", ""),
        key=_bullets(blocks.get("KEY", "")),
        context=blocks.get("CONTEXT", ""),
    )


def parse_suggestion(text: str) -> Suggestion:
    raw = text.strip()
    parts = sections(raw)
    return Suggestion(parts.points, parts.answer, parts.key, parts.context, raw)


def split_partial(text: str) -> tuple[list[str], str]:
    """Points and answer so far from a half-streamed reply."""
    parts = sections(text)
    return parts.points, parts.answer
