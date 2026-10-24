"""Preparing before the call: a bank of likely questions and a review of your CV for the vacancy."""

from __future__ import annotations

from careertwin.interview import languages
from careertwin.interview.brief import InterviewBrief
from careertwin.llm import parse_json_object


class PrepareError(RuntimeError):
    """The model did not return something usable."""


QUESTIONS_SYSTEM = """\
You prepare a candidate for an interview. From the candidate context and the vacancy, write \
the questions this interviewer is most likely to ask, mixing definitions and technical \
questions on the vacancy's stack, behavioral questions and motivation. For each give three to \
five short first-person lines the candidate can say. Definition and technical-knowledge \
questions get a correct explanation (general knowledge is fine). Statements about the \
candidate's own experience, employers, numbers and skills must come ONLY from the context; if \
the candidate lacks experience, a line says so plainly ("I haven't used X in production") and \
the next lines bridge to the closest real experience. Never write "in my context", "the CV" or \
"not stated" inside the lines: speak as the candidate. Write everything in the requested \
language and keep company names, technologies and job titles as written. Reply with JSON only: \
{"questions": [{"question": str, "kind": str, "outline": [str]}]} where kind is one of \
intro, definition, technical, behavioral, motivation, salary, logistics.\
"""

ANALYSIS_SYSTEM = """\
You review a candidate's CV against a vacancy before an interview. Use ONLY facts in the \
candidate context and the documents; never invent experience. Be specific and honest. Write \
everything in the requested language and keep company names, technologies and job titles as \
written. Reply with JSON only: {"summary": str, "strengths": [str], "gaps": [{"gap": str, \
"how_to_address": str}], "likely_questions": [str], "cv_fixes": [str]}. summary is two or \
three sentences on how well the CV fits and what the vacancy cares about most. strengths are \
the real facts to lead with. gaps are requirements the CV does not show, each with an honest \
way to address it in the interview. likely_questions are the questions those gaps invite. \
cv_fixes are concrete edits to the CV text, not generic advice. Keep it short: at most five \
strengths, five gaps, six questions and six fixes, each under 30 words.\
"""


def questions_prompt(brief: InterviewBrief, count: int, focus: str, language: str | None) -> str:
    prompt = f"{brief.render()}\n\nWrite {count} questions in {languages.name(language)}."
    return f"{prompt} Focus on: {focus}." if focus else prompt


def analysis_prompt(brief: InterviewBrief, language: str | None) -> str:
    return f"{brief.render()}\n\nReview the CV against the vacancy. Write in {languages.name(language)}."


def parse_questions(text: str, count: int) -> list[dict]:
    data = parse_json_object(text) if text.strip() else None
    questions = data.get("questions") if isinstance(data, dict) else None
    if not isinstance(questions, list) or not questions:
        raise PrepareError("The model did not return questions. Try again or pick another model.")
    return [
        {
            "question": str(item.get("question", "")).strip(),
            "kind": str(item.get("kind", "")).strip(),
            "outline": [str(point) for point in item.get("outline", [])][:6],
        }
        for item in questions
        if isinstance(item, dict) and item.get("question")
    ][:count]


def parse_analysis(text: str) -> dict:
    data = parse_json_object(text) if text.strip() else None
    if not isinstance(data, dict) or not (data.get("summary") or data.get("gaps")):
        raise PrepareError("The model did not return a CV review. Try again or pick another model.")
    gaps = [
        {"gap": str(item.get("gap", "")).strip(), "how_to_address": str(item.get("how_to_address", "")).strip()}
        for item in data.get("gaps", [])
        if isinstance(item, dict) and item.get("gap")
    ]
    return {
        "summary": str(data.get("summary", "")).strip(),
        "strengths": [str(item) for item in data.get("strengths", [])],
        "gaps": gaps,
        "likely_questions": [str(item) for item in data.get("likely_questions", [])],
        "cv_fixes": [str(item) for item in data.get("cv_fixes", [])],
    }
