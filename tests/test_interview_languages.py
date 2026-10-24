"""Questions are recognised in the language they are asked in, Ukrainian included."""

from __future__ import annotations

import re
import time

import pytest

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.prompts import user_prompt
from careertwin.interview.questions import (
    FALLBACK_WORDS,
    LEXICON,
    Kind,
    classify,
    detect_language,
    is_question,
)
from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
from careertwin.interview.transcript import TranscriptLog

UKRAINIAN = [
    ("Розкажіть про себе", Kind.INTRO),
    ("Чому ви обрали нашу компанію", Kind.MOTIVATION),
    ("Що таке REST", Kind.DEFINITION),
    ("Поясніть різницю між процесом і потоком", Kind.DEFINITION),
    ("Яка у вас очікувана зарплата", Kind.SALARY),
    ("Коли ви зможете вийти на роботу", Kind.LOGISTICS),
    ("Наведіть приклад складної ситуації", Kind.BEHAVIORAL),
    ("Чи є у вас запитання до нас", Kind.CANDIDATE_QUESTIONS),
    ("Як ви працюєте з базами даних", Kind.TECHNICAL),
]


@pytest.mark.parametrize(("text", "kind"), UKRAINIAN)
def test_a_ukrainian_question_is_heard_without_a_question_mark(text, kind):
    assert is_question(text, "uk") and is_question(text)
    assert classify(text) is kind


@pytest.mark.parametrize("text", ["Дякую", "Добре", "Так", "Привіт", "Зрозуміло", "Доброго дня"])
def test_a_ukrainian_pleasantry_is_not_a_question(text):
    assert not is_question(text, "uk")
    assert classify(text) is Kind.SMALL_TALK


def test_a_ukrainian_question_is_not_taken_for_russian():
    assert detect_language("Розкажіть про себе") == "uk"
    assert detect_language("Що таке REST?") == "uk"
    assert detect_language("Як ви працюєте з Docker") == "uk"
    assert detect_language("Расскажите о себе") == "ru"
    assert detect_language("Что такое REST и как это работает?") == "ru"
    assert detect_language("Раскажы пра сябе, ўсё добра") == "be"
    assert detect_language("Opowiedz o sobie, proszę") == "pl"
    assert detect_language("Tell me about yourself") == "en"


def test_the_language_the_answer_is_written_in_follows_what_was_asked(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text("## Contact\n| Full name | A |\n")
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    brief = InterviewBrief.load()
    text = user_prompt(brief, "Чому ви хочете працювати в нас?", classify("Чому ви хочете працювати в нас?"), [], None)
    assert "INTERVIEW LANGUAGE: Ukrainian" in text and "Russian" not in text
    assert "INTERVIEW LANGUAGE: Russian" in user_prompt(brief, "Почему вы хотите у нас работать?", Kind.MOTIVATION, [], None)


@pytest.mark.parametrize(
    ("language", "text"),
    [
        ("de", "Erzählen Sie mir von sich"), ("de", "Warum möchten Sie bei uns arbeiten"),
        ("fr", "Parlez-moi de votre expérience"), ("fr", "Pourquoi voulez-vous ce poste"),
        ("es", "Cuéntame sobre tu experiencia"), ("es", "Por qué quieres trabajar con nosotros"),
        ("ru", "Расскажите о своём опыте"), ("pl", "Opowiedz o swoim doświadczeniu"),
        ("en", "Tell me about your last project"),
    ],
)
def test_the_other_languages_with_a_lexicon_hear_a_question_without_a_mark(language, text):
    assert is_question(text, language)


def test_a_language_with_no_lexicon_still_gets_its_dialogue():
    text = "Bize son projenizden biraz bahsedin lütfen"
    assert "tr" not in LEXICON and len(text.split()) >= FALLBACK_WORDS
    assert is_question(text, "tr")  # long enough: sent rather than silently dropped
    assert not is_question("Evet tamam", "tr")  # a pleasantry is too short to be a question
    assert not is_question(text)  # without a known language the guess stays conservative
    assert is_question("Soru nedir?", "tr")  # a question mark always counts
    assert is_question("Siz kimsiniz؟")  # so does the Arabic one


def test_every_lexicon_entry_is_complete_and_cannot_match_everything():
    fields = set(LEXICON["en"])
    for code, entry in LEXICON.items():
        assert set(entry) == fields, code
        for field, fragment in entry.items():
            assert re.compile(fragment, re.I).search("") is None, (code, field)


# ---- a heard Ukrainian question reaches the model --------------------------------------------------


class RecordingCoach:
    def __init__(self):
        self.calls = []

    def stream(self, question, history, cancel, language=None, image=None):
        self.calls.append((question, language))
        yield "POINTS:\n- я\nANSWER:\nвідповідь"


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def test_a_ukrainian_utterance_without_a_question_mark_sends_the_request(tmp_path):
    coach = RecordingCoach()
    session = InterviewSession(
        InterviewBrief(candidate="x"), None, coach, Callbacks(on_turn=lambda s, t: None),
        language="uk", log=TranscriptLog(tmp_path), debounce=0.05,
    )
    session.handle_text(INTERVIEWER, "Розкажіть про себе і свій досвід")
    assert wait_for(lambda: coach.calls)
    assert coach.calls[0] == ("Розкажіть про себе і свій досвід", "uk")
    session.handle_text(INTERVIEWER, "Дякую")
    time.sleep(0.2)
    assert len(coach.calls) == 1  # a pleasantry is logged, not answered
    session.stop()


def test_the_interview_language_decides_when_the_dialogue_is_not_in_the_table(tmp_path):
    coach = RecordingCoach()
    session = InterviewSession(
        InterviewBrief(candidate="x"), None, coach, Callbacks(on_turn=lambda s, t: None),
        language="tr", log=TranscriptLog(tmp_path), debounce=0.05,
    )
    session.handle_text(INTERVIEWER, "Bize son projenizden biraz bahsedin lütfen")
    assert wait_for(lambda: coach.calls)
    session.stop()


# ---- a heading that is still streaming ----------------------------------------------------------


@pytest.mark.parametrize("tail", ["ANSW", "ANSWER", "A", "KE", "CONT", "**ANSWE", "context"])
def test_a_heading_that_is_still_streaming_is_not_shown_as_a_line(tail):
    from careertwin.interview.prompts import sections, split_partial

    parts = sections(f"POINTS:\n- first line\n- second line\n{tail}")
    assert parts.points == ["first line", "second line"] and parts.answer == ""
    points, answer = split_partial(f"POINTS:\n- one\n{tail}")
    assert points == ["one"] and answer == ""


def test_real_headings_and_real_lines_are_untouched():
    from careertwin.interview.prompts import sections

    assert sections("POINTS:\n- a\nANSWER:\nfull text").answer == "full text"
    assert sections("POINTS:\n- a\n- Answer the question\nANSWER:\nx").points == ["a", "Answer the question"]
    assert sections("Just a plain reply without headings").answer == "Just a plain reply without headings"
    assert sections("I will answer").answer == "I will answer"
    assert sections("POINTS:\n- a\n- context").points == ["a", "context"]  # a bullet is never a heading
