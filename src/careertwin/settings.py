"""Paths, LLM provider table and constants shared across CareerTwin."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
DATA_DIR = Path(os.getenv("CAREERTWIN_DATA_DIR", str(REPO_ROOT / "data")))

OUTPUT_DIR = Path(os.getenv("OUTPUT_DIR", str(DATA_DIR / "outputs")))
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

DB_PATH = Path(os.getenv("CAREERTWIN_DB_PATH", str(DATA_DIR / "careertwin.db")))

CACHE_DIR = Path(os.getenv("CACHE_DIR", str(DATA_DIR / "cache")))
CACHE_DIR.mkdir(parents=True, exist_ok=True)

SCREENSHOT_DIR = Path(os.getenv("SCREENSHOT_DIR", str(DATA_DIR / "screenshots")))
SCREENSHOT_DIR.mkdir(parents=True, exist_ok=True)

COMPANIES_FILE = Path(__file__).resolve().parent / "jobs" / "data" / "companies.json"

CACHE_TTL: dict[str, int] = {
    "remoteok": 3600,
    "hn": 3600,
    "justjoin": 3600,
    "nofluffjobs": 3600,
    "linkedin": 43200,
    "indeed": 43200,
    "glassdoor": 43200,
    "ziprecruiter": 43200,
    "searxng": 1800,
    "tavily": 1800,
}

SKILLS = [
    "Python", "TypeScript", "JavaScript", "React", "Next.js", "Angular", "Vue",
    "Node.js", "FastAPI", "Django", "Flask", "Go", "Rust", "Java", "Kotlin",
    "PostgreSQL", "MySQL", "Redis", "MongoDB", "Docker", "Kubernetes", "AWS",
    "GCP", "Azure", "GraphQL", "REST", "LLM", "LangChain", "LangGraph",
    "OpenAI", "Anthropic", "Aiogram", "ComfyUI", "CUDA", "PyTorch", "TensorFlow",
]

LANGUAGE_LABELS: dict[str, str] = {
    "en": "English",
    "pl": "Polish",
    "ru": "Russian",
    "de": "German",
    "fr": "French",
    "es": "Spanish",
    "pt": "Portuguese",
    "uk": "Ukrainian",
    "ja": "Japanese",
    "zh": "Chinese",
}

TEMPLATE_MAP: dict[str, str] = {
    "clean-slate": "minimal",
    "graphite": "executive",
    "signal": "modern",
    "north": "ats",
    "modern": "modern",
    "classic": "classic",
    "minimal": "minimal",
    "ats": "ats",
    "executive": "executive",
    "sidebar": "sidebar",
    "technical": "technical",
    "senior": "senior",
    "baseline": "senior",
}

PROVIDERS: dict[str, dict[str, str]] = {
    "switchyard": {
        "kind": "openai",
        "url": os.getenv("SWITCHYARD_URL", "http://127.0.0.1:3260").rstrip("/") + "/v1/chat/completions",
        "key_env": "SWITCHYARD_API_KEY",
        "default_model": os.getenv("SWITCHYARD_MODEL", "agent"),
    },
    "lmstudio": {
        "kind": "lmstudio",
        "url": os.getenv("LMSTUDIO_BASE_URL", "http://127.0.0.1:1234/api/v1/chat"),
        "base_url": os.getenv("LMSTUDIO_API_BASE", "http://127.0.0.1:1234"),
        "default_model": os.getenv(
            "LMSTUDIO_MODEL",
            "qwythos-9b-v2",
        ),
    },
    "ollama": {
        "kind": "ollama",
        "url": os.getenv("OLLAMA_BASE_URL", "http://localhost:11434/api/generate"),
        "default_model": os.getenv("OLLAMA_MODEL", "qwen2.5:3b"),
    },
    "groq": {
        "kind": "openai",
        "url": "https://api.groq.com/openai/v1/chat/completions",
        "key_env": "GROQ_API_KEY",
        "default_model": os.getenv("GROQ_MODEL", "llama-3.1-8b-instant"),
    },
    "openai": {
        "kind": "openai",
        "url": "https://api.openai.com/v1/chat/completions",
        "key_env": "OPENAI_API_KEY",
        "default_model": os.getenv("OPENAI_MODEL", "gpt-4o-mini"),
    },
    "openrouter": {
        "kind": "openai",
        "url": "https://openrouter.ai/api/v1/chat/completions",
        "key_env": "OPENROUTER_API_KEY",
        "default_model": os.getenv(
            "OPENROUTER_MODEL",
            "meta-llama/llama-3.1-8b-instruct:free",
        ),
    },
    "openrouter-free": {
        "kind": "openai",
        "url": "https://openrouter.ai/api/v1/chat/completions",
        "key_env": "OPENROUTER_API_KEY",
        "default_model": "meta-llama/llama-3.1-8b-instruct:free",
    },
}

AGENT_BROWSER_LOCAL_BIN = REPO_ROOT / "node_modules" / ".bin" / "agent-browser"


JOB_SOURCE_DOMAINS: dict[str, str] = {
    "linkedin": "linkedin.com/jobs",
    "indeed": "indeed.com",
    "glassdoor": "glassdoor.com",
    "ziprecruiter": "ziprecruiter.com",
    "justjoin": "justjoin.it",
    "nofluffjobs": "nofluffjobs.com",
    "remoteok": "remoteok.com",
}


SEARXNG_URL = os.getenv("SEARXNG_URL", "http://searxng:8080")


TAVILY_API_KEY = os.getenv("TAVILY_API_KEY", "")


COMPANY_WORKERS: dict[str, dict[str, Any]] = {
    "epam": {
        "name": "EPAM",
        "search_urls": [
            "https://www.epam.com/careers/job-listings?query={query}",
            "https://www.epam.com/careers/job-listings?query={query}&experience=internship",
        ],
        "career_domain": "epam.com",
        "platforms": ["linkedin", "glassdoor"],
    },
    "amazon": {
        "name": "Amazon",
        "search_urls": [
            "https://www.amazon.jobs/en/search?offset=0&result_limit=10&sort=relevant&category=software-development&query={query}+intern",
            "https://www.amazon.jobs/en/search?query={query}+intern&business_category=student-programs",
        ],
        "career_domain": "amazon.jobs",
        "platforms": ["linkedin", "glassdoor"],
    },
    "google": {
        "name": "Google",
        "search_urls": [
            "https://careers.google.com/jobs/results/?q={query}%20intern",
        ],
        "career_domain": "careers.google.com",
        "platforms": ["linkedin", "glassdoor"],
    },
    "meta": {
        "name": "Meta",
        "search_urls": [
            "https://www.metacareers.com/jobs?q={query}%20intern",
        ],
        "career_domain": "metacareers.com",
        "platforms": ["linkedin", "glassdoor"],
    },
    "microsoft": {
        "name": "Microsoft",
        "search_urls": [
            "https://jobs.careers.microsoft.com/global/en/search?q={query}%20intern",
        ],
        "career_domain": "jobs.careers.microsoft.com",
        "platforms": ["linkedin", "glassdoor"],
    },
    "apple": {
        "name": "Apple",
        "search_urls": [
            "https://jobs.apple.com/en-us/search?q={query}%20intern",
        ],
        "career_domain": "jobs.apple.com",
        "platforms": ["linkedin", "glassdoor"],
    },
    "nvidia": {
        "name": "NVIDIA",
        "search_urls": [
            "https://nvidia.wd5.myworkdayjobs.com/en-US/NVIDIAExternalCareerSite?q={query}%20intern",
        ],
        "career_domain": "nvidia.wd5.myworkdayjobs.com",
        "platforms": ["linkedin", "glassdoor"],
    },
    "deloitte": {
        "name": "Deloitte",
        "search_urls": [
            "https://jobs.deloitte.com/search?q={query}%20intern",
        ],
        "career_domain": "jobs.deloitte.com",
        "platforms": ["linkedin", "indeed"],
    },
    "pwc": {
        "name": "PwC",
        "search_urls": [
            "https://jobs.pwc.com/search-jobs?q={query}%20intern",
        ],
        "career_domain": "jobs.pwc.com",
        "platforms": ["linkedin", "indeed"],
    },
    "accenture": {
        "name": "Accenture",
        "search_urls": [
            "https://acn.wd12.myworkdayjobs.com/en-US/AccentureCareers?q={query}%20intern",
        ],
        "career_domain": "accenture.com",
        "platforms": ["linkedin", "indeed"],
    },
}


def _agent_result(status: str, detail: str, **extra: Any) -> dict[str, Any]:
    payload: dict[str, Any] = {"status": status, "detail": detail}
    payload.update(extra)
    return payload
