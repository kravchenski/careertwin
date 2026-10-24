"""MCP server exposing the CareerTwin tool catalogue.

Register it with any MCP client, including the coding agents that run on a
Switchyard gateway:

    claude mcp add careertwin -- uv run careertwin-mcp
    claude mcp add --transport http careertwin http://127.0.0.1:8765/mcp
"""

from __future__ import annotations

import argparse

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from careertwin.agentic.prompts import SYSTEM_PROMPT
from careertwin.agentic.registry import all_tools


def build_server(*, host: str = "127.0.0.1", port: int = 8765) -> FastMCP:
    server = FastMCP("careertwin", instructions=SYSTEM_PROMPT, host=host, port=port)
    for item in all_tools():
        server.add_tool(
            item.fn,
            name=item.name,
            description=item.description,
            annotations=ToolAnnotations(
                readOnlyHint=not item.mutating,
                destructiveHint=item.outward,
                openWorldHint=item.outward,
            ),
        )
    return server


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="CareerTwin MCP server")
    parser.add_argument("--transport", choices=["stdio", "http"], default="stdio")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--api-url", help="CareerTwin API base URL (CAREERTWIN_API_URL)")
    parser.add_argument("--list", action="store_true", help="Print the tools and exit")
    args = parser.parse_args(argv)
    if args.list:
        for item in all_tools():
            kind = "employer" if item.outward else "local" if item.mutating else "read"
            print(f"{item.name:<28}{kind:<10}{item.summary}")
        return
    if args.api_url:
        import os

        os.environ["CAREERTWIN_API_URL"] = args.api_url
    server = build_server(host=args.host, port=args.port)
    server.run(transport="stdio" if args.transport == "stdio" else "streamable-http")


if __name__ == "__main__":
    main()
