"""Tool-calling agent loop on top of a Switchyard (OpenAI-compatible) gateway."""

from __future__ import annotations

import json
import os
import sys
from collections.abc import Callable
from typing import Any

import httpx
from mcp.server.fastmcp.exceptions import ToolError

from careertwin.agentic.client import ApiError
from careertwin.agentic.prompts import SYSTEM_PROMPT
from careertwin.agentic.registry import AgentTool, get_tool, openai_tool_specs

MAX_RESULT_CHARS = 12_000
Confirm = Callable[[str], bool]


def gateway_url() -> str:
    return os.getenv("SWITCHYARD_URL", "http://127.0.0.1:3260").rstrip("/")


def gateway_model() -> str:
    return os.getenv("SWITCHYARD_AGENT_MODEL") or os.getenv("SWITCHYARD_MODEL") or "auto"


def ask_on_terminal(prompt: str) -> bool:
    if not sys.stdin.isatty():
        return False
    return input(f"{prompt} [y/N] ").strip().lower() in {"y", "yes"}


class AgentRunner:
    def __init__(
        self,
        *,
        model: str | None = None,
        max_steps: int = 25,
        confirm: Confirm = ask_on_terminal,
        on_event: Callable[[str], None] = print,
        transport: httpx.AsyncBaseTransport | None = None,
    ):
        self.model = model or gateway_model()
        self.max_steps = max_steps
        self.confirm = confirm
        self.on_event = on_event
        self.transport = transport
        self.messages: list[dict[str, Any]] = [{"role": "system", "content": SYSTEM_PROMPT}]

    async def ask(self, task: str) -> str:
        """Run one user turn to completion and return the agent's final answer."""
        self.messages.append({"role": "user", "content": task})
        for _ in range(self.max_steps):
            message = await self._complete()
            self.messages.append(message)
            calls = message.get("tool_calls") or []
            if not calls:
                return (message.get("content") or "").strip()
            for call in calls:
                self.messages.append(await self._execute(call))
        return f"Stopped after {self.max_steps} steps without a final answer."

    async def _complete(self) -> dict[str, Any]:
        headers = {}
        if key := os.getenv("SWITCHYARD_API_KEY"):
            headers["Authorization"] = f"Bearer {key}"
        body = {
            "model": self.model,
            "messages": self.messages,
            "tools": openai_tool_specs(),
            "tool_choice": "auto",
            "temperature": 0.2,
        }
        try:
            async with httpx.AsyncClient(timeout=300, transport=self.transport) as client:
                response = await client.post(
                    f"{gateway_url()}/v1/chat/completions", json=body, headers=headers
                )
        except httpx.HTTPError as exc:
            raise ApiError(
                f"Switchyard is not reachable at {gateway_url()} ({type(exc).__name__}). "
                "Start the gateway and connect a provider."
            ) from exc
        if response.status_code >= 400:
            raise ApiError(f"Switchyard returned {response.status_code}: {response.text[:300]}")
        message = response.json()["choices"][0]["message"]
        return {key: value for key, value in message.items() if value is not None}

    async def _execute(self, call: dict[str, Any]) -> dict[str, Any]:
        function = call.get("function") or {}
        name = function.get("name", "")
        result = await self._run_tool(name, function.get("arguments"))
        return {
            "role": "tool",
            "tool_call_id": call.get("id", name),
            "content": result[:MAX_RESULT_CHARS],
        }

    async def _run_tool(self, name: str, raw_arguments: Any) -> str:
        try:
            item = get_tool(name)
        except KeyError:
            return json.dumps({"error": f"Unknown tool {name!r}"})
        try:
            arguments = (
                raw_arguments
                if isinstance(raw_arguments, dict)
                else json.loads(raw_arguments or "{}")
            )
        except json.JSONDecodeError as exc:
            return json.dumps({"error": f"Arguments are not valid JSON: {exc}"})
        self.on_event(f"→ {name}({_short(arguments)})")
        if item.outward and not self._approved(item, arguments):
            return json.dumps({"error": "The candidate did not approve this action."})
        try:
            value = await item.call(arguments)
        except (ApiError, ToolError) as exc:
            self.on_event(f"  ✗ {exc}")
            return json.dumps({"error": str(exc)})
        text = json.dumps(value, ensure_ascii=False, default=str)
        self.on_event(f"  ✓ {_short(text, 160)}")
        return text

    def _approved(self, item: AgentTool, arguments: dict[str, Any]) -> bool:
        return self.confirm(f"Allow {item.name}({_short(arguments)}) — this contacts the employer?")


def _short(value: Any, limit: int = 100) -> str:
    text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)
    return text if len(text) <= limit else text[: limit - 1] + "…"
