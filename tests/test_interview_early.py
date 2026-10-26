"""The answer starts while the interviewer is still asking, and what is not a question gets none."""

from __future__ import annotations

import threading
import time

import numpy as np
import pytest

from careertwin.interview.brief import InterviewBrief
from careertwin.interview.session import (
    EARLY_AFTER,
    INTERVIEWER,
    YOU,
    Callbacks,
    InterviewSession,
    covers,
)
from careertwin.interview.transcript import TranscriptLog
from careertwin.interview.vad import FRAME_SAMPLES

ABOUT = "# C\n\n## Experience\n- Built FastAPI services.\n"

LONG_QUESTION = (
    "Imagine a service that handles a million requests a day and one morning it suddenly slows down"
)
WHOLE_QUESTION = (
    LONG_QUESTION + ", walk me through how you would find out why and what you would change first?"
)
STATEMENT = "Our company has offices in several countries and we work with many different clients"


@pytest.fixture
def about(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text(ABOUT)
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))


class Speech:
    """The recogniser: says what it was told for each clip, in order."""

    last_language, last_probability, language = None, 0.0, None

    def __init__(self, *texts):
        self.texts = list(texts)

    def transcribe(self, pcm, prompt=""):
        return self.texts.pop(0) if len(self.texts) > 1 else self.texts[0]


class Coach:
    """Records what it was asked and answers at once, in the pieces it was given."""

    supports_listening = True

    def __init__(self, *pieces):
        self.pieces = pieces or ("An answer.",)
        self.asked: list[tuple[str, list[str]]] = []
        self.listening: list[str | None] = []
        self.cancelled: list[bool] = []

    def stream(self, question, history, cancel, language=None, image=None, listening=None):
        self.asked.append((question, list(history)))
        self.listening.append(listening)
        yield from self.pieces
        self.cancelled.append(cancel.is_set())


class Screen:
    """What the window would show."""

    def __init__(self):
        self.started: list[str] = []
        self.chunks: list[str] = []
        self.turns: list[tuple[str, str]] = []
        self.discarded = 0
        self.statuses: list[str] = []

    def callbacks(self) -> Callbacks:
        return Callbacks(
            on_suggestion_start=self.started.append,
            on_suggestion_chunk=self.chunks.append,
            on_suggestion_discard=lambda: setattr(self, "discarded", self.discarded + 1),
            on_turn=lambda speaker, text: self.turns.append((speaker, text)),
            on_status=self.statuses.append,
        )


def make(tmp_path, *texts, early_after=EARLY_AFTER, coach=None):
    coach, screen = coach or Coach(), Screen()
    session = InterviewSession(
        InterviewBrief.load(),
        Speech(*texts),
        coach,
        screen.callbacks(),
        debounce=0.01,
        log=TranscriptLog(tmp_path),
        early_after=early_after,
    )
    session._ready.set()
    thread = threading.Thread(target=session._transcribe_loop, daemon=True)
    thread.start()
    return session, coach, screen, thread


def pcm(seconds):
    return np.zeros(int(16_000 * seconds), dtype=np.int16)


def wait_for(condition, timeout=3.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.01)
    return False


def finish(session, thread):
    session.stop()
    thread.join(timeout=3)


def test_the_answer_starts_after_three_seconds_without_waiting_for_the_end(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION)
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: screen.started), (
        "nothing was answered while the interviewer was speaking"
    )
    assert screen.started == [LONG_QUESTION]
    assert screen.turns == [], "the question is not over: it is not a turn yet"
    assert wait_for(lambda: coach.asked) and coach.asked[0][0] == LONG_QUESTION
    assert coach.listening == ["hearing"]
    finish(session, thread)


def test_nothing_starts_early_for_a_short_caption_a_few_words_a_statement_or_your_own_voice(
    about, tmp_path
):
    cases = [
        ((INTERVIEWER, pcm(2.0), True), LONG_QUESTION),  # not three seconds yet
        ((YOU, pcm(8.0), True), LONG_QUESTION),  # your own voice
        ((INTERVIEWER, pcm(6.0), True), "Well, so, um, yes"),  # four words
        (
            (INTERVIEWER, pcm(4.0), True),
            "The weather has been quite nice this week in the city",
        ),  # a statement
    ]
    for item, text in cases:
        session, coach, screen, thread = make(tmp_path, text)
        session._utterances.put(item)
        time.sleep(0.25)
        assert not screen.started and not coach.asked, text
        finish(session, thread)


def test_a_long_statement_is_shown_to_the_model_which_may_say_skip(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, STATEMENT)
    session._utterances.put((INTERVIEWER, pcm(4.0), True))
    assert wait_for(lambda: coach.asked), "twelve words are enough for the model to judge"
    finish(session, thread)


def test_it_can_be_turned_off(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION, early_after=None)
    session._utterances.put((INTERVIEWER, pcm(7.0), True))
    time.sleep(0.3)
    assert not screen.started
    finish(session, thread)


def test_a_question_that_ends_as_it_began_keeps_the_answer_already_started(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION, LONG_QUESTION + "?")
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: screen.started)
    session._utterances.put((INTERVIEWER, pcm(7.0), False))
    assert wait_for(lambda: screen.turns)
    time.sleep(0.3)
    assert len(screen.started) == 1 and len(coach.asked) == 1, (
        "the answer is not started a second time"
    )
    assert screen.discarded == 0
    finish(session, thread)


def test_a_question_that_goes_on_to_say_what_is_asked_gets_a_new_answer_on_all_of_it(
    about, tmp_path
):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION, WHOLE_QUESTION)
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: screen.started)
    session._utterances.put((INTERVIEWER, pcm(11.0), False))
    assert wait_for(lambda: len(screen.started) == 2)
    assert screen.started[1] == WHOLE_QUESTION
    assert wait_for(lambda: len(coach.asked) == 2) and coach.asked[1][0] == WHOLE_QUESTION
    assert coach.listening == ["hearing", "heard"]
    finish(session, thread)


def test_the_early_answer_knows_the_conversation_so_far_but_not_the_question_twice(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION)
    session.log.add(INTERVIEWER, "Hello, thanks for joining.")
    session.log.add(YOU, "Thank you.")
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: coach.asked)
    assert coach.asked[0][1] == ["Interviewer: Hello, thanks for joining.", "You: Thank you."]
    finish(session, thread)


def test_only_one_early_answer_is_started_for_one_question(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION)
    for seconds in (3.2, 4.7, 6.2):
        session._utterances.put((INTERVIEWER, pcm(seconds), True))
        time.sleep(0.05)
    assert wait_for(lambda: screen.started)
    time.sleep(0.3)
    assert len(screen.started) == 1
    finish(session, thread)


def test_covering_is_about_the_same_words_and_not_much_more():
    assert covers(LONG_QUESTION, LONG_QUESTION + "?")
    assert covers(
        "tell me about a time you led a project", "Tell me about a time you led a project, ok."
    )
    assert not covers(LONG_QUESTION, WHOLE_QUESTION)
    assert not covers("what is a hash map", "tell me about your last job in detail please")
    assert not covers("", "anything") and not covers("anything", "")


class OneShot:
    def __init__(self, audio):
        self.audio = audio

    def frames(self):
        for start in range(0, len(self.audio) - FRAME_SAMPLES + 1, FRAME_SAMPLES):
            yield self.audio[start : start + FRAME_SAMPLES]

    def close(self):
        pass


def tone(seconds, amplitude=4000):
    t = np.arange(int(16_000 * seconds)) / 16_000
    return (amplitude * np.sin(2 * np.pi * 220 * t)).astype(np.int16)


def test_the_listener_takes_a_caption_exactly_when_the_speech_reaches_the_mark(about, tmp_path):
    session = InterviewSession(
        InterviewBrief.load(), None, Coach(), Callbacks(), log=TranscriptLog(tmp_path)
    )
    items = []
    session._utterances.put = items.append

    def quiet(frames):
        return np.zeros(FRAME_SAMPLES * frames, dtype=np.int16)

    session._listen(INTERVIEWER, OneShot(np.concatenate([quiet(10), tone(7.2), quiet(40)])))
    lengths = [len(p) / 16_000 for _s, p, partial in items if partial]
    assert any(abs(seconds - EARLY_AFTER) < 0.15 for seconds in lengths), lengths
    assert items[-1][2] is False


# ---- telling a question from the rest of what the computer plays ---------------------------


def test_what_the_model_says_is_not_a_question_gets_no_answer_and_no_empty_card(about, tmp_path):
    coach = Coach("SK", "IP")
    session, coach, screen, thread = make(
        tmp_path, "The Git folder really does hold every commit and branch.", coach=coach
    )
    session._utterances.put((INTERVIEWER, pcm(3.5), False))
    assert wait_for(lambda: coach.asked), "an eight-word statement is shown to the model"
    assert coach.listening == ["heard"]
    assert wait_for(lambda: screen.statuses)
    assert screen.started == [] and screen.chunks == [], "no card was opened for it"
    finish(session, thread)


def test_an_answer_is_announced_at_its_first_words_and_all_of_it_reaches_the_screen(
    about, tmp_path
):
    coach = Coach("SK", "ILL is what I would show: ", "a Git log.")
    session, coach, screen, thread = make(
        tmp_path, "Tell me about the tools you use every day?", coach=coach
    )
    session._utterances.put((INTERVIEWER, pcm(3.5), False))
    assert wait_for(lambda: "".join(screen.chunks).endswith("a Git log."))
    assert screen.started == ["Tell me about the tools you use every day?"]
    assert "".join(screen.chunks) == "SKILL is what I would show: a Git log."
    finish(session, thread)


def test_a_question_you_typed_is_never_judged_and_never_skipped(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, "unused", coach=Coach("SKIP"))
    session.ask("What is a hash map?")
    assert wait_for(lambda: screen.chunks)
    assert coach.listening == [None] and screen.started == ["What is a hash map?"]
    finish(session, thread)


def test_the_answer_started_on_part_of_a_question_is_dropped_when_the_whole_is_answered(
    about, tmp_path
):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION, WHOLE_QUESTION)
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: screen.started)
    assert screen.discarded == 0
    session._utterances.put((INTERVIEWER, pcm(11.0), False))
    assert wait_for(lambda: len(screen.started) == 2)
    assert screen.discarded == 1, "the early card goes when the answer to the whole question begins"
    session.ask("And what about Docker?")
    assert wait_for(lambda: len(screen.started) == 3)
    assert screen.discarded == 1, "a typed question leaves the cards alone"
    finish(session, thread)


def test_a_reply_that_is_skipped_does_not_drop_the_card_that_is_showing(about, tmp_path):
    session, coach, screen, thread = make(
        tmp_path,
        "Thanks for that, we will have a look at it together later today.",
        coach=Coach("SKIP"),
    )
    session._early_shown = time.monotonic()
    session._utterances.put((INTERVIEWER, pcm(4.0), False))
    assert wait_for(lambda: coach.asked)
    time.sleep(0.3)
    assert screen.discarded == 0 and screen.started == []
    finish(session, thread)


class Scripted(Coach):
    """A coach that answers each request in turn with the pieces given for it."""

    def __init__(self, *replies):
        super().__init__()
        self.replies = list(replies)

    def stream(self, question, history, cancel, language=None, image=None, listening=None):
        self.pieces = self.replies.pop(0) if len(self.replies) > 1 else self.replies[0]
        return super().stream(question, history, cancel, language, image, listening)


def test_when_the_first_words_were_only_the_set_up_it_tries_again_as_more_is_said(about, tmp_path):
    coach = Scripted(("SKIP",), ("An answer ", "to the question."))
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION, WHOLE_QUESTION, coach=coach)
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: len(coach.asked) == 1) and wait_for(lambda: screen.statuses)
    assert screen.started == [], "the model said SKIP to the set-up"
    time.sleep(2.1)  # a second try waits a couple of seconds, and for new words
    session._utterances.put((INTERVIEWER, pcm(6.4), True))
    assert wait_for(lambda: len(coach.asked) == 2)
    assert coach.asked[1][0] == WHOLE_QUESTION
    assert wait_for(lambda: screen.started == [WHOLE_QUESTION])
    assert coach.listening == ["hearing", "hearing"]
    time.sleep(2.1)
    session._utterances.put((INTERVIEWER, pcm(8.0), True))
    time.sleep(0.3)
    assert len(coach.asked) == 2, "once an answer is on the screen the question is not tried again"
    finish(session, thread)


def test_a_try_waits_for_the_one_before_it_and_for_new_words(about, tmp_path):
    session, coach, screen, thread = make(tmp_path, LONG_QUESTION, coach=Scripted(("SKIP",)))
    for seconds in (3.2, 4.7, 6.2):  # the same words each time: nothing new has been said
        session._utterances.put((INTERVIEWER, pcm(seconds), True))
        time.sleep(0.05)
    assert wait_for(lambda: coach.asked)
    time.sleep(2.2)
    session._utterances.put((INTERVIEWER, pcm(7.7), True))
    time.sleep(0.3)
    assert len(coach.asked) == 1, "no new words, no new try"
    finish(session, thread)


def test_a_task_or_a_scenario_put_to_the_candidate_counts_as_something_to_answer():
    from careertwin.interview.questions import is_question, worth_an_answer

    for said in [
        "Imagine a service that handles a million requests a day and it slows down.",
        "Walk me through how you would debug it.",
        "Let's talk about your experience with Java.",
        "Okay, tell me about Git.",
        "And how do you use Git?",
        "I'd like to hear about a project you are proud of.",
        "Представьте, что сервис замедлился.",
        "Давайте поговорим про базы данных.",
    ]:
        assert is_question(said), said
    for said in ["Thanks, great.", "Right.", "ok", "Yes."]:
        assert not worth_an_answer(said), said
    # a sentence of some length is shown to the model, which says SKIP when it is only a statement
    assert worth_an_answer(STATEMENT) and not is_question(STATEMENT)
    assert not worth_an_answer("We are based in Gdansk")


def test_a_statement_is_shown_to_the_model_once_only(about, tmp_path):
    more = STATEMENT + " and each of them has its own team and its own way of working with us"
    session, coach, screen, thread = make(tmp_path, STATEMENT, more, coach=Scripted(("SKIP",)))
    session._utterances.put((INTERVIEWER, pcm(3.2), True))
    assert wait_for(lambda: len(coach.asked) == 1)
    time.sleep(2.1)
    session._utterances.put((INTERVIEWER, pcm(6.4), True))
    time.sleep(0.3)
    assert len(coach.asked) == 1, "a set-up costs one request, not one for every few words more"
    finish(session, thread)
