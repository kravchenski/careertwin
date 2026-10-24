#!/usr/bin/env python3
"""Render the interview overlay offscreen with a sample exchange (for the docs).

Usage: uv run --extra interview python scripts/render_overlay_preview.py docs/media/interview-overlay.png
"""

from __future__ import annotations

import os
import sys

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_SCALE_FACTOR", "2")

from PySide6.QtWidgets import QApplication  # noqa: E402

from careertwin.interview.overlay import Bridge, OverlayWindow  # noqa: E402

QUESTION = "Tell me about a time you had to debug a production incident."
REPLY = (
    "POINTS:\n"
    "- Situation: API latency doubled after a release\n"
    "- Action: traced slow queries, added the missing index\n"
    "- Result: latency back to normal within the hour\n"
    "ANSWER:\n"
    "After a release our API slowed down. I traced it to a query without an index, "
    "added it, and set up an alert so we would catch the next one earlier."
)


def main(target: str) -> None:
    app = QApplication(sys.argv[:1])
    bridge = Bridge()
    window = OverlayWindow(
        bridge, on_ask=lambda _: None, on_hint=lambda: None, on_pause=lambda _: None
    )
    bridge.status.emit("Listening")
    bridge.turn.emit("Interviewer", "Thanks for joining. " + QUESTION)
    bridge.turn.emit("You", "Sure, one comes to mind.")
    bridge.started.emit(QUESTION)
    bridge.done.emit(REPLY)
    app.processEvents()
    window.grab().save(target)
    print(f"saved {target}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "overlay.png")
