"""The read only page that shows what the agents are doing (`careertwin dashboard`)."""

from __future__ import annotations

import base64
import hashlib
import re
import shutil
import subprocess
import sys
from urllib.parse import unquote

import httpx
import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from careertwin.api import dashboard, security

TOKEN = "t0ken-" + "x" * 40


@pytest.fixture
def client(monkeypatch):
    """The page behind the real security middleware, on an app of its own: the real app keeps its count
    of wrong tokens for the whole run, and these tests must not lock out the ones that need it."""
    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    for name in ("CAREERTWIN_ALLOWED_HOSTS", "CORS_ORIGINS", "CAREERTWIN_API_DOCS"):
        monkeypatch.delenv(name, raising=False)
    app = FastAPI()
    app.add_middleware(security.SecurityMiddleware)
    app.include_router(dashboard.router)

    @app.get("/v1/private")
    async def private():
        return {"cv": "secret data"}

    return TestClient(app, base_url="http://localhost")


def inline(html: str, tag: str) -> str:
    return re.search(rf"<{tag}>(.*?)</{tag}>", html, re.S)[1]


def pin(text: str) -> str:
    return "'sha256-" + base64.b64encode(hashlib.sha256(text.encode()).digest()).decode() + "'"


def test_the_page_opens_without_a_token_because_it_carries_no_data(client):
    response = client.get("/dashboard")
    assert response.status_code == 200 and response.headers["content-type"].startswith("text/html")
    assert response.headers["cache-control"] == "no-store" and response.headers["x-frame-options"] == "DENY"
    assert "CareerTwin agents" in response.text
    assert TOKEN not in response.text


def test_nothing_but_the_pinned_script_and_style_can_run_in_it(client):
    response = client.get("/dashboard")
    csp = response.headers["content-security-policy"]
    assert f"script-src {pin(inline(response.text, 'script'))}" in csp
    assert f"style-src {pin(inline(response.text, 'style'))}" in csp
    assert "default-src 'none'" in csp and "connect-src 'self'" in csp and "frame-ancestors 'none'" in csp
    assert "unsafe-inline" not in csp and "unsafe-eval" not in csp


def test_the_page_has_no_inline_handlers_style_attributes_or_html_injection():
    html = dashboard.page()
    assert not re.search(r"\son\w+\s*=", html), "an inline event handler is blocked by the policy"
    assert 'style="' not in html, "an inline style attribute is blocked by the policy"
    for risky in ("innerHTML", "outerHTML", "insertAdjacentHTML", "document.write", "eval(", "new Function"):
        assert risky not in html, f"vacancy titles and mail subjects come from outside: {risky}"
    assert not re.search(r"""(?:src|href)\s*=\s*["']https?://""", html), "the page loads nothing from outside"


@pytest.mark.skipif(shutil.which("node") is None, reason="needs node to parse the script")
def test_the_script_parses(tmp_path):
    script = tmp_path / "dashboard.js"
    script.write_text(inline(dashboard.page(), "script"), encoding="utf-8")
    result = subprocess.run(["node", "--check", str(script)], capture_output=True, text=True)
    assert result.returncode == 0, result.stderr


def test_only_the_page_and_the_health_check_are_open_and_the_real_app_serves_it(monkeypatch):
    assert security.PUBLIC_PATHS == {"/v1/health", "/dashboard"}
    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    from careertwin.api.app import app

    page = TestClient(app, base_url="http://localhost").get("/dashboard")  # a public path: no wrong token is counted
    assert page.status_code == 200 and "content-security-policy" in page.headers


def test_the_data_behind_the_page_needs_the_token(client):
    assert client.get("/v1/private").status_code == 401
    assert client.get("/v1/private", headers={"Authorization": f"Bearer {TOKEN}"}).json() == {"cv": "secret data"}
    assert "secret data" not in client.get("/dashboard").text


def test_a_page_reaching_it_through_a_dns_name_is_refused(client):
    assert client.get("/dashboard", headers={"Host": "attacker.example:8000"}).status_code == 400


def test_opening_the_page_puts_the_token_in_the_fragment_and_never_prints_it(monkeypatch):
    opened = []
    monkeypatch.setattr(dashboard.httpx, "get", lambda url, **_: httpx.Response(200, request=httpx.Request("GET", url)))
    monkeypatch.setattr(dashboard.webbrowser, "open", opened.append)
    address = dashboard.open_dashboard(base_url="http://127.0.0.1:8000/", token="a b/c+d")
    assert address == "http://127.0.0.1:8000/dashboard"
    assert len(opened) == 1 and opened[0].startswith("http://127.0.0.1:8000/dashboard#")
    assert unquote(opened[0].split("#", 1)[1]) == "a b/c+d"
    assert dashboard.open_dashboard(base_url="http://127.0.0.1:8000", token=TOKEN, browser=False) == address
    assert len(opened) == 1


def answers(monkeypatch, *replies):
    """`httpx.get` as the health check: each call takes the next reply (True answers, False refuses)."""
    calls = iter(replies)

    def get(url, **_):
        if not next(calls, True):
            raise httpx.ConnectError("refused")
        return httpx.Response(200, request=httpx.Request("GET", url))

    monkeypatch.setattr(dashboard.httpx, "get", get)


def test_it_starts_the_api_when_it_is_not_running_and_waits_for_it(monkeypatch, tmp_path):
    started = []
    answers(monkeypatch, False, False, True)
    monkeypatch.setattr(dashboard, "DATA_DIR", tmp_path)
    monkeypatch.setattr(dashboard.subprocess, "Popen", lambda command, **options: started.append((command, options)))
    monkeypatch.setattr(dashboard.time, "sleep", lambda _: None)
    dashboard.ensure_api("http://127.0.0.1:8123")
    assert len(started) == 1
    command, options = started[0]
    assert command[1:] == ["-m", "careertwin", "serve", "--port", "8123"]
    assert options["start_new_session"] is True and (tmp_path / "api.log").exists()


def test_it_says_so_instead_of_starting_an_api_it_was_told_not_to_or_on_another_machine(monkeypatch):
    answers(monkeypatch, False, False)
    monkeypatch.setattr(dashboard.subprocess, "Popen", lambda *a, **k: pytest.fail("started an API"))
    with pytest.raises(RuntimeError, match="does not answer at http://127.0.0.1:8000"):
        dashboard.open_dashboard(base_url="http://127.0.0.1:8000", token=TOKEN, start=False)
    with pytest.raises(RuntimeError, match="does not answer at http://203.0.113.7:8000"):
        dashboard.open_dashboard(base_url="http://203.0.113.7:8000", token=TOKEN)


def test_it_gives_up_when_the_started_api_never_answers(monkeypatch, tmp_path):
    answers(monkeypatch, *[False] * 200)
    monkeypatch.setattr(dashboard, "DATA_DIR", tmp_path)
    monkeypatch.setattr(dashboard.subprocess, "Popen", lambda *a, **k: None)
    clock = iter(range(0, 1000))
    monkeypatch.setattr(dashboard.time, "monotonic", lambda: next(clock))
    monkeypatch.setattr(dashboard.time, "sleep", lambda _: None)
    with pytest.raises(RuntimeError, match="was started but does not answer"):
        dashboard.ensure_api("http://127.0.0.1:8000", wait=5)


def test_the_command_prints_the_address_without_the_token(monkeypatch, capsys):
    from careertwin import cli

    monkeypatch.setenv(security.TOKEN_VARIABLE, TOKEN)
    monkeypatch.setattr(dashboard.httpx, "get", lambda url, **_: httpx.Response(200, request=httpx.Request("GET", url)))
    monkeypatch.setattr(sys, "argv", ["careertwin", "dashboard", "--no-open"])
    cli.main()
    out = capsys.readouterr().out
    assert out.strip() == "http://127.0.0.1:8000/dashboard" and TOKEN not in out


# ---- the visual rules of the page: sizes, spacing, radius, backgrounds, motion, fonts -----------------

TYPE_SCALE = {12, 14, 16, 18, 20, 24, 30, 36, 48, 60, 72, 96, 128}
SPACING = {0, 2, 4, 8, 12, 16, 24, 32, 40, 48, 64, 80, 96}
RADII = {0, 2, 4, 6, 8, 12, 16, 24, 9999}  # 9999 is rounded full
DARK_SURFACES = {"#000000", "#181818", "#1f1f1f", "#272727", "#313131", "#131209"}


def css() -> str:
    text = inline(dashboard.page(), "style")
    text = re.sub(r"url\(data:[^)]*\)", "url()", text)  # the embedded fonts
    return re.sub(r"/\*.*?\*/", "", text, flags=re.S)


def tokens() -> dict[str, str]:
    return dict(re.findall(r"(--[\w-]+):\s*([^;]+);", css()))


def declarations(*properties: str) -> list[str]:
    pattern = rf"(?:^|[;{{\s])(?:{'|'.join(properties)})\s*:\s*([^;}}]+)"
    return re.findall(pattern, css())


def pixels(value: str) -> list[int]:
    resolved = re.sub(r"var\((--[\w-]+)\)", lambda m: tokens().get(m[1], m[0]), value)
    return [int(n) for n in re.findall(r"(?<![\w.#-])(\d+)px", resolved)]


def test_every_font_size_is_a_step_of_the_type_scale():
    sizes = [px for value in declarations("font-size") for px in pixels(value)]
    sizes += [int(m) for m in re.findall(r"font:\s*[\d.]+\s+(\d+)px/\d+px", css())]
    assert sizes and set(sizes) <= TYPE_SCALE, sorted(set(sizes) - TYPE_SCALE)


def test_every_gap_padding_and_margin_comes_from_the_spacing_table():
    values = [px for value in declarations("padding", "margin", "gap", "top", "left", "border-spacing") for px in pixels(value)]
    assert values and set(values) <= SPACING, sorted(set(values) - SPACING)


def test_every_radius_is_a_tailwind_radius():
    values = [px for value in declarations("border-radius") for px in pixels(value)]
    assert values and set(values) <= RADII, sorted(set(values) - RADII)


def test_no_gradients_italics_heavy_weights_or_one_sided_borders():
    text = css()
    assert "gradient(" not in text and "italic" not in text and "oblique" not in text
    assert not re.search(r"border-(top|right|bottom|left)\s*:", text), "a border goes all the way round or not at all"
    weights = [int(w) for w in re.findall(r"font-weight:\s*(\d+)", text) if "100 900" not in w]
    weights += [int(m) for m in re.findall(r"font:\s*(\d+)\s", text)]
    assert max(weights) <= 600


def test_the_dark_surfaces_are_the_approved_ones():
    dark = re.search(r"prefers-color-scheme: dark\)\s*\{\s*:root\s*\{(.*?)\}", css(), re.S)[1]
    surfaces = {name: value for name, value in re.findall(r"(--(?:bg|panel|raised|hover|line|field)):\s*(#\w+)", dark)}
    assert set(surfaces) == {"--bg", "--panel", "--raised", "--hover", "--line", "--field"}
    assert {value.lower() for value in surfaces.values()} <= DARK_SURFACES


def test_motion_uses_the_weighted_curve_and_only_moves_transform_and_opacity():
    text = css()
    assert tokens()["--ease"] == "cubic-bezier(0.22, 1, 0.36, 1)"
    for timing in re.findall(r"(?:transition|animation)\s*:\s*([^;]+);", text):
        if timing.startswith("none"):  # switched off for reduced motion
            continue
        assert "var(--ease)" in timing, timing
        assert not re.search(r"\b(?:ease|linear|ease-in-out|ease-in|ease-out)\b", timing.replace("var(--ease)", "")), timing
    for property_list in re.findall(r"transition\s*:\s*([^;]+);", text):
        for part in property_list.split(","):
            assert part.split()[0] in {"transform", "opacity", "color", "background-color", "border-color"}, part
    for frame in re.findall(r"@keyframes\s+\w+\s*\{(.*?\})\s*\}", text, re.S):
        assert not re.search(r"\b(?:top|left|width|height|margin|padding)\s*:", frame), frame
    assert "prefers-reduced-motion: reduce" in text


def test_one_typeface_and_none_of_the_banned_ones():
    families = " ".join(declarations("font-family")) + " ".join(re.findall(r"font:[^;]*?(\"[^\"]+\"|var\(--\w+\))", css())) + tokens()["--font"] + tokens()["--mono"]
    assert "Geist" in families
    for banned in ("Inter", "Roboto", "Arial", "Open Sans", "Helvetica", "Poppins", "Manrope"):
        assert banned not in families, banned


def test_the_copy_has_no_hyphens_and_no_exclamation_marks():
    body = re.sub(r"<(script|style)>.*?</\1>", "", dashboard.page(), flags=re.S)
    body = re.sub(r"<code>.*?</code>", "", body, flags=re.S)
    text = " ".join(re.findall(r">([^<>]+)<", body))
    assert "-" not in text and "!" not in text, text


def test_the_states_every_interactive_element_needs_exist():
    text = css()
    for state in ("button:hover", "button:active", "button:focus-visible", "input:focus-visible"):
        assert state in text, state
    html = dashboard.page()
    assert 'class="skip"' in html and "aria-busy" in html and 'role="alert"' in html and "<main" in html
    assert "window.alert" not in html and "alert(" not in html
