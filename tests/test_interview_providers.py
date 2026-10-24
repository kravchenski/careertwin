"""Providers: any OpenAI-style address works, and the errors say what to change."""

from __future__ import annotations

import json
import threading

import httpx
import pytest

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import CoachError, StreamingCoach
from careertwin.interview.providers import PROVIDERS, base_url, provider_for


def test_a_pasted_address_is_reduced_to_its_base():
    assert base_url("https://api.groq.com/openai/v1/") == "https://api.groq.com/openai"
    assert base_url("https://api.openai.com/v1/chat/completions") == "https://api.openai.com"
    assert base_url("http://localhost:11434/chat/completions") == "http://localhost:11434"
    assert base_url(" http://127.0.0.1:3260/ ") == "http://127.0.0.1:3260"


def test_presets_are_found_from_the_address_people_paste():
    assert provider_for("https://api.groq.com/openai/v1").id == "groq"
    assert provider_for("HTTP://127.0.0.1:3260/").id == "switchyard"
    assert provider_for("http://10.0.0.5:3260") is None
    assert [p.id for p in PROVIDERS if not p.cloud] == ["switchyard", "ollama", "lmstudio"]


def make(url, model="m", handler=None):
    return StreamingCoach(
        InterviewBrief.load(allow_empty=True), url=url, api_key="k", model=model,
        transport=httpx.MockTransport(handler or (lambda request: httpx.Response(500))),
    )


def test_an_address_ending_in_v1_does_not_get_a_second_v1():
    seen = []

    def handler(request):
        seen.append(str(request.url))
        return httpx.Response(200, json={"choices": [{"message": {"content": "hi"}}]})

    assert make("https://api.groq.com/openai/v1", handler=handler).complete("s", "p") == "hi"
    assert seen == ["https://api.groq.com/openai/v1/chat/completions"]


def test_no_gateway_is_explained_without_naming_switchyard_only():
    def refuse(request):
        raise httpx.ConnectError("refused")

    for call in (
        lambda coach: coach.complete("s", "p"),
        lambda coach: list(coach.stream("Why?", [], threading.Event(), "en")),
    ):
        with pytest.raises(CoachError) as caught:
            call(make("http://127.0.0.1:3260", handler=refuse))
        message = str(caught.value)
        assert "No AI gateway answers at http://127.0.0.1:3260" in message
        assert "Settings → AI model" in message and "ConnectError" in message


def test_a_providers_errors_name_it_and_say_what_to_change():
    def reply(status):
        return lambda request: httpx.Response(status, text="nope")

    with pytest.raises(CoachError, match=r"Groq returned 401: nope \(check the API key"):
        make("https://api.groq.com/openai", handler=reply(401)).complete("s", "p")
    with pytest.raises(CoachError, match=r"OpenAI returned 404.*choose a model"):
        make("https://api.openai.com/v1", model="auto", handler=reply(404)).complete("s", "p")
    with pytest.raises(CoachError) as caught:  # a real model id: no advice about picking one
        make("https://api.openai.com", model="gpt-x", handler=reply(404)).complete("s", "p")
    assert "choose a model" not in str(caught.value)
    with pytest.raises(CoachError) as caught:  # Switchyard knows "auto": the model advice does not apply
        make("http://127.0.0.1:3260", model="auto", handler=reply(404)).complete("s", "p")
    assert "choose a model" not in str(caught.value) and "Switchyard (local) returned 404" in str(caught.value)


def test_other_providers_are_not_asked_for_a_model_called_auto():
    models = []

    def handler(request):
        models.append(json.loads(request.content)["model"])
        return httpx.Response(429, text="rate limited")

    with pytest.raises(CoachError, match="Groq returned 429"):
        list(make("https://api.groq.com/openai", model="llama-x", handler=handler).stream("Why?", [], threading.Event(), "en"))
    assert models == ["llama-x"]


def test_an_unknown_gateway_still_falls_back_and_reports_the_chosen_models_error():
    models = []

    def handler(request):
        model = json.loads(request.content)["model"]
        models.append(model)
        return httpx.Response(503 if model == "x" else 400, text="down" if model == "x" else "no auto")

    with pytest.raises(CoachError, match="503: down"):
        list(make("http://10.0.0.5:3260", model="x", handler=handler).stream("Why?", [], threading.Event(), "en"))
    assert models == ["x", "auto"]
