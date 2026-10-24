"""`careertwin security` reports what is protected and what is not, and never prints a secret."""

from __future__ import annotations

import os
import subprocess

import pytest

from careertwin import security_check, vault
from careertwin.api import security
from careertwin.automation import guard

SECRET = "correct-horse-battery-staple-1234"


@pytest.fixture
def home(tmp_path, monkeypatch):
    for name in list(os.environ):
        if vault.SECRET_NAME.search(name) or name.startswith(("CAREERTWIN_", "MAIL_", "CORS_")):
            monkeypatch.delenv(name)
    root = tmp_path / "repo"
    root.mkdir()
    monkeypatch.chdir(root)
    monkeypatch.setattr(guard, "DATA_DIR", tmp_path / "data")
    monkeypatch.setattr(security, "token_file", lambda: tmp_path / "data" / "api-token")
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(root / "ABOUT.md"))
    monkeypatch.setenv("SWITCHYARD_URL", "http://127.0.0.1:3260")
    return root


def run(root):
    lines = []
    code = security_check.main(root=root, out=lines.append)
    return code, "\n".join(lines)


def levels(root):
    return {item.title: item.level for item in security_check.audit(root)}


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_a_shared_env_file_is_reported_and_fixed(home):
    env = home / ".env"
    env.write_text(f"ACCOUNT_PASSWORD={SECRET}\n")
    env.chmod(0o644)
    assert levels(home)[".env can be read by other users"] == "warn"
    code, output = run(home)
    assert code == 0 and "run with --fix" in output
    lines = []
    security_check.main(fix=True, root=home, out=lines.append)
    assert "fixed: .env can be read by other users" in lines and oct(env.stat().st_mode & 0o777) == "0o600"
    assert any(title.endswith("is private to you") for title in levels(home))


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_a_readable_token_file_is_a_failure(home, tmp_path):
    token = tmp_path / "data" / "api-token"
    token.parent.mkdir()
    token.write_text("x" * 40)
    token.chmod(0o644)
    assert levels(home)["The API token file can be read by other users"] == "fail"
    assert run(home)[0] == 1


def test_a_profile_that_git_would_commit_is_a_failure(home):
    subprocess.run(["git", "init", "-q"], cwd=home, check=True)
    (home / ".gitignore").write_text(".env\n")
    found = levels(home)
    assert found["ABOUT.md is not in .gitignore"] == "fail" and found["data/ is not in .gitignore"] == "fail"
    assert found[".env is ignored by git"] == "ok"


@pytest.mark.parametrize(
    ("variable", "value", "title"),
    [
        ("CORS_ORIGINS", "*", "CORS_ORIGINS allows every site"),
        ("CORS_ORIGINS", "https://a.example, *", "CORS_ORIGINS allows every site"),
        ("SWITCHYARD_URL", "http://gateway.example:3260", "SWITCHYARD_URL is a remote address over plain HTTP"),
        ("CAREERTWIN_API_TOKEN", "short", "CAREERTWIN_API_TOKEN is shorter than 32 characters"),
    ],
)
def test_dangerous_settings_fail(home, monkeypatch, variable, value, title):
    monkeypatch.setenv(variable, value)
    assert levels(home)[title] == "fail"
    assert run(home)[0] == 1


@pytest.mark.parametrize(
    ("variable", "value", "title"),
    [
        ("CAREERTWIN_ALLOWED_HOSTS", "panel.example", "The API answers to names besides localhost"),
        ("CAREERTWIN_API_DOCS", "1", "The API docs page is on"),
    ],
)
def test_risky_settings_warn_without_failing(home, monkeypatch, variable, value, title):
    monkeypatch.setenv(variable, value)
    assert levels(home)[title] == "warn" and run(home)[0] == 0


def test_a_weak_secret_is_listed_by_name(home, monkeypatch):
    monkeypatch.setenv("TAVILY_API_KEY", "changeme")
    found = [item for item in security_check.audit(home) if item.title == "Weak or placeholder secrets"]
    assert found and "TAVILY_API_KEY" in found[0].detail


def test_the_report_never_contains_a_secret(home, monkeypatch):
    for name in ("ACCOUNT_PASSWORD", "MAIL_PASSWORD", "SWITCHYARD_API_KEY", "TAVILY_API_KEY"):
        monkeypatch.setenv(name, SECRET + name)
    monkeypatch.setenv("CAREERTWIN_LOGIN_DOMAINS", "linkedin.com")
    _, output = run(home)
    assert SECRET not in output and "ACCOUNT_PASSWORD" in output and "linkedin.com" in output


def test_the_account_password_says_where_it_may_be_typed(home, monkeypatch):
    monkeypatch.setenv("ACCOUNT_PASSWORD", SECRET)
    assert levels(home)["The account password may be typed on: nowhere (CAREERTWIN_LOGIN_DOMAINS is empty)"] == "info"
    monkeypatch.setenv("CAREERTWIN_LOGIN_DOMAINS", "linkedin.com")
    assert levels(home)["The account password may be typed on: linkedin.com"] == "ok"


def test_the_kill_switch_and_standing_permissions_are_visible(home, monkeypatch):
    guard.stop("test")
    monkeypatch.setenv("CAREERTWIN_AGENT_ALLOW_MAIL", "1")
    found = levels(home)
    assert found["The kill switch is on (data/STOP)"] == "info" and found["A chat agent may send email after you confirm in the chat"] == "info"


def test_settings_that_remove_the_human_are_warned_about(home, tmp_path, monkeypatch):
    from careertwin import db
    from careertwin.automation.models import Settings
    from careertwin.automation.store import Store

    monkeypatch.setattr(db, "DB_PATH", tmp_path / "x.db")
    store = Store()
    store.initialize()
    store.save_settings(Settings(context="## Contact\n| Full name | A |\n", auto_submit=True, mail_auto_send=True, decision_backend="typesafe"))
    found = levels(home)
    assert found["auto_submit is on"] == "warn" and found["mail_auto_send is on"] == "warn"
    assert found["Decisions go to TypeSafe, not through Switchyard"] == "warn"


def test_the_cli_knows_both_commands(home, monkeypatch, capsys):
    import sys

    from careertwin import cli

    monkeypatch.setattr(sys, "argv", ["careertwin", "security"])
    with pytest.raises(SystemExit) as stop:
        cli.main()
    assert stop.value.code in {0, 1} and "secret(s) are set" in capsys.readouterr().out
