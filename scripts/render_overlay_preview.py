#!/usr/bin/env python3
"""Render the interview overlay offscreen with a sample exchange (for the docs).

Usage: uv run --extra interview python scripts/render_overlay_preview.py docs/media/interview-overlay.png [language] [answer|code|technical]

"code" shows the answer to a screenshot of a coding task; "technical" shows a design question with its
likely next questions.
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
    "pl": (
        "Czym jest Python?",
        "POINTS:\n"
        "- **Python** to wysokopoziomowy, interpretowany język z typowaniem dynamicznym.\n"
        "- Znany z **czytelnej składni** i dużej biblioteki standardowej.\n"
        "- Używany do **backendów webowych**, automatyzacji, danych i uczenia maszynowego.\n"
        "- Kompromis: jest **wolniejszy** od języków kompilowanych, a GIL ogranicza wątki.\n"
        "ANSWER:\n"
        "Python to język ogólnego przeznaczenia, interpretowany i dynamicznie typowany, stawiający na czytelność. "
        "Ogromny ekosystem sprawia, że jest częstym wyborem do backendów, skryptów i pracy z danymi.\n"
        "KEY:\n"
        "- **Typowanie dynamiczne**: typy sprawdzane są w trakcie działania, kod pisze się szybciej.\n"
        "- **GIL**: bajtkod wykonuje jeden wątek naraz, więc ciężkie zadania idą do procesów.\n"
        "- **Ekosystem**: FastAPI, Django, asyncio, NumPy i tysiące pakietów.\n"
        "CONTEXT:\n"
        "Używam go do asynchronicznych usług na FastAPI i PostgreSQL, a ciężkie zadania wynoszę do osobnych procesów.",
    ),
    "de": (
        "Was ist Python?",
        "POINTS:\n"
        "- **Python** ist eine höhere, interpretierte Sprache mit dynamischer Typisierung.\n"
        "- Bekannt für **lesbare Syntax** und eine große Standardbibliothek.\n"
        "- Eingesetzt für **Web-Backends**, Automatisierung, Daten und maschinelles Lernen.\n"
        "- Der Preis: Sie ist **langsamer** als kompilierte Sprachen, und die GIL begrenzt Threads.\n"
        "ANSWER:\n"
        "Python ist eine universelle, interpretierte Sprache mit dynamischer Typisierung und Fokus auf Lesbarkeit. "
        "Das große Ökosystem macht sie zur häufigen Wahl für Backends, Skripte und Datenarbeit.\n"
        "KEY:\n"
        "- **Dynamische Typisierung**: Typen werden zur Laufzeit geprüft, der Code entsteht schneller.\n"
        "- **GIL**: Ein Thread führt Bytecode aus, daher läuft Schweres in Prozessen.\n"
        "- **Ökosystem**: FastAPI, Django, asyncio, NumPy und tausende Pakete.\n"
        "CONTEXT:\n"
        "Ich nutze sie für asynchrone Dienste mit FastAPI und PostgreSQL und lagere Schweres in eigene Prozesse aus.",
    ),
    "fr": (
        "Qu’est-ce que Python ?",
        "POINTS:\n"
        "- **Python** est un langage de haut niveau, interprété, à typage dynamique.\n"
        "- Connu pour sa **syntaxe lisible** et sa grande bibliothèque standard.\n"
        "- Utilisé pour les **backends web**, l’automatisation, les données et le machine learning.\n"
        "- Le compromis : il est **plus lent** que les langages compilés, et le GIL limite les threads.\n"
        "ANSWER:\n"
        "Python est un langage généraliste, interprété et à typage dynamique, qui privilégie la lisibilité. "
        "Son immense écosystème en fait un choix courant pour les backends, les scripts et les données.\n"
        "KEY:\n"
        "- **Typage dynamique** : les types sont vérifiés à l’exécution, on écrit plus vite.\n"
        "- **GIL** : un seul thread exécute le bytecode, le lourd passe donc par des processus.\n"
        "- **Écosystème** : FastAPI, Django, asyncio, NumPy et des milliers de paquets.\n"
        "CONTEXT:\n"
        "Je l’utilise pour des services asynchrones avec FastAPI et PostgreSQL, et je déporte le lourd dans des processus séparés.",
    ),
    "es": (
        "¿Qué es Python?",
        "POINTS:\n"
        "- **Python** es un lenguaje de alto nivel, interpretado y de tipado dinámico.\n"
        "- Conocido por su **sintaxis legible** y su gran biblioteca estándar.\n"
        "- Se usa en **backends web**, automatización, datos y aprendizaje automático.\n"
        "- El compromiso: es **más lento** que los lenguajes compilados, y el GIL limita los hilos.\n"
        "ANSWER:\n"
        "Python es un lenguaje de propósito general, interpretado y de tipado dinámico, que prioriza la legibilidad. "
        "Su enorme ecosistema lo hace habitual en backends, scripts y trabajo con datos.\n"
        "KEY:\n"
        "- **Tipado dinámico**: los tipos se comprueban al ejecutar, se escribe más rápido.\n"
        "- **GIL**: un solo hilo ejecuta bytecode, así que lo pesado va en procesos.\n"
        "- **Ecosistema**: FastAPI, Django, asyncio, NumPy y miles de paquetes.\n"
        "CONTEXT:\n"
        "Lo uso para servicios asíncronos con FastAPI y PostgreSQL, y llevo lo pesado a procesos aparte.",
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


CODE_REPLY = (
    "POINTS:\n"
    "- The array is **sorted**, so use **two pointers**: one at each end.\n"
    "- Move the left one up when the sum is too small, the right one down when it is too big.\n"
    "- It is **O(n)** time and **O(1)** space, with no hash map.\n"
    "ANSWER:\n"
    "```python\n"
    "def two_sum(nums: list[int], target: int) -> list[int]:\n"
    "    # Time O(n), space O(1)\n"
    "    left, right = 0, len(nums) - 1\n"
    "    while left < right:\n"
    "        total = nums[left] + nums[right]\n"
    "        if total == target:\n"
    "            return [left + 1, right + 1]\n"
    "        if total < target:\n"
    "            left += 1\n"
    "        else:\n"
    "            right -= 1\n"
    "    return []\n"
    "```"
)


TECHNICAL_QUESTION = "How would you design a rate limiter for an API?"
TECHNICAL_REPLY = (
    "POINTS:\n"
    "- I would assume **many instances**, so the counters must be **shared**, not per process.\n"
    "- A **token bucket** in **Redis**: each request takes a token, an empty bucket means **429**.\n"
    "- The bucket size sets the allowed burst, the refill rate sets the steady limit.\n"
    "- The take must be **atomic**: one Lua script, not a read followed by a write.\n"
    "- If Redis is down I **fail open** with a small local limit and raise an alert.\n"
    "- I work with **FastAPI** and **Docker** for API services.\n"
    "ANSWER:\n"
    "I would put a token bucket per client key in Redis, because the limit has to hold across all "
    "instances. A middleware takes a token with one atomic script and answers 429 with a Retry-After "
    "header when the bucket is empty. The cost is one network hop per request and a dependency on "
    "Redis, so I would fall back to a small local limit if it is unreachable, and watch the rejection "
    "rate and the added latency.\n"
    "KEY:\n"
    "- **Token bucket**: allows bursts up to the bucket size and refills at a fixed rate.\n"
    "- **Atomic take**: a Lua script keeps two instances from spending the same token.\n"
    "- **Fail open or closed**: open keeps the API up, closed protects the backend; say which and why.\n"
    "CONTEXT:\n"
    "Public APIs use this to protect downstream services and to enforce per-plan quotas.\n"
    "NEXT:\n"
    "- What if one client uses many keys? - Limit by IP and account as well, and add a global ceiling.\n"
    "- Why not a fixed window? - A client can send twice the limit around the window edge.\n"
)


def main(target: str, language: str = "en", kind: str = "answer") -> None:
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
    window.resize(900, {"code": 640, "technical": 900}.get(kind, 700))
    window.apply_setting("show_full_answer", True)
    window.show()
    question, reply = SAMPLES[language if language in SAMPLES else "en"]
    if kind == "code":
        question, reply = "[screenshot]", CODE_REPLY
    elif kind == "technical":
        question, reply = TECHNICAL_QUESTION, TECHNICAL_REPLY
    bridge.state.emit("listening")
    bridge.level.emit("Interviewer", 0.62)
    bridge.turn.emit("Interviewer", question)
    bridge.started.emit(question)
    bridge.done.emit(reply)
    app.processEvents()
    window.grab().save(target)
    print(f"saved {target}")


if __name__ == "__main__":
    main(*(sys.argv[1:4] or ["overlay.png"]))
