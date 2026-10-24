"""Cheap, dependency-free recognition of what an interviewer just asked.

What counts as a question, and what kind, is a table per language (`LEXICON`). Adding a language is
adding an entry: nothing else changes. A language with no entry is not ignored: see `is_question`.
"""

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


# Regular-expression fragments, one per kind, plus the words a question starts with and the short
# replies that need no answer. All fragments are matched case-insensitively.
LEXICON: dict[str, dict[str, str]] = {
    "en": {
        "salary": r"salary|compensation|pay expectation|rate expectation|how much.*(earn|want|expect)",
        "candidate_questions": r"any questions (for|to) (us|me)|do you have (any )?questions",
        "intro": r"tell me about yourself|introduce yourself|walk me through your (resume|cv|background)",
        "behavioral": r"tell me about a time|describe a (time|situation)|give me an example|"
        r"how did you (handle|deal)|biggest (challenge|failure|mistake)|conflict",
        "motivation": r"why (do you want|are you interested|this (company|role|position)|should we)|"
        r"where do you see yourself|why are you leaving",
        "definition": r"^(so,? )?(what is|what are|what's|what does .+ mean|explain|define|describe how|"
        r"how does .+ work|difference between|what's the difference|why use)\b(?! your| you\b)",
        "logistics": r"notice period|when can you start|available to start|start date|availability|work permit|"
        r"visa|relocat|remote|on-?site|hybrid|in the office|time ?zone|commut|full[- ]time|part[- ]time|"
        r"hours (a|per) (week|day)|\d+ hours|manage .*(studies|study|university|exams?|classes)|"
        r"(studies|university|exams?|classes) .*(work|internship|hours)",
        "start": r"what|why|how|when|where|who|which|can|could|would|do|does|did|are|is|have|has|will|"
        r"tell|describe|explain|walk|talk|give|share|show|name|list|compare",
        "small_talk": r"hi|hello|hey|thanks|thank you|great|sure|okay|ok|nice|good|right|yes|no",
    },
    "ru": {
        "salary": r"зарплат|ожидани\w+ по (деньгам|оплате)|вилк",
        "candidate_questions": r"есть (ли )?(у вас )?вопросы|ваши вопросы",
        "intro": r"расскажите о себе|представьтесь|расскажи о себе",
        "behavioral": r"расскажите о (случае|ситуации)|приведите пример|как вы (справились|решали)",
        "motivation": r"почему (вы )?(хотите|выбрали|ушли|уходите)|что вас привлекает",
        "definition": r"что так(ое|ая|ие|ой)|что значит|объясни|расскажи(те)? (про|о том, что)|в чём разница|"
        r"чем отличается|как работает",
        "logistics": r"когда (вы )?(сможете|готовы) выйти|срок уведомления|переезд|удал[её]н|гибрид|в офисе|"
        r"сколько часов|совмещать .*(учёб|универ)|график",
        "start": r"что|почему|как|когда|где|кто|какой|какая|какие|можете|расскажите|опишите|объясните|"
        r"приведите|назовите|сравните|есть ли",
        "small_talk": r"привет|здравствуйте|спасибо|отлично|хорошо|ясно|понятно|да|нет",
    },
    "pl": {
        "salary": r"wynagrodzeni|oczekiwania finansowe|stawk",
        "candidate_questions": r"masz (jakieś )?pytania",
        "intro": r"opowiedz o sobie|przedstaw się",
        "behavioral": r"opowiedz o sytuacji|podaj przykład|jak (poradziłeś|poradziłaś)",
        "motivation": r"dlaczego (chcesz|wybrałeś|wybrałaś|odchodzisz)",
        "definition": r"co to (jest|są)|czym (jest|są)|wyjaśnij|jaka jest różnica|czym się różni|jak działa",
        "logistics": r"okres wypowiedzenia|kiedy możesz zacząć|relokacj|zdaln|w biurze|ile godzin|studi",
        "start": r"co|dlaczego|jak|kiedy|gdzie|kto|który|czy|opowiedz|opisz|wyjaśnij|podaj",
        "small_talk": r"cześć|dzień dobry|dziękuję|dobrze",
    },
    "uk": {
        "salary": r"зарплат|очікуван\w+ (щодо|по) (зарплат|оплат|грош)|вилк|оплат\w+ праці|"
        r"скільки (ви )?хочете (заробляти|отримувати)",
        "candidate_questions": r"є (у вас )?(якісь )?запитання|ваші запитання|маєте (якісь )?запитання",
        "intro": r"розкажіть про себе|розкажи про себе|представтеся|представся|розкажіть про свій досвід",
        "behavioral": r"розкажіть про (випадок|ситуацію)|наведіть приклад|як ви (впоралися|вирішували|справлялися)|"
        r"найбільш\w+ (складн|помилк|невдач)|конфлікт",
        "motivation": r"чому (ви )?(хочете|обрали|вибрали|пішли|йдете)|що вас приваблює|де ви бачите себе",
        "definition": r"що так(е|а|і|ий)|що означає|поясніть|поясни|розкажіть (про|що таке)|у чому різниця|"
        r"чим відрізняється|як працює",
        "logistics": r"коли (ви )?(зможете|готові) (вийти|почати)|термін повідомлення|переїзд|віддален|гібрид|"
        r"в офісі|скільки годин|поєднувати .*(навчан|універ)|графік|робочий дозвіл|віза|"
        r"повн\w+ (день|зайнятість)|часткова зайнятість",
        "start": r"що|чому|як|коли|де|хто|який|яка|яке|які|чи|можете|розкажіть|розкажи|опишіть|поясніть|поясни|"
        r"наведіть|назвіть|порівняйте|скільки|навіщо|звідки|куди|чим",
        "small_talk": r"привіт|вітаю|добрий день|доброго дня|добридень|дякую|дякуємо|чудово|добре|зрозуміло|"
        r"ясно|так|ні|гаразд",
    },
    "de": {
        "salary": r"gehalt|vergütung|gehaltsvorstellung",
        "candidate_questions": r"haben sie (noch )?fragen|ihre fragen",
        "intro": r"erzählen sie (mir )?(etwas )?(von|über) sich|stellen sie sich vor",
        "behavioral": r"erzählen sie (mir )?von einer situation|nennen sie ein beispiel|wie sind sie .*umgegangen|"
        r"größte[rn]? (herausforderung|fehler)|konflikt",
        "motivation": r"warum (möchten|wollen) sie|warum haben sie sich .*beworben|wo sehen sie sich|"
        r"warum verlassen sie",
        "definition": r"was (ist|sind|bedeutet)|erklären sie|definieren sie|wie funktioniert|unterschied zwischen",
        "logistics": r"kündigungsfrist|wann können sie (anfangen|starten)|arbeitserlaubnis|visum|umzug|homeoffice|"
        r"vor ort|stunden pro woche|vollzeit|teilzeit|studium",
        "start": r"was|warum|wieso|weshalb|wie|wann|wo|wer|welche[rsmn]?|können sie|könnten sie|würden sie|"
        r"haben sie|sind sie|erzählen sie|beschreiben sie|erklären sie|nennen sie|vergleichen sie|gibt es|"
        r"wie viele|woher|wohin|sagen sie",
        "small_talk": r"hallo|guten (tag|morgen|abend)|danke|vielen dank|gut|klar|ja|nein|genau|verstanden|super",
    },
    "fr": {
        "salary": r"salaire|prétentions salariales|rémunération|attentes salariales",
        "candidate_questions": r"avez-vous des questions|vos questions",
        "intro": r"parlez-moi de vous|présentez-vous|racontez-moi votre parcours",
        "behavioral": r"racontez-moi une (situation|fois)|donnez-moi un exemple|comment avez-vous (géré|résolu|réagi)|"
        r"plus grand(e)? (défi|échec|erreur)|conflit",
        "motivation": r"pourquoi (voulez|souhaitez|avez-vous choisi|quittez)|où vous voyez-vous",
        "definition": r"qu['’]est-ce que|qu['’]est-ce qu|que signifie|expliquez|définissez|comment fonctionne|"
        r"différence entre",
        "logistics": r"préavis|quand pouvez-vous (commencer|démarrer)|permis de travail|déménagement|télétravail|"
        r"hybride|sur site|heures par semaine|temps plein|temps partiel|études",
        "start": r"qu['’]est-ce|que|quoi|pourquoi|comment|quand|où|qui|quel|quelle|quels|quelles|pouvez-vous|"
        r"pourriez-vous|est-ce que|avez-vous|êtes-vous|parlez-moi|décrivez|expliquez|donnez|citez|comparez|"
        r"combien|dites-moi|racontez",
        "small_talk": r"bonjour|salut|bonsoir|merci|d['’]accord|bien|oui|non|parfait|compris|super|très bien",
    },
    "es": {
        "salary": r"salario|sueldo|expectativas salariales|remuneración|pretensiones",
        "candidate_questions": r"tienes (alguna )?pregunta|tiene (alguna )?pregunta|alguna pregunta para nosotros",
        "intro": r"háblame de ti|cuéntame sobre ti|preséntate|hábleme de usted|cuénteme sobre usted",
        "behavioral": r"cuéntame (sobre )?(una vez|una situación)|dame un ejemplo|cómo (manejaste|resolviste|"
        r"afrontaste)|mayor (reto|desafío|error|fracaso)|conflicto",
        "motivation": r"por qué (quieres|te interesa|elegiste|dejas|te vas)|dónde te ves",
        "definition": r"qué (es|son|significa)|explica|define|cómo funciona|diferencia entre|"
        r"cuál es la diferencia",
        "logistics": r"preaviso|cuándo puedes (empezar|incorporarte)|permiso de trabajo|visado|reubicación|remoto|"
        r"híbrido|presencial|horas (a la|por) semana|tiempo completo|media jornada|estudios",
        "start": r"qué|por qué|cómo|cuándo|dónde|quién|cuál|cuáles|puedes|puede|podría|podrías|cuéntame|"
        r"cuénteme|describe|describa|explica|explique|dime|dígame|háblame|hábleme|hay|tiene|tienes|cuánto|"
        r"cuántos|para qué",
        "small_talk": r"hola|buenos días|buenas tardes|gracias|muchas gracias|bien|vale|claro|sí|perfecto|"
        r"entendido|genial",
    },
}

# What a question looks like in any language the table does not know: a question mark.
QUESTION_MARKS = ("?", "？", "¿", "؟", "；")
# A reply in a language with no lexicon is sent as a question when it is at least this many words,
# because a missing answer is worse than an extra one (the Ask button and Pause are there).
FALLBACK_WORDS = 5

_KINDS = (
    ("salary", Kind.SALARY),
    ("candidate_questions", Kind.CANDIDATE_QUESTIONS),
    ("intro", Kind.INTRO),
    ("behavioral", Kind.BEHAVIORAL),
    ("motivation", Kind.MOTIVATION),
    ("definition", Kind.DEFINITION),
    ("logistics", Kind.LOGISTICS),
)


def _union(field: str) -> str:
    return "|".join(f"(?:{entry[field]})" for entry in LEXICON.values() if entry.get(field))


_PATTERNS: list[tuple[Kind, re.Pattern[str]]] = [
    (kind, re.compile(_union(field), re.I)) for field, kind in _KINDS
]
_QUESTION_START = re.compile(rf"^(?:{_union('start')})\b", re.I)
_SMALL_TALK = re.compile(rf"^(?:{_union('small_talk')})\b[\s,.!]*$", re.I)


def is_question(text: str, language: str | None = None) -> bool:
    """True when an interviewer utterance expects the candidate to answer.

    `language` is the interview language when it is known. A language with no `LEXICON` entry is
    answered when the utterance has a question mark or is at least `FALLBACK_WORDS` words long, so a
    dialogue in it does not stay silent just because nobody wrote its question words yet.
    """
    value = " ".join(text.split())[:2000]
    if len(value) < 5 or _SMALL_TALK.match(value):
        return False
    if any(mark in value for mark in QUESTION_MARKS) or _QUESTION_START.match(value):
        return True
    if any(pattern.search(value) for _, pattern in _PATTERNS):
        return True
    return bool(language) and language not in LEXICON and len(value.split()) >= FALLBACK_WORDS


def classify(text: str) -> Kind:
    value = " ".join(text.split())[:2000]  # a spoken question is short; the cut keeps a hostile one cheap
    if _SMALL_TALK.match(value):
        return Kind.SMALL_TALK
    for kind, pattern in _PATTERNS:
        if pattern.search(value):
            return kind
    return Kind.TECHNICAL


_UKRAINIAN_LETTERS = re.compile(r"[іїєґІЇЄҐ]")
_RUSSIAN_LETTERS = re.compile(r"[ыэъёЫЭЪЁ]")
_UKRAINIAN_WORDS = re.compile(
    r"\b(що|як|чому|де|коли|хто|який|яка|які|чи|будь ласка|дякую|так|ні|розкажіть|поясніть)\b", re.I
)
_RUSSIAN_WORDS = re.compile(
    r"\b(что|как|почему|где|когда|кто|какой|какая|какие|ли|пожалуйста|спасибо|да|нет|расскажите|объясните)\b",
    re.I,
)


def detect_language(text: str) -> str:
    """Return `uk`, `be`, `ru`, `pl` or `en`; enough to tell the model which language to answer in."""
    if re.search(r"[а-яёА-ЯЁіїєґІЇЄҐўЎ]", text):
        if re.search(r"[ўЎ]", text):
            return "be"
        ukrainian = len(_UKRAINIAN_LETTERS.findall(text)) * 2 + len(_UKRAINIAN_WORDS.findall(text))
        russian = len(_RUSSIAN_LETTERS.findall(text)) * 2 + len(_RUSSIAN_WORDS.findall(text))
        return "uk" if ukrainian > russian else "ru"
    if re.search(r"[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]", text):
        return "pl"
    return "en"
