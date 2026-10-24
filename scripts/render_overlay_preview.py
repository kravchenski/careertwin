#!/usr/bin/env python3
"""Render the interview overlay offscreen with a sample exchange (for the docs).

Usage: uv run --extra interview python scripts/render_overlay_preview.py docs/media/interview-overlay.png [ru|en|pl]
"""

from __future__ import annotations

import os
import sys

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_SCALE_FACTOR", "2")

from careertwin.interview.overlay import Bridge, OverlayWindow, create_app  # noqa: E402

SAMPLES = {
    "en": (
        "Tell me about a time you fixed a production problem.",
        "POINTS:\n"
        "- **Situation:** order API latency doubled after a release\n"
        "- Traced it to a query with **no index** on customer id\n"
        "- Added the **index** and a latency alert\n"
        "- **Result:** back to normal within the hour\n"
        "ANSWER:\n"
        "After a release our order API got twice as slow. I traced it to a query without an "
        "index, added one, and we were back to normal within the hour.",
    ),
    "ru": (
        "Расскажите о случае, когда вы чинили проблему в продакшене.",
        "POINTS:\n"
        "- **Ситуация:** после релиза задержка order API выросла вдвое\n"
        "- Нашёл запрос **без индекса** по customer id\n"
        "- Добавил **индекс** и алерт на задержку\n"
        "- **Итог:** всё вернулось в норму за час\n"
        "ANSWER:\n"
        "После релиза наш order API замедлился вдвое. Я нашёл запрос без индекса, добавил его, "
        "и через час всё работало как раньше.",
    ),
}


def main(target: str, language: str = "en") -> None:
    app = create_app(sys.argv[:1])
    bridge = Bridge()
    window = OverlayWindow(
        bridge,
        on_ask=lambda _: None,
        on_hint=lambda: None,
        on_pause=lambda _: None,
        on_screenshot=lambda: None,
        on_add_document=lambda _: [],
        sources=[
            ("about", "ABOUT.md · 20.6k"),
            ("cv", "Maksim_Krauchyk_CV_Python_EN.pdf · 5.2k"),
            ("job", "Python Developer at Example Corp"),
        ],
        ui_language=language,
        frameless=True,
    )
    window.resize(900, 400)
    window.show()
    question, reply = SAMPLES[language if language in SAMPLES else "en"]
    bridge.state.emit("listening")
    bridge.level.emit("Interviewer", 0.62)
    bridge.turn.emit("Interviewer", question)
    bridge.started.emit(question)
    bridge.done.emit(reply)
    app.processEvents()
    window.grab().save(target)
    print(f"saved {target}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "overlay.png", sys.argv[2] if len(sys.argv) > 2 else "en")
