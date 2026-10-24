"""Secrets: known by name, read only from the environment, never written to a log, a prompt or a file.

A secret is any environment variable whose name ends in KEY, TOKEN, PASSWORD, SECRET or PASSPHRASE
(`.env` is loaded into the environment by `careertwin.env`). Three rules follow from that:

* a log line, an event, an exported file or an API reply goes through `redact`, so a value that
  leaked into a string is replaced by `[secret:NAME]`;
* text that is about to go to a model goes through `assert_clean`, which raises instead of sending a
  secret to the gateway or to a provider behind it;
* an agent that must type a secret into a page writes a placeholder, `{{secret:NAME}}`, and only the
  executor resolves it, right before the call, for a name on the agent allowlist.
"""

from __future__ import annotations

import logging
import os
import re
import stat
from collections.abc import Iterable
from pathlib import Path
from typing import Any

SECRET_NAME = re.compile(r"(?:KEY|TOKEN|PASSWORD|SECRET|PASSPHRASE)$", re.I)
MIN_LENGTH = 6  # a shorter value would redact ordinary words; the security check flags it as weak

# Secrets an agent may ask the executor to type into a page. Everything else stays out of reach of
# a model, whatever the page or the model says.
AGENT_SECRETS = ("ACCOUNT_PASSWORD",)

# Shapes of credentials that are secrets even when they never passed through the environment.
_PATTERNS = [
    re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{12,}"),
    re.compile(r"\bsk-(?:proj-|ant-)?[A-Za-z0-9_-]{20,}"),
    re.compile(r"\bgsk_[A-Za-z0-9]{20,}"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9]{30,}"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{30,}"),
    re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"(?i)(?<=://)[^/\s:@]+:[^/\s@]+(?=@)"),  # user:password@host
    re.compile(r"(?i)(?<=[?&])(?:api[_-]?key|token|password|passwd|secret|access[_-]?token)=[^&\s]+"),
]
_SENSITIVE_KEY = re.compile(r"(?i)pass(?:word|wd)|secret|token|authorization|api[_-]?key|cookie")
PLACEHOLDER = re.compile(r"\{\{\s*secret:([A-Z][A-Z0-9_]{1,63})\s*\}\}")


class SecretError(RuntimeError):
    """A secret is missing, or one was about to leave through a door that is closed to it."""


def secret_names() -> list[str]:
    return sorted(name for name, value in os.environ.items() if SECRET_NAME.search(name) and len(value) >= MIN_LENGTH)


def _by_value() -> list[tuple[str, str]]:
    pairs = {os.environ[name]: name for name in secret_names()}
    return sorted(pairs.items(), key=lambda pair: -len(pair[0]))


def get(name: str) -> str:
    """The value of a secret. The error names the variable and never its value."""
    value = os.environ.get(name, "")
    if not value:
        raise SecretError(f"{name} is not set. Add it to .env (see .env.example).")
    return value


def redact(text: str) -> str:
    """`text` with every known secret and every credential-shaped string masked."""
    if not text:
        return text
    for value, name in _by_value():
        text = text.replace(value, f"[secret:{name}]")
    for pattern in _PATTERNS:
        text = pattern.sub("[redacted]", text)
    return text


def redact_data(value: Any) -> Any:
    """`redact` applied to every string in a structure; values under a sensitive key are dropped."""
    if isinstance(value, str):
        return redact(value)
    if isinstance(value, dict):
        return {
            key: "[redacted]" if isinstance(key, str) and _SENSITIVE_KEY.search(key) and item else redact_data(item)
            for key, item in value.items()
        }
    if isinstance(value, (list, tuple)):
        return [redact_data(item) for item in value]
    return value


def leaks(text: str) -> list[str]:
    """Names of the secrets whose value appears in `text`."""
    return [name for value, name in _by_value() if value in text]


def assert_clean(text: str, where: str = "a prompt") -> None:
    """Raise when `text` carries a secret. The message names the secret, not its value."""
    found = leaks(text)
    if found:
        raise SecretError(f"{', '.join(found)} would be sent in {where}; refusing to send it")


def resolve(text: str, allowed: Iterable[str] = AGENT_SECRETS) -> tuple[str, list[str]]:
    """Replace `{{secret:NAME}}` by the secret and return the text with the names that were used.

    A name outside `allowed`, or one that is not set, raises: a model cannot ask for any other secret."""
    used: list[str] = []
    permitted = set(allowed)

    def substitute(match: re.Match[str]) -> str:
        name = match[1]
        if name not in permitted:
            raise SecretError(f"{name} is not a secret an agent may type")
        used.append(name)
        return get(name)

    return PLACEHOLDER.sub(substitute, text), used


def placeholders(text: str) -> list[str]:
    return PLACEHOLDER.findall(text)


class RedactingFilter(logging.Filter):
    """Masks secrets in every record before a handler formats it."""

    def filter(self, record: logging.LogRecord) -> bool:
        record.msg = redact(record.getMessage())
        record.args = None
        if record.exc_info and not record.exc_text:
            record.exc_text = logging.Formatter().formatException(record.exc_info)
        if record.exc_text:
            record.exc_text = redact(record.exc_text)
        return True


def install_log_redaction() -> None:
    """Put the filter on the root logger's handlers (and on handlers added later by `basicConfig`)."""
    root = logging.getLogger()
    for handler in root.handlers:
        if not any(isinstance(item, RedactingFilter) for item in handler.filters):
            handler.addFilter(RedactingFilter())


def file_problems(path: Path) -> list[str]:
    """What is wrong with the permissions of a file that holds secrets (POSIX only)."""
    if os.name != "posix" or not path.exists():
        return []
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode & 0o077:
        return [f"{path} is readable by other users (mode {mode:o}); run `chmod 600 {path}`"]
    return []


def fix_file(path: Path) -> bool:
    if os.name != "posix" or not path.exists():
        return False
    path.chmod(0o600)
    return True


PLACEHOLDER_VALUES = {"changeme", "password", "secret", "your-key-here", "xxx", "example"}


def weak_secrets() -> list[str]:
    """Secrets that are short or look like a placeholder left over from .env.example.

    A site password is the user's own and may be short, so only the placeholder check applies to it."""
    weak = []
    for name in secret_names():
        value = os.environ[name]
        too_short = len(value) < 16 and name not in AGENT_SECRETS and not name.startswith("MAIL_")
        if too_short or value.lower() in PLACEHOLDER_VALUES:
            weak.append(name)
    return weak
