"""The page that shows what the agents are doing and lets the candidate move them on.

The page itself carries no data, so it is the one route besides the health check that needs no token:
a browser cannot attach a header to a navigation. Every number on it is fetched from the API with the
token, which the page takes from the address fragment (`careertwin dashboard` opens it that way) or
asks for. Its script and style are pinned by hash in the Content-Security-Policy, so nothing else can
run in it, and it may talk to this origin only. Its buttons queue the steps that stay on this computer
(rank, prepare, fill, skip): sending an application or an email is not among them.
"""

from __future__ import annotations

import base64
import functools
import hashlib
import re
import subprocess
import sys
import time
import webbrowser
from pathlib import Path
from urllib.parse import quote, urlparse

import httpx
from fastapi import APIRouter
from fastapi.responses import HTMLResponse

from careertwin.settings import DATA_DIR

PAGE = Path(__file__).with_name("dashboard.html")
FONTS = Path(__file__).with_name("fonts")
PATH = "/dashboard"
EMBEDDED = {  # placeholder in the page -> font file: the page loads nothing from outside
    "{{GEIST_LATIN}}": "geist-latin.woff2",
    "{{GEIST_LATIN_EXT}}": "geist-latin-ext.woff2",
    "{{GEIST_MONO_LATIN}}": "geist-mono-latin.woff2",
}
router = APIRouter()


@functools.cache
def page() -> str:
    """The page with its fonts inlined as data."""
    html = PAGE.read_text(encoding="utf-8")
    for placeholder, name in EMBEDDED.items():
        html = html.replace(placeholder, base64.b64encode((FONTS / name).read_bytes()).decode())
    return html


def _pin(html: str, tag: str) -> str:
    body = re.search(rf"<{tag}>(.*?)</{tag}>", html, re.S)
    if body is None:
        raise ValueError(f"the dashboard has no <{tag}>")
    return "'sha256-" + base64.b64encode(hashlib.sha256(body[1].encode()).digest()).decode() + "'"


def policy(html: str) -> str:
    return "; ".join(
        [
            "default-src 'none'",
            f"script-src {_pin(html, 'script')}",
            f"style-src {_pin(html, 'style')}",
            "font-src data:",
            "img-src data:",
            "connect-src 'self'",
            "base-uri 'none'",
            "form-action 'none'",
            "frame-ancestors 'none'",
        ]
    )


@router.get(PATH, include_in_schema=False)
async def dashboard() -> HTMLResponse:
    html = page()
    return HTMLResponse(html, headers={"Content-Security-Policy": policy(html)})


def _answers(base: str) -> bool:
    try:
        httpx.get(f"{base}/v1/health", timeout=3).raise_for_status()
    except httpx.HTTPError:
        return False
    return True


def ensure_api(base_url: str, *, start: bool = True, wait: float = 30.0) -> None:
    """Make sure the API answers. When it does not and the address is this computer's loopback, start
    one in the background (its output goes to `data/api.log`). Raises RuntimeError otherwise."""
    base = base_url.rstrip("/")
    if _answers(base):
        return
    parts = urlparse(base)
    if not start or parts.hostname not in {"127.0.0.1", "localhost", "::1"}:
        raise RuntimeError(f"The API does not answer at {base}: start it with `careertwin serve`")
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    with open(DATA_DIR / "api.log", "ab") as log:
        subprocess.Popen(
            [sys.executable, "-m", "careertwin", "serve", "--port", str(parts.port or 8000)],
            stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True,
        )
    deadline = time.monotonic() + wait
    while time.monotonic() < deadline:
        if _answers(base):
            return
        time.sleep(0.5)
    raise RuntimeError(f"The API was started but does not answer at {base}: see {DATA_DIR / 'api.log'}")


def open_dashboard(*, base_url: str, token: str, browser: bool = True, start: bool = True) -> str:
    """Open the page in the browser, signed in; return the address. Starts the API if it is not running.

    The token goes in the fragment: it is never sent to the server, and the page removes it from the
    address bar at once. It is not printed."""
    base = base_url.rstrip("/")
    ensure_api(base, start=start)
    if browser:
        webbrowser.open(f"{base}{PATH}#{quote(token, safe='')}" if token else f"{base}{PATH}")
    return f"{base}{PATH}"
