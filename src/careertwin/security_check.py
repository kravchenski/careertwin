"""`careertwin security`: what is protected, what is not, and what to do about it.

It reads the environment, the files that hold secrets and the saved settings. It never prints a secret,
only the name of a variable.
"""

from __future__ import annotations

import os
import subprocess
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

from careertwin import vault
from careertwin.automation import guard
from careertwin.settings import REPO_ROOT

ICONS = {"ok": "✓", "info": "·", "warn": "!", "fail": "✗"}


@dataclass
class Finding:
    level: str  # ok | info | warn | fail
    title: str
    detail: str = ""
    fix: Callable[[], bool] | None = None  # a safe automatic fix, when there is one


def _ignored(root: Path, path: str) -> bool | None:
    """True when git ignores the path, False when it would be committed, None outside a repository."""
    try:
        result = subprocess.run(["git", "-C", str(root), "check-ignore", "-q", path], capture_output=True, timeout=10)
    except (OSError, subprocess.SubprocessError):
        return None
    return {0: True, 1: False}.get(result.returncode)


def _loopback(url: str) -> bool:
    host = (urlsplit(url).hostname or "").lower()
    return host in {"localhost", "127.0.0.1", "::1"}


def audit(root: Path | None = None) -> list[Finding]:
    root = root or REPO_ROOT
    found: list[Finding] = []
    add = found.append

    # -- files that hold secrets
    for path in dict.fromkeys([root / ".env", Path.cwd() / ".env"]):
        if path.exists():
            problems = vault.file_problems(path)
            add(Finding("warn", f"{path.name} can be read by other users", problems[0], lambda p=path: vault.fix_file(p)) if problems else Finding("ok", f"{path} is private to you"))
    from careertwin.profile.context import context_path

    about = context_path()
    if about.exists():
        problems = vault.file_problems(about)
        add(Finding("warn", "ABOUT.md can be read by other users", problems[0], lambda: vault.fix_file(about)) if problems else Finding("ok", "ABOUT.md is private to you"))
    from careertwin.api.security import token_file

    token = token_file()
    if token.exists():
        problems = vault.file_problems(token)
        add(Finding("fail", "The API token file can be read by other users", problems[0], lambda: vault.fix_file(token)) if problems else Finding("ok", "The API token file is private to you"))
    for private in (".env", "ABOUT.md", "data/"):
        ignored = _ignored(root, private)
        if ignored is False:
            add(Finding("fail", f"{private} is not in .gitignore", "It would be committed with `git add .`"))
        elif ignored:
            add(Finding("ok", f"{private} is ignored by git"))

    # -- the secrets themselves
    names = vault.secret_names()
    add(Finding("info", f"{len(names)} secret(s) are set: " + (", ".join(names) or "none")))
    if weak := vault.weak_secrets():
        add(Finding("warn", "Weak or placeholder secrets", ", ".join(weak) + ": replace them with long random values"))
    supplied = os.getenv("CAREERTWIN_API_TOKEN", "")
    if supplied and len(supplied) < 32:
        add(Finding("fail", "CAREERTWIN_API_TOKEN is shorter than 32 characters", "Use `python -c \"import secrets; print(secrets.token_urlsafe(32))\"`"))

    # -- the network
    gateway = os.getenv("SWITCHYARD_URL", "http://127.0.0.1:3260")
    if _loopback(gateway):
        add(Finding("ok", "The Switchyard gateway is on this computer"))
    elif urlsplit(gateway).scheme != "https":
        add(Finding("fail", "SWITCHYARD_URL is a remote address over plain HTTP", "Your profile and answers would cross the network unencrypted"))
    else:
        add(Finding("info", "The Switchyard gateway is remote (HTTPS)"))
    origins = [item.strip() for item in os.getenv("CORS_ORIGINS", "").split(",") if item.strip()]
    if "*" in origins:
        add(Finding("fail", "CORS_ORIGINS allows every site", "Any web page could talk to your API"))
    elif origins:
        add(Finding("info", "CORS_ORIGINS: " + ", ".join(origins)))
    hosts = [item.strip() for item in os.getenv("CAREERTWIN_ALLOWED_HOSTS", "").split(",") if item.strip()]
    if hosts:
        add(Finding("warn", "The API answers to names besides localhost", ", ".join(hosts) + ": put TLS in front of it"))
    if os.getenv("CAREERTWIN_API_DOCS", "").lower() in {"1", "true", "yes"}:
        add(Finding("warn", "The API docs page is on", "It cannot send the token; turn it off when you are done"))

    # -- what the agents may do
    if guard.stopped():
        add(Finding("info", "The kill switch is on (data/STOP)", "Nothing is submitted or sent until you remove it"))
    for variable, what in (("CAREERTWIN_AGENT_ALLOW_SUBMIT", "submit forms"), ("CAREERTWIN_AGENT_ALLOW_MAIL", "send email")):
        if os.getenv(variable) == "1":
            add(Finding("info", f"A chat agent may {what} after you confirm in the chat", variable))
    if os.getenv("ACCOUNT_PASSWORD"):
        domains = sorted(guard.login_domains())
        add(Finding("ok" if domains else "info", "The account password may be typed on: " + (", ".join(domains) if domains else "nowhere (CAREERTWIN_LOGIN_DOMAINS is empty)")))
    if os.getenv("MAIL_PASSWORD"):
        add(Finding("info", "Mail is configured; reading is read-only and sending needs your approval" + (" (dry run)" if os.getenv("MAIL_DRY_RUN") else "")))

    # -- the saved settings
    try:
        from careertwin.automation.store import Store

        settings = Store().settings()
        if settings.auto_submit:
            add(Finding("warn", "auto_submit is on", "Applications are sent without asking you"))
        if settings.mail_auto_send:
            add(Finding("warn", "mail_auto_send is on", "Email drafts are sent without asking you"))
        if settings.decision_backend == "typesafe":
            add(Finding("warn", "Decisions go to TypeSafe, not through Switchyard", "Vacancy and profile text leaves through that service"))
    except Exception:  # no database yet: nothing was saved
        pass
    return found


def main(*, fix: bool = False, root: Path | None = None, out=print) -> int:
    findings = audit(root)
    for item in findings:
        out(f"{ICONS[item.level]} {item.title}" + (f"\n    {item.detail}" if item.detail else ""))
    if fix:
        for item in findings:
            if item.fix and item.level in {"warn", "fail"}:
                out(("fixed: " if item.fix() else "could not fix: ") + item.title)
    failed = sum(1 for item in findings if item.level == "fail")
    warned = sum(1 for item in findings if item.level == "warn")
    out(f"\n{failed} to fix, {warned} to look at" + ("" if fix or not any(i.fix for i in findings if i.level in {"warn", "fail"}) else "  (run with --fix for the file permissions)"))
    return 1 if failed else 0
