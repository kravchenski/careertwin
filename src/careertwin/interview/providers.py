"""Where the answers come from: a Switchyard gateway or any provider with an OpenAI-style API.

All of them take `POST <base>/v1/chat/completions` and `GET <base>/v1/models`, so a provider is
just a base address (plus a key for the cloud ones)."""

from __future__ import annotations

from dataclasses import dataclass

OTHER = "other"


@dataclass(frozen=True)
class Provider:
    id: str
    label: str
    url: str
    cloud: bool  # needs an API key, and receives your CV and ABOUT text with every request


PROVIDERS = (
    Provider("switchyard", "Switchyard (local)", "http://127.0.0.1:3260", cloud=False),
    Provider("groq", "Groq", "https://api.groq.com/openai", cloud=True),
    Provider("openai", "OpenAI", "https://api.openai.com", cloud=True),
    Provider("openrouter", "OpenRouter", "https://openrouter.ai/api", cloud=True),
    Provider("ollama", "Ollama (local)", "http://localhost:11434", cloud=False),
    Provider("lmstudio", "LM Studio (local)", "http://localhost:1234", cloud=False),
)


def base_url(url: str) -> str:
    """The address without a trailing slash, `/v1` or `/chat/completions`: the form people paste."""
    url = url.strip().rstrip("/")
    for suffix in ("/v1/chat/completions", "/chat/completions", "/v1"):
        if url.endswith(suffix):
            return url[: -len(suffix)].rstrip("/")
    return url


def provider_for(url: str) -> Provider | None:
    """The preset this address belongs to, if any."""
    wanted = base_url(url).casefold()
    return next((p for p in PROVIDERS if p.url.casefold() == wanted), None)
