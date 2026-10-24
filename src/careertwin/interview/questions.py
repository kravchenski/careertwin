"""Cheap, dependency-free recognition of what an interviewer just asked."""

from __future__ import annotations

import re
from enum import StrEnum


class Kind(StrEnum):
    INTRO = "intro"
    BEHAVIORAL = "behavioral"
    MOTIVATION = "motivation"
    SALARY = "salary"
    LOGISTICS = "logistics"
    CANDIDATE_QUESTIONS = "candidate_questions"
    DEFINITION = "definition"
    TECHNICAL = "technical"
    SMALL_TALK = "small_talk"


_PATTERNS: list[tuple[Kind, re.Pattern[str]]] = [
    (Kind.SALARY, re.compile(
        r"salary|compensation|pay expectation|rate expectation|how much.*(earn|want|expect)|"
        r"зарплат|ожидани\w+ по (деньгам|оплате)|вилк|"
        r"wynagrodzeni|oczekiwania finansowe|stawk", re.I)),
    (Kind.CANDIDATE_QUESTIONS, re.compile(
        r"any questions (for|to) (us|me)|do you have (any )?questions|"
        r"есть (ли )?(у вас )?вопросы|ваши вопросы|masz (jakieś )?pytania", re.I)),
    (Kind.INTRO, re.compile(
        r"tell me about yourself|introduce yourself|walk me through your (resume|cv|background)|"
        r"расскажите о себе|представьтесь|расскажи о себе|opowiedz o sobie|przedstaw się", re.I)),
    (Kind.BEHAVIORAL, re.compile(
        r"tell me about a time|describe a (time|situation)|give me an example|"
        r"how did you (handle|deal)|biggest (challenge|failure|mistake)|conflict|"
        r"расскажите о (случае|ситуации)|приведите пример|как вы (справились|решали)|"
        r"opowiedz o sytuacji|podaj przykład|jak (poradziłeś|poradziłaś)", re.I)),
    (Kind.MOTIVATION, re.compile(
        r"why (do you want|are you interested|this (company|role|position)|should we)|"
        r"where do you see yourself|why are you leaving|"
        r"почему (вы )?(хотите|выбрали|ушли|уходите)|что вас привлекает|"
        r"dlaczego (chcesz|wybrałeś|wybrałaś|odchodzisz)", re.I)),
    (Kind.DEFINITION, re.compile(
        r"^(so,? )?(what is|what are|what's|what does .+ mean|explain|define|describe how|"
        r"how does .+ work|difference between|what's the difference|why use)\b(?! your| you\b)|"
        r"что так(ое|ая|ие|ой)|что значит|объясни|расскажи(те)? (про|о том, что)|в чём разница|чем отличается|"
        r"как работает|"
        r"co to (jest|są)|czym (jest|są)|wyjaśnij|jaka jest różnica|czym się różni|jak działa", re.I)),
    (Kind.LOGISTICS, re.compile(
        r"notice period|when can you start|available to start|work permit|visa|relocat|"
        r"remote|on-?site|hybrid|time ?zone|"
        r"когда (вы )?(сможете|готовы) выйти|срок уведомления|переезд|удал[её]н|гибрид|"
        r"okres wypowiedzenia|kiedy możesz zacząć|relokacj|zdaln", re.I)),
]

_QUESTION_START = re.compile(
    r"^(what|why|how|when|where|who|which|can|could|would|do|does|did|are|is|have|has|will|"
    r"tell|describe|explain|walk|talk|give|share|show|name|list|compare|"
    r"что|почему|как|когда|где|кто|какой|какая|какие|можете|расскажите|опишите|объясните|"
    r"приведите|назовите|сравните|есть ли|"
    r"co|dlaczego|jak|kiedy|gdzie|kto|który|czy|opowiedz|opisz|wyjaśnij|podaj)\b",
    re.I,
)
_SMALL_TALK = re.compile(
    r"^(hi|hello|hey|thanks|thank you|great|sure|okay|ok|nice|good|right|yes|no|"
    r"привет|здравствуйте|спасибо|отлично|хорошо|ясно|понятно|да|нет|"
    r"cześć|dzień dobry|dziękuję|dobrze)\b[\s,.!]*$",
    re.I,
)


def is_question(text: str) -> bool:
    """True when an interviewer utterance expects the candidate to answer."""
    value = " ".join(text.split())
    if len(value) < 5 or _SMALL_TALK.match(value):
        return False
    return "?" in value or bool(_QUESTION_START.match(value)) or any(
        pattern.search(value) for _, pattern in _PATTERNS
    )


def classify(text: str) -> Kind:
    value = " ".join(text.split())
    if _SMALL_TALK.match(value):
        return Kind.SMALL_TALK
    for kind, pattern in _PATTERNS:
        if pattern.search(value):
            return kind
    return Kind.TECHNICAL


def detect_language(text: str) -> str:
    """Return `ru`, `pl` or `en`; enough to tell the model which language to answer in."""
    if re.search(r"[а-яёА-ЯЁ]", text):
        return "ru"
    if re.search(r"[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]", text):
        return "pl"
    return "en"
