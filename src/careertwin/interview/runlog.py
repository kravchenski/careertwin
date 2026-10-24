"""A short log of what the window did and why something failed.

The launcher starts the program without a console, so its output is lost; this file keeps the
reasons. It lives next to the settings: data/interview/interview.log.
"""

from __future__ import annotations

import threading
import time
from pathlib import Path

from careertwin.interview.settings import interview_dir

MAX_BYTES = 200_000
_lock = threading.Lock()


def path() -> Path:
    return interview_dir() / "interview.log"


def log(message: str) -> None:
    """Append one timestamped line. It never raises: a log must not break the window."""
    try:
        with _lock:
            file = path()
            file.parent.mkdir(parents=True, exist_ok=True)
            if file.exists() and file.stat().st_size > MAX_BYTES:
                file.replace(file.with_name("interview.log.1"))
            with file.open("a", encoding="utf-8") as out:
                out.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')}  {' '.join(message.split())}\n")
    except OSError:
        pass
