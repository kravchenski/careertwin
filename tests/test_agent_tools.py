"""Agent interface contracts: tool catalogue, API client errors, Switchyard loop."""

from __future__ import annotations

import json

import httpx
import pytest

from careertwin.agentic import tools
from careertwin.agentic.client import ApiError, CareerTwinClient
from careertwin.agentic.mcp_server import build_server
from careertwin.agentic.registry import all_tools, get_tool, openai_tool_specs
from careertwin.agentic.runner import AgentRunner


class FakeApi:
    """Stands in for the CareerTwin API behind CareerTwinClient."""

    def __init__(self):
        self.calls: list[tuple[str, str, object]] = []
        self.jobs = {
            "j1": {
                "id": "j1",
                "state": "priority",
                "vacancy": {"title": "Python Developer", "company": "Example", "url": "https://e.co/1"},
                "evaluation": {"score": 88.0, "category": "priority"},
            }
        }

    def handler(self, request: httpx.Request) -> httpx.Response:
        body = json.loads(request.content) if request.content else None
        self.calls.append((request.method, request.url.path, body))
        path = request.url.path
        if path == "/v1/health":
            return httpx.Response(200, json={"status": "ok"})
        if path == "/v1/automation/jobs" and request.method == "GET":
            return httpx.Response(200, json=list(self.jobs.values()))
        if path.startswith("/v1/automation/jobs/") and request.method == "GET":
            job = self.jobs.get(path.rsplit("/", 1)[1])
            if job is None:
                return httpx.Response(404, json={"detail": "Vacancy not found"})
            return httpx.Response(200, json={"job": job, "events": [], "files": []})
        if path.endswith("/actions"):
            return httpx.Response(200, json={"status": "queued"})
        return httpx.Response(404, json={"detail": "unexpected " + path})


@pytest.fixture
def api(monkeypatch):
    fake = FakeApi()
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: CareerTwinClient(base_url="http://api.test", transport=httpx.MockTransport(fake.handler)),
    )
    return fake


def test_catalogue_is_consistent_between_openai_and_mcp():
    names = {item.name for item in all_tools()}
    assert {"get_status", "process_vacancy", "submit_application"} <= names
    assert names == {spec["function"]["name"] for spec in openai_tool_specs()}
    assert names == {item.name for item in build_server()._tool_manager.list_tools()}
    for item in all_tools():
        assert item.description
        assert item.parameters["type"] == "object"


def test_only_submit_is_outward():
    assert [item.name for item in all_tools() if item.outward] == ["submit_application"]
    assert not get_tool("list_vacancies").mutating


async def test_list_vacancies_returns_brief_rows(api):
    rows = await get_tool("list_vacancies").call({"state": "priority"})
    assert rows[0]["id"] == "j1" and rows[0]["fit_score"] == 88.0
    assert await get_tool("list_vacancies").call({"state": "skip"}) == []


async def test_wait_for_vacancy_returns_when_state_is_not_transient(api):
    result = await get_tool("wait_for_vacancy").call({"job_id": "j1", "timeout_seconds": 5})
    assert result["finished"] is True and result["state"] == "priority"


async def test_api_errors_are_readable(api):
    with pytest.raises(Exception, match="Vacancy not found"):
        await get_tool("get_vacancy").call({"job_id": "missing"})


async def test_submit_needs_confirm_and_opt_in(api, monkeypatch):
    monkeypatch.delenv("CAREERTWIN_AGENT_ALLOW_SUBMIT", raising=False)
    with pytest.raises(Exception, match="confirm=true"):
        await get_tool("submit_application").call({"job_id": "j1"})
    with pytest.raises(Exception, match="disabled"):
        await get_tool("submit_application").call({"job_id": "j1", "confirm": True})
    assert not any(path.endswith("/actions") for _, path, _ in api.calls)
    monkeypatch.setenv("CAREERTWIN_AGENT_ALLOW_SUBMIT", "1")
    await get_tool("submit_application").call({"job_id": "j1", "confirm": True})
    assert api.calls[-1] == ("POST", "/v1/automation/jobs/j1/actions", {"operation": "submit"})


async def test_client_reports_missing_api():
    def refuse(request):
        raise httpx.ConnectError("refused")

    client = CareerTwinClient(base_url="http://api.test", transport=httpx.MockTransport(refuse))
    with pytest.raises(ApiError, match="not running"):
        await client.get("/v1/health")


def switchyard(replies):
    sent = []

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(json.loads(request.content))
        return httpx.Response(200, json={"choices": [{"message": replies.pop(0)}]})

    return httpx.MockTransport(handler), sent


def tool_call(name, arguments, call_id="c1"):
    return {
        "role": "assistant",
        "content": None,
        "tool_calls": [
            {"id": call_id, "type": "function", "function": {"name": name, "arguments": json.dumps(arguments)}}
        ],
    }


async def test_runner_executes_tool_calls_until_final_answer(api):
    transport, sent = switchyard(
        [tool_call("list_vacancies", {"state": "priority"}), {"role": "assistant", "content": "One priority vacancy."}]
    )
    log: list[str] = []
    runner = AgentRunner(model="auto", transport=transport, on_event=log.append)
    answer = await runner.ask("What is ready?")
    assert answer == "One priority vacancy."
    assert sent[0]["model"] == "auto" and len(sent[0]["tools"]) == len(all_tools())
    tool_message = [m for m in sent[1]["messages"] if m["role"] == "tool"][0]
    assert tool_message["tool_call_id"] == "c1" and "Python Developer" in tool_message["content"]
    assert any(line.startswith("→ list_vacancies") for line in log)


async def test_runner_blocks_outward_tool_without_approval(api, monkeypatch):
    monkeypatch.setenv("CAREERTWIN_AGENT_ALLOW_SUBMIT", "1")
    transport, sent = switchyard(
        [
            tool_call("submit_application", {"job_id": "j1", "confirm": True}),
            {"role": "assistant", "content": "Not submitted."},
        ]
    )
    runner = AgentRunner(transport=transport, confirm=lambda _prompt: False, on_event=lambda _: None)
    await runner.ask("Apply to j1")
    tool_message = [m for m in sent[1]["messages"] if m["role"] == "tool"][0]
    assert "did not approve" in tool_message["content"]
    assert not any(path.endswith("/actions") for _, path, _ in api.calls)


async def test_runner_survives_bad_arguments_and_unknown_tools(api):
    transport, sent = switchyard(
        [
            {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {"id": "a", "type": "function", "function": {"name": "nope", "arguments": "{}"}},
                    {"id": "b", "type": "function", "function": {"name": "get_vacancy", "arguments": "{bad"}},
                ],
            },
            {"role": "assistant", "content": "done"},
        ]
    )
    runner = AgentRunner(transport=transport, on_event=lambda _: None)
    assert await runner.ask("go") == "done"
    contents = [m["content"] for m in sent[1]["messages"] if m["role"] == "tool"]
    assert "Unknown tool" in contents[0] and "not valid JSON" in contents[1]
