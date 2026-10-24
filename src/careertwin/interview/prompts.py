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
- Code: put any code in ANSWER inside one fenced block with its language (```python), indented \
properly, never in POINTS or KEY. Write identifiers, commands and file names in `single backticks`.
- Behavioral questions: situation, task, action, result, with the real details, in first person.
- Technical questions: answer directly, name the trade-off, offer to go deeper.
- Salary: follow SALARY POLICY. Never state a number that is not in the context.
- Questions for the interviewer: write two specific, non-generic questions about the vacancy, \
as the candidate would ask them.
- Questions about the candidate's own project (see the PROJECT section and RELEVANT CODE): answer \
as its author. Say what the code does and why it is built that way, name the real files, modules \
and technologies, give the trade-off, and offer one concrete improvement when asked. Use only what \
the dossier and the code show; where they are silent say so plainly. A reason marked (inferred) is \
a guess the candidate has not confirmed: phrase it as "the main reason is probably", never as fact, \
unless the candidate notes confirm it.
- When ANSWER STYLE is concise, leave out KEY and CONTEXT.
- When ANSWER LAYOUT is main answer only, reply with just "ANSWER:" and the paragraph: no POINTS, \
no KEY, no CONTEXT, and start the paragraph at once. It is read aloud, so write it as speech.
- Never invent routines, methods, habits, tools or examples that are not in the context. If the \
context has no such fact, say plainly what is known and what the candidate can offer, and do not \
make up a plan.
- No filler such as "great question". No markdown other than the four headings, bullets and bold.
"""

SCREENSHOT_SYSTEM = """\
You solve what a screenshot shows (a coding task, a test question, a slide) for a candidate in a \
live interview. Be fast: no preamble, start with the shape below at once.

POINTS:
- two to four short lines: the approach in the words the candidate would say, key terms in \
**double asterisks**
ANSWER:
for a coding task one fenced code block with the complete working code, the exact function \
signature shown on the screen and one comment line with the time and space complexity; for any \
other question one short paragraph to read aloud

Rules
- Write the two headings exactly as shown, each on its own line: "POINTS:" before the lines and \
"ANSWER:" before the code. Never put code under POINTS.
- Read the task carefully: the constraints, the examples, the signature and the language the task \
names or the editor shows. Use the CANDIDATE STACK only when the screen names no language.
- Correct first, then efficient. Check the examples on the screen in your head before you answer.
- No other headings and no explanation after the code. Code stays in English; the lines and any \
prose are written in the interview language.
- If the screenshot shows no task or question, write one POINTS bullet that says so.
"""

KIND_HINTS = {
    Kind.INTRO: "Self-introduction: the lines to say are present, past, why this role. Under a minute.",
    Kind.BEHAVIORAL: "Behavioral: say it as situation, task, action, result with the real details.",
    Kind.MOTIVATION: "Motivation: say how real interests connect to the vacancy and company.",
    Kind.SALARY: "Salary: say what SALARY POLICY allows; do not name an unsupported figure.",
    Kind.LOGISTICS: (
        "Logistics: answer with the facts from the context and the candidate notes (availability, "
        "hours, studies, start date, office days, location). Do not invent a plan or a routine; "
        "if something is not stated, say it can be agreed."
    ),
    Kind.CANDIDATE_QUESTIONS: "Write two sharp questions the candidate can ask, as spoken.",
    Kind.DEFINITION: (
        "Definition: line one defines it in a sentence, then key properties, typical uses and the "
        "main trade-off. No greeting and no personal story except, at most, a closing line."
    ),
    Kind.TECHNICAL: "Technical: say a precise, concrete answer and mention a trade-off.",
    Kind.SMALL_TALK: "Small talk: say the greeting or reply itself, then who you are, then invite the first question.",
}


LAYOUT_HINTS = {
    "main": (
        "ANSWER LAYOUT: main answer only. Write only the ANSWER section, as one spoken paragraph "
        "the candidate can read aloud."
    ),
    "lines": "ANSWER LAYOUT: lines to say, then the detailed answer.",
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
    layout: str = "lines",
    code: str = "",
) -> str:
    """`language` is the interview language; without one it is guessed from the question.
    `code` is what the search of the candidate's project found for this question."""
    name = languages.name(language or detect_language(question))
    recent = "\n".join(history[-6:]) or "(start of the interview)"
    excerpts = f"RELEVANT CODE FROM THE PROJECT (excerpts)\n{code}\n\n" if code else ""
    return (
        f"{brief.render()}\n\n{excerpts}RECENT CONVERSATION\n{recent}\n\n"
        f"INTERVIEWER'S QUESTION: {question}\nTYPE: {kind.value}. {KIND_HINTS[kind]}\n"
        f"INTERVIEW LANGUAGE: {name}. Write your reply in {name}.\n"
        f"{STYLE_HINTS.get(style, STYLE_HINTS['standard'])}\n"
        f"{LAYOUT_HINTS.get(layout, LAYOUT_HINTS['lines'])}"
    )


def screenshot_prompt(brief: InterviewBrief, language: str | None, layout: str = "lines") -> str:
    """What goes next to the picture. The CV stays out: a task on the screen needs the stack only."""
    name = languages.name(language)
    stack = brief.stack_hint() or "(not stated)"
    return (
        f"CANDIDATE STACK: {stack}\n"
        f"INTERVIEW LANGUAGE: {name}. Write the lines and prose in {name}.\n"
        f"{LAYOUT_HINTS.get(layout, LAYOUT_HINTS['lines'])}\n"
        "Solve what the screenshot shows."
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
    """The lines of a list; a fenced code block stays in one piece, with its indentation."""
    items, code = [], None
    for line in block.splitlines():
        if code is not None:
            code.append(line)
            if line.strip().startswith("```") and len(code) > 1:
                items.append("\n".join(code))
                code = None
        elif line.strip().startswith("```"):
            code = [line.strip()]
        elif line.strip():
            items.append(_BULLET.sub("", line).strip())
    if code is not None:  # a block that is still streaming
        items.append("\n".join(code))
    return items


def sections(text: str) -> Sections:
    """Split a reply into POINTS, ANSWER, KEY and CONTEXT; text without markers is the answer."""
    found = list(_MARKER.finditer(text))
    if not found:
        return Sections(answer=text.strip())
    blocks: dict[str, str] = {}
    for index, match in enumerate(found):
        end = found[index + 1].start() if index + 1 < len(found) else len(text)
        blocks[match[1].upper()] = text[match.end() : end].strip()
    points_block, answer = blocks.get("POINTS", ""), blocks.get("ANSWER", "")
    fence = points_block.find("```")
    if fence >= 0 and not answer:
        points_block, answer = points_block[:fence], points_block[fence:].strip()
    return Sections(
        points=_bullets(points_block),
        answer=answer,
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
