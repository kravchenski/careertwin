"""Answer coach: turns a question and the candidate's background into talking points."""

from __future__ import annotations

import json
import os
import threading
from collections.abc import Iterator

import httpx

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.prepare import (
    ANALYSIS_SYSTEM,
    QUESTIONS_SYSTEM,
    PrepareError,
    analysis_prompt,
    parse_analysis,
    parse_questions,
    questions_prompt,
)
from careertwin.interview.prompts import (
    SYSTEM,
    Suggestion,
    parse_suggestion,
    screenshot_prompt,
    user_prompt,
)
from careertwin.interview.providers import base_url, provider_for
from careertwin.interview.questions import classify
from careertwin.llm import call_ai


def model_name() -> str:
    return os.getenv("INTERVIEW_MODEL") or os.getenv("SWITCHYARD_MODEL") or "auto"


def gateway_url() -> str:
    return base_url(os.getenv("SWITCHYARD_URL", "http://127.0.0.1:3260"))


def unreachable(base: str, exc: Exception) -> str:
    return (
        f"No AI gateway answers at {base} ({type(exc).__name__}). "
        "Pick a provider in Settings → AI, or start Switchyard."
    )


def failed(base: str, model: str, status: int, text: str) -> str:
    """What a provider's error means for the person at the window."""
    provider = provider_for(base)
    message = f"{provider.label if provider else 'The AI gateway'} returned {status}: {text[:200]}"
    if status in (400, 404) and model == "auto" and not (provider and provider.id == "switchyard"):
        message += " (choose a model in Settings → AI: Load models)"
    if status in (401, 403):
        message += " (check the API key in Settings → AI)"
    return message


class CoachError(RuntimeError):
    """The language model could not be reached or returned nothing."""


async def suggest(
    brief: InterviewBrief,
    question: str,
    history: list[str] | None = None,
    *,
    model: str | None = None,
    language: str | None = None,
) -> Suggestion:
    """One-shot suggestion through the configured Switchyard provider."""
    prompt = user_prompt(brief, question, classify(question), history or [], language)
    text = await call_ai(SYSTEM, prompt, "switchyard", model or model_name(), max_tokens=1600)
    if not text.strip():
        raise CoachError("The model returned no text. Is Switchyard running with a provider?")
    return parse_suggestion(text)


async def practice_questions(
    brief: InterviewBrief, count: int = 8, focus: str = "", language: str | None = None
) -> list[dict]:
    """Likely questions for the vacancy with answer outlines drawn from the candidate's facts."""
    text = await call_ai(
        QUESTIONS_SYSTEM,
        questions_prompt(brief, count, focus, language),
        "switchyard",
        model_name(),
        max_tokens=2500,
        temperature=0.4,
    )
    try:
        return parse_questions(text, count)
    except PrepareError as exc:
        raise CoachError(str(exc)) from exc


async def cv_analysis(brief: InterviewBrief, language: str | None = None) -> dict:
    """Review the CV against the vacancy: strengths, honest gaps, likely questions, CV fixes."""
    text = await call_ai(
        ANALYSIS_SYSTEM,
        analysis_prompt(brief, language),
        "switchyard",
        model_name(),
        max_tokens=2500,
        temperature=0.3,
    )
    try:
        return parse_analysis(text)
    except PrepareError as exc:
        raise CoachError(str(exc)) from exc


class StreamingCoach:
    """Blocking, cancellable token stream for the live overlay."""

    def __init__(
        self,
        brief: InterviewBrief,
        *,
        model: str | None = None,
        transport: httpx.BaseTransport | None = None,
        url: str | None = None,
        api_key: str | None = None,
        style: str = "standard",
        layout: str = "lines",
    ):
        self.brief = brief
        self.model = model
        self.transport = transport
        self.url = url
        self.api_key = api_key
        self.style = style
        self.layout = layout

    def complete(self, system: str, prompt: str, *, max_tokens: int = 2500, temperature: float = 0.3) -> str:
        """One blocking completion through the configured gateway, model and key."""
        headers = {}
        if key := self.api_key or os.getenv("SWITCHYARD_API_KEY"):
            headers["Authorization"] = f"Bearer {key}"
        base = base_url(self.url or gateway_url())
        body = {
            "model": self.model or model_name(),
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        try:
            with httpx.Client(timeout=httpx.Timeout(180, connect=5), transport=self.transport) as client:
                response = client.post(f"{base}/v1/chat/completions", json=body, headers=headers)
        except httpx.HTTPError as exc:
            raise CoachError(unreachable(base, exc)) from exc
        if response.status_code >= 400:
            raise CoachError(failed(base, body["model"], response.status_code, response.text))
        try:
            text = response.json()["choices"][0]["message"]["content"] or ""
        except (ValueError, KeyError, IndexError, TypeError) as exc:
            raise CoachError("The AI gateway returned an unreadable answer") from exc
        if not text.strip():
            raise CoachError("The model returned no text. Check the model in Settings → AI.")
        return text

    def stream(
        self,
        question: str,
        history: list[str],
        cancel: threading.Event,
        language: str | None = None,
        image: str | None = None,
    ) -> Iterator[str]:
        """Stream a hint. `image` is a base64 PNG of the screen; the question is then ignored.

        If the chosen model fails before saying anything (rate limit, outage) on a Switchyard
        gateway, the request is repeated once on automatic routing, so one bad provider does not
        cost a live answer. Other providers have no "auto", so their error is shown as it is."""
        model = self.model or model_name()
        started = False
        try:
            for chunk in self._stream_model(model, question, history, cancel, language, image):
                started = True
                yield chunk
        except CoachError as first:
            known = provider_for(base_url(self.url or gateway_url()))
            if (
                started or cancel.is_set() or model == self.FALLBACK_MODEL
                or (known and known.id != "switchyard")  # only a gateway routes "auto"
            ):
                raise
            try:
                for chunk in self._stream_model(
                    self.FALLBACK_MODEL, question, history, cancel, language, image
                ):
                    started = True
                    yield chunk
            except CoachError:
                if started:
                    raise
                raise first from None  # the chosen model's error says more than "auto"'s

    FALLBACK_MODEL = "auto"

    def _stream_model(
        self,
        model: str,
        question: str,
        history: list[str],
        cancel: threading.Event,
        language: str | None,
        image: str | None,
    ) -> Iterator[str]:
        if image:
            text = screenshot_prompt(self.brief, history, language, self.style, self.layout)
            content: str | list[dict] = [
                {"type": "text", "text": text},
                {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{image}"}},
            ]
        else:
            content = user_prompt(
                self.brief, question, classify(question), history, language, self.style, self.layout
            )
        body = {
            "model": model,
            "messages": [
                {"role": "system", "content": SYSTEM},
                {"role": "user", "content": content},
            ],
            "temperature": 0.3,
            "max_tokens": 1800,
            "stream": True,
        }
        headers = {"Accept": "text/event-stream"}
        if key := self.api_key or os.getenv("SWITCHYARD_API_KEY"):
            headers["Authorization"] = f"Bearer {key}"
        base = base_url(self.url or gateway_url())
        try:
            with httpx.Client(timeout=httpx.Timeout(120, connect=5), transport=self.transport) as client:
                with client.stream(
                    "POST", f"{base}/v1/chat/completions", json=body, headers=headers
                ) as response:
                    if response.status_code >= 400:
                        response.read()
                        raise CoachError(failed(base, model, response.status_code, response.text))
                    yield from _chunks(response, cancel)
        except httpx.HTTPError as exc:
            raise CoachError(unreachable(base, exc)) from exc


def _chunks(response: httpx.Response, cancel: threading.Event) -> Iterator[str]:
    if "text/event-stream" not in response.headers.get("content-type", ""):
        data = json.loads(response.read() or b"{}")
        message = (data.get("choices") or [{}])[0].get("message") or {}
        if message.get("content"):
            yield message["content"]
        return
    for line in response.iter_lines():
        if cancel.is_set():
            return
        if not line.startswith("data:"):
            continue
        payload = line[5:].strip()
        if payload == "[DONE]":
            return
        try:
            delta = (json.loads(payload).get("choices") or [{}])[0].get("delta") or {}
        except json.JSONDecodeError:
            continue
        if delta.get("content"):
            yield delta["content"]
