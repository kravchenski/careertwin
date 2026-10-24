"""CareerTwin API — AI provider abstractions."""

from __future__ import annotations

import json
import os
import re
from typing import Any

import httpx

from careertwin.settings import PROVIDERS
from careertwin.vault import assert_clean


async def call_ai(
    system_prompt: str,
    user_prompt: str,
    provider: str,
    model: str,
    *,
    max_tokens: int = 2500,
    temperature: float = 0.2,
) -> str:
    assert_clean(f"{system_prompt}\n{user_prompt}", "a model prompt")
    config = PROVIDERS.get(provider, PROVIDERS["lmstudio"])
    selected_model = model or config["default_model"]
    kind = config["kind"]
    strict_system_prompt = system_prompt.rstrip()

    try:
        async with httpx.AsyncClient(timeout=580.0) as client:
            if kind == "lmstudio":
                url = f'{config.get("base_url", "http://127.0.0.1:1234").rstrip("/")}/v1/chat/completions'
                payload = {
                    "model": selected_model,
                    "messages": [
                        {"role": "system", "content": strict_system_prompt},
                        {"role": "user", "content": user_prompt},
                    ],
                    "temperature": temperature,
                    "max_tokens": max_tokens,
                }
                try:
                    response = await client.post(
                        url,
                        headers={"Content-Type": "application/json"},
                        json=payload,
                    )
                    response.raise_for_status()
                    data = response.json()
                    if isinstance(data, dict):
                        choices = data.get("choices")
                        if isinstance(choices, list) and choices:
                            message = choices[0].get("message") or {}
                            if isinstance(message.get("content"), str):
                                content = message.get("content", "").strip()
                                if content:
                                    return content
                            reasoning = message.get("reasoning_content")
                            if isinstance(reasoning, str) and reasoning.strip():
                                json_match = parse_json_object(reasoning)
                                if json_match:
                                    return json.dumps(json_match)
                            text = choices[0].get("text")
                            if isinstance(text, str):
                                return text
                except Exception as e:
                    print(f"[AI] LM Studio error: {type(e).__name__}: {e}")
                    pass
                return ""

            if kind == "ollama":
                response = await client.post(
                    config["url"],
                    headers={"Content-Type": "application/json"},
                    json={
                        "model": selected_model,
                        "prompt": f"{system_prompt}\n\n{user_prompt}",
                        "stream": False,
                        "options": {"temperature": temperature, "num_predict": max_tokens},
                    },
                )
                response.raise_for_status()
                return response.json().get("response", "")

            headers = {"Content-Type": "application/json"}
            key_env = config.get("key_env", "")
            if key_env:
                api_key = os.getenv(key_env, "")
                if api_key:
                    headers["Authorization"] = f"Bearer {api_key}"
            if provider == "openrouter":
                headers["HTTP-Referer"] = "https://careertwin.local"
                headers["X-OpenRouter-Title"] = "CareerTwin"

            response = await client.post(
                config["url"],
                headers=headers,
                json={
                    "model": selected_model,
                    "messages": [
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": user_prompt},
                    ],
                    "temperature": temperature,
                    "max_tokens": max_tokens,
                },
            )
            response.raise_for_status()
            data = response.json()
            return data.get("choices", [{}])[0].get("message", {}).get("content", "")
    except Exception:
        return ""


def parse_json_object(content: str, max_length: int = 100_000) -> dict[str, Any] | None:
    if not content or len(content) > max_length:
        return None
    match = re.search(r"\{[\s\S]*\}", content)
    if not match:
        return None
    try:
        return json.loads(match.group())
    except json.JSONDecodeError:
        return None


async def provider_status(provider_id: str) -> dict[str, Any]:
    config = PROVIDERS[provider_id]
    selected_model = config["default_model"]
    kind = config["kind"]

    if kind == "lmstudio":
        base_url = config.get("base_url", "http://127.0.0.1:1234").rstrip("/")
        checks = [
            (
                "chat-completions",
                "POST",
                f"{base_url}/v1/chat/completions",
                {
                    "model": selected_model,
                    "messages": [
                        {"role": "system", "content": "Return only the final answer. Plain text only. No reasoning."},
                        {"role": "user", "content": "ping"},
                    ],
                    "temperature": 0.2,
                    "max_tokens": 200,
                },
            ),
            ("models", "GET", f"{base_url}/v1/models", None),
        ]
        last_error = "Unknown LM Studio error"
        try:
            async with httpx.AsyncClient(timeout=4.0) as client:
                for flavor, method, url, payload in checks:
                    try:
                        if method == "GET":
                            response = await client.get(url)
                        else:
                            response = await client.post(
                                url,
                                headers={"Content-Type": "application/json"},
                                json=payload,
                            )
                        response.raise_for_status()
                        if flavor == "chat-completions":
                            data = response.json()
                            choices = data.get("choices", []) if isinstance(data, dict) else []
                            message = choices[0].get("message", {}) if choices else {}
                            content = str(message.get("content", "")).strip()
                            if not content:
                                raise ValueError("LM Studio returned empty content")
                        return {
                            "id": provider_id,
                            "status": "connected",
                            "model": selected_model,
                            "flavor": flavor,
                        }
                    except Exception as exc:
                        last_error = str(exc)
        except Exception as exc:
            last_error = str(exc)
        return {
            "id": provider_id,
            "status": "disconnected",
            "model": selected_model,
            "reason": last_error,
        }

    if kind == "ollama":
        base_url = config["url"].rsplit("/", 2)[0]
        try:
            async with httpx.AsyncClient(timeout=4.0) as client:
                response = await client.get(f"{base_url}/tags")
                response.raise_for_status()
                models = [item.get("name", "") for item in response.json().get("models", [])]
            return {
                "id": provider_id,
                "status": "connected",
                "model": selected_model,
                "available_models": models[:10],
            }
        except Exception as exc:
            return {
                "id": provider_id,
                "status": "disconnected",
                "model": selected_model,
                "reason": str(exc),
            }

    key_env = config.get("key_env", "")
    api_key = os.getenv(key_env, "") if key_env else ""
    return {
        "id": provider_id,
        "status": "configured" if api_key else "missing_key",
        "model": selected_model,
        "key_env": key_env,
    }
