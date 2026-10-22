"""`careertwin-interview`: live interview helper with a Qt overlay."""

from __future__ import annotations

import argparse
import os
import shutil
import sys

import httpx

from careertwin.interview.audio import AudioError, ParecSource, default_sources, list_sources
from careertwin.interview.brief import BriefError, InterviewBrief
from careertwin.interview.coach import StreamingCoach, gateway_url, model_name
from careertwin.interview.languages import normalize
from careertwin.interview.screenshot import ScreenshotError, available_tool, capture_png, to_base64
from careertwin.interview.session import INTERVIEWER, YOU, InterviewSession
from careertwin.interview.stt import WhisperTranscriber


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="careertwin-interview",
        description="Live help in online interviews: local transcription, grounded answer hints.",
    )
    parser.add_argument("--job", help="Vacancy id from the pipeline, a text file, or a URL")
    parser.add_argument("--username", default="default", help="Whose ABOUT.md to use")
    parser.add_argument("--notes", default="", help="Extra facts for this interview")
    parser.add_argument(
        "--resume", action="append", default=[], metavar="FILE",
        help="CV to read: PDF, DOCX or text; repeat for several (also CAREERTWIN_RESUME)",
    )
    parser.add_argument("--whisper", default=None, help="Whisper model: base, small, medium, large-v3-turbo")
    parser.add_argument(
        "--language", default="auto",
        help="Interview language for speech and answers: auto, en, ru, pl, de, ... (default: auto)",
    )
    parser.add_argument("--model", help="Gateway model id (INTERVIEW_MODEL, SWITCHYARD_MODEL, auto)")
    parser.add_argument("--source", help="Pulse source carrying the call audio (default: sink monitor)")
    parser.add_argument("--mic", action="store_true", help="Also transcribe your microphone (use headphones)")
    parser.add_argument("--text-only", action="store_true", help="No audio: type or paste questions")
    parser.add_argument("--xcb", action="store_true", help="Run through XWayland so 'Pin' works on Wayland")
    parser.add_argument("--framed", action="store_true", help="Normal window with a title bar")
    parser.add_argument("--no-pin", action="store_true", help="Do not keep the window on top")
    parser.add_argument("--list-sources", action="store_true", help="Print audio sources and exit")
    parser.add_argument("--doctor", action="store_true", help="Check the setup and exit")
    return parser


def doctor(args: argparse.Namespace) -> int:
    problems = 0

    def report(ok: bool, text: str) -> None:
        nonlocal problems
        problems += not ok
        print(f"{'ok ' if ok else 'FAIL'}  {text}")

    try:
        import PySide6  # noqa: F401
        report(True, "PySide6 installed")
    except ImportError:
        report(False, "PySide6 missing: uv sync --extra interview")
    try:
        import faster_whisper  # noqa: F401
        report(True, "faster-whisper installed")
    except ImportError:
        report(False, "faster-whisper missing: uv sync --extra interview")
    for binary in ("parec", "pactl"):
        report(bool(shutil.which(binary)), f"{binary} found (pipewire-pulse)")
    try:
        sources = default_sources()
        report(True, f"call audio: {sources.interviewer}; microphone: {sources.microphone}")
    except AudioError as exc:
        report(False, str(exc))
    try:
        response = httpx.get(f"{gateway_url()}/v1/models", timeout=5)
        report(response.status_code < 400, f"Switchyard {gateway_url()} answers (model {args.model or model_name()})")
    except httpx.HTTPError:
        report(False, f"Switchyard not reachable at {gateway_url()}")
    try:
        brief = InterviewBrief.load(
            username=args.username, job=args.job, notes=args.notes, resumes=args.resume
        )
        report(True, f"ABOUT.md loaded ({len(brief.candidate)} characters)")
        if brief.documents:
            for doc in brief.documents:
                report(True, f"CV loaded: {doc.name} ({len(doc.text)} characters)")
        else:
            print("note  no CV attached: pass --resume cv.pdf, set CAREERTWIN_RESUME, or drop a PDF on the window")
    except BriefError as exc:
        report(False, str(exc))
    tool = available_tool()
    print(f"{'ok ' if tool else 'note'}  screenshots: {tool or 'no tool (install grim, spectacle, maim or scrot)'}")
    return 1 if problems else 0


def main(argv: list[str] | None = None) -> None:
    args = build_parser().parse_args(argv)
    if args.list_sources:
        try:
            print("\n".join(list_sources()))
        except AudioError as exc:
            raise SystemExit(str(exc)) from exc
        return
    if args.doctor:
        raise SystemExit(doctor(args))
    if args.xcb:
        os.environ["QT_QPA_PLATFORM"] = "xcb"

    try:
        brief = InterviewBrief.load(
            username=args.username, job=args.job, notes=args.notes, resumes=args.resume
        )
        from PySide6.QtCore import QLocale, QTimer

        from careertwin.interview.overlay import Bridge, OverlayWindow, create_app
    except BriefError as exc:
        raise SystemExit(f"error: {exc}") from exc
    except ImportError as exc:
        raise SystemExit(f"error: {exc}. Install with: uv sync --extra interview") from exc

    app = create_app(sys.argv[:1])
    bridge = Bridge()
    language = normalize(args.language)
    coach = StreamingCoach(brief, model=args.model)
    transcriber = None if args.text_only else WhisperTranscriber(args.whisper, language=language)
    session = InterviewSession(brief, transcriber, coach, bridge.callbacks(), language=language)

    sources = {}
    if not args.text_only:
        try:
            defaults = default_sources()
            sources[INTERVIEWER] = ParecSource(args.source or defaults.interviewer)
            if args.mic:
                sources[YOU] = ParecSource(defaults.microphone)
        except AudioError as exc:
            raise SystemExit(f"error: {exc}") from exc

    def add_document(path: str) -> list[tuple[str, str]]:
        brief.add_document(path)
        return brief.sources()

    window_ref: list = []

    def take_screenshot() -> None:
        window = window_ref[0]
        previous = window.windowOpacity()
        window.setWindowOpacity(0.0)
        app.processEvents()

        def capture() -> None:
            try:
                png = capture_png()
            except ScreenshotError as exc:
                window.setWindowOpacity(previous)
                bridge.error.emit(str(exc))
                return
            window.setWindowOpacity(previous)
            session.ask_screenshot(to_base64(png))

        QTimer.singleShot(250, capture)

    window = OverlayWindow(
        bridge,
        on_ask=session.ask,
        on_hint=session.hint_now,
        on_pause=session.pause,
        on_screenshot=take_screenshot if available_tool() else None,
        on_add_document=add_document,
        on_language=session.set_language,
        sources=brief.sources(),
        listening=not args.text_only,
        ui_language=language or QLocale().name().split("_")[0],
        frameless=not args.framed,
        pinned=not args.no_pin,
    )
    window_ref.append(window)
    window.show()
    session.start(sources)
    code = app.exec()
    session.stop()
    if saved := session.log.save():
        print(f"Transcript saved to {saved}")
    raise SystemExit(code)


if __name__ == "__main__":
    main()
