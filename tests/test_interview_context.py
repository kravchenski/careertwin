"""Interview context: CV attachment, long ABOUT.md, interview language, screenshots."""

from __future__ import annotations

import json
import threading
import time

import httpx
import numpy as np
import pytest

from careertwin import db
from careertwin.interview.brief import BriefError, InterviewBrief, fit_context
from careertwin.interview.coach import StreamingCoach
from careertwin.interview.documents import DocumentError, read_document
from careertwin.interview.languages import LanguageTracker, name, normalize
from careertwin.interview.prompts import user_prompt
from careertwin.interview.questions import Kind
from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
from careertwin.interview.transcript import TranscriptLog

CV_TEXT = (
    "Alex Demo, backend developer. Built FastAPI services on PostgreSQL at Northwind Logistics "
    "from 2022 to 2025 and deployed them with Docker. BSc Computer Science 2020."
)
ABOUT = "# Candidate\n\n## Experience\n- Built FastAPI services.\n\n## Skills\n- Python, Docker\n"


@pytest.fixture
def about(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    monkeypatch.delenv("CAREERTWIN_RESUME", raising=False)
    return path


def test_fit_context_keeps_stories_and_limits_when_it_must_cut():
    sections = {
        "Experience": "e" * 3000,
        "Interview stories (STAR)": "s" * 1000,
        "Honest limits": "h" * 300,
        "Never claim": "n" * 400,
        "CV files": "c" * 500,
        "Applications log": "a" * 6000,
    }
    text = "# Name\n\n" + "".join(f"## {title}\n{body}\n\n" for title, body in sections.items())
    fitted = fit_context(text, 6000)
    assert "## Interview stories" in fitted and "## Honest limits" in fitted and "## Never claim" in fitted
    assert "## Applications log" not in fitted and len(fitted) <= 6000
    assert fit_context(text, 100_000) == text


def test_long_about_is_not_cut_in_the_middle(about):
    about.write_text(ABOUT + "".join(f"## S{i}\n{'x' * 1000}\n" for i in range(20)) + "## Honest limits\nNo Kubernetes.\n")
    brief = InterviewBrief.load()
    assert "No Kubernetes." in brief.render()


def test_documents_read_text_pdf_docx(tmp_path):
    txt = tmp_path / "cv.txt"
    txt.write_text(CV_TEXT)
    assert "FastAPI" in read_document(txt)

    from docx import Document

    word = Document()
    word.add_paragraph(CV_TEXT)
    word.save(tmp_path / "cv.docx")
    assert "Northwind" in read_document(tmp_path / "cv.docx")

    weasyprint = pytest.importorskip("weasyprint")
    weasyprint.HTML(string=f"<p>{CV_TEXT}</p>").write_pdf(tmp_path / "cv.pdf")
    assert "Northwind" in read_document(tmp_path / "cv.pdf")


def test_documents_reject_blank_missing_and_unsupported(tmp_path):
    blank = tmp_path / "blank.txt"
    blank.write_text("hi")
    with pytest.raises(DocumentError, match="almost no readable text"):
        read_document(blank)
    with pytest.raises(DocumentError, match="does not exist"):
        read_document(tmp_path / "missing.pdf")
    odd = tmp_path / "cv.xyz"
    odd.write_text(CV_TEXT)
    with pytest.raises(DocumentError, match="unsupported"):
        read_document(odd)


def test_brief_attaches_cv_once_and_lists_sources(about, tmp_path):
    cv = tmp_path / "Alex_CV.txt"
    cv.write_text(CV_TEXT)
    brief = InterviewBrief.load(resumes=[str(cv)])
    again = brief.add_document(cv)
    assert len(brief.documents) == 1 and again is brief.documents[0]
    rendered = brief.render()
    assert "CV THE EMPLOYER HAS READ: Alex_CV.txt" in rendered and "Northwind Logistics" in rendered
    kinds = [kind for kind, _ in brief.sources()]
    assert kinds == ["about", "cv"] and "Alex_CV.txt" in brief.sources()[1][1]
    with pytest.raises(BriefError, match="almost no readable text"):
        brief.add_document(tmp_path / "also.txt") if (tmp_path / "also.txt").write_text("x") else None


def test_brief_reads_cvs_from_the_environment(about, tmp_path, monkeypatch):
    first, second = tmp_path / "a.txt", tmp_path / "b.txt"
    first.write_text(CV_TEXT)
    second.write_text(CV_TEXT + " Second version.")
    monkeypatch.setenv("CAREERTWIN_RESUME", f"{first}:{second}")
    assert [doc.name for doc in InterviewBrief.load().documents] == ["a.txt", "b.txt"]


def test_vacancy_id_attaches_the_cv_that_was_sent(about, tmp_path, monkeypatch):
    from careertwin.automation.models import Vacancy
    from careertwin.automation.store import Store

    monkeypatch.setattr(db, "DB_PATH", tmp_path / "pipeline.db")
    store = Store()
    store.initialize()
    job = store.add(Vacancy(url="https://example.com/jobs/1", title="Python Developer", company="Example"))
    sent = tmp_path / "sent_cv.txt"
    sent.write_text(CV_TEXT)
    store.update(job["id"], package={"cv_path": str(sent)})
    brief = InterviewBrief.load(job=job["id"])
    assert brief.job_title == "Python Developer"
    assert [doc.name for doc in brief.documents] == ["sent_cv.txt"]


def test_language_names_and_normalising():
    assert name("ru") == "Russian" and name("de") == "German" and name(None) == "English"
    assert normalize("ru-RU") == "ru" and normalize("Polish") == "pl" and normalize("auto") is None
    assert normalize("") is None and normalize("EN") == "en"


def test_language_tracker_is_sticky_and_respects_a_fixed_choice():
    tracker = LanguageTracker()
    assert tracker.observe("ru", 0.95) == "ru"
    assert tracker.observe("en", 0.9) is None and tracker.current == "ru"
    assert tracker.observe("pl", 0.3) is None
    assert tracker.observe("en", 0.9) == "en"
    fixed = LanguageTracker("de")
    assert fixed.observe("ru", 0.99) is None and fixed.current == "de"
    assert fixed.set_fixed(None) is None
    assert fixed.observe("fr", 0.9) == "fr"


def test_prompt_names_the_interview_language(about):
    brief = InterviewBrief.load()
    assert "INTERVIEW LANGUAGE: German" in user_prompt(brief, "Tell me about yourself", Kind.INTRO, [], "de")
    assert "INTERVIEW LANGUAGE: Russian" in user_prompt(brief, "Расскажите о себе", Kind.INTRO, [])
    assert "INTERVIEW LANGUAGE: English" in user_prompt(brief, "Tell me about yourself", Kind.INTRO, [])


def sse(*chunks: str) -> httpx.Response:
    lines = [f"data: {json.dumps({'choices': [{'delta': {'content': c}}]})}\n\n" for c in chunks]
    return httpx.Response(
        200, headers={"content-type": "text/event-stream"}, content="".join(lines + ["data: [DONE]\n\n"])
    )


def test_streaming_coach_sends_language_and_screenshot(about):
    sent = []

    def handler(request):
        sent.append(json.loads(request.content))
        return sse("ok")

    coach = StreamingCoach(InterviewBrief.load(), transport=httpx.MockTransport(handler))
    assert "".join(coach.stream("Why us?", [], threading.Event(), "pl")) == "ok"
    assert "INTERVIEW LANGUAGE: Polish" in sent[0]["messages"][1]["content"]
    list(coach.stream("[screenshot]", [], threading.Event(), "ru", image="QUJD"))
    parts = sent[1]["messages"][1]["content"]
    assert parts[0]["type"] == "text" and "INTERVIEW LANGUAGE: Russian" in parts[0]["text"]
    assert parts[1]["image_url"]["url"] == "data:image/png;base64,QUJD"


class RecordingCoach:
    def __init__(self):
        self.calls = []

    def stream(self, question, history, cancel, language=None, image=None):
        self.calls.append({"question": question, "language": language, "image": image})
        yield "POINTS:\n- a\nANSWER:\nb"


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


class DetectingTranscriber:
    def __init__(self, text, language, probability=0.95):
        self.text, self.last_language, self.last_probability = text, language, probability
        self.language = None

    def transcribe(self, pcm, prompt=""):
        return self.text


class OneShotSource:
    def __init__(self, audio):
        self.audio = audio

    def frames(self):
        for start in range(0, len(self.audio) - 319, 320):
            yield self.audio[start : start + 320]

    def close(self):
        pass


def speech_audio():
    t = np.arange(16000) / 16000
    tone = (4000 * np.sin(2 * np.pi * 220 * t)).astype(np.int16)
    silence = np.zeros(16000, dtype=np.int16)
    return np.concatenate([silence, tone, silence])


def test_session_answers_in_the_detected_interview_language(about, tmp_path):
    coach, languages_seen, levels = RecordingCoach(), [], []
    session = InterviewSession(
        InterviewBrief.load(),
        DetectingTranscriber("Расскажите о вашем опыте с Python?", "ru"),
        coach,
        Callbacks(on_language=languages_seen.append, on_level=lambda s, v: levels.append((s, v))),
        debounce=0.05,
        log=TranscriptLog(tmp_path),
    )
    session.start({INTERVIEWER: OneShotSource(speech_audio())})
    assert wait_for(lambda: coach.calls)
    assert languages_seen == ["ru"] and coach.calls[0]["language"] == "ru"
    assert levels and all(speaker == INTERVIEWER and 0 <= level <= 1 for speaker, level in levels)
    assert max(level for _, level in levels) > 0.3
    session.stop()


def test_a_fixed_language_overrides_detection_and_reaches_whisper(about, tmp_path):
    coach = RecordingCoach()
    transcriber = DetectingTranscriber("Tell me about your Python experience?", "en")
    session = InterviewSession(
        InterviewBrief.load(), transcriber, coach, language="de", debounce=0.05, log=TranscriptLog(tmp_path)
    )
    assert transcriber.language == "de"
    session.ask("Erzähl mir von dir")
    assert wait_for(lambda: coach.calls) and coach.calls[0]["language"] == "de"
    assert session.set_language("pl") == "pl" and transcriber.language == "pl"
    assert session.set_language("auto") is None and transcriber.language is None
    session.stop()


def test_screenshot_goes_to_the_coach_as_an_image(about, tmp_path):
    coach, turns = RecordingCoach(), []
    session = InterviewSession(
        InterviewBrief.load(), None, coach, Callbacks(on_turn=lambda s, t: turns.append((s, t))),
        log=TranscriptLog(tmp_path),
    )
    session.ask_screenshot("QUJD")
    assert wait_for(lambda: coach.calls)
    assert coach.calls[0]["image"] == "QUJD" and turns == [(INTERVIEWER, "[screenshot]")]
    session.hint_now()
    assert wait_for(lambda: len(coach.calls) == 1)
    session.stop()


def test_screenshot_tool_choice_depends_on_the_session(monkeypatch):
    from careertwin.interview import screenshot

    monkeypatch.setattr(screenshot.shutil, "which", lambda tool: "/usr/bin/" + tool if tool in {"maim", "grim"} else None)
    monkeypatch.setenv("WAYLAND_DISPLAY", "wayland-1")
    assert screenshot.available_tool() == "grim"
    monkeypatch.setattr(screenshot.shutil, "which", lambda tool: "/usr/bin/maim" if tool == "maim" else None)
    assert screenshot.available_tool() is None
    monkeypatch.delenv("WAYLAND_DISPLAY")
    assert screenshot.available_tool() == "maim"
    monkeypatch.setattr(screenshot.shutil, "which", lambda tool: None)
    with pytest.raises(screenshot.ScreenshotError, match="grim"):
        screenshot.capture_png()


def test_app_parser_accepts_resume_and_language_options():
    from careertwin.interview.app import build_parser

    args = build_parser().parse_args(
        ["--resume", "a.pdf", "--resume", "b.pdf", "--language", "ru", "--framed", "--no-pin"]
    )
    assert args.resume == ["a.pdf", "b.pdf"] and args.language == "ru" and args.framed and args.no_pin


def test_the_system_prompt_asks_for_words_to_say_not_advice():
    from careertwin.interview.prompts import KIND_HINTS, STYLE_HINTS, SYSTEM

    assert "the actual words to say" in SYSTEM and "never advice" in SYSTEM
    assert "Greet warmly" in SYSTEM and "Wrong:" in SYSTEM and "Right:" in SYSTEM
    assert "first person" in SYSTEM and "No bluff" in SYSTEM
    assert "say the greeting" in KIND_HINTS[Kind.SMALL_TALK].lower().replace("small talk: ", "")
    assert all("lines" in hint for hint in STYLE_HINTS.values())


def test_definition_questions_get_an_explanation_not_a_greeting(about):
    from careertwin.interview.prompts import SYSTEM, user_prompt

    brief = InterviewBrief.load()
    prompt = user_prompt(
        brief, "что такое pytohn", Kind.DEFINITION, ["Interviewer: hi", "Interviewer: Tell me about yourself"], "ru"
    )
    assert "TYPE: definition" in prompt and "line one defines it" in prompt
    assert "INTERVIEW LANGUAGE: Russian" in prompt and "Interviewer: hi" in prompt
    assert "Answer ONLY the question asked now" in SYSTEM and "never repeat an earlier greeting" in SYSTEM
    assert "pytohn" in SYSTEM and "No greeting, no self-introduction" in SYSTEM


def test_prepare_parsers_accept_good_json_and_reject_junk():
    import pytest

    from careertwin.interview.prepare import PrepareError, parse_analysis, parse_questions

    bank = parse_questions('{"questions": [{"question": " Why? ", "kind": "motivation", "outline": ["a"]}, {"x": 1}]}', 5)
    assert bank == [{"question": "Why?", "kind": "motivation", "outline": ["a"]}]
    for junk in ("", "not json", '{"questions": []}'):
        with pytest.raises(PrepareError):
            parse_questions(junk, 5)
    review = parse_analysis('{"summary": "ok", "gaps": [{"gap": "K8s", "how_to_address": "be honest"}, {"nope": 1}]}')
    assert review["gaps"] == [{"gap": "K8s", "how_to_address": "be honest"}] and review["strengths"] == []
    with pytest.raises(PrepareError):
        parse_analysis('{"strengths": ["x"]}')


def test_coach_complete_uses_the_in_app_gateway_settings(about):
    seen = {}

    def handler(request):
        seen.update(url=str(request.url), auth=request.headers.get("authorization"), body=json.loads(request.content))
        return httpx.Response(200, json={"choices": [{"message": {"content": "hello"}}]})

    coach = StreamingCoach(
        InterviewBrief.load(), url="http://gw:9/", api_key="k", model="qwen-chat",
        transport=httpx.MockTransport(handler),
    )
    assert coach.complete("system", "prompt", max_tokens=50) == "hello"
    assert seen["url"] == "http://gw:9/v1/chat/completions" and seen["auth"] == "Bearer k"
    assert seen["body"]["model"] == "qwen-chat" and seen["body"]["max_tokens"] == 50
    from careertwin.interview.coach import CoachError

    for response in (httpx.Response(503, text="no"), httpx.Response(200, json={"choices": [{"message": {"content": " "}}]}), httpx.Response(200, text="<html>")):
        bad = StreamingCoach(InterviewBrief.load(), transport=httpx.MockTransport(lambda r, resp=response: resp))
        with pytest.raises(CoachError):
            bad.complete("s", "p")
