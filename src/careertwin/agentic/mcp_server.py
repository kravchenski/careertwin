"""MCP server exposing the CareerTwin tool catalogue.

Register it with any MCP client, including the coding agents that run on a
Switchyard gateway:

    claude mcp add careertwin -- uv run careertwin-mcp
    claude mcp add --transport http careertwin http://127.0.0.1:8765/mcp \\
        --header "Authorization: Bearer $(cat data/api-token)"

Over HTTP the server asks for the same bearer token as the API (see docs/SECURITY.md): it holds that token and
runs the tools with it, so an unauthenticated listener would be a way around the API's own gate. It listens on
the loopback only unless `--allow-remote` is given.
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


def build_http_app(server: FastMCP):
    """The server's streamable-HTTP app behind the API's gate: token, known Host, no foreign Origin, limits."""
    from careertwin.api.security import SecurityMiddleware

    app = server.streamable_http_app()
    app.add_middleware(SecurityMiddleware)
    return app


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="CareerTwin MCP server")
    parser.add_argument("--transport", choices=["stdio", "http"], default="stdio")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--api-url", help="CareerTwin API base URL (CAREERTWIN_API_URL)")
    parser.add_argument("--allow-remote", action="store_true", help="Listen beyond the loopback (put TLS in front of it)")
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
    if args.transport == "stdio":
        server.run(transport="stdio")
        return
    if args.host not in {"127.0.0.1", "localhost", "::1"} and not args.allow_remote:
        raise SystemExit(
            f"Refusing to listen on {args.host}: the tools can send applications. "
            "Use 127.0.0.1, or pass --allow-remote behind a TLS reverse proxy."
        )
    import uvicorn

    uvicorn.run(build_http_app(server), host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
