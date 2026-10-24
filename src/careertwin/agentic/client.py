"""Async HTTP client for the CareerTwin API."""

from __future__ import annotations

import os
from typing import Any

import httpx

DEFAULT_API_URL = "http://127.0.0.1:8000"


class ApiError(RuntimeError):
    """The CareerTwin API is unreachable or returned an error."""


def api_url() -> str:
    return os.getenv("CAREERTWIN_API_URL", DEFAULT_API_URL).rstrip("/")


class CareerTwinClient:
    def __init__(
        self,
        base_url: str | None = None,
        timeout: float = 60.0,
        transport: httpx.AsyncBaseTransport | None = None,
    ):
        self.base_url = (base_url or api_url()).rstrip("/")
        self.timeout = timeout
        self.transport = transport

    async def request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
        json: Any = None,
        timeout: float | None = None,
    ) -> Any:
        params = {k: v for k, v in (params or {}).items() if v is not None}
        try:
            async with httpx.AsyncClient(
                base_url=self.base_url, timeout=timeout or self.timeout, transport=self.transport
            ) as client:
                response = await client.request(method, path, params=params, json=json)
        except httpx.ConnectError as exc:
            raise ApiError(
                f"CareerTwin API is not running at {self.base_url}. Start it with `make api`."
            ) from exc
        except httpx.HTTPError as exc:
            raise ApiError(f"{method} {path} failed: {type(exc).__name__}: {exc}") from exc
        if response.status_code >= 400:
            try:
                detail = response.json().get("detail", response.text)
            except ValueError:
                detail = response.text
            raise ApiError(f"{method} {path} returned {response.status_code}: {detail}")
        if not response.content:
            return None
        try:
            return response.json()
        except ValueError:
            return response.text

    async def get(self, path: str, **params: Any) -> Any:
        return await self.request("GET", path, params=params)

    async def post(self, path: str, body: Any = None, *, timeout: float | None = None) -> Any:
        return await self.request("POST", path, json=body, timeout=timeout)

    async def put(self, path: str, body: Any) -> Any:
        return await self.request("PUT", path, json=body)
