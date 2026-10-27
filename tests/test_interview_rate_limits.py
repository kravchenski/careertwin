"""A rate-limited model must not cost an answer: the fallback chain, smaller requests, task questions."""

from __future__ import annotations

import json
import threading
import time

import httpx
import pytest

from careertwin.interview.brief import Document, InterviewBrief
from careertwin.interview.coach import CoachError, StreamingCoach, retry_after
from careertwin.interview.prompts import user_prompt
from careertwin.interview.questions import Kind, classify, is_question, worth_an_answer
from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
from careertwin.interview.transcript import TranscriptLog

ABOUT = (
    "# Candidate\n\n## Contact\nphone 123\n\n## Experience\nBuilt async pipelines.\n\n"
    "## Skills\nPython, SQL\n\n## Standard form answers\nSalary: negotiable.\n\n"
    "## Interview stories (STAR)\nThe night the queue stalled.\n\n## Honest limits\nNo Spark in production.\n"
)


def sse(*parts):
    lines = "".join(f"data: {json.dumps({'choices': [{'delta': {'content': part}}]})}\n\n" for part in parts)
    return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=lines + "data: [DONE]\n\n")


def brief(tmp_path, monkeypatch, *, cv=True):
    path = tmp_path / "ABOUT.md"
    path.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    monkeypatch.delenv("CAREERTWIN_RESUME", raising=False)
    result = InterviewBrief.load()
    if cv:
        result.documents.append(Document("cv.pdf", "CV: Pandas and PySpark for data cleaning", kind="cv"))
    return result


def coach(tmp_path, monkeypatch, handler, **options):
    return StreamingCoach(brief(tmp_path, monkeypatch), transport=httpx.MockTransport(handler), **options)


def ask(stream_coach, question="What is a left join?"):
    return "".join(stream_coach.stream(question, [], threading.Event(), "en"))


def test_a_429_says_how_long_to_wait_in_a_header_or_in_the_message():
    assert retry_after(httpx.Response(429, headers={"retry-after": "7"}, text="x")) == 7
    assert retry_after(httpx.Response(429, text="all 4 Groq keys are rate limited or rejected; next one in 32s")) == 32
    assert retry_after(httpx.Response(429, headers={"retry-after": "9999"}, text="x")) == 90  # never for long
    assert retry_after(httpx.Response(429, text="slow down")) is None
    assert retry_after(httpx.Response(503, headers={"retry-after": "7"}, text="down")) is None


def test_a_rate_limited_model_is_followed_by_the_chain_in_order(tmp_path, monkeypatch):
    asked = []

    def handler(request):
        model = json.loads(request.content)["model"]
        asked.append(model)
        if model in ("groq/big", "other/slow"):
            return httpx.Response(429, text="next one in 5s")
        return sse("ANSWER:\n", f"from {model}")

    chain = ["other/slow", "ollama/same-model", "auto"]
    stream_coach = coach(tmp_path, monkeypatch, handler, model="groq/big", fallback_models=chain)
    assert ask(stream_coach) == "ANSWER:\nfrom ollama/same-model"
    assert asked == ["groq/big", "other/slow", "ollama/same-model"]
    assert stream_coach.fell_back == "ollama/same-model"


def test_a_model_that_asked_for_time_is_left_alone_until_then(tmp_path, monkeypatch):
    asked = []

    def handler(request):
        model = json.loads(request.content)["model"]
        asked.append(model)
        return httpx.Response(429, text="next one in 30s") if model == "groq/big" else sse("ok")

    stream_coach = coach(tmp_path, monkeypatch, handler, model="groq/big", fallback_models=["backup"])
    ask(stream_coach)
    ask(stream_coach)
    assert asked == ["groq/big", "backup", "backup"]  # the second question did not spend a round trip on it
    stream_coach._wait_until[("", "groq/big")] = 0.0  # the time has passed
    ask(stream_coach)
    assert asked[-2:] == ["groq/big", "backup"]


def test_the_last_model_of_the_chain_is_asked_even_when_it_was_told_to_wait(tmp_path, monkeypatch):
    asked = []

    def handler(request):
        asked.append(json.loads(request.content)["model"])
        return httpx.Response(429, text="next one in 30s")

    stream_coach = coach(tmp_path, monkeypatch, handler, model="a", fallback_models=["b"])
    with pytest.raises(CoachError, match="429"):
        ask(stream_coach)
    with pytest.raises(CoachError):
        ask(stream_coach)
    assert asked == ["a", "b", "b"]


def test_the_chosen_models_error_is_the_one_shown_when_the_whole_chain_fails(tmp_path, monkeypatch):
    def handler(request):
        model = json.loads(request.content)["model"]
        return httpx.Response(429 if model == "a" else 503, text="limited" if model == "a" else "down")

    with pytest.raises(CoachError, match="429: limited"):
        ask(coach(tmp_path, monkeypatch, handler, model="a", fallback_models=["b", "c"]))


def test_a_screenshot_does_not_go_to_a_model_that_cannot_see(tmp_path, monkeypatch):
    asked = []

    def handler(request):
        model = json.loads(request.content)["model"]
        asked.append(model)
        return httpx.Response(503, text="down") if model == "eyes" else sse("ok")

    stream_coach = coach(tmp_path, monkeypatch, handler, model="text", vision_model="eyes", fallback_models=["text-only"])
    list(stream_coach.stream("", [], threading.Event(), "en", image="aGk="))
    assert asked == ["eyes", "auto"]


def test_without_a_chain_the_old_behaviour_stays(tmp_path, monkeypatch):
    asked = []

    def handler(request):
        asked.append(json.loads(request.content)["model"])
        return httpx.Response(429, text="limited") if len(asked) == 1 else sse("ok")

    assert ask(coach(tmp_path, monkeypatch, handler, model="a")) == "ok"
    assert asked == ["a", "auto"]


def test_the_window_is_told_which_model_answered(tmp_path, monkeypatch):
    class Fallen:
        fell_back = "ollama/same-model"
        supports_listening = False

        def stream(self, question, history, cancel, language=None, image=None):
            yield "ANSWER:\nok"

    statuses = []
    session = InterviewSession(
        brief(tmp_path, monkeypatch), None, Fallen(), Callbacks(on_status=statuses.append), log=TranscriptLog(tmp_path)
    )
    session.ask("What is a left join?")
    for _ in range(300):
        if any("ollama/same-model" in status for status in statuses):
            break
        threading.Event().wait(0.01)
    assert any("rate limited or down: answered by ollama/same-model" in status for status in statuses)


def test_a_question_about_a_technology_is_sent_without_what_only_a_personal_question_needs(tmp_path, monkeypatch):
    small = brief(tmp_path, monkeypatch)
    knowledge = small.render(Kind.TECHNICAL)
    assert "Built async pipelines." in knowledge and "No Spark in production." in knowledge
    assert "phone 123" not in knowledge and "Salary: negotiable." not in knowledge
    assert "The night the queue stalled." not in knowledge and "Pandas and PySpark" not in knowledge
    assert small.render(Kind.DEFINITION) == knowledge

    personal = small.render(Kind.BEHAVIORAL)
    assert "The night the queue stalled." in personal and "Pandas and PySpark" in personal
    assert small.render() == personal
    assert len(knowledge) < len(personal)
    assert knowledge in user_prompt(small, "What is a left join?", Kind.TECHNICAL, [], "en")  # and it is what is sent


def test_the_cv_stays_when_there_is_nothing_else_to_know_the_candidate_by(tmp_path, monkeypatch):
    only_cv = InterviewBrief(candidate="", documents=[Document("cv.pdf", "CV: Pandas", kind="cv")])
    assert "CV: Pandas" in only_cv.render(Kind.TECHNICAL)


def test_speech_recognition_is_hinted_with_the_terms_of_the_vacancy_topics():
    vacancy = InterviewBrief(
        candidate="", job_title="Data Engineering Intern",
        job_description="Python, algorithms, SQL, Pandas or PySpark, cloud computing models and Git.",
    )
    words = vacancy.vocabulary().split(", ")
    assert {"tuple", "LEFT JOIN", "DataFrame", "lazy evaluation", "rebase", "IaaS"} <= set(words)
    assert len(words) <= 60
    assert "tuple" not in InterviewBrief(candidate="", job_description="You will paint walls.").vocabulary()


@pytest.mark.parametrize(
    "text",
    [
        "Write a function that checks whether a string is a palindrome.",
        "Implement binary search in Python.",
        "Reverse a linked list.",
        "Define a primary key.",
        "Optimize this query.",
    ],
)
def test_a_task_worded_as_an_order_is_something_to_answer(text):
    assert is_question(text) and worth_an_answer(text)


def test_a_statement_is_still_not_a_question():
    assert not is_question("Okay, thanks.") and not is_question("That sounds good to me")
    assert classify("Write a function that checks a palindrome.") is Kind.TECHNICAL


def test_a_cancelled_request_stops_waiting_for_a_provider_that_says_nothing(tmp_path, monkeypatch):
    silent = threading.Event()

    def handler(request):
        silent.wait(5)  # a provider that has not answered yet
        return sse("late")

    stream_coach = coach(tmp_path, monkeypatch, handler, model="slow", fallback_models=["slow"])
    cancel = threading.Event()
    threading.Timer(0.2, cancel.set).start()
    started = time.monotonic()
    assert list(stream_coach.stream("What is a left join?", [], cancel, "en")) == []
    assert time.monotonic() - started < 1.5, "the next question would have waited for this one"
    silent.set()


def test_an_answer_given_up_on_reports_no_error(tmp_path, monkeypatch):
    errors, states = [], []

    class Gone:
        supports_listening = False

        def stream(self, question, history, cancel, language=None, image=None):
            cancel.set()  # a newer question took over
            raise CoachError("down")
            yield ""

    session = InterviewSession(
        brief(tmp_path, monkeypatch), None, Gone(),
        Callbacks(on_error=errors.append, on_state=states.append), log=TranscriptLog(tmp_path),
    )
    session.ask("What is a left join?")
    time.sleep(0.3)
    assert errors == [] and "error" not in states


def test_a_coding_task_gets_code_even_in_the_main_answer_layout():
    from careertwin.interview.prompts import LAYOUT_HINTS, SYSTEM

    hint = LAYOUT_HINTS["main"]
    assert "one spoken paragraph" in hint, "an ordinary question is still answered as speech"
    assert "write, implement, fix or solve" in hint and "complete code in fenced blocks" in hint
    assert "is the exception" in SYSTEM and "never a description of the code" in SYSTEM
    prompt = user_prompt(InterviewBrief(candidate="## Skills\n- Python\n"), "Write a function that reverses a string.",
                         classify("Write a function that reverses a string."), [], "en", "standard", "main")
    assert "complete code in fenced blocks" in prompt and "ANSWER LAYOUT: main answer only" in prompt
