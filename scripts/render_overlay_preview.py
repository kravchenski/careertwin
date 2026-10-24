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
        "What is Python?",
        "POINTS:\n"
        "- **Python** is a high-level, interpreted language with dynamic typing.\n"
        "- It is known for **readable syntax** and a huge standard library.\n"
        "- It is used for **web backends**, automation, data and machine learning.\n"
        "- The trade-off: it is **slower** than compiled languages, and the GIL limits threads.\n"
        "ANSWER:\n"
        "Python is a general-purpose, high-level language that is interpreted and dynamically typed. "
        "It favours readability and has a very large ecosystem, which is why it is common in backends, "
        "scripting and data work.\n"
        "KEY:\n"
        "- **Dynamic typing**: types are checked at run time, which speeds up writing code.\n"
        "- **GIL**: one thread runs Python bytecode at a time, so CPU-bound work uses processes.\n"
        "- **Ecosystem**: FastAPI, Django, asyncio, NumPy and thousands of packages.\n"
        "CONTEXT:\n"
        "I use it for async services on FastAPI and PostgreSQL, and move heavy work into separate "
        "processes or services.",
    ),
    "uk": (
        "Що таке Python?",
        "POINTS:\n"
        "- **Python** — високорівнева інтерпретована мова з динамічною типізацією.\n"
        "- Відома **читабельним синтаксисом** і великою стандартною бібліотекою.\n"
        "- Її застосовують для **веббекенду**, автоматизації, даних і машинного навчання.\n"
        "- Компроміс: вона **повільніша** за компільовані мови, а GIL обмежує потоки.\n"
        "ANSWER:\n"
        "Python — мова загального призначення: інтерпретована, з динамічною типізацією та наголосом на "
        "читабельності. Величезна екосистема робить її частим вибором для бекенду, скриптів і даних.\n"
        "KEY:\n"
        "- **Динамічна типізація**: типи перевіряються під час виконання, код пишеться швидше.\n"
        "- **GIL**: байткод виконує один потік за раз, тож важке виносять у процеси.\n"
        "- **Екосистема**: FastAPI, Django, asyncio, NumPy і тисячі пакетів.\n"
        "CONTEXT:\n"
        "Я використовую її для async-сервісів на FastAPI і PostgreSQL, а важкі ділянки виношу в окремі "
        "процеси або сервіси.",
    ),
    "ru": (
        "Что такое Python?",
        "POINTS:\n"
        "- **Python** — высокоуровневый интерпретируемый язык с динамической типизацией.\n"
        "- Известен **читаемым синтаксисом** и большой стандартной библиотекой.\n"
        "- Применяют для **веб-бэкенда**, автоматизации, данных и машинного обучения.\n"
        "- Компромисс: он **медленнее** компилируемых языков, а GIL ограничивает потоки.\n"
        "ANSWER:\n"
        "Python — язык общего назначения: интерпретируемый, с динамической типизацией и упором на "
        "читаемость. Огромная экосистема делает его частым выбором для бэкенда, скриптов и данных.\n"
        "KEY:\n"
        "- **Динамическая типизация**: типы проверяются при выполнении, код пишется быстрее.\n"
        "- **GIL**: байткод выполняет один поток за раз, поэтому тяжёлое делают в процессах.\n"
        "- **Экосистема**: FastAPI, Django, asyncio, NumPy и тысячи пакетов.\n"
        "CONTEXT:\n"
        "Я использую его для async-сервисов на FastAPI и PostgreSQL, а тяжёлые участки выношу в "
        "отдельные процессы или сервисы.",
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
        sources=[
            ("about", "ABOUT.md · 20.6k"),
            ("cv", "Alex_Demo_CV.pdf · 5.2k"),
            ("job", "Python Developer at Example Corp"),
        ],
        ui_language=language,
        frameless=True,
    )
    window.resize(900, 700)
    window.apply_setting("show_full_answer", True)
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
