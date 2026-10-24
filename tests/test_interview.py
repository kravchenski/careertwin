"""Interview assistant: question detection, segmentation, brief, coach and session flow."""

from __future__ import annotations

import json
import threading
import time

import httpx
import numpy as np
import pytest

from careertwin.interview.brief import BriefError, InterviewBrief
from careertwin.interview.coach import CoachError, StreamingCoach, suggest
from careertwin.interview.prompts import parse_suggestion, user_prompt
from careertwin.interview.questions import Kind, classify, detect_language, is_question
from careertwin.interview.session import INTERVIEWER, YOU, Callbacks, InterviewSession
from careertwin.interview.transcript import TranscriptLog
from careertwin.interview.vad import FRAME_SAMPLES, UtteranceSegmenter, utterances


@pytest.mark.parametrize(
    "text",
    [
        "Tell me about a time you disagreed with a teammate.",
        "How would you design a rate limiter",
        "Расскажите о своём опыте с PostgreSQL.",
        "Dlaczego chcesz pracować u nas?",
        "What are your salary expectations",
    ],
)
def test_questions_are_detected(text):
    assert is_question(text)


@pytest.mark.parametrize("text", ["Okay.", "Thanks, great.", "Спасибо!", "mm", "We use Kubernetes here and love it."])
def test_statements_are_not_questions(text):
    assert not is_question(text)


def test_classification_and_language():
    assert classify("Tell me about yourself") is Kind.INTRO
    assert classify("Tell me about a time you failed") is Kind.BEHAVIORAL
    assert classify("What are your salary expectations?") is Kind.SALARY
    assert classify("Какие у вас ожидания по зарплате?") is Kind.SALARY
    assert classify("Why do you want to work here?") is Kind.MOTIVATION
    assert classify("How does the GIL work?") is Kind.DEFINITION
    assert classify("что такое pytohn") is Kind.DEFINITION
    assert classify("What is the difference between a process and a thread") is Kind.DEFINITION
    assert classify("Czym jest Docker?") is Kind.DEFINITION
    assert classify("What is your salary expectation?") is Kind.SALARY
    assert classify("What are you looking for in a role?") is Kind.TECHNICAL
    assert classify("Design a rate limiter for an API") is Kind.TECHNICAL
    assert detect_language("Расскажите о себе") == "ru"
    assert detect_language("Opowiedz o sobie, proszę") == "pl"
    assert detect_language("Tell me about yourself") == "en"


def tone(ms: int, amplitude: float = 4000) -> np.ndarray:
    count = FRAME_SAMPLES * (ms // 20)
    t = np.arange(count) / 16000
    return (amplitude * np.sin(2 * np.pi * 220 * t)).astype(np.int16)


def silence(ms: int) -> np.ndarray:
    return np.zeros(FRAME_SAMPLES * (ms // 20), dtype=np.int16)


def frames(*parts: np.ndarray):
    audio = np.concatenate(parts)
    for start in range(0, len(audio), FRAME_SAMPLES):
        yield audio[start : start + FRAME_SAMPLES]


def test_segmenter_splits_utterances_on_silence():
    found = list(utterances(frames(silence(400), tone(1000), silence(1200), tone(600), silence(1000))))
    assert len(found) == 2
    assert 1000 <= len(found[0]) / 16 <= 1700
    assert len(found[1]) < len(found[0])


def test_segmenter_ignores_clicks_and_flushes_tail():
    assert list(utterances(frames(silence(400), tone(60), silence(1000)))) == []
    tail = list(utterances(frames(silence(200), tone(800))))
    assert len(tail) == 1


def test_segmenter_caps_long_speech():
    segmenter = UtteranceSegmenter(max_utterance_ms=1000)
    out = [u for f in frames(tone(2500)) if (u := segmenter.push(f)) is not None]
    assert len(out) >= 2


ABOUT = "## Contact\n| Full name | Test Candidate |\n## Experience\n- Built FastAPI services.\n"


@pytest.fixture
def brief(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    return InterviewBrief.load(notes="Prefers backend roles")


def test_brief_renders_context_and_notes(brief):
    text = brief.render()
    assert "Built FastAPI services" in text and "Prefers backend roles" in text
    assert "SALARY POLICY: auto" in text


def test_brief_requires_context(tmp_path, monkeypatch):
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(tmp_path / "missing.md"))
    with pytest.raises(BriefError, match="No candidate context"):
        InterviewBrief.load()


def test_brief_reads_a_job_file(tmp_path, brief):
    job = tmp_path / "job.txt"
    job.write_text("Senior Python role using FastAPI and PostgreSQL")
    brief._attach_job(str(job))
    assert "FastAPI" in brief.render() and "PostgreSQL" in brief.vocabulary()
    with pytest.raises(BriefError):
        brief._attach_job("nonsense")


def test_prompt_grounds_answer_language(brief):
    prompt = user_prompt(brief, "Расскажите о себе", Kind.INTRO, ["Interviewer: Hi"])
    assert "INTERVIEW LANGUAGE: Russian" in prompt and "Interviewer: Hi" in prompt


def test_parse_suggestion_splits_points_and_answer():
    parsed = parse_suggestion("POINTS:\n- FastAPI at scale\n- Own the deploy\nANSWER:\nI built APIs. I ran them.")
    assert parsed.points == ["FastAPI at scale", "Own the deploy"]
    assert parsed.answer == "I built APIs. I ran them."
    assert parse_suggestion("just text").answer == "just text"


async def test_suggest_uses_the_gateway(brief, monkeypatch):
    seen = {}

    async def fake_call_ai(system, user, provider, model, **kwargs):
        seen.update(system=system, user=user, provider=provider)
        return "POINTS:\n- one\nANSWER:\nSure."

    monkeypatch.setattr("careertwin.interview.coach.call_ai", fake_call_ai)
    result = await suggest(brief, "How do you test APIs?")
    assert result.points == ["one"] and seen["provider"] == "switchyard"
    assert "How do you test APIs?" in seen["user"]

    async def empty(*args, **kwargs):
        return ""

    monkeypatch.setattr("careertwin.interview.coach.call_ai", empty)
    with pytest.raises(CoachError):
        await suggest(brief, "Anything?")


def sse(*chunks: str) -> httpx.Response:
    lines = [f"data: {json.dumps({'choices': [{'delta': {'content': c}}]})}\n\n" for c in chunks]
    return httpx.Response(200, headers={"content-type": "text/event-stream"}, content="".join(lines + ["data: [DONE]\n\n"]))


def test_streaming_coach_yields_deltas_and_honours_cancel(brief):
    coach = StreamingCoach(brief, transport=httpx.MockTransport(lambda r: sse("PO", "INTS:", " x")))
    assert "".join(coach.stream("Why us?", [], threading.Event())) == "POINTS: x"
    cancelled = threading.Event()
    cancelled.set()
    assert list(coach.stream("Why us?", [], cancelled)) == []


def test_streaming_coach_accepts_plain_json_and_reports_errors(brief):
    body = {"choices": [{"message": {"content": "whole answer"}}]}
    coach = StreamingCoach(brief, transport=httpx.MockTransport(lambda r: httpx.Response(200, json=body)))
    assert list(coach.stream("q?", [], threading.Event())) == ["whole answer"]
    broken = StreamingCoach(brief, transport=httpx.MockTransport(lambda r: httpx.Response(503, text="no providers")))
    with pytest.raises(CoachError, match="503"):
        list(broken.stream("q?", [], threading.Event()))


class FakeCoach:
    def __init__(self, reply="POINTS:\n- a\nANSWER:\nb"):
        self.reply, self.questions, self.languages, self.images = reply, [], [], []

    def stream(self, question, history, cancel, language=None, image=None):
        self.questions.append((question, list(history)))
        self.languages.append(language)
        self.images.append(image)
        yield from (self.reply[:6], self.reply[6:])


class Events:
    def __init__(self):
        self.turns, self.chunks, self.done, self.errors = [], [], [], []
        self.callbacks = Callbacks(
            on_turn=lambda s, t: self.turns.append((s, t)),
            on_suggestion_chunk=self.chunks.append,
            on_suggestion_done=self.done.append,
            on_error=self.errors.append,
        )


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def make_session(brief, tmp_path, coach=None, debounce=0.05):
    events = Events()
    session = InterviewSession(
        brief, None, coach or FakeCoach(), events.callbacks, debounce=debounce,
        log=TranscriptLog(tmp_path),
    )
    return session, events


def test_interviewer_question_streams_a_suggestion(brief, tmp_path):
    session, events = make_session(brief, tmp_path)
    session.handle_text(INTERVIEWER, "Tell me about a time you led a project.")
    assert wait_for(lambda: events.done)
    assert "".join(events.chunks) == events.done[0]
    assert session.coach.questions[0][0].startswith("Tell me about a time")
    session.stop()


def test_candidate_speech_and_statements_trigger_nothing(brief, tmp_path):
    session, events = make_session(brief, tmp_path)
    session.handle_text(YOU, "How do you do? I built a FastAPI service.")
    session.handle_text(INTERVIEWER, "We are a small team of eight.")
    time.sleep(0.3)
    assert events.done == [] and len(events.turns) == 2
    session.stop()


def test_split_questions_are_merged_by_debounce(brief, tmp_path):
    session, events = make_session(brief, tmp_path, debounce=0.2)
    session.handle_text(INTERVIEWER, "Tell me about your last project?")
    session.handle_text(INTERVIEWER, "And what was your role in it?")
    assert wait_for(lambda: events.done)
    assert len(session.coach.questions) == 1
    assert "last project" in session.coach.questions[0][0] and "your role" in session.coach.questions[0][0]
    session.stop()


def test_ask_and_hint_now_and_errors(brief, tmp_path):
    session, events = make_session(brief, tmp_path)
    session.ask("  Why  this company? ")
    assert wait_for(lambda: events.done)
    assert events.turns[0] == (INTERVIEWER, "Why this company?")
    events.done.clear()
    session.hint_now()
    assert wait_for(lambda: events.done)

    class Broken:
        def stream(self, *args, **kwargs):
            raise CoachError("Switchyard is not reachable")
            yield

    failing, failing_events = make_session(brief, tmp_path, coach=Broken())
    failing.ask("Anything?")
    assert wait_for(lambda: failing_events.errors)
    session.stop()
    failing.stop()


class ScriptedSource:
    def __init__(self, audio):
        self.audio, self.closed = audio, False

    def frames(self):
        yield from frames(self.audio)

    def close(self):
        self.closed = True


class ScriptedTranscriber:
    def __init__(self, texts):
        self.texts = list(texts)

    def transcribe(self, pcm, prompt=""):
        return self.texts.pop(0)


def test_audio_pipeline_end_to_end(brief, tmp_path):
    audio = np.concatenate([silence(400), tone(1000), silence(1200)])
    events = Events()
    session = InterviewSession(
        brief, ScriptedTranscriber(["Why do you want this job?"]), FakeCoach(), events.callbacks,
        debounce=0.05, log=TranscriptLog(tmp_path),
    )
    source = ScriptedSource(audio)
    session.start({INTERVIEWER: source})
    assert wait_for(lambda: events.done)
    assert events.turns == [(INTERVIEWER, "Why do you want this job?")]
    session.stop()
    assert source.closed


def test_transcript_is_saved_as_markdown(brief, tmp_path):
    session, _events = make_session(brief, tmp_path)
    session.handle_text(INTERVIEWER, "Why us?")
    session.handle_text(YOU, "Because of the product.")
    assert wait_for(lambda: session.log.suggestions)
    saved = session.log.save()
    body = saved.read_text()
    assert "Interviewer:** Why us?" in body and "You:** Because of the product." in body
    assert "Suggestions shown" in body
    assert TranscriptLog(tmp_path / "empty").save() is None
    session.stop()


async def test_interview_routes(brief, monkeypatch):
    from fastapi.testclient import TestClient

    from careertwin.api.app import app

    async def fake_call_ai(system, user, provider, model, **kwargs):
        if "JSON only" in system:
            return json.dumps(
                {"questions": [{"question": "Why FastAPI?", "kind": "technical", "outline": ["Built services"]}]}
            )
        return "POINTS:\n- Built FastAPI services\nANSWER:\nI built FastAPI services."

    monkeypatch.setattr("careertwin.interview.coach.call_ai", fake_call_ai)
    client = TestClient(app)
    seen = []
    original = fake_call_ai

    async def spying(system, user, provider, model, **kwargs):
        seen.append(user)
        return await original(system, user, provider, model, **kwargs)

    monkeypatch.setattr("careertwin.interview.coach.call_ai", spying)
    russian = client.post("/v1/interview/suggest", json={"question": "Tell me about yourself", "language": "ru"})
    assert russian.json()["language"] == "ru" and "INTERVIEW LANGUAGE: Russian" in seen[-1]
    reply = client.post("/v1/interview/suggest", json={"question": "Tell me about yourself"})
    assert reply.status_code == 200
    body = reply.json()
    assert body["kind"] == "intro" and body["points"] == ["Built FastAPI services"]
    practice = client.post("/v1/interview/practice", json={"count": 3}).json()
    assert practice["questions"][0]["question"] == "Why FastAPI?"
    assert client.post("/v1/interview/suggest", json={"question": "x"}).status_code == 422


def test_interview_tools_are_registered():
    from careertwin.agentic.registry import get_tool

    assert get_tool("suggest_interview_answer").parameters["required"] == ["question"]
    assert not get_tool("prepare_interview_questions").mutating


def test_overlay_renders_a_streamed_suggestion(monkeypatch):
    pytest.importorskip("PySide6")
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from careertwin.interview.overlay import Bridge, OverlayWindow, create_app

    app = create_app([])
    bridge, asked, hinted = Bridge(), [], []
    window = OverlayWindow(
        bridge, on_ask=asked.append, on_hint=lambda: hinted.append(1), on_pause=lambda _: None,
        sources=[("about", "ABOUT.md · 20.6k")],
    )
    bridge.turn.emit("Interviewer", "Why us?")
    bridge.started.emit("Why us?")
    reply = "POINTS:\n- **Real** product\n- Strong **team**\nANSWER:\nBecause of the product."
    bridge.chunk.emit(reply[:20])
    bridge.done.emit(reply)
    app.processEvents()
    assert "Why us?" in window.transcript.toPlainText()
    plain = window.answer.toPlainText()
    assert "Why us?" in plain and "Real product" in plain and "Because of the product" not in plain
    window.act_full.setChecked(True)
    assert "Because of the product." in window.answer.toPlainText()
    assert window.answer_text == "• Real product\n• Strong team\nBecause of the product."
    assert window.page.text() == "1/1"
    window.input.setText("  How do you test?  ")
    window.input.returnPressed.emit()
    assert asked == ["How do you test?"]
    window.ask.clicked.emit()
    assert hinted == [1]
    assert "ABOUT.md" in window.context.full
    window.close()


def test_overlay_pages_through_answers_and_follows_the_interview_language(monkeypatch):
    pytest.importorskip("PySide6")
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from careertwin.interview.overlay import Bridge, OverlayWindow, create_app, render_bold

    app = create_app([])
    bridge, picked = Bridge(), []
    window = OverlayWindow(
        bridge, on_ask=lambda _: None, on_hint=lambda: None, on_pause=lambda _: None,
        on_language=picked.append, on_screenshot=None, ui_language="en",
    )
    for number in (1, 2):
        bridge.started.emit(f"Question {number}")
        bridge.done.emit(f"POINTS:\n- point {number}\nANSWER:\nspoken {number}")
    app.processEvents()
    assert window.page.text() == "2/2" and "Question 2" in window.answer.toPlainText()
    window.previous.click()
    assert window.page.text() == "1/2" and "Question 1" in window.answer.toPlainText()
    window.next.click()
    assert window.page.text() == "2/2"
    assert not window.shot.isEnabled()
    bridge.language.emit("ru")
    app.processEvents()
    assert window.ask.title.text() == "Спросить" and window.get_answer.text() == "Получить ответ"
    bridge.language.emit("xx")
    assert window.ask.title.text() == "Ask"
    window.language_actions["de"].trigger()
    assert picked == ["de"]
    assert render_bold("a **b** <c> **d") == 'a <b style="color:#ffffff">b</b> &lt;c&gt; d'
    window.close()


def test_overlay_shows_state_level_and_errors(monkeypatch):
    pytest.importorskip("PySide6")
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from careertwin.interview.overlay import Bridge, OverlayWindow, create_app

    app = create_app([])
    bridge = Bridge()
    window = OverlayWindow(bridge, on_ask=lambda _: None, on_hint=lambda: None, on_pause=lambda _: None)
    bridge.state.emit("listening")
    bridge.level.emit("Interviewer", 0.9)
    app.processEvents()
    assert window.pill.level == pytest.approx(0.9) and window.pill.text == ""
    bridge.level.emit("You", 1.0)
    assert window.pill.level == pytest.approx(0.9)
    bridge.state.emit("thinking")
    assert window.pill.text == "Thinking…"
    bridge.error.emit("Switchyard is not reachable")
    assert window.pill.kind == "error" and "not reachable" in window.pill.text
    window.close()


def test_whisper_falls_back_to_cpu_when_gpu_libraries_are_missing():
    from careertwin.interview.stt import WhisperTranscriber

    class Segment:
        text = " hello "

    class GpuBroken:
        def transcribe(self, *args, **kwargs):
            raise RuntimeError("Library libcublas.so.12 is not found or cannot be loaded")

    class CpuModel:
        def transcribe(self, *args, **kwargs):
            return [Segment()], None

    transcriber = WhisperTranscriber("tiny")
    transcriber._model = GpuBroken()
    transcriber.load = lambda: setattr(transcriber, "_model", transcriber._model or CpuModel())
    assert transcriber.transcribe(np.zeros(1600, dtype=np.int16)) == "hello"
    assert transcriber.device == "cpu"

    other = WhisperTranscriber("tiny", device="cpu")
    other._model = type("M", (), {"transcribe": lambda *a, **k: (_ for _ in ()).throw(RuntimeError("boom"))})()
    with pytest.raises(RuntimeError, match="boom"):
        other.transcribe(np.zeros(1600, dtype=np.int16))


def test_vocabulary_combines_vacancy_and_skills():
    brief = InterviewBrief(
        candidate="## Skills\n\n- Professional: Python, FastAPI, GitHub Actions, Linux.\n"
        "- Coursework only: Kubernetes (a lab cluster).\n\n## Education\n- BSc\n",
        job_title="Backend Developer",
        company="Example Corp",
        job_description="You will use PostgreSQL daily.",
    )
    words = brief.vocabulary().split(", ")
    assert {"Example Corp", "PostgreSQL", "FastAPI", "GitHub Actions", "Kubernetes"} <= set(words)
    assert "BSc" not in words
    assert InterviewBrief(candidate="no skills section").vocabulary() == ""


async def test_cv_review_route_and_tool(brief, monkeypatch, tmp_path):
    from fastapi.testclient import TestClient

    from careertwin import db
    from careertwin.agentic.registry import get_tool
    from careertwin.api.app import app
    from careertwin.automation.models import Vacancy
    from careertwin.automation.store import Store

    monkeypatch.setattr(db, "DB_PATH", tmp_path / "pipeline.db")
    store = Store()
    store.initialize()
    job = store.add(Vacancy(url="https://example.com/jobs/1", title="Python Developer", company="Example"))
    seen = []

    async def fake_call_ai(system, user, provider, model, **kwargs):
        seen.append(user)
        return json.dumps({"summary": "Good fit.", "strengths": ["FastAPI"], "gaps": [], "likely_questions": [], "cv_fixes": []})

    monkeypatch.setattr("careertwin.interview.coach.call_ai", fake_call_ai)
    client = TestClient(app)
    missing = client.post("/v1/interview/cv-review", json={"job_id": job["id"]})
    assert missing.status_code == 422 and "no prepared CV" in missing.json()["detail"]
    cv = tmp_path / "sent_cv.txt"
    cv.write_text("Alex Demo built FastAPI services on PostgreSQL at Northwind Logistics for three years, with Docker deployments and CI in GitHub Actions.")
    store.update(job["id"], package={"cv_path": str(cv)})
    reply = client.post("/v1/interview/cv-review", json={"job_id": job["id"], "language": "ru"})
    assert reply.status_code == 200 and reply.json()["summary"] == "Good fit."
    assert "Write in Russian" in seen[-1] and "sent_cv.txt" in seen[-1]
    assert get_tool("review_cv_for_vacancy").parameters["required"] == ["job_id"]
