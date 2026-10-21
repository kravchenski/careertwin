"""Outbound URL safety checks."""

from __future__ import annotations

from urllib.parse import urlparse

_ALLOWED_URL_SCHEMES = frozenset({"http", "https"})
_SSRF_BLOCKED_HOSTS = frozenset({
    "localhost", "127.0.0.1", "0.0.0.0", "::1",
    "169.254.169.254",
    "metadata.google.internal",
    "host.docker.internal",
})


def validate_public_url(url: str) -> str:
    """Reject internal/SSRF-prone URLs; allow only http/https to public hosts."""
    parsed = urlparse(url)
    if parsed.scheme not in _ALLOWED_URL_SCHEMES:
        raise ValueError(f"URL scheme {parsed.scheme!r} is not allowed")
    hostname = (parsed.hostname or "").lower().rstrip(".")
    if hostname in _SSRF_BLOCKED_HOSTS:
        raise ValueError(f"URL host {hostname!r} is blocked")
    if hostname.startswith("192.168.") or hostname.startswith("10.") or hostname.startswith("172."):
        raise ValueError("Private-range URLs are not allowed")
    return url
