"""Several API keys for one address: which one is in use, which are spare, how they are named and shown.

An account is a key for a base address. The key in use is `gateway_key` in the settings; the other accounts for the
same address are tried in turn when it is rate limited or rejected, before the next model of the fallback chain is
asked. A key is never sent to an address it was not saved for."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from careertwin.interview.providers import base_url

MAX_ACCOUNTS = 20
NAME_LIMIT = 40


class AlreadySaved(ValueError):
    """This key is saved for this address already."""


@dataclass(frozen=True)
class Account:
    name: str
    url: str
    key: str

    def tail(self) -> str:
        """The end of the key, enough to tell two apart and never enough to use."""
        return "…" + self.key[-(4 if len(self.key) < 16 else 6) :]


def clean(items: Any) -> list[dict[str, str]]:
    """The saved list, checked: each item has a name, an http(s) address and a key. The same key twice for one
    address is kept once, and two accounts for one address never share a name."""
    if not isinstance(items, list):
        raise ValueError("accounts must be a list")
    result: list[dict[str, str]] = []
    for item in items:
        if not isinstance(item, dict) or not all(isinstance(item.get(field), str) for field in ("name", "url", "key")):
            raise ValueError("an account needs a name, an address and a key")
        name, url, key = item["name"].strip()[:NAME_LIMIT], base_url(item["url"]), item["key"].strip()
        if not name or not key or not url.startswith(("http://", "https://")):
            raise ValueError("an account needs a name, an address starting with http:// or https://, and a key")
        if any(a["url"].casefold() == url.casefold() and a["key"] == key for a in result):
            continue
        if any(a["url"].casefold() == url.casefold() and a["name"] == name for a in result):
            name = next_name(result, url)
        result.append({"name": name, "url": url, "key": key})
    if len(result) > MAX_ACCOUNTS:
        raise ValueError(f"at most {MAX_ACCOUNTS} accounts")
    return result


def for_url(items: list[dict[str, str]], url: str) -> list[Account]:
    """The accounts saved for this address, in the order they were added."""
    wanted = base_url(url).casefold()
    return [Account(**item) for item in items if item["url"].casefold() == wanted]


def next_name(items: list[dict[str, str]], url: str) -> str:
    """`default`, then `default-2`, `default-3`...: the first name not taken for this address."""
    taken = {account.name for account in for_url(items, url)}
    if "default" not in taken:
        return "default"
    number = 2
    while f"default-{number}" in taken:
        number += 1
    return f"default-{number}"


def add(items: list[dict[str, str]], url: str, key: str) -> list[dict[str, str]]:
    """The list with `key` added under the next free name. AlreadySaved when this address has the key already."""
    key = key.strip()
    if not key:
        raise ValueError("Paste the API key")
    if any(account.key == key for account in for_url(items, url)):
        raise AlreadySaved("This key is already saved for this address")
    return clean([*items, {"name": next_name(items, url), "url": url, "key": key}])


def remove(items: list[dict[str, str]], url: str, name: str) -> list[dict[str, str]]:
    wanted = base_url(url).casefold()
    return [item for item in items if not (item["url"].casefold() == wanted and item["name"] == name)]


def spares(items: list[dict[str, str]], url: str, active: str) -> list[str]:
    """The keys to try after `active` for this address, in order."""
    return [account.key for account in for_url(items, url) if account.key != active]
