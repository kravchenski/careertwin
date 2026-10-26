"""Whisper writes words on music and noise, and repeats its prompt: none of that may reach the screen."""

from __future__ import annotations

from types import SimpleNamespace

import numpy as np
import pytest

from careertwin.interview.brief import InterviewBrief, vacancy_terms
from careertwin.interview.stt import WhisperTranscriber, echoes_prompt, is_speech

PROMPT = "Vention, Python, SQL, REST, Linux, PostgreSQL, tuple, JOIN"


def segment(text="hello", *, logprob=-0.4, silence=0.05, compression=1.2):
    return SimpleNamespace(text=text, avg_logprob=logprob, no_speech_prob=silence, compression_ratio=compression)


def transcriber(*segments):
    whisper = WhisperTranscriber("tiny", language="en", device="cpu")
    whisper._model = SimpleNamespace(
        transcribe=lambda *a, **k: (iter(segments), SimpleNamespace(language="en", language_probability=1.0))
    )
    return whisper


def test_a_segment_whisper_does_not_believe_is_dropped():
    assert is_speech(segment())
    assert not is_speech(segment(logprob=-2.0, silence=0.6)), "the music that became 'ADVANCE,'"
    assert not is_speech(segment(logprob=-1.7)), "a guess"
    assert not is_speech(segment(logprob=-1.2, silence=0.8)), "it thinks nobody speaks and is not sure"
    assert is_speech(segment(logprob=-1.2, silence=0.3)), "an accent is not a reason to drop"
    assert not is_speech(segment("REST, REST, REST, REST", compression=3.1)), "a loop"
    assert is_speech(SimpleNamespace(text="no figures")), "a model that reports nothing is trusted"


def test_the_prompt_read_back_is_not_a_transcript():
    for text in ("REST, REST, REST.", "Linux, Python, REST, Python, REST", "PostgreSQL,", "SQL JOIN"):
        assert echoes_prompt(text, PROMPT), text
    for text in ("What is REST?", "Explain REST", "Tell me about SQL joins", "REST?", "Liovsky and PostgreSQL work"):
        assert not echoes_prompt(text, PROMPT), text
    assert not echoes_prompt("", PROMPT)


def test_the_transcriber_returns_nothing_for_music_and_the_prompt_and_keeps_speech():
    audio = np.zeros(16_000, dtype=np.int16)
    assert transcriber(segment("ADVANCE,", logprob=-2.0, silence=0.6)).transcribe(audio, PROMPT) == ""
    assert transcriber(segment("REST, REST, REST.")).transcribe(audio, PROMPT) == ""
    kept = transcriber(segment("What is a tuple in Python?"), segment("Liovsky", logprob=-1.9))
    assert kept.transcribe(audio, PROMPT) == "What is a tuple in Python?"


def test_a_vacancy_gives_technologies_not_the_words_that_begin_its_sentences():
    text = (
        "What you'll do\nBuild production grade data applications. Our teams work with Python, Java, Scala, and R "
        "to build pipelines.\nWhat you bring\nExperience with SQL and data tools like Pandas or PySpark. "
        "Understanding of Git. Fluent English."
    )
    found = vacancy_terms(text)
    assert {"Python", "Java", "Scala", "Pandas", "PySpark", "SQL"} <= set(found)
    assert not {"What", "Build", "Our", "Experience", "Understanding", "Fluent"} & set(found)
    assert "C#" in vacancy_terms("We use C# daily") and "GitHub" in vacancy_terms("We host on GitHub")


@pytest.mark.parametrize("vacancy", ["Internship Data Engineering at Vention. What you bring: Python, SQL, Pandas or PySpark and Git."])
def test_the_vocabulary_for_speech_stays_short_and_useful(vacancy):
    words = InterviewBrief(candidate="", company="Vention", job_description=vacancy).vocabulary().split(", ")
    assert len(words) <= 50 and "What" not in words and {"Vention", "Python", "PySpark"} <= set(words)


# ---- a fixed language that does not fit the speaker ------------------------------------------------------


def two_readings(fixed, auto):
    """A model that reads the clip once per language it is asked for: (text, logprob, language, probability)."""
    asked = []

    def transcribe(audio, language=None, **options):
        asked.append(language)
        text, logprob, detected, probability = fixed if language else auto
        return iter([segment(text, logprob=logprob)]), SimpleNamespace(language=language or detected, language_probability=probability)

    whisper = WhisperTranscriber("tiny", language="en", device="cpu")
    whisper._model = SimpleNamespace(transcribe=transcribe)
    return whisper, asked


def test_russian_read_as_english_is_read_again_in_russian():
    whisper, asked = two_readings(("tu tutakoe, levoe s'iedininev, PostgreSQL.", -0.9, "en", 1.0), ("Что такое левое соединение в PostgreSQL?", -0.3, "ru", 0.95))
    assert whisper.transcribe(np.zeros(16_000, dtype=np.int16), PROMPT) == "Что такое левое соединение в PostgreSQL?"
    assert asked == ["en", None] and whisper.last_language == "ru"


def test_a_confident_english_reading_is_not_read_twice():
    whisper, asked = two_readings(("Tell me about yourself", -0.2, "en", 1.0), ("never used", -0.1, "ru", 0.99))
    assert whisper.transcribe(np.zeros(16_000, dtype=np.int16), PROMPT) == "Tell me about yourself" and asked == ["en"]


def test_a_shaky_reading_stays_unless_the_other_language_is_certain_and_better():
    audio = np.zeros(16_000, dtype=np.int16)
    same, asked = two_readings(("an accented question about SQL", -0.8, "en", 1.0), ("an accented question about SQL", -0.7, "en", 1.0))
    assert same.transcribe(audio, PROMPT) == "an accented question about SQL" and asked == ["en", None]
    unsure, _ = two_readings(("what is a join", -0.8, "en", 1.0), ("что такое джойн", -0.3, "ru", 0.55))
    assert unsure.transcribe(audio, PROMPT) == "what is a join", "not sure enough to switch"
    worse, _ = two_readings(("what is a join", -0.6, "en", 1.0), ("что-то", -0.9, "ru", 0.9))
    assert worse.transcribe(audio, PROMPT) == "what is a join", "the other reading is not better"


def test_a_short_utterance_that_names_a_technology_is_worth_showing_to_the_model():
    from careertwin.interview.questions import mentions

    vocabulary = "Vention, Python, SQL / PostgreSQL, LEFT JOIN, tuple"
    assert mentions("TuTaco и левое соединение в PostgreSQL", vocabulary)
    assert mentions("levoe PostgreSQL", vocabulary) and mentions("a tuple, please", vocabulary)
    assert not mentions("PostgreSQL", vocabulary), "one word"
    assert not mentions("Okay, thanks", vocabulary) and not mentions("Let's move on", vocabulary)
    assert not mentions("the postgresqlish thing", vocabulary), "whole words only"
    assert not mentions("anything at all", "")


def test_the_session_sends_such_an_utterance_to_the_coach(tmp_path, monkeypatch):
    import threading
    import time

    from careertwin.interview.session import INTERVIEWER, Callbacks, InterviewSession
    from careertwin.interview.transcript import TranscriptLog

    asked = []

    class Coach:
        supports_listening = False

        def stream(self, question, history, cancel, language=None, image=None):
            asked.append(question)
            yield "ANSWER:\nok"

    brief = InterviewBrief(candidate="## Skills\n\n- Strong: Python, SQL / PostgreSQL\n", job_description="You will use Python.")
    session = InterviewSession(brief, None, Coach(), Callbacks(), debounce=0.01, log=TranscriptLog(tmp_path))
    session.handle_text(INTERVIEWER, "levoe PostgreSQL")
    session.handle_text(INTERVIEWER, "Okay, thanks")
    for _ in range(200):
        if asked:
            break
        time.sleep(0.01)
    assert asked == ["levoe PostgreSQL"] and threading.active_count() >= 1
