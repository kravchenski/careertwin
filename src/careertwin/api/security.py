"""Who may call the API: a bearer token, a known Host, no foreign Origin, bounded bodies and rates.

The API holds a CV, an ABOUT.md and the power to send an application, so it is closed by default:

* every route but `/v1/health` needs `Authorization: Bearer <token>`; the token is
  `CAREERTWIN_API_TOKEN`, or a random one written once to `data/api-token` (mode 0600);
* the Host header must be a loopback name (or one in `CAREERTWIN_ALLOWED_HOSTS`), which stops a web
  page from reaching the API through a DNS name that points at 127.0.0.1;
* a browser request from another origin is refused;
* a body larger than `MAX_BODY`, too many requests, and repeated wrong tokens are refused;
* every reply says that it must not be cached, sniffed or framed.
"""

from __future__ import annotations

import hmac
import json
import os
import secrets
import time
from collections import defaultdict, deque
from pathlib import Path

from starlette.types import ASGIApp, Message, Receive, Scope, Send

from careertwin import vault
from careertwin.settings import DATA_DIR

TOKEN_VARIABLE = "CAREERTWIN_API_TOKEN"
MIN_TOKEN_LENGTH = 32
PUBLIC_PATHS = frozenset({"/v1/health"})
DOC_PATHS = frozenset({"/docs", "/redoc", "/openapi.json", "/docs/oauth2-redirect"})
LOOPBACK_HOSTS = frozenset({"localhost", "127.0.0.1", "::1"})
MAX_BODY = 12 * 1024 * 1024
RATE_LIMIT, RATE_WINDOW = 300, 10.0  # requests per client per window
FAILURE_LIMIT, FAILURE_WINDOW = 10, 60.0  # wrong tokens per client per window before a lockout


def token_file() -> Path:
    return DATA_DIR / "api-token"


def client_token() -> str:
    """The token a client sends: the variable, else the file the server wrote; "" when there is none."""
    value = os.getenv(TOKEN_VARIABLE, "")
    if value:
        return value
    path = token_file()
    return path.read_text().strip() if path.is_file() else ""


def server_token() -> str:
    """The token the server checks. Creates the file on first use and makes the value known to `vault`
    so it is masked in logs."""
    value = os.getenv(TOKEN_VARIABLE, "")
    if value and len(value) < MIN_TOKEN_LENGTH:
        raise vault.SecretError(f"{TOKEN_VARIABLE} must be at least {MIN_TOKEN_LENGTH} characters")
    if not value:
        path = token_file()
        if not path.is_file():
            path.parent.mkdir(parents=True, exist_ok=True)
            descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "w") as stream:
                stream.write(secrets.token_urlsafe(32) + "\n")
        vault.fix_file(path)
        value = path.read_text().strip()
        os.environ[TOKEN_VARIABLE] = value  # so redaction knows it
    return value


def allowed_hosts() -> set[str]:
    extra = {item.strip().lower() for item in os.getenv("CAREERTWIN_ALLOWED_HOSTS", "").split(",") if item.strip()}
    return set(LOOPBACK_HOSTS) | extra


def host_name(header: str) -> str:
    """`example.com:8000` -> `example.com`; `[::1]:8000` -> `::1`."""
    header = header.strip().lower()
    if header.startswith("["):
        return header[1 : header.find("]")] if "]" in header else header
    return header.rsplit(":", 1)[0] if header.count(":") == 1 else header


def docs_enabled() -> bool:
    return os.getenv("CAREERTWIN_API_DOCS", "").lower() in {"1", "true", "yes"}


def cors_origins() -> list[str]:
    return [item.strip() for item in os.getenv("CORS_ORIGINS", "").split(",") if item.strip()]


class BodyTooLarge(Exception):
    pass


class Window:
    """Events per key in the last `span` seconds."""

    def __init__(self, span: float):
        self.span, self.seen = span, defaultdict(deque)

    def hit(self, key: str, now: float) -> int:
        queue = self.seen[key]
        queue.append(now)
        while queue and queue[0] < now - self.span:
            queue.popleft()
        return len(queue)

    def count(self, key: str, now: float) -> int:
        queue = self.seen[key]
        while queue and queue[0] < now - self.span:
            queue.popleft()
        return len(queue)


HEADERS = [
    (b"cache-control", b"no-store"),
    (b"x-content-type-options", b"nosniff"),
    (b"referrer-policy", b"no-referrer"),
    (b"x-frame-options", b"DENY"),
    (b"cross-origin-resource-policy", b"same-origin"),
]
API_POLICY = (b"content-security-policy", b"default-src 'none'; frame-ancestors 'none'")


class SecurityMiddleware:
    def __init__(self, app: ASGIApp):
        self.app = app
        self.requests, self.failures = Window(RATE_WINDOW), Window(FAILURE_WINDOW)
        server_token()  # fail at start, not on the first request, when the token is unusable

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] not in {"http", "websocket"}:
            await self.app(scope, receive, send)
            return
        headers = {key.decode("latin-1").lower(): value.decode("latin-1") for key, value in scope["headers"]}
        path = scope["path"]
        client = (scope.get("client") or ("unknown", 0))[0]
        now = time.monotonic()

        refusal = self.refuse(headers, path, client, now)
        if refusal:
            status, detail, extra = refusal
            await self.reply(scope, send, status, detail, extra, path)
            return

        started = False
        received = 0

        async def counted() -> Message:
            nonlocal received
            message = await receive()
            if message["type"] == "http.request":
                received += len(message.get("body", b""))
                if received > MAX_BODY:
                    raise BodyTooLarge
            return message

        async def secured(message: Message) -> None:
            nonlocal started
            if message["type"] == "http.response.start":
                started = True
                present = {key.lower() for key, _ in message.get("headers", [])}
                extra = [pair for pair in HEADERS if pair[0] not in present]
                if not (docs_enabled() and path in DOC_PATHS) and API_POLICY[0] not in present:
                    extra.append(API_POLICY)
                message = {**message, "headers": [*message.get("headers", []), *extra]}
            await send(message)

        try:
            await self.app(scope, counted if scope["type"] == "http" else receive, secured)
        except BodyTooLarge:
            if not started:
                await self.reply(scope, send, 413, "The request body is too large", [], path)

    def refuse(self, headers: dict[str, str], path: str, client: str, now: float):
        if host_name(headers.get("host", "")) not in allowed_hosts():
            return 400, "Invalid Host header", []
        origin = headers.get("origin")
        if origin and origin not in cors_origins() and host_name(origin.split("://", 1)[-1]) != host_name(headers.get("host", "")):
            return 403, "Cross-origin requests are not allowed", []
        if self.requests.hit(client, now) > RATE_LIMIT:
            return 429, "Too many requests", [(b"retry-after", str(int(RATE_WINDOW)).encode())]
        if path in PUBLIC_PATHS or (docs_enabled() and path in DOC_PATHS):
            return None
        if self.failures.count(client, now) >= FAILURE_LIMIT:
            return 429, "Too many wrong tokens; wait a minute", [(b"retry-after", str(int(FAILURE_WINDOW)).encode())]
        scheme, _, supplied = headers.get("authorization", "").partition(" ")
        if scheme.lower() != "bearer" or not hmac.compare_digest(supplied.strip().encode(), server_token().encode()):
            self.failures.hit(client, now)
            return 401, "Missing or invalid API token", [(b"www-authenticate", b"Bearer")]
        return None

    async def reply(self, scope: Scope, send: Send, status: int, detail: str, extra: list, path: str) -> None:
        if scope["type"] == "websocket":
            await send({"type": "websocket.close", "code": 1008})
            return
        body = json.dumps({"detail": detail}).encode()
        headers = [(b"content-type", b"application/json"), (b"content-length", str(len(body)).encode()), *HEADERS, API_POLICY, *extra]
        await send({"type": "http.response.start", "status": status, "headers": headers})
        await send({"type": "http.response.body", "body": body})
