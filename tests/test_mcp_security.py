"""The MCP server over HTTP is a second door to the same tools: it asks for the API's token."""

from __future__ import annotations

import json

import pytest
from fastapi.testclient import TestClient

from careertwin.agentic import mcp_server
from careertwin.api import security

TOKEN = "t0ken-" + "x" * 40
INIT = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}
HEADERS = {"content-type": "application/json", "accept": "application/json, text/event-stream"}


@pytest.fixture
def client(monkeypatch):
    for name in ("CAREERTWIN_ALLOWED_HOSTS", "CORS_ORIGINS"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    app = mcp_server.build_http_app(mcp_server.build_server())
    with TestClient(app, base_url="http://localhost:8765") as session:
        yield session


def test_the_tools_are_not_offered_without_the_token(client):
    for headers in (HEADERS, {**HEADERS, "Authorization": "Bearer wrong"}, {**HEADERS, "Authorization": TOKEN}):
        response = client.post("/mcp", headers=headers, content=json.dumps(INIT))
        assert response.status_code == 401, headers


def test_the_right_token_opens_the_door(client):
    response = client.post("/mcp", headers={**HEADERS, "Authorization": f"Bearer {TOKEN}"}, content=json.dumps(INIT))
    assert response.status_code == 200 and "careertwin" in response.text


def test_a_foreign_host_or_origin_is_refused_even_with_the_token(client):
    auth = {**HEADERS, "Authorization": f"Bearer {TOKEN}"}
    assert client.post("/mcp", headers={**auth, "Host": "evil.example"}, content=json.dumps(INIT)).status_code == 400
    assert client.post("/mcp", headers={**auth, "Origin": "http://evil.example"}, content=json.dumps(INIT)).status_code == 403


def test_listing_the_tools_needs_the_token_too(client):
    auth = {**HEADERS, "Authorization": f"Bearer {TOKEN}"}
    first = client.post("/mcp", headers=auth, content=json.dumps(INIT))
    session = first.headers.get("mcp-session-id")
    follow = {**auth, **({"Mcp-Session-Id": session} if session else {})}
    body = {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}
    assert client.post("/mcp", headers={k: v for k, v in follow.items() if k != "Authorization"}, content=json.dumps(body)).status_code == 401
    listed = client.post("/mcp", headers=follow, content=json.dumps(body))
    assert listed.status_code == 200 and "submit_application" in listed.text


def test_the_server_will_not_listen_beyond_the_loopback_unless_asked(monkeypatch):
    started = []
    monkeypatch.setattr("uvicorn.run", lambda *args, **kwargs: started.append((args, kwargs)))
    with pytest.raises(SystemExit, match="Refusing to listen"):
        mcp_server.main(["--transport", "http", "--host", "0.0.0.0"])
    assert started == []
    mcp_server.main(["--transport", "http", "--host", "0.0.0.0", "--allow-remote"])
    assert started and started[0][1]["host"] == "0.0.0.0"
    mcp_server.main(["--transport", "http"])
    assert started[-1][1]["host"] == "127.0.0.1"


def test_the_cli_passes_the_remote_flag_on(monkeypatch):
    import sys

    from careertwin import cli

    seen = []
    monkeypatch.setattr("careertwin.agentic.mcp_server.main", lambda argv: seen.append(argv))
    monkeypatch.setattr(sys, "argv", ["careertwin", "mcp", "--transport", "http", "--allow-remote"])
    cli.main()
    assert "--allow-remote" in seen[0]
