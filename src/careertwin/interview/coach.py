"""Answer coach: turns a question and the candidate's background into talking points."""

from __future__ import annotations

import contextlib
import json
import os
import queue
import re
import threading
import time
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
    SCREENSHOT_SYSTEM,
    SYSTEM,
    Suggestion,
    parse_suggestion,
    screenshot_prompt,
    user_prompt,
)
from careertwin.interview.providers import base_url, default_gateway, provider_for
from careertwin.interview.questions import classify
from careertwin.llm import call_ai


SCREENSHOT_MAX_TOKENS = 3500  # room for a whole solution; a short one still ends in a second or two


def model_name() -> str:
    return os.getenv("INTERVIEW_MODEL") or os.getenv("SWITCHYARD_MODEL") or "auto"


def gateway_url() -> str:
    return default_gateway()


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

    def __init__(self, message: str = "", *, retry_after: float | None = None, status: int | None = None):
        super().__init__(message)
        self.retry_after = retry_after  # seconds until a rate-limited model may be asked again
        self.status = status  # the HTTP status, when the provider answered


KEY_PROBLEMS = (401, 402, 403, 429)  # another key may be accepted where this one is limited, rejected or out of credit
KEY_REST = 600.0  # a key the provider rejected is left alone this long
MAX_COOLDOWN = 90.0  # a model that said "wait" is left alone this long at most
CANCEL_POLL = 0.05  # seconds between looks at the cancel flag while the model has said nothing yet
_WAIT_HINT = re.compile(r"next one in (\d+)\s*s", re.I)


def retry_after(response: httpx.Response) -> float | None:
    """How long a 429 asks to wait: the Retry-After header, or "next one in 32s" in the message."""
    if response.status_code != 429:
        return None
    try:
        seconds = float(response.headers.get("retry-after", ""))
    except ValueError:
        hint = _WAIT_HINT.search(response.text)
        seconds = float(hint[1]) if hint else 0.0
    return min(max(seconds, 0.0), MAX_COOLDOWN) or None


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
        vision_model: str | None = None,
        fallback_models: list[str] | None = None,
        spare_keys: list[str] | None = None,
    ):
        self.brief = brief
        self.model = model
        self.vision_model = vision_model
        self.fallback_models = fallback_models  # asked in this order when the chosen model fails; None: "auto"
        self.last_finish: str | None = None  # why the last reply ended: "stop", or "length" when cut off
        self.fell_back: str | None = None  # the model that answered when it was not the chosen one
        self.spare_keys = spare_keys  # keys of the same address, tried in turn when the one in use is limited or rejected
        self._wait_until: dict[tuple[str, str], float] = {}  # (key, model) that was rate limited, and until when; model "" is the whole key
        self._http: httpx.Client | None = None
        self.transport = transport
        self.url = url
        self.api_key = api_key
        self.style = style
        self.layout = layout

    def _client(self) -> httpx.Client:
        """One client for the whole call: its open connection saves the handshake on each answer."""
        if self._http is None:
            self._http = httpx.Client(timeout=httpx.Timeout(120, connect=5), transport=self.transport)
        return self._http

    def _auth(self, key: str | None = None) -> dict[str, str]:
        key = key or self.api_key or os.getenv("SWITCHYARD_API_KEY")
        return {"Authorization": f"Bearer {key}"} if key else {}

    def _keys(self) -> list[str | None]:
        """The key in use first, then the spare ones; `[None]` when there is no key at all."""
        own = self.api_key or os.getenv("SWITCHYARD_API_KEY")
        return list(dict.fromkeys(key for key in [own, *(self.spare_keys or [])] if key)) or [None]

    def warm_up(self) -> None:
        """Open the connection to the gateway now, so the first answer of a call does not pay for it."""
        with contextlib.suppress(httpx.HTTPError):
            self._client().get(
                f"{base_url(self.url or gateway_url())}/v1/models", headers=self._auth(), timeout=5
            )

    def close(self) -> None:
        if self._http is not None:
            self._http.close()
            self._http = None

    def complete(self, system: str, prompt: str, *, max_tokens: int = 2500, temperature: float = 0.3) -> str:
        """One blocking completion through the configured gateway, model and key."""
        base = base_url(self.url or gateway_url())
        body = {
            "model": self.model or model_name(),
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        keys = self._keys()
        try:
            with httpx.Client(timeout=httpx.Timeout(180, connect=5), transport=self.transport) as client:
                for position, key in enumerate(keys):
                    response = client.post(f"{base}/v1/chat/completions", json=body, headers=self._auth(key))
                    if response.status_code not in KEY_PROBLEMS or position == len(keys) - 1:
                        break  # otherwise the next key of this address may be accepted
        except httpx.HTTPError as exc:
            raise CoachError(unreachable(base, exc)) from exc
        if response.status_code >= 400:
            raise CoachError(failed(base, body["model"], response.status_code, response.text), status=response.status_code)
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
        listening: str | None = None,
    ) -> Iterator[str]:
        """Stream a hint. `image` is a base64 PNG of the screen; the question is then ignored.
        `listening` says the words came from the call audio and may not be a question ("heard") or
        are not finished ("hearing"); see `prompts.LISTENING_HINTS`.

        If the chosen model fails before saying anything (rate limit, outage), the next model of the
        fallback chain is asked, so one bad provider does not cost a live answer. A model that said
        how long to wait is not asked again until then. By default the chain is one step, "auto", on
        a Switchyard gateway; other providers have no "auto", so their error is shown as it is."""
        model = (image and self.vision_model) or self.model or model_name()
        chain = list(dict.fromkeys([model, *self._fallbacks(bool(image))]))
        keys = self._keys()
        started = False
        first: CoachError | None = None
        self.fell_back = None
        for position, name in enumerate(chain):
            for turn, key in enumerate(keys):
                last = position == len(chain) - 1 and turn == len(keys) - 1
                if not last and self._waiting(name, key):
                    continue
                try:
                    for chunk in self._stream_model(name, question, history, cancel, language, image, listening, key):
                        started = True
                        self.fell_back = name if name != model else None
                        yield chunk
                    return
                except CoachError as error:
                    if started or cancel.is_set():
                        raise
                    self._rest(name, key, error, len(keys) > 1)
                    first = first or error  # the chosen model's error says more than the fallback's
                    if error.status not in KEY_PROBLEMS:
                        break  # the same answer would come with another key: on to the next model
        if first:
            raise first

    def _fallbacks(self, image: bool) -> list[str]:
        if self.fallback_models and not image:  # a picture needs a model that can see: leave that to "auto"
            return self.fallback_models
        known = provider_for(base_url(self.url or gateway_url()))
        return [] if known and known.id != "switchyard" else [self.FALLBACK_MODEL]

    def _waiting(self, model: str, key: str | None = None) -> bool:
        until = max(self._wait_until.get((key or "", model), 0.0), self._wait_until.get((key or "", ""), 0.0))
        return time.monotonic() < until

    def _rest(self, model: str, key: str | None, error: CoachError, several: bool) -> None:
        """Leave a limited model alone for as long as it asked; a rejected key (with others to use) for good measure."""
        if error.retry_after:
            self._wait_until[(key or "", model)] = time.monotonic() + error.retry_after
        elif several and error.status in (401, 402, 403):
            self._wait_until[(key or "", "")] = time.monotonic() + KEY_REST

    FALLBACK_MODEL = "auto"
    # The session may ask this coach to tell a question from other audio (it then replies SKIP).
    supports_listening = True

    def _stream_model(
        self,
        model: str,
        question: str,
        history: list[str],
        cancel: threading.Event,
        language: str | None,
        image: str | None,
        listening: str | None = None,
        key: str | None = None,
    ) -> Iterator[str]:
        system, temperature, max_tokens = SYSTEM, 0.3, 1800
        if image:
            # The picture is the question: no CV, no history, so the model has little to read first.
            system, temperature, max_tokens = SCREENSHOT_SYSTEM, 0.2, SCREENSHOT_MAX_TOKENS
            url = image if image.startswith("data:") else f"data:image/png;base64,{image}"
            content: str | list[dict] = [
                {"type": "text", "text": screenshot_prompt(self.brief, language, self.layout)},
                {"type": "image_url", "image_url": {"url": url}},
            ]
        else:
            code = self.brief.project.retrieve(question) if self.brief.project else ""
            content = user_prompt(
                self.brief, question, classify(question), history, language, self.style, self.layout, code,
                listening,
            )
        body = {
            "model": model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": content},
            ],
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": True,
        }
        headers = {"Accept": "text/event-stream", **self._auth(key)}
        base = base_url(self.url or gateway_url())
        finish: dict[str, str] = {}
        replies: queue.Queue[str | Exception | None] = queue.Queue()

        def read() -> None:
            # A thread of its own: a provider that is slow to say anything cannot hold up the next
            # question, which stops waiting for this one the moment it is cancelled.
            try:
                with self._client().stream(
                    "POST", f"{base}/v1/chat/completions", json=body, headers=headers
                ) as response:
                    if response.status_code >= 400:
                        response.read()
                        raise CoachError(
                            failed(base, model, response.status_code, response.text),
                            retry_after=retry_after(response),
                            status=response.status_code,
                        )
                    for chunk in _chunks(response, cancel, finish):
                        replies.put(chunk)
                replies.put(None)
            except httpx.HTTPError as exc:
                replies.put(CoachError(unreachable(base, exc)))
            except Exception as exc:  # handed to the caller, which raises it
                replies.put(exc)

        threading.Thread(target=read, name="coach-read", daemon=True).start()
        self.last_finish = None
        try:
            while True:
                try:
                    reply = replies.get(timeout=CANCEL_POLL)
                except queue.Empty:
                    if cancel.is_set():
                        return
                    continue
                if reply is None:
                    return
                if isinstance(reply, Exception):
                    raise reply
                yield reply
        finally:
            self.last_finish = finish.get("reason")


def _chunks(
    response: httpx.Response, cancel: threading.Event, finish: dict[str, str] | None = None
) -> Iterator[str]:
    """The text of a reply as it arrives; `finish["reason"]` is set to why the model stopped."""
    finish = {} if finish is None else finish
    if "text/event-stream" not in response.headers.get("content-type", ""):
        data = json.loads(response.read() or b"{}")
        choice = (data.get("choices") or [{}])[0]
        message = choice.get("message") or {}
        if choice.get("finish_reason"):
            finish["reason"] = choice["finish_reason"]
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
            choice = (json.loads(payload).get("choices") or [{}])[0]
        except json.JSONDecodeError:
            continue
        if choice.get("finish_reason"):
            finish["reason"] = choice["finish_reason"]
        if (choice.get("delta") or {}).get("content"):
            yield choice["delta"]["content"]
