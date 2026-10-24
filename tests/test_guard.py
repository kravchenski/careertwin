"""Limits no model output can talk its way past: where a form is filled, where a password is typed, the kill switch."""

from __future__ import annotations

import json

import pytest

from careertwin.automation import guard
from careertwin.automation.models import Settings


@pytest.mark.parametrize(
    ("host", "expected"),
    [
        ("example.com", "example.com"), ("jobs.example.com", "example.com"), ("a.b.c.example.com", "example.com"),
        ("example.co.uk", "example.co.uk"), ("careers.example.co.uk", "example.co.uk"), ("shop.example.com.pl", "example.com.pl"),
        ("localhost", "localhost"), ("EXAMPLE.com.", "example.com"),
    ],
)
def test_the_registrable_domain(host, expected):
    assert guard.registrable(host) == expected


def test_a_form_may_be_filled_on_the_employers_site_and_on_application_systems():
    allowed = guard.form_domains("https://careers.example.com/jobs/1")
    for url in (
        "https://example.com/apply", "https://careers.example.com/x", "https://boards.greenhouse.io/acme/jobs/1",
        "https://acme.wd5.myworkdayjobs.com/en-US/job", "https://jobs.lever.co/acme", "https://apply.workable.com/acme",
    ):
        assert guard.host_allowed(url, allowed), url


@pytest.mark.parametrize(
    "url",
    [
        "https://attacker.example/apply", "https://example.com.evil.io/apply", "https://evilexample.com/apply",
        "https://example.com@evil.io/apply", "https://evil.io/?next=https://example.com", "https://greenhouse.io.evil.io/",
        "javascript:alert(1)", "about:blank", "chrome://settings", "", "https:///nohost",
    ],
)
def test_a_look_alike_or_foreign_address_is_not_the_employers_site(url):
    assert not guard.host_allowed(url, guard.form_domains("https://careers.example.com/jobs/1"))


def test_extra_form_domains_come_from_the_environment(monkeypatch):
    monkeypatch.setenv("CAREERTWIN_FORM_DOMAINS", "portal.partner.org, other.net")
    allowed = guard.form_domains("https://example.com/job")
    assert guard.host_allowed("https://x.partner.org/a", allowed) and guard.host_allowed("https://other.net", allowed)


def test_the_password_is_allowed_nowhere_until_a_login_domain_is_listed(monkeypatch):
    monkeypatch.delenv("CAREERTWIN_LOGIN_DOMAINS", raising=False)
    assert guard.login_domains() == set()
    assert not guard.password_allowed("https://boards.greenhouse.io/login")  # an application system is not enough
    assert not guard.password_allowed("https://example.com/login")
    monkeypatch.setenv("CAREERTWIN_LOGIN_DOMAINS", "linkedin.com, accounts.google.com")
    assert guard.password_allowed("https://www.linkedin.com/login")
    assert guard.password_allowed("https://accounts.google.com/signin")
    assert not guard.password_allowed("https://linkedin.com.evil.io/login")
    assert not guard.password_allowed("https://boards.greenhouse.io/login")


def test_a_login_domain_is_also_somewhere_a_form_may_be(monkeypatch):
    monkeypatch.setenv("CAREERTWIN_LOGIN_DOMAINS", "linkedin.com")
    assert guard.host_allowed("https://www.linkedin.com/jobs/view/1", guard.form_domains("https://example.com/j"))


@pytest.mark.parametrize(
    ("listing", "tab", "url"),
    [
        (json.dumps({"currentTabId": 7, "tabs": [{"id": 7, "url": "https://a.example/x"}, {"id": 8, "url": "https://b"}]}), 7, "https://a.example/x"),
        (json.dumps({"tabs": [{"tabId": 9, "url": "https://c.example"}]}), "9", "https://c.example"),
        (json.dumps([{"id": 1, "url": "https://list.example"}]), 1, "https://list.example"),
        (json.dumps({"tabs": []}), 7, None),
        (json.dumps({"tabs": [{"id": 7}]}), 7, None),
        (json.dumps({"tabs": [{"id": 8, "url": "https://other"}]}), 7, None),
        ("not json", 7, None),
        ("", 7, None),
        (json.dumps({"tabs": "nope"}), 7, None),
    ],
)
def test_the_address_of_a_tab_is_read_from_a_listing(listing, tab, url):
    assert guard.tab_url(listing, tab) == url


def test_the_kill_switch_is_a_file_and_blocks_until_it_is_removed(tmp_path, monkeypatch):
    monkeypatch.setattr(guard, "DATA_DIR", tmp_path / "data")
    assert not guard.stopped()
    guard.assert_running()
    guard.stop("testing")
    assert guard.stopped() and (tmp_path / "data" / "STOP").read_text() == "testing\n"
    with pytest.raises(guard.Stopped, match="kill switch"):
        guard.assert_running()
    guard.resume()
    guard.resume()  # removing it twice is fine
    guard.assert_running()


def test_the_agents_talk_to_switchyard_whatever_the_settings_say():
    assert Settings().provider == "switchyard"
    assert Settings(provider="openai").provider == "switchyard"
    assert Settings.model_validate_json('{"provider": "lmstudio", "context": "x"}').provider == "switchyard"

