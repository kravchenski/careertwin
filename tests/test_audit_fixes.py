"""Regression tests for what the security audit found: each one failed before its fix."""

from __future__ import annotations

import os
import re
import stat
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


class Listener:
    """A server on loopback that records every request: if the code under test calls it, it was steered inside."""

    def __init__(self):
        self.hits: list[str] = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                outer.hits.append(self.path)
                self.send_response(404)
                self.end_headers()

            def log_message(self, *args):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f"http://127.0.0.1:{self.server.server_port}"

    def close(self):
        self.server.shutdown()


@pytest.fixture
def listener():
    server = Listener()
    yield server
    server.close()


# ---- the PDF renderer could be made to call internal addresses (HTML injection, SSRF) -------------


def hostile_graph(url):
    from careertwin.graph.schema import CareerGraph, Profile

    payload = f'Engineer <img src="{url}/internal-admin?leak=1"> <b>bold</b>'
    return CareerGraph(profile=Profile(full_name="Alex Demo", headline=payload, summary=payload, email="a@b.example"))


@pytest.mark.parametrize("template", ["modern", "classic", "minimal", "technical", "ats"])
def test_text_from_a_profile_is_escaped_in_every_template(listener, template):
    from careertwin.rendering import PdfRenderer

    html = PdfRenderer().render_to_html(hostile_graph(listener.url), template)
    assert "<img" not in html and "&lt;img" in html and "<b>bold</b>" not in html


def test_a_hostile_profile_does_not_make_the_pdf_engine_call_an_internal_address(listener, tmp_path):
    from careertwin.rendering import PdfRenderer, RenderConfig

    PdfRenderer().render(hostile_graph(listener.url), RenderConfig(template="modern", output_path=str(tmp_path / "cv.pdf")))
    assert (tmp_path / "cv.pdf").read_bytes().startswith(b"%PDF") and listener.hits == []


def test_even_unescaped_html_cannot_make_the_engine_fetch_anything(listener, tmp_path):
    """The second layer: should escaping ever be bypassed, the PDF engine still loads nothing."""
    from careertwin.rendering.safe import html

    secret = tmp_path / "secret.png"
    secret.write_bytes(b"not an image")
    page = (
        f'<html><body><img src="{listener.url}/a"><img src="file://{secret}">'
        f'<link rel="stylesheet" href="{listener.url}/style.css">'
        f'<style>@import url("{listener.url}/b.css"); div {{ background: url({listener.url}/c.png) }}</style>'
        "<div>text</div></body></html>"
    )
    html(string=page).write_pdf(str(tmp_path / "out.pdf"))
    assert listener.hits == []


def test_the_engine_may_still_use_data_urls_and_the_fonts_it_was_given(tmp_path):
    from careertwin.rendering.safe import ResourceBlocked, fetcher

    font = tmp_path / "font.ttf"
    font.write_bytes(b"font")
    fetch = fetcher([str(font)])
    assert fetch.fetch("data:text/plain;base64,aGk=").read() == b"hi" if hasattr(fetch.fetch("data:text/plain;base64,aGk="), "read") else True
    for url in (f"file://{tmp_path}/other.ttf", "file:///etc/hostname", "http://127.0.0.1/x", "https://example.com/x", "ftp://example.com/x"):
        with pytest.raises(ResourceBlocked):
            fetch.fetch(url)
    assert fetch.fetch(f"file://{font}") is not None


def test_a_page_given_by_filename_is_read_by_us_not_opened_by_the_engine(tmp_path):
    from careertwin.rendering.safe import html

    page = tmp_path / "cv.html"
    page.write_text("<html><body><p>Hello</p></body></html>", encoding="utf-8")
    html(filename=str(page)).write_pdf(str(tmp_path / "cv.pdf"))
    assert (tmp_path / "cv.pdf").read_bytes().startswith(b"%PDF")


def test_every_pdf_goes_through_the_restricted_engine():
    """No code calls weasyprint.HTML directly: a new call site cannot skip the fetcher."""
    offenders = []
    for path in (ROOT / "src").rglob("*.py"):
        if path.name == "safe.py":
            continue
        text = path.read_text()
        if re.search(r"from weasyprint import[^\n]*\bHTML\b", text) or re.search(r"weasyprint\.HTML\(", text):
            offenders.append(path.relative_to(ROOT).as_posix())
    assert offenders == []


# ---- a request body could send a real browser anywhere -------------------------------------------


@pytest.fixture
def api(monkeypatch):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    from careertwin.api.routes import boards, jobs

    app = FastAPI()
    app.include_router(jobs.router)
    app.include_router(boards.router)
    return TestClient(app)


INTERNAL = ["http://127.0.0.1:3260/mcp", "http://localhost:8000/", "file:///etc/passwd", "http://169.254.169.254/latest/meta-data/", "http://[::1]/", "http://10.0.0.5/", "javascript:alert(1)", "http://2130706433/"]


@pytest.mark.parametrize("url", INTERNAL)
@pytest.mark.parametrize("path", ["/v1/jobs/scrape-page", "/v1/jobs/fetch-description", "/v1/jobs/mcp/fetch-description"])
def test_a_browser_is_never_pointed_at_an_internal_address_by_a_request(api, monkeypatch, path, url):
    started = []

    async def forbidden(*args, **kwargs):
        started.append(args)
        raise AssertionError("a browser was started")

    monkeypatch.setattr("careertwin.jobs.career_pages.scrape_career_page", forbidden)
    monkeypatch.setattr("careertwin.jobs.scanner._fetch_job_description_crawl4ai", forbidden)
    monkeypatch.setattr("playwright.async_api.async_playwright", forbidden)
    response = api.post(path, json={"url": url})
    assert response.status_code == 422 and "not allowed" in response.json()["detail"] and started == []


@pytest.mark.parametrize("body", [{"url": 5}, {"url": ["http://example.com"]}, {"url": {"a": 1}}])
def test_a_url_that_is_not_text_is_refused_not_crashed_on(api, body):
    assert api.post("/v1/jobs/scrape-page", json=body).status_code in {422}


def test_a_name_that_resolves_inside_is_refused_before_a_browser_sees_it(monkeypatch):
    import socket

    from careertwin.net import check_public_url

    monkeypatch.setattr(socket, "getaddrinfo", lambda host, port, **kw: [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("10.1.2.3", port))])
    with pytest.raises(ValueError, match="non-public"):
        check_public_url("https://innocent.example/jobs")
    monkeypatch.setattr(socket, "getaddrinfo", lambda host, port, **kw: [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("93.184.216.34", port))])
    assert check_public_url("https://example.com/jobs") == "https://example.com/jobs"


def test_the_agent_browser_helper_refuses_an_internal_address(monkeypatch):
    from careertwin import browser

    ran = []
    monkeypatch.setattr(browser, "_agent_browser_run", lambda *a, **k: ran.append(a) or (True, "", ""))
    ok, _, error = browser._agent_browser_open("http://169.254.169.254/latest/meta-data/", session="s")
    assert not ok and "blocked" in error and ran == []
    ok, _, error = browser._agent_browser_open("--headed", session="s")  # an option is not an address
    assert not ok and ran == []


async def test_a_page_guard_aborts_requests_to_internal_addresses_and_lets_the_rest_through(monkeypatch):
    from careertwin import net

    decided = {}

    class Route:
        def __init__(self, url):
            self.request = type("R", (), {"url": url})()

        async def continue_(self):
            decided[self.request.url] = "continue"

        async def abort(self, reason):
            decided[self.request.url] = f"abort:{reason}"

    class Page:
        async def route(self, pattern, handler):
            self.handler = handler

    monkeypatch.setattr(net, "check_public_url", lambda url: url if "example.com" in url or url.startswith("data:") else (_ for _ in ()).throw(ValueError("internal")))
    page = Page()
    await net.guard_page(page)
    for url in ("https://example.com/a", "http://127.0.0.1/b", "http://redirected-to-internal.test/c", "data:text/plain,hi", "about:blank", "blob:xyz"):
        await page.handler(Route(url))
    assert decided["https://example.com/a"] == "continue" and decided["data:text/plain,hi"] == "continue"
    assert decided["about:blank"] == "continue" and decided["blob:xyz"] == "continue"
    assert decided["http://127.0.0.1/b"] == "abort:blockedbyclient" and decided["http://redirected-to-internal.test/c"] == "abort:blockedbyclient"


def test_every_browser_launch_is_guarded():
    """Each file that opens a Playwright page guards it; a new one cannot forget."""
    for path in (ROOT / "src").rglob("*.py"):
        text = path.read_text()
        if "async_playwright()" in text:
            assert text.count("guard_page(") >= text.count("async_playwright()"), path.name


# ---- regular expressions that could hold the program for minutes ---------------------------------


def finishes_within(code: str, limit: float = 10.0) -> bool:
    """Run `code` in a fresh interpreter: a backtracking regex holds the GIL, so only another process can time it."""
    import subprocess
    import sys

    try:
        subprocess.run([sys.executable, "-c", code], timeout=limit, check=True, capture_output=True)
    except subprocess.TimeoutExpired:
        return False
    return True


@pytest.mark.parametrize("hostile", ['"a." * 100_000', '"x" * 200_000', '"token=" + "a." * 50_000', '("api_key" + "." * 40) * 3_000'])
def test_redacting_a_hostile_line_is_fast(hostile):
    assert finishes_within(f"from careertwin.interview.project import redact\nredact({hostile})")


@pytest.mark.parametrize(
    ("line", "kept", "gone"),
    [
        ("API_KEY=abcd1234efgh5678", "API_KEY=", "abcd1234efgh5678"),
        ('db.password: "hunter2hunter2"', "db.password", "hunter2hunter2"),
        ("GITHUB_TOKEN = 'ghp_abcdefghijklmnopqrstuvwxyz0123'", "GITHUB_TOKEN", "abcdefghijklmnopqrstuvwxyz0123"),
        ("my-service.secret_key=zzzzzzzzzzzz", "secret_key=", "zzzzzzzzzzzz"),
        ("some_really_long_prefix_name_over_forty_characters_long_secret=abcdefgh12345678", "secret=", "abcdefgh12345678"),
        ("Authorization: Bearer abcdefghijklmnop12345", "Authorization", "abcdefghijklmnop12345"),
        ("client_secret : shhhhhhhhhhhh", "client_secret", "shhhhhhhhhhhh"),
    ],
)
def test_the_faster_redaction_still_blanks_what_it_blanked(line, kept, gone):
    from careertwin.interview.project import redact

    out = redact(line)
    assert gone not in out and "[redacted]" in out and kept in out


def test_a_posting_cannot_hold_the_experience_check_for_minutes():
    from careertwin.jobs import seniority

    assert finishes_within('from careertwin.jobs import seniority\nseniority.requires_experience_or_degree("1" * 300_000 + " years of experience")')
    assert seniority.requires_experience_or_degree("We need 5+ years of professional experience with Python")
    assert seniority.requires_experience_or_degree("3-5 yrs experience required")


def test_a_hostile_utterance_is_classified_quickly():
    for text in ('"1" * 100_000', '"word " * 40_000', '"." * 100_000'):
        assert finishes_within(f"from careertwin.interview.questions import classify, is_question\nt = {text}\nclassify(t); is_question(t, 'en')")


# ---- files and sockets other accounts could reach ---------------------------------------------------


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_the_control_socket_lives_in_a_folder_only_its_owner_can_enter(tmp_path, monkeypatch):
    from careertwin.interview import control

    monkeypatch.delenv("XDG_RUNTIME_DIR", raising=False)
    monkeypatch.setattr(control.tempfile, "gettempdir", lambda: str(tmp_path))
    path = control.socket_path()
    folder = Path(path).parent
    assert folder.name.startswith("careertwin-interview-") and stat.S_IMODE(folder.stat().st_mode) == 0o700
    assert control.socket_path() == path  # asking again is fine


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_a_folder_someone_else_prepared_in_tmp_is_refused(tmp_path, monkeypatch):
    from careertwin.interview import control

    monkeypatch.delenv("XDG_RUNTIME_DIR", raising=False)
    monkeypatch.setattr(control.tempfile, "gettempdir", lambda: str(tmp_path))
    folder = tmp_path / f"careertwin-interview-{os.getuid()}"
    folder.mkdir(mode=0o755)
    folder.chmod(0o755)  # readable by everyone: not ours to trust
    with pytest.raises(control.ControlError, match="not private"):
        control.socket_path()
    folder.rmdir()
    target = tmp_path / "elsewhere"
    target.mkdir(mode=0o700)
    folder.symlink_to(target)  # a link planted at the predictable name
    with pytest.raises(control.ControlError, match="not private"):
        control.socket_path()


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_a_runtime_directory_that_others_can_enter_is_not_used(tmp_path, monkeypatch):
    from careertwin.interview import control

    open_dir = tmp_path / "run"
    open_dir.mkdir()
    open_dir.chmod(0o755)
    monkeypatch.setenv("XDG_RUNTIME_DIR", str(open_dir))
    monkeypatch.setattr(control.tempfile, "gettempdir", lambda: str(tmp_path / "tmp"))
    (tmp_path / "tmp").mkdir()
    assert not control.socket_path().startswith(str(open_dir))


def test_the_window_does_not_start_when_its_socket_folder_is_unsafe(tmp_path, monkeypatch):
    from careertwin.interview import control

    def refuse():
        raise control.ControlError("/tmp/x is not private to you")

    monkeypatch.setattr(control, "socket_path", refuse)
    monkeypatch.setattr(control, "send", lambda *a, **k: (_ for _ in ()).throw(control.ControlError("nobody")))
    server = control.ControlServer(lambda command: None)
    assert server.start() is False and "not private" in server.error


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_settings_with_keys_are_never_readable_by_others_not_even_briefly(tmp_path, monkeypatch):
    from careertwin.interview import settings as module

    seen = []
    original = os.chmod

    def spy(path, mode, *args, **kwargs):
        seen.append(stat.S_IMODE(os.stat(path).st_mode))  # the mode the file has before chmod runs
        return original(path, mode, *args, **kwargs)

    monkeypatch.setattr(module, "DATA_DIR", tmp_path / "data")
    monkeypatch.setattr(module.os, "chmod", spy)
    value = module.InterviewSettings()
    value.gateway_key = "k" * 20
    value.save()
    assert seen and seen[0] == 0o600 and stat.S_IMODE(value.path().stat().st_mode) == 0o600


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_the_api_and_the_window_make_private_files_by_default(tmp_path):
    import subprocess
    import sys

    code = (
        "import os, sys, pathlib\n"
        "from careertwin import vault\n"
        "vault.private_umask()\n"
        "p = pathlib.Path(sys.argv[1]); p.write_text('x'); d = p.with_name('d'); d.mkdir()\n"
        "print(oct(p.stat().st_mode & 0o777), oct(d.stat().st_mode & 0o777))"
    )
    out = subprocess.run([sys.executable, "-c", code, str(tmp_path / "f.txt")], capture_output=True, text=True, check=True).stdout.split()
    assert out == ["0o600", "0o700"]
    text = (ROOT / "src/careertwin/api/app.py").read_text()
    assert "private_umask()" in text and "private_umask()" in (ROOT / "src/careertwin/interview/app.py").read_text()


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_the_security_check_reports_and_fixes_a_data_folder_others_can_enter(tmp_path, monkeypatch):
    from careertwin import security_check, settings

    data = tmp_path / "data"
    data.mkdir()
    data.chmod(0o755)
    monkeypatch.setattr(settings, "DATA_DIR", data)
    found = {item.title: item for item in security_check.audit(tmp_path)}
    item = found["The data folder can be entered by other users"]
    assert item.level == "warn" and item.fix and item.fix()
    assert stat.S_IMODE(data.stat().st_mode) == 0o700
    assert "The data folder is private to you" in {i.title for i in security_check.audit(tmp_path)}


def test_git_is_run_on_a_chosen_folder_without_its_hooks_monitor_or_prompts(tmp_path, monkeypatch):
    import subprocess

    from careertwin.interview import project

    seen = {}

    def fake(cmd, **kwargs):
        seen["cmd"], seen["env"] = cmd, kwargs.get("env", {})
        return subprocess.CompletedProcess(cmd, 0, b"a.py\0", b"")

    monkeypatch.setattr(project.subprocess, "run", fake)
    assert project._git_files(tmp_path) == ["a.py"]
    joined = " ".join(seen["cmd"])
    assert "core.fsmonitor=false" in joined and f"core.hooksPath={os.devnull}" in joined
    assert seen["env"]["GIT_TERMINAL_PROMPT"] == "0"


# ---- the pipeline and what it is built from -----------------------------------------------------------


def test_every_github_action_is_pinned_to_a_commit_and_the_token_is_read_only_by_default():
    workflows = (ROOT / ".github" / "workflows").glob("*.yml")
    for path in workflows:
        text = path.read_text()
        for line in re.findall(r"uses:\s*(\S+)", text):
            assert re.fullmatch(r"[\w./-]+@[0-9a-f]{40}", line), f"{path.name}: {line} is not pinned to a commit"
        assert re.search(r"^permissions:\n\s+contents: read", text, re.M), f"{path.name} has no read-only default"


def test_the_vulnerable_versions_are_not_allowed_back():
    """Floors from the dependency audit: below these, known advisories apply."""
    import tomllib

    declared = tomllib.loads((ROOT / "pyproject.toml").read_text())["project"]
    specs = declared["dependencies"] + [item for group in declared["optional-dependencies"].values() for item in group]
    floors = {"pypdf": (6, 21, 2), "weasyprint": (70,)}
    for name, floor in floors.items():
        spec = next(item for item in specs if item.lower().startswith(name))
        version = tuple(int(part) for part in re.search(r">=([\d.]+)", spec)[1].split("."))
        assert version >= floor, f"{spec} allows a version with known advisories"
    lock = (ROOT / "uv.lock").read_text()
    for name, floor in {"pypdf": (6, 21, 2), "weasyprint": (70, 0), "pillow": (13, 0), "urllib3": (2, 8), "certifi": (2026, 10, 18)}.items():
        locked = tuple(int(part) for part in re.search(rf'name = "{name}"\nversion = "([\d.]+)"', lock)[1].split(".")[: len(floor)])
        assert locked >= floor, f"uv.lock pins {name} below {floor}"
