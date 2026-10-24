"""Secrets are found by name, masked wherever text leaves, and kept out of prompts."""

from __future__ import annotations

import io
import logging
import os
import stat

import pytest

from careertwin import vault
from careertwin.vault import SecretError

PASSWORD = "correct-horse-battery-staple"
TOKEN = "tok_9f8e7d6c5b4a39281706"


@pytest.fixture(autouse=True)
def secrets(monkeypatch):
    for name in list(os.environ):
        if vault.SECRET_NAME.search(name):
            monkeypatch.delenv(name)
    monkeypatch.setenv("ACCOUNT_PASSWORD", PASSWORD)
    monkeypatch.setenv("CAREERTWIN_API_TOKEN", TOKEN)
    monkeypatch.setenv("SHORT_KEY", "abc")  # shorter than MIN_LENGTH: not treated as a secret
    monkeypatch.setenv("PUBLIC_VALUE", "visible-value")


def test_secrets_are_found_by_the_end_of_the_variable_name():
    assert vault.secret_names() == ["ACCOUNT_PASSWORD", "CAREERTWIN_API_TOKEN"]


def test_a_value_is_replaced_by_its_name():
    text = f"login with {PASSWORD} and header {TOKEN}; visible-value stays"
    clean = vault.redact(text)
    assert PASSWORD not in clean and TOKEN not in clean
    assert "[secret:ACCOUNT_PASSWORD]" in clean and "[secret:CAREERTWIN_API_TOKEN]" in clean
    assert "visible-value" in clean


@pytest.mark.parametrize(
    "leak",
    [
        "Authorization: Bearer abcdefghijklmnop1234",
        "key sk-proj-abcdefghijklmnopqrstuvwx",
        "key gsk_abcdefghijklmnopqrstuvwx",
        "https://user:hunter2@example.com/path",
        "https://example.com/cb?api_key=abc123&x=1",
        "token ghp_abcdefghijklmnopqrstuvwxyz0123456789",
    ],
)
def test_credential_shaped_strings_are_masked_even_when_not_in_the_environment(leak):
    clean = vault.redact(leak)
    assert "[redacted]" in clean
    for fragment in ("abcdefghijklmnop", "hunter2", "abc123", "abcdefghijklmnopqrstuvwxyz0123456789"):
        assert fragment not in clean


def test_structures_are_masked_and_sensitive_keys_are_dropped():
    data = {"email": "a@b.c", "password": "x", "nested": [{"note": f"pw {PASSWORD}"}], "Authorization": "Bearer zzzzzzzzzzzzzzzz"}
    clean = vault.redact_data(data)
    assert clean["email"] == "a@b.c"
    assert clean["password"] == "[redacted]" and clean["Authorization"] == "[redacted]"
    assert PASSWORD not in str(clean)


def test_a_secret_is_refused_before_it_goes_into_a_prompt_and_the_error_does_not_repeat_it():
    with pytest.raises(SecretError) as error:
        vault.assert_clean(f"candidate password is {PASSWORD}", "a model prompt")
    assert "ACCOUNT_PASSWORD" in str(error.value) and PASSWORD not in str(error.value)
    vault.assert_clean("nothing secret here")


async def test_the_model_call_refuses_a_prompt_with_a_secret():
    from careertwin.llm import call_ai

    with pytest.raises(SecretError):
        await call_ai("system", f"the token is {TOKEN}", "switchyard", "auto")


async def test_the_agent_conversation_refuses_a_secret():
    from careertwin.agentic.runner import AgentRunner

    runner = AgentRunner()
    runner.messages.append({"role": "tool", "content": f'{{"leaked": "{PASSWORD}"}}'})
    with pytest.raises(SecretError):
        await runner._complete()


def test_a_placeholder_is_resolved_only_for_a_name_an_agent_may_type():
    text, used = vault.resolve("{{secret:ACCOUNT_PASSWORD}}")
    assert text == PASSWORD and used == ["ACCOUNT_PASSWORD"]
    with pytest.raises(SecretError, match="not a secret an agent may type"):
        vault.resolve("{{secret:CAREERTWIN_API_TOKEN}}")
    with pytest.raises(SecretError, match="not set"):
        vault.resolve("{{secret:MAIL_PASSWORD}}", allowed=["MAIL_PASSWORD"])
    assert vault.resolve("plain text") == ("plain text", [])
    assert vault.placeholders("a {{secret:ACCOUNT_PASSWORD}} b {{ secret:OTHER_ONE }}") == ["ACCOUNT_PASSWORD", "OTHER_ONE"]


def test_the_log_filter_masks_the_message_its_arguments_and_the_traceback():
    stream = io.StringIO()
    handler = logging.StreamHandler(stream)
    handler.setFormatter(logging.Formatter("%(message)s"))
    handler.addFilter(vault.RedactingFilter())
    logger = logging.getLogger("test.vault")
    logger.handlers, logger.propagate = [handler], False
    logger.warning("password is %s", PASSWORD)
    logger.error(f"token {TOKEN}")
    try:
        raise ValueError(f"bad {PASSWORD}")
    except ValueError:
        logger.exception("failed")
    output = stream.getvalue()
    assert PASSWORD not in output and TOKEN not in output
    assert "[secret:ACCOUNT_PASSWORD]" in output


def test_installing_the_filter_covers_the_handlers_of_the_root_logger():
    root = logging.getLogger()
    handler = logging.StreamHandler(io.StringIO())
    root.addHandler(handler)
    try:
        vault.install_log_redaction()
        vault.install_log_redaction()  # twice is still one filter
        assert sum(isinstance(item, vault.RedactingFilter) for item in handler.filters) == 1
    finally:
        root.removeHandler(handler)


@pytest.mark.skipif(os.name != "posix", reason="file modes")
def test_a_secrets_file_that_others_can_read_is_reported_and_fixed(tmp_path):
    env = tmp_path / ".env"
    env.write_text("ACCOUNT_PASSWORD=x\n")
    env.chmod(0o644)
    assert "readable by other users" in vault.file_problems(env)[0]
    assert vault.fix_file(env)
    assert stat.S_IMODE(env.stat().st_mode) == 0o600 and vault.file_problems(env) == []
    assert vault.file_problems(tmp_path / "missing") == []


def test_weak_secrets_are_listed_without_their_values(monkeypatch):
    monkeypatch.setenv("TYPESAFE_API_KEY", "short-key-1")
    monkeypatch.setenv("TAVILY_API_KEY", "changeme")
    weak = vault.weak_secrets()
    assert "TYPESAFE_API_KEY" in weak and "TAVILY_API_KEY" in weak
    assert "ACCOUNT_PASSWORD" not in weak and "CAREERTWIN_API_TOKEN" not in weak
