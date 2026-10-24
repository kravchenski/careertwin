"""Load `.env` so every entry point sees the same configuration."""

from __future__ import annotations

import os
from pathlib import Path

from dotenv import load_dotenv

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_env() -> None:
    """Read `.env` from the repository root, then the working directory.

    Variables already in the environment win. Set CAREERTWIN_NO_DOTENV=1 to skip,
    which the test suite does so a developer's `.env` cannot change test results.
    """
    if os.getenv("CAREERTWIN_NO_DOTENV"):
        return
    for path in (REPO_ROOT / ".env", Path.cwd() / ".env"):
        if path.is_file():
            load_dotenv(path, override=False)
