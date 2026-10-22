#!/usr/bin/env python3
"""Render a tab of the interview settings window offscreen (for the docs).

Usage: uv run --extra interview python scripts/render_settings_preview.py docs/media/interview-settings.png [tab] [en|ru|pl]
"""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

WORK = Path(tempfile.mkdtemp(prefix="careertwin-settings-"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_SCALE_FACTOR", "2")
os.environ["CAREERTWIN_NO_DOTENV"] = "1"
os.environ["CAREERTWIN_DATA_DIR"] = str(WORK)
os.environ["CAREERTWIN_ABOUT_PATH"] = str(Path(__file__).resolve().parent / "demo" / "ABOUT.demo.md")

from careertwin.interview import screenshot  # noqa: E402
from careertwin.interview.brief import InterviewBrief  # noqa: E402
from careertwin.interview.coach import StreamingCoach  # noqa: E402
from careertwin.interview.library import ContextController, Library  # noqa: E402
from careertwin.interview.overlay import Bridge, OverlayWindow, create_app  # noqa: E402
from careertwin.interview.runtime import Runtime  # noqa: E402
from careertwin.interview.session import InterviewSession  # noqa: E402
from careertwin.interview.settings import InterviewSettings  # noqa: E402


def main(target: str, tab: str = "docs", language: str = "en") -> None:
    app = create_app(sys.argv[:1])
    screenshot.available_tools = lambda: ["grim", "spectacle"]
    brief = InterviewBrief.load(allow_empty=True)
    context = ContextController(brief, Library())
    coach = StreamingCoach(brief)
    bridge = Bridge()
    session = InterviewSession(brief, None, coach, bridge.callbacks())
    runtime = Runtime(InterviewSettings(), context, coach, session)
    window = OverlayWindow(bridge, runtime=runtime, listening=False, ui_language=language)
    runtime.on_window = window.apply_setting

    cv = WORK / "Alex_Demo_CV.txt"
    cv.write_text("Alex Demo, backend developer. Built FastAPI services on PostgreSQL at Northwind. " * 40)
    extra = WORK / "linkedin_export.txt"
    extra.write_text("Speaks at local meetups about Python testing and observability. " * 25)
    context.add_documents([str(cv), str(extra)])
    window.refresh_controls()
    window.show()
    window.open_settings(tab)
    for _ in range(3):
        app.processEvents()
    window.settings_dialog.grab().save(target)
    print(f"saved {target}")


if __name__ == "__main__":
    main(*sys.argv[1:4]) if len(sys.argv) > 1 else main("settings.png")
