"""CareerTwin API — provider routes."""

from __future__ import annotations

import asyncio
import os
from typing import Any

from fastapi import APIRouter

from ai import provider_status
from config import PROVIDERS

router = APIRouter()


@router.get("/v1/providers")
async def list_providers() -> dict[str, Any]:
    return {
        "providers": [
            {
                "id": provider_id,
                "default_model": config["default_model"],
            }
            for provider_id, config in PROVIDERS.items()
        ]
    }


@router.get("/v1/providers/status")
async def providers_status() -> dict[str, Any]:
    statuses = await asyncio.gather(*(provider_status(pid) for pid in PROVIDERS))
    return {"providers": list(statuses)}


@router.get("/v1/providers/available")
async def available_providers() -> dict[str, Any]:
    """Show which providers are configured (have API keys) and ready to use."""
    available = []
    for pid, cfg in PROVIDERS.items():
        key_env = cfg.get("key_env", "")
        has_key = bool(os.getenv(key_env, "")) if key_env else True
        available.append({
            "id": pid,
            "kind": cfg.get("kind", ""),
            "model": cfg.get("default_model", ""),
            "configured": has_key,
            "key_env": key_env,
        })
    return {"providers": available}
