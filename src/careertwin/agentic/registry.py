"""Tool catalogue shared by the MCP server and the Switchyard agent runner."""

from __future__ import annotations

import inspect
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Any

from mcp.server.fastmcp.tools import Tool as _SchemaTool

ToolFn = Callable[..., Awaitable[Any]]


@dataclass(frozen=True)
class AgentTool:
    name: str
    description: str
    parameters: dict[str, Any]
    fn: ToolFn
    mutating: bool = False
    outward: bool = False
    summary: str = ""
    impl: _SchemaTool | None = None

    async def call(self, arguments: dict[str, Any]) -> Any:
        """Validate `arguments` against the tool schema and run the function."""
        assert self.impl is not None
        return await self.impl.run(arguments, convert_result=False)


_REGISTRY: dict[str, AgentTool] = {}


def tool(
    summary: str, *, mutating: bool = False, outward: bool = False
) -> Callable[[ToolFn], ToolFn]:
    """Register an async function as an agent tool.

    `summary` is the one-line label shown in listings. `mutating` tools change local
    CareerTwin state. `outward` tools act on a third party (an application form) and are
    blocked unless explicitly allowed.
    """

    def decorate(fn: ToolFn) -> ToolFn:
        schema = _SchemaTool.from_function(fn)
        _REGISTRY[fn.__name__] = AgentTool(
            name=fn.__name__,
            description=" ".join(inspect.cleandoc(fn.__doc__ or "").split()),
            parameters=schema.parameters,
            fn=fn,
            impl=schema,
            mutating=mutating,
            outward=outward,
            summary=summary,
        )
        return fn

    return decorate


def all_tools() -> list[AgentTool]:
    from careertwin.agentic import tools  # noqa: F401  (registers the catalogue)

    return list(_REGISTRY.values())


def get_tool(name: str) -> AgentTool:
    all_tools()
    if name not in _REGISTRY:
        raise KeyError(f"Unknown tool {name!r}")
    return _REGISTRY[name]


def openai_tool_specs() -> list[dict[str, Any]]:
    return [
        {
            "type": "function",
            "function": {
                "name": item.name,
                "description": item.description,
                "parameters": item.parameters,
            },
        }
        for item in all_tools()
    ]
