"""The API is closed by default: a token, a known Host, no foreign Origin, bounded bodies and rates."""

from __future__ import annotations

import argparse
import os
import stat
from pathlib import Path

import httpx
import pytest
from fastapi import FastAPI, Request
from fastapi.testclient import TestClient

from careertwin import vault
from careertwin.api import security
from careertwin.api.routes import files

TOKEN = "t0ken-" + "x" * 40
AUTH = {"Authorization": f"Bearer {TOKEN}"}
ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def outputs(tmp_path, monkeypatch):
    folder = tmp_path / "outputs"
    (folder / "applications" / "job1" / "rev1").mkdir(parents=True)
    (folder / "applications" / "job1" / "rev1" / "cv.pdf").write_bytes(b"%PDF-1.4 cv")
    (folder / "applications" / "job1" / "rev1" / "profile-snapshot.md").write_text("# whole ABOUT.md")
    (folder / "applications" / "job1" / "rev1" / "job.json").write_text("{}")
    (folder / ".hidden.pdf").write_bytes(b"x")
    (folder / "CareerTwin-applications.xlsx").write_bytes(b"PK")
    monkeypatch.setattr(files, "OUTPUT_DIR", folder)
    return folder


@pytest.fixture
def client(monkeypatch, outputs):
    for name in ("CAREERTWIN_ALLOWED_HOSTS", "CORS_ORIGINS", "CAREERTWIN_API_DOCS"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    app = FastAPI()
    app.add_middleware(security.SecurityMiddleware)

    @app.get("/v1/health")
    async def health():
        return {"status": "ok"}

    @app.get("/v1/private")
    async def private():
        return {"cv": "secret data"}

    @app.post("/v1/echo")
    async def echo(request: Request):
        return {"size": len(await request.body())}

    app.include_router(files.router)
    return TestClient(app, base_url="http://localhost")


def test_only_the_health_check_is_open(client):
    assert client.get("/v1/health").json() == {"status": "ok"}
    for path in ("/v1/private", "/outputs/CareerTwin-applications.xlsx", "/openapi.json", "/docs"):
        assert client.get(path).status_code == 401, path


def test_a_wrong_or_malformed_token_is_refused(client):
    for header in ("Bearer nope", "Basic " + TOKEN, TOKEN, "Bearer", "Bearer  "):
        response = client.get("/v1/private", headers={"Authorization": header})
        assert response.status_code == 401 and response.headers["www-authenticate"] == "Bearer"
        assert "secret data" not in response.text


def test_the_right_token_is_accepted(client):
    assert client.get("/v1/private", headers=AUTH).json() == {"cv": "secret data"}


def test_a_page_that_reaches_the_api_through_a_dns_name_is_refused_even_with_the_token(client, monkeypatch):
    response = client.get("/v1/private", headers={**AUTH, "Host": "attacker.example:8000"})
    assert response.status_code == 400
    monkeypatch.setenv("CAREERTWIN_ALLOWED_HOSTS", "attacker.example")
    assert client.get("/v1/private", headers={**AUTH, "Host": "attacker.example:8000"}).status_code == 200


@pytest.mark.parametrize("host", ["localhost:8000", "127.0.0.1", "[::1]:8000", "LOCALHOST"])
def test_loopback_names_are_accepted(client, host):
    assert client.get("/v1/private", headers={**AUTH, "Host": host}).status_code == 200


def test_a_request_from_another_origin_is_refused(client, monkeypatch):
    assert client.get("/v1/private", headers={**AUTH, "Origin": "http://evil.example"}).status_code == 403
    assert client.get("/v1/private", headers={**AUTH, "Origin": "null"}).status_code == 403
    assert client.get("/v1/private", headers={**AUTH, "Origin": "http://localhost"}).status_code == 200
    monkeypatch.setenv("CORS_ORIGINS", "https://panel.example")
    assert client.get("/v1/private", headers={**AUTH, "Origin": "https://panel.example"}).status_code == 200


def test_repeated_wrong_tokens_lock_the_client_out(client):
    for _ in range(security.FAILURE_LIMIT):
        assert client.get("/v1/private", headers={"Authorization": "Bearer wrong"}).status_code == 401
    locked = client.get("/v1/private", headers=AUTH)
    assert locked.status_code == 429 and "retry-after" in locked.headers
    assert client.get("/v1/health").status_code == 200  # the liveness check stays reachable


def test_too_many_requests_are_refused(client, monkeypatch):
    monkeypatch.setattr(security, "RATE_LIMIT", 5)
    codes = [client.get("/v1/private", headers=AUTH).status_code for _ in range(8)]
    assert codes[:5] == [200] * 5 and set(codes[5:]) == {429}


def test_a_large_body_is_refused_whether_or_not_its_length_is_declared(client, monkeypatch):
    monkeypatch.setattr(security, "MAX_BODY", 100)
    assert client.post("/v1/echo", headers=AUTH, content=b"a" * 50).json() == {"size": 50}
    assert client.post("/v1/echo", headers=AUTH, content=b"a" * 500).status_code == 413
    chunks = (b"a" * 60 for _ in range(5))  # no Content-Length: counted while it is read
    assert client.post("/v1/echo", headers=AUTH, content=chunks).status_code == 413


def test_every_reply_forbids_caching_sniffing_and_framing(client):
    for response in (client.get("/v1/health"), client.get("/v1/private", headers=AUTH), client.get("/v1/private")):
        assert response.headers["cache-control"] == "no-store"
        assert response.headers["x-content-type-options"] == "nosniff"
        assert response.headers["x-frame-options"] == "DENY"
        assert response.headers["referrer-policy"] == "no-referrer"
        assert "frame-ancestors 'none'" in response.headers["content-security-policy"]


def test_documents_are_served_to_the_token_holder_as_downloads(client):
    response = client.get("/outputs/applications/job1/rev1/cv.pdf", headers=AUTH)
    assert response.status_code == 200 and response.content.startswith(b"%PDF")
    assert response.headers["content-disposition"].startswith("attachment")
    assert client.get("/outputs/CareerTwin-applications.xlsx", headers=AUTH).status_code == 200


@pytest.mark.parametrize(
    "path",
    [
        "applications/job1/rev1/profile-snapshot.md",  # the whole ABOUT.md
        "applications/job1/rev1/job.json",  # not a served type
        ".hidden.pdf",
        "applications/job1/../../../outside.pdf",
        "../outside.pdf",
        "applications/missing.pdf",
        "applications",
    ],
)
def test_nothing_else_under_outputs_is_served(client, path):
    assert client.get("/outputs/" + path, headers=AUTH).status_code == 404


def test_a_path_cannot_leave_the_outputs_folder(outputs, tmp_path):
    (tmp_path / "outside.pdf").write_bytes(b"%PDF outside")
    (outputs / "link.pdf").symlink_to(tmp_path / "outside.pdf")
    for path in ("../outside.pdf", "link.pdf", "a/../../outside.pdf", "a\\..\\outside.pdf", "x\0.pdf", "/etc/passwd"):
        with pytest.raises(Exception) as error:
            files.locate(path)
        assert getattr(error.value, "status_code", None) == 404, path


def test_the_server_makes_a_private_token_once_and_keeps_it(monkeypatch, tmp_path):
    monkeypatch.delenv(security.TOKEN_VARIABLE, raising=False)
    monkeypatch.setattr(security, "token_file", lambda: tmp_path / "data" / "api-token")
    first = security.server_token()
    assert len(first) >= 43 and os.environ[security.TOKEN_VARIABLE] == first
    path = tmp_path / "data" / "api-token"
    if os.name == "posix":
        assert stat.S_IMODE(path.stat().st_mode) == 0o600
    monkeypatch.delenv(security.TOKEN_VARIABLE)
    assert security.server_token() == first and security.client_token() == first
    assert first not in vault.redact(f"token {first}")  # known to the redactor once it exists


def test_a_short_token_from_the_environment_is_refused(monkeypatch):
    monkeypatch.setenv(security.TOKEN_VARIABLE, "short")
    with pytest.raises(vault.SecretError, match="at least"):
        security.server_token()


def test_the_real_app_is_closed(monkeypatch):
    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    monkeypatch.delenv("CAREERTWIN_API_DOCS", raising=False)
    from careertwin.api.app import app

    assert app.docs_url is None and app.openapi_url is None
    paths = {getattr(route, "path", "") for route in app.routes}
    assert not any(path in {"/screenshots", "/screenshots/{path:path}"} for path in paths)
    client = TestClient(app, base_url="http://localhost")  # no `with`: the engine is not started
    for path in ("/", "/v1/automation/settings", "/v1/automation/jobs", "/v1/profile/default", "/outputs/x.pdf", "/screenshots/x.png"):
        assert client.get(path).status_code == 401, path
    assert client.get("/v1/health").json()["status"] == "ok"


def test_the_agent_client_sends_the_token_and_explains_a_refusal(monkeypatch):
    from careertwin.agentic.client import ApiError, CareerTwinClient

    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    seen = []

    def handler(request: httpx.Request) -> httpx.Response:
        seen.append(request.headers.get("authorization"))
        return httpx.Response(200 if request.url.path == "/ok" else 401, json={"detail": "x"})

    client = CareerTwinClient(base_url="http://localhost", transport=httpx.MockTransport(handler))
    import asyncio

    assert asyncio.run(client.get("/ok")) == {"detail": "x"} and seen == [f"Bearer {TOKEN}"]
    with pytest.raises(ApiError, match="CAREERTWIN_API_TOKEN"):
        asyncio.run(client.get("/other"))


def test_serving_beyond_the_loopback_must_be_asked_for():
    from careertwin.cli import _run_service

    args = argparse.Namespace(host="0.0.0.0", port=8000, reload=False, allow_remote=False)
    with pytest.raises(SystemExit, match="Refusing to listen"):
        _run_service("serve", args)


def test_the_container_and_the_start_script_listen_on_the_loopback_with_a_protected_desktop():
    supervisor = (ROOT / "docker" / "supervisord.conf").read_text()
    assert "0.0.0.0" not in supervisor and "-nopw" not in supervisor
    assert "--host 127.0.0.1" in supervisor and "-localhost" in supervisor and "ENV_VNC_PASSWORD" in supervisor
    compose = (ROOT / "docker-compose.yml").read_text()
    assert "no-new-privileges" in compose and "VNC_PASSWORD:?" in compose
    start = (ROOT / "start.sh").read_text()
    assert "Refusing to listen" in start and 'BACKEND_HOST:-127.0.0.1' in start
