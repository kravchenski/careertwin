"""`careertwin-interview`: live interview helper with a Qt window. Everything is set in the window."""

from __future__ import annotations

import argparse
import os
import shutil
import sys

import httpx

from careertwin.interview.audio import (
    AudioError,
    audio_state,
    default_sources,
    list_sources,
    listen_briefly,
    next_monitor,
)
from careertwin.interview.brief import BriefError, InterviewBrief
from careertwin.interview.coach import StreamingCoach, gateway_url, model_name
from careertwin.interview.languages import normalize
from careertwin.interview.library import ContextController, Library
from careertwin.interview.providers import base_url
from careertwin.interview.runtime import Runtime
from careertwin.interview.screenshot import available_tools
from careertwin.interview.session import InterviewSession
from careertwin.interview.settings import InterviewSettings
from careertwin.interview.stt import WhisperTranscriber
from careertwin.interview.transcript import TranscriptLog


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="careertwin-interview",
        description=(
            "Live help in online interviews: local transcription, grounded answer hints. "
            "Every option below is also in the window's Settings; flags override the saved "
            "settings for this run only."
        ),
    )
    parser.add_argument("--job", help="Vacancy id from the pipeline, a text file, or a URL")
    parser.add_argument("--username", default="default", help="Whose ABOUT.md to use")
    parser.add_argument("--notes", default="", help="Extra facts for this interview")
    parser.add_argument(
        "--resume", action="append", default=[], metavar="FILE",
        help="CV to read: PDF, DOCX or text; repeat for several (also CAREERTWIN_RESUME)",
    )
    parser.add_argument("--language", help="Interview language: auto, en, ru, pl, de, ...")
    parser.add_argument("--whisper", help="Speech model: tiny, base, small, medium, large-v3-turbo")
    parser.add_argument("--model", help="Gateway model id")
    parser.add_argument("--source", help="Pulse source carrying the call audio (default: sink monitor)")
    parser.add_argument("--mic", action="store_true", help="Also transcribe your microphone (use headphones)")
    parser.add_argument("--text-only", action="store_true", help="No audio: type or paste questions")
    parser.add_argument("--xcb", action="store_true", help="Run through XWayland so 'pin on top' works")
    parser.add_argument("--framed", action="store_true", help="Normal window with a title bar")
    parser.add_argument("--no-pin", action="store_true", help="Do not keep the window on top")
    parser.add_argument("--list-sources", action="store_true", help="Print audio sources and exit")
    parser.add_argument("--doctor", action="store_true", help="Check the setup and exit")
    return parser


def settings_for_run(args: argparse.Namespace) -> InterviewSettings:
    """The saved settings with this run's command-line overrides (not saved)."""
    settings = InterviewSettings.load()
    overrides = {
        "language": args.language,
        "whisper_model": args.whisper,
        "model": args.model,
        "call_source": args.source,
        "use_mic": True if args.mic else None,
        "listen": False if args.text_only else None,
        "frameless": False if args.framed else None,
        "pinned": False if args.no_pin else None,
    }
    for name, value in overrides.items():
        if value is not None:
            settings.set(name, value, save=False)
    return settings


def doctor(args: argparse.Namespace) -> int:
    problems = 0

    def report(ok: bool, text: str) -> None:
        nonlocal problems
        problems += not ok
        print(f"{'ok ' if ok else 'FAIL'}  {text}")

    settings = settings_for_run(args)
    try:
        import PySide6  # noqa: F401

        report(True, "PySide6 installed")
    except ImportError:
        report(False, "PySide6 missing: uv sync --extra interview")
    try:
        import faster_whisper  # noqa: F401

        report(True, f"faster-whisper installed (model {settings.whisper_model})")
        try:
            probe = WhisperTranscriber(settings.whisper_model, device=settings.whisper_device)
            probe.load()
            where = "the GPU (CUDA)" if probe.active_device == "cuda" else "the CPU"
            report(True, f"speech recognition runs on {where}")
        except Exception as exc:
            report(False, f"speech model did not load: {exc}")
    except ImportError:
        report(False, "faster-whisper missing: uv sync --extra interview")
    if sys.platform != "win32":
        for binary in ("parec", "pactl"):
            report(bool(shutil.which(binary)), f"{binary} found (pipewire-pulse)")
    try:
        sources = default_sources()
        if settings.call_source:
            call = f"{settings.call_source} (pinned)"
        else:
            default, playing = audio_state()
            call = f"automatic, now {next_monitor(None, default, playing, None)}"
        report(True, f"call audio: {call}; microphone: {sources.microphone}")
        if sys.platform == "win32":
            level = listen_briefly(settings.call_source)
            report(True, f"call audio opens; peak level {level} (0 when nothing plays: play something and check again)")
    except AudioError as exc:
        report(False, str(exc))
    base = base_url(settings.gateway_url or gateway_url())
    try:
        response = httpx.get(f"{base}/v1/models", timeout=5)
        report(response.status_code < 400, f"AI gateway {base} answers (model {settings.model or model_name()})")
    except httpx.HTTPError:
        report(False, f"no AI gateway answers at {base}: pick a provider in Settings → AI model")
    try:
        brief = InterviewBrief.load(
            username=args.username, job=args.job, notes=args.notes, resumes=args.resume, allow_empty=True
        )
        ContextController(brief, Library.load()).restore()
        if brief.candidate.strip():
            report(True, f"ABOUT.md loaded ({len(brief.candidate)} characters)")
        else:
            print("note  no ABOUT.md yet: write it in Settings → About me")
        for doc in brief.documents:
            report(True, f"{doc.kind}: {doc.name} ({len(doc.text)} characters)")
        if brief.is_empty():
            report(False, "no context: add your CV or ABOUT in the window's Settings")
    except BriefError as exc:
        report(False, str(exc))
    tools = available_tools()
    print(f"{'ok ' if tools else 'note'}  screenshots: {', '.join(tools) or 'no tool (install grim, spectacle, maim or scrot)'}")
    print(f"note  settings file: {InterviewSettings.path()}")
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
        from PySide6.QtCore import QLocale

        from careertwin.interview.overlay import Bridge, OverlayWindow, create_app
    except ImportError as exc:
        raise SystemExit(f"error: {exc}. Install with: uv sync --extra interview") from exc
    try:
        brief = InterviewBrief.load(
            username=args.username, job=args.job, notes=args.notes, resumes=args.resume, allow_empty=True
        )
    except BriefError as exc:
        raise SystemExit(f"error: {exc}") from exc

    settings = settings_for_run(args)
    context = ContextController(brief, Library.load())
    problems = context.restore()
    language = normalize(settings.language)
    coach = StreamingCoach(brief)
    transcriber = (
        WhisperTranscriber(settings.whisper_model, language=language, device=settings.whisper_device)
        if settings.listen
        else None
    )
    app = create_app(sys.argv[:1])
    bridge = Bridge()
    session = InterviewSession(
        brief, transcriber, coach, bridge.callbacks(), language=language, log=TranscriptLog()
    )
    runtime = Runtime(settings, context, coach, session)
    runtime.on_error = bridge.error.emit
    runtime.apply_all()

    ui = settings.ui_language if settings.ui_language != "auto" else language
    window = OverlayWindow(
        bridge,
        runtime=runtime,
        listening=settings.listen,
        ui_language=ui or QLocale().name().split("_")[0],
        frameless=settings.frameless,
        pinned=settings.pinned,
    )
    runtime.on_window = window.apply_setting
    window.apply_settings(settings)
    window.refresh_controls()
    window.show()
    if problems:
        bridge.error.emit(problems[0])
    runtime.start_audio()
    code = app.exec()
    session.stop()
    if settings.save_transcripts and (saved := session.log.save()):
        print(f"Transcript saved to {saved}")
    raise SystemExit(code)


if __name__ == "__main__":
    main()
