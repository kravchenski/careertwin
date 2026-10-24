"""Interview coaching routes: answer suggestions and practice questions."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from careertwin.interview.brief import BriefError, InterviewBrief
from careertwin.interview.coach import CoachError, cv_analysis, practice_questions, suggest
from careertwin.interview.languages import normalize
from careertwin.interview.questions import classify, detect_language

router = APIRouter(prefix="/v1/interview", tags=["interview"])


class SuggestRequest(BaseModel):
    question: str = Field(min_length=3, max_length=2000)
    job_id: str | None = Field(default=None, max_length=24)
    username: str = Field(default="default", min_length=1, max_length=64)
    history: list[str] = Field(default_factory=list, max_length=12)
    notes: str = Field(default="", max_length=4000)
    language: str | None = Field(
        default=None, max_length=20, description="Interview language, e.g. en, ru, pl; default: the question's"
    )


class PracticeRequest(BaseModel):
    job_id: str | None = Field(default=None, max_length=24)
    username: str = Field(default="default", min_length=1, max_length=64)
    count: int = Field(default=8, ge=3, le=15)
    focus: str = Field(default="", max_length=200)
    language: str | None = Field(default=None, max_length=20, description="Interview language, e.g. en, ru, pl")


class ReviewRequest(BaseModel):
    job_id: str = Field(max_length=24, description="Pipeline vacancy id; its prepared CV is reviewed")
    username: str = Field(default="default", min_length=1, max_length=64)
    language: str | None = Field(default=None, max_length=20, description="Language of the review")


def _brief(username: str, job_id: str | None, notes: str = "") -> InterviewBrief:
    try:
        return InterviewBrief.load(username=username, job=job_id, notes=notes)
    except BriefError as exc:
        raise HTTPException(422, str(exc)) from exc


@router.post("/suggest")
async def suggest_answer(request: SuggestRequest) -> dict[str, Any]:
    """Talking points and a spoken answer for one interviewer question, grounded in ABOUT.md."""
    brief = _brief(request.username, request.job_id, request.notes)
    try:
        result = await suggest(
            brief, request.question, request.history, language=normalize(request.language)
        )
    except CoachError as exc:
        raise HTTPException(502, str(exc)) from exc
    return {
        "kind": classify(request.question).value,
        "language": normalize(request.language) or detect_language(request.question),
        **result.as_dict(),
    }


@router.post("/practice")
async def practice(request: PracticeRequest) -> dict[str, Any]:
    """Likely interview questions for the vacancy, each with an outline from the candidate's facts."""
    brief = _brief(request.username, request.job_id)
    try:
        items = await practice_questions(
            brief, request.count, request.focus, normalize(request.language)
        )
    except CoachError as exc:
        raise HTTPException(502, str(exc)) from exc
    return {"questions": items}


@router.post("/cv-review")
async def review_cv(request: ReviewRequest) -> dict[str, Any]:
    """Compare the CV prepared for a vacancy with the vacancy: strengths, honest gaps, CV fixes."""
    brief = _brief(request.username, request.job_id)
    if not any(doc.kind == "cv" for doc in brief.documents):
        raise HTTPException(422, "This vacancy has no prepared CV yet; run prepare on it first")
    try:
        return await cv_analysis(brief, normalize(request.language))
    except CoachError as exc:
        raise HTTPException(502, str(exc)) from exc
