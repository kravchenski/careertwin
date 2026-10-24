"""A link written by someone else cannot steer a fetch at this machine or its network."""

from __future__ import annotations

import re
import socket
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import httpx
import pytest

from careertwin import net
from careertwin.net import UnsafeURL, fetch_text, is_public_address, safe_async_client, validate_public_url

SRC = Path(__file__).resolve().parents[1] / "src" / "careertwin"
PUBLIC = "93.184.216.34"

BLOCKED = [
    "http://127.0.0.1/", "http://127.0.0.2:8000/", "http://localhost/", "http://localhost./", "http://sub.localhost/",
    "http://2130706433/", "http://0x7f.1/", "http://017700000001/", "http://0/", "http://[::1]/",
    "http://[::ffff:127.0.0.1]/", "http://[fd00::1]/", "http://[fe80::1]/", "http://169.254.169.254/latest/meta-data",
    "http://metadata.google.internal/", "http://100.64.0.1/", "http://10.1.2.3:8080/", "http://172.16.0.1/",
    "http://172.31.255.255/", "http://192.168.1.1/", "http://printer.local/", "http://db.internal/", "http://intranet/",
    "http://user:pass@example.com/", "ftp://example.com/", "file:///etc/passwd", "javascript:alert(1)", "http:///nohost",
    "http://example.com:notaport/", "http://host.docker.internal:3260/",
]
ALLOWED = [
    "https://example.com/jobs", "https://boards.greenhouse.io/acme", "http://8.8.8.8/", "https://jobs.lever.co/x?y=1",
    "http://172.32.0.1/", "http://[2606:4700::1111]/", "https://careers.example.co.uk:8443/apply",
]


@pytest.mark.parametrize("url", BLOCKED)
def test_a_url_that_points_inside_is_rejected(url):
    with pytest.raises(ValueError):
        validate_public_url(url)


@pytest.mark.parametrize("url", ALLOWED)
def test_a_public_url_is_accepted(url):
    assert validate_public_url(url) == url


@pytest.mark.parametrize(
    ("address", "public"),
    [
        (PUBLIC, True), ("8.8.8.8", True), ("2606:4700::1111", True), ("::ffff:8.8.8.8", True),
        ("127.0.0.1", False), ("10.0.0.5", False), ("172.20.0.1", False), ("192.168.0.1", False), ("169.254.169.254", False),
        ("100.64.0.1", False), ("0.0.0.0", False), ("224.0.0.1", False), ("::1", False), ("::ffff:127.0.0.1", False),
        ("::ffff:10.0.0.1", False), ("fc00::1", False), ("fe80::1%eth0", False), ("2002:7f00:1::", False),
        ("64:ff9b::7f00:1", False), ("not an ip", False),
    ],
)
def test_which_addresses_are_public(address, public):
    assert is_public_address(address) is public


class Recorder:
    """Stands in for the network under SafeTransport and records where the request was sent."""

    def __init__(self, monkeypatch, resolve, responses):
        self.sent, self.resolved = [], []
        responses = iter(responses)

        async def getaddrinfo(self_loop, host, port, **kwargs):
            self.resolved.append(host)
            return [(socket.AF_INET, socket.SOCK_STREAM, 6, "", (address, port)) for address in resolve[host]]

        async def parent(transport, request):
            self.sent.append(request)
            return next(responses)

        monkeypatch.setattr("asyncio.base_events.BaseEventLoop.getaddrinfo", getaddrinfo)
        monkeypatch.setattr(httpx.AsyncHTTPTransport, "handle_async_request", parent)


async def test_the_request_goes_to_the_address_that_was_checked_and_keeps_its_name(monkeypatch):
    network = Recorder(monkeypatch, {"good.example": [PUBLIC]}, [httpx.Response(200, text="hello")])
    async with safe_async_client() as client:
        assert (await client.get("https://good.example/jobs")).text == "hello"
    request = network.sent[0]
    assert request.url.host == PUBLIC  # connected to the checked address, not resolved again
    assert request.headers["host"] == "good.example"
    assert request.extensions["sni_hostname"] == "good.example"  # TLS is verified against the name


async def test_a_name_that_resolves_inside_is_not_contacted(monkeypatch):
    network = Recorder(monkeypatch, {"evil.example": ["10.0.0.5"]}, [])
    async with safe_async_client() as client:
        with pytest.raises(UnsafeURL, match="non-public"):
            await client.get("https://evil.example/")
    assert network.sent == []


async def test_one_private_answer_among_public_ones_is_enough_to_refuse(monkeypatch):
    network = Recorder(monkeypatch, {"rebind.example": [PUBLIC, "127.0.0.1"]}, [])
    async with safe_async_client() as client:
        with pytest.raises(UnsafeURL):
            await client.get("http://rebind.example/")
    assert network.sent == []


async def test_a_redirect_into_the_network_is_stopped_at_the_hop(monkeypatch):
    redirect = httpx.Response(302, headers={"location": "http://internal.example/admin"})
    network = Recorder(monkeypatch, {"good.example": [PUBLIC], "internal.example": ["192.168.1.10"]}, [redirect])
    async with safe_async_client(follow_redirects=True) as client:
        with pytest.raises(UnsafeURL):
            await client.get("https://good.example/")
    assert len(network.sent) == 1 and network.resolved == ["good.example", "internal.example"]


async def test_a_redirect_to_a_literal_internal_address_is_stopped(monkeypatch):
    redirect = httpx.Response(307, headers={"location": "http://169.254.169.254/latest/meta-data/"})
    network = Recorder(monkeypatch, {"good.example": [PUBLIC]}, [redirect])
    async with safe_async_client(follow_redirects=True) as client:
        with pytest.raises(UnsafeURL):
            await client.get("https://good.example/")
    assert len(network.sent) == 1


async def test_a_redirect_loop_ends(monkeypatch):
    loop = [httpx.Response(302, headers={"location": "https://good.example/again"}) for _ in range(20)]
    Recorder(monkeypatch, {"good.example": [PUBLIC]}, loop)
    async with safe_async_client(follow_redirects=True) as client:
        with pytest.raises(httpx.TooManyRedirects):
            await client.get("https://good.example/")


async def test_a_local_server_is_never_reached():
    hits = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            hits.append(self.path)
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"internal")

        def log_message(self, *args):
            pass

    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        url = f"http://127.0.0.1:{server.server_port}/secret"
        async with safe_async_client() as client:
            with pytest.raises(UnsafeURL):
                await client.get(url)
        with pytest.raises(ValueError):
            await fetch_text(url)
    finally:
        server.shutdown()
    assert hits == []


def test_the_synchronous_client_checks_too(monkeypatch):
    monkeypatch.setattr("socket.getaddrinfo", lambda host, port, **kw: [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("10.0.0.9", port))])
    with net.safe_client() as client, pytest.raises(UnsafeURL):
        client.get("http://sneaky.example/")


async def test_a_page_is_read_only_up_to_the_size_limit(monkeypatch):
    big = httpx.Response(200, content=b"x" * 50_000, headers={"content-type": "text/html"})
    Recorder(monkeypatch, {"good.example": [PUBLIC]}, [big])
    text = await fetch_text("https://good.example/", max_bytes=1_000)
    assert len(text) == 1_000


async def test_an_error_status_is_an_error(monkeypatch):
    Recorder(monkeypatch, {"good.example": [PUBLIC]}, [httpx.Response(404, request=httpx.Request("GET", "https://good.example/"))])
    with pytest.raises(httpx.HTTPStatusError):
        await fetch_text("https://good.example/")


async def test_the_vacancy_page_fetch_uses_the_safe_path(monkeypatch):
    from careertwin.automation import sources

    Recorder(monkeypatch, {"good.example": [PUBLIC]}, [httpx.Response(200, text="<html>vacancy</html>")])
    assert await sources.fetch("https://good.example/job/1") == "<html>vacancy</html>"
    with pytest.raises(ValueError):
        await sources.fetch("http://localhost:3260/mcp")


# Files that talk to services on this machine on purpose (the gateway, LM Studio, Ollama, SearXNG, the API).
LOCAL_ON_PURPOSE = {
    "llm.py", "vision/__init__.py", "agentic/runner.py", "agentic/client.py", "automation/browser.py",
    "automation/ranker.py", "api/routes/automation.py", "api/routes/providers.py", "rendering/trends.py",
}


def test_no_fetch_of_a_foreign_url_uses_a_plain_client_that_follows_redirects():
    """Every `follow_redirects=True` goes through the safe client, except for the SearXNG on loopback."""
    offenders = []
    for path in SRC.rglob("*.py"):
        relative = path.relative_to(SRC).as_posix()
        if relative.startswith("interview/") or relative in LOCAL_ON_PURPOSE or relative == "net.py":
            continue
        text = path.read_text()
        for match in re.finditer(r"httpx\.(?:Async)?Client\(([^)]*)\)", text):
            if "follow_redirects=True" in match[1] and "SEARXNG" not in text[match.end() : match.end() + 200]:
                offenders.append(relative)
    assert offenders == []
