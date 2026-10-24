"""Outbound fetches that cannot be steered at the machine or the network they run on.

A vacancy link, a company site or a redirect is text written by someone else. Three layers apply:

* `validate_public_url` is a fast check on a URL a caller hands in (scheme, no credentials, no
  loopback or internal name, no private IP written in any notation);
* `SafeTransport` and `SafeSyncTransport` resolve the host of every request, and of every redirect,
  refuse an address that is not public, and connect to the address they checked, so a name cannot
  answer 8.8.8.8 to the check and 127.0.0.1 to the connection;
* `safe_async_client` / `safe_client` / `fetch_text` put those transports behind a client with a small
  redirect limit and, for `fetch_text`, a cap on the size of the body.

Local services (the Switchyard gateway, LM Studio, Ollama, the API itself) are not reached through
these clients: they are on loopback on purpose.
"""

from __future__ import annotations

import asyncio
import ipaddress
import socket
from typing import Any
from urllib.parse import urlparse

import httpx

_ALLOWED_URL_SCHEMES = frozenset({"http", "https"})
_BLOCKED_NAMES = frozenset({
    "localhost", "ip6-localhost", "ip6-loopback", "broadcasthost",
    "metadata", "metadata.google.internal", "instance-data", "host.docker.internal",
    "gateway.docker.internal", "kubernetes.docker.internal",
})
_BLOCKED_SUFFIXES = (".localhost", ".local", ".internal", ".intranet", ".lan", ".home", ".corp", ".localdomain")
_NOT_PUBLIC = [
    ipaddress.ip_network(net)
    for net in (
        "0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8", "169.254.0.0/16", "172.16.0.0/12",
        "192.0.0.0/24", "192.0.2.0/24", "192.168.0.0/16", "198.18.0.0/15", "198.51.100.0/24",
        "203.0.113.0/24", "224.0.0.0/4", "240.0.0.0/4",
        "::/128", "::1/128", "64:ff9b::/96", "64:ff9b:1::/48", "100::/64", "2001:db8::/32",
        "2002::/16", "fc00::/7", "fe80::/10", "ff00::/8",
    )
]
MAX_REDIRECTS = 5
DEFAULT_MAX_BYTES = 2_000_000


class UnsafeURL(httpx.TransportError):
    """The address is not one a fetch may reach."""


def is_public_address(value: str) -> bool:
    """True for a globally routable IP; an IPv4 written inside IPv6 is judged as the IPv4."""
    try:
        address = ipaddress.ip_address(value.split("%", 1)[0])
    except ValueError:
        return False
    if address.version == 6 and address.ipv4_mapped:
        address = address.ipv4_mapped
    if not address.is_global:
        return False
    return not any(address in network for network in _NOT_PUBLIC if network.version == address.version)


def _literal_ip(host: str) -> str | None:
    """The IP a host spells out, in dotted, decimal, hex or octal notation; None for a name."""
    try:
        return str(ipaddress.ip_address(host))
    except ValueError:
        pass
    try:
        return socket.inet_ntoa(socket.inet_aton(host))  # 2130706433, 0x7f.1, 017700000001
    except (OSError, ValueError):
        return None


def validate_public_url(url: str) -> str:
    """Reject a URL that points inside: wrong scheme, credentials, internal names, private IPs."""
    try:
        parsed = urlparse(url.strip())
        host = (parsed.hostname or "").lower().rstrip(".")
        parsed.port  # noqa: B018 - raises ValueError on a bad port
    except ValueError as error:
        raise ValueError("The URL is not valid") from error
    if parsed.scheme not in _ALLOWED_URL_SCHEMES:
        raise ValueError(f"URL scheme {parsed.scheme!r} is not allowed")
    if parsed.username is not None or parsed.password is not None:
        raise ValueError("URLs with credentials are not allowed")
    if not host:
        raise ValueError("The URL has no host")
    if host in _BLOCKED_NAMES or host.endswith(_BLOCKED_SUFFIXES):
        raise ValueError(f"URL host {host!r} is blocked")
    literal = _literal_ip(host)
    if literal is not None and not is_public_address(literal):
        raise ValueError("Private-range URLs are not allowed")
    if literal is None and "." not in host:
        raise ValueError(f"URL host {host!r} is not a public name")
    return url


def _check(host: str, addresses: list[str]) -> str:
    public = [address for address in dict.fromkeys(addresses)]
    if not public:
        raise UnsafeURL(f"{host} did not resolve")
    refused = [address for address in public if not is_public_address(address)]
    if refused:
        raise UnsafeURL(f"{host} resolves to a non-public address; the request was not sent")
    return public[0]


def _pin(request: httpx.Request, address: str) -> None:
    """Connect to the checked address; keep the name for the Host header and for TLS."""
    host = request.url.host
    request.extensions = {**request.extensions, "sni_hostname": host}
    request.url = request.url.copy_with(host=address)


def _host_and_port(request: httpx.Request) -> tuple[str, int]:
    scheme = request.url.scheme
    if scheme not in _ALLOWED_URL_SCHEMES:
        raise UnsafeURL(f"URL scheme {scheme!r} is not allowed")
    return request.url.host, request.url.port or (443 if scheme == "https" else 80)


class SafeTransport(httpx.AsyncHTTPTransport):
    async def handle_async_request(self, request: httpx.Request) -> httpx.Response:
        host, port = _host_and_port(request)
        literal = _literal_ip(host)
        if literal is not None:
            addresses = [literal]
        else:
            try:
                found = await asyncio.get_running_loop().getaddrinfo(host, port, type=socket.SOCK_STREAM)
            except OSError as error:
                raise UnsafeURL(f"{host} did not resolve") from error
            addresses = [item[4][0] for item in found]
        _pin(request, _check(host, addresses))
        return await super().handle_async_request(request)


class SafeSyncTransport(httpx.HTTPTransport):
    def handle_request(self, request: httpx.Request) -> httpx.Response:
        host, port = _host_and_port(request)
        literal = _literal_ip(host)
        if literal is not None:
            addresses = [literal]
        else:
            try:
                found = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
            except OSError as error:
                raise UnsafeURL(f"{host} did not resolve") from error
            addresses = [item[4][0] for item in found]
        _pin(request, _check(host, addresses))
        return super().handle_request(request)


def safe_async_client(**options: Any) -> httpx.AsyncClient:
    """An `httpx.AsyncClient` that only reaches public addresses, redirects included."""
    options.setdefault("max_redirects", MAX_REDIRECTS)
    options.setdefault("transport", SafeTransport())
    return httpx.AsyncClient(**options)


def safe_client(**options: Any) -> httpx.Client:
    options.setdefault("max_redirects", MAX_REDIRECTS)
    options.setdefault("transport", SafeSyncTransport())
    return httpx.Client(**options)


async def fetch_text(
    url: str,
    *,
    max_bytes: int = DEFAULT_MAX_BYTES,
    timeout: float = 25,
    headers: dict[str, str] | None = None,
    client: httpx.AsyncClient | None = None,
) -> str:
    """The text of a public page, at most `max_bytes` of it, with redirects checked at every hop."""
    validate_public_url(url)
    owned = client is None
    client = client or safe_async_client(timeout=timeout, follow_redirects=True)
    try:
        async with client.stream("GET", url, headers=headers or {"User-Agent": "CareerTwin/2.1"}) as response:
            response.raise_for_status()
            body = bytearray()
            async for chunk in response.aiter_bytes():
                body += chunk
                if len(body) >= max_bytes:
                    break
            return bytes(body[:max_bytes]).decode(response.encoding or "utf-8", errors="replace")
    finally:
        if owned:
            await client.aclose()
