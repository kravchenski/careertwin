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


def test_replies_split_into_points_answer_key_and_context():
    from careertwin.interview.prompts import parse_suggestion, sections, split_partial

    full = (
        "POINTS:\n- **React** is a UI library\n- built on **components**\n"
        "ANSWER:\nReact is a library.\nKEY:\n- **Components**: reusable UI units\n"
        "- **Virtual DOM**: batches updates\nCONTEXT:\nUsed for SPAs and React Native."
    )
    parts = sections(full)
    assert parts.points == ["**React** is a UI library", "built on **components**"]
    assert parts.answer == "React is a library." and parts.has_details
    assert parts.key == ["**Components**: reusable UI units", "**Virtual DOM**: batches updates"]
    assert parts.context == "Used for SPAs and React Native."
    assert parse_suggestion(full).as_dict() == {
        "points": ["React is a UI library", "built on components"],
        "answer": "React is a library.",
        "key_points": ["Components: reusable UI units", "Virtual DOM: batches updates"],
        "context": "Used for SPAs and React Native.",
    }
    assert sections("POINTS:\n- **Py** a\n- **Do").points == ["**Py** a", "**Do"]
    assert not sections("POINTS:\n- x").has_details
    assert sections("plain text").answer == "plain text" and sections("plain text").points == []
    assert split_partial("POINTS:\n- a\nANSWER:\nb") == (["a"], "b")
    assert sections("**POINTS:**\n- a\n**ANSWER:** b\n**KEY:**\n- **T**: x").key == ["**T**: x"]


def test_the_prompt_asks_for_the_four_sections_and_drops_two_when_concise():
    from careertwin.interview.prompts import STYLE_HINTS, SYSTEM

    for marker in ("POINTS:", "ANSWER:", "KEY:", "CONTEXT:"):
        assert marker in SYSTEM
    assert "leave out KEY and CONTEXT" in SYSTEM
    assert "no KEY or CONTEXT" in STYLE_HINTS["concise"] and "KEY" in STYLE_HINTS["detailed"]


def test_answers_skip_the_bookkeeping_sections_but_keep_everything_useful(about):
    from careertwin.interview.brief import slim_for_answers

    text = (
        "# Name\n\n## Experience\n- Built FastAPI services.\n\n## Projects\n- BookSeat.\n\n"
        "### Project catalog\n| Repo | Use |\n| --- | --- |\n| a | b |\n\n"
        "## Matching a job ad: must-have and nice-to-have\nrules\n\n## Skills\n- Python\n\n"
        "## Interview stories (STAR)\n- Live platforms.\n\n## Honest limits\n- English is B2+.\n\n"
        "## CV files\n- names\n\n## Applications log\n| Acme | Java |\n"
    )
    slim = slim_for_answers(text)
    for kept in ("## Experience", "## Projects", "- BookSeat.", "## Skills", "## Interview stories", "## Honest limits"):
        assert kept in slim
    for gone in ("Project catalog", "Matching a job ad", "CV files", "Applications log", "| Acme |"):
        assert gone not in slim
    about.write_text(text)
    rendered = InterviewBrief.load().render()
    assert "Interview stories" in rendered and "Applications log" not in rendered and "Project catalog" not in rendered


def test_the_prompt_can_ask_for_the_main_answer_only(about):
    from careertwin.interview.prompts import SYSTEM, screenshot_prompt, user_prompt

    brief = InterviewBrief.load()
    main = user_prompt(brief, "Why this internship?", Kind.MOTIVATION, [], "en", "standard", "main")
    assert "ANSWER LAYOUT: main answer only" in main and "only the ANSWER section" in main
    assert "ANSWER LAYOUT: lines to say" in user_prompt(brief, "Why?", Kind.MOTIVATION, [], "en")
    assert "main answer only" in screenshot_prompt(brief, "en", "main")
    assert "reply with just \"ANSWER:\"" in SYSTEM and "start the paragraph at once" in SYSTEM
    assert "Never invent routines, methods, habits, tools or examples" in SYSTEM
    assert "time-boxed" not in SYSTEM


def test_the_coach_sends_the_chosen_layout(about):
    seen = []

    def handler(request):
        seen.append(json.loads(request.content)["messages"][1]["content"])
        return sse("ANSWER:\nok")

    coach = StreamingCoach(InterviewBrief.load(), transport=httpx.MockTransport(handler))
    list(coach.stream("Why us?", [], threading.Event(), "en"))
    coach.layout = "main"
    list(coach.stream("Why us?", [], threading.Event(), "en"))
    assert "lines to say, then the detailed answer" in seen[0]
    assert "ANSWER LAYOUT: main answer only" in seen[1]


def test_a_failing_model_falls_back_to_automatic_routing_before_the_first_word(about):
    from careertwin.interview.coach import CoachError

    models = []

    def handler(request):
        model = json.loads(request.content)["model"]
        models.append(model)
        if model == "groq/openai/gpt-oss-120b":
            return httpx.Response(429, text="rate limited")
        return sse("ANSWER:\n", "from auto")

    coach = StreamingCoach(
        InterviewBrief.load(), model="groq/openai/gpt-oss-120b", transport=httpx.MockTransport(handler)
    )
    assert "".join(coach.stream("Why us?", [], threading.Event(), "en")) == "ANSWER:\nfrom auto"
    assert models == ["groq/openai/gpt-oss-120b", "auto"]

    models.clear()
    both_fail = StreamingCoach(
        InterviewBrief.load(), model="x", transport=httpx.MockTransport(lambda r: httpx.Response(503, text="down"))
    )
    with pytest.raises(CoachError, match="503"):
        list(both_fail.stream("Why us?", [], threading.Event(), "en"))

    auto = StreamingCoach(InterviewBrief.load(), model="auto", transport=httpx.MockTransport(lambda r: models.append("auto") or httpx.Response(503, text="down")))
    with pytest.raises(CoachError):
        list(auto.stream("Why us?", [], threading.Event(), "en"))
    assert models == ["auto"]


def test_no_fallback_once_words_have_been_shown(about):

    calls = []

    def handler(request):
        calls.append(json.loads(request.content)["model"])
        lines = f"data: {json.dumps({'choices': [{'delta': {'content': 'hi'}}]})}\n\n"
        return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=lines + "data: {broken")

    coach = StreamingCoach(InterviewBrief.load(), model="m", transport=httpx.MockTransport(handler))
    assert "".join(coach.stream("Why?", [], threading.Event(), "en")) == "hi"
    assert calls == ["m"]
