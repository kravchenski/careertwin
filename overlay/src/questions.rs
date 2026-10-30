//! Cheap, dependency-light recognition of what an interviewer just asked.
//!
//! What counts as a question, and what kind, is a table per language (`lexicon::LEXICON`). Adding a language is
//! adding an entry: nothing else changes. A language with no entry is not ignored: see [`is_question`].

use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::lexicon::LEXICON;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Intro,
    Behavioral,
    Motivation,
    Salary,
    Logistics,
    CandidateQuestions,
    Definition,
    Technical,
    SmallTalk,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Intro => "intro",
            Kind::Behavioral => "behavioral",
            Kind::Motivation => "motivation",
            Kind::Salary => "salary",
            Kind::Logistics => "logistics",
            Kind::CandidateQuestions => "candidate_questions",
            Kind::Definition => "definition",
            Kind::Technical => "technical",
            Kind::SmallTalk => "small_talk",
        }
    }

    pub fn parse(value: &str) -> Option<Kind> {
        [
            Kind::Intro,
            Kind::Behavioral,
            Kind::Motivation,
            Kind::Salary,
            Kind::Logistics,
            Kind::CandidateQuestions,
            Kind::Definition,
            Kind::Technical,
            Kind::SmallTalk,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }

    /// A question about a technology: answered from the skills and the projects, with a smaller background.
    pub fn is_knowledge(self) -> bool {
        matches!(self, Kind::Technical | Kind::Definition)
    }
}

/// What a question looks like in any language the table does not know: a question mark.
const QUESTION_MARKS: [char; 5] = ['?', '？', '¿', '؟', '；'];
/// A reply in a language with no lexicon is sent as a question when it is at least this many words, because a
/// missing answer is worse than an extra one (the Ask button and Pause are there).
const FALLBACK_WORDS: usize = 5;
/// Words from which a sentence that is not a question is still shown to the model to judge.
const SENTENCE_WORDS: usize = 8;

/// Kinds in the order they are tried.
const KINDS: [(&str, Kind); 7] = [
    ("salary", Kind::Salary),
    ("candidate_questions", Kind::CandidateQuestions),
    ("intro", Kind::Intro),
    ("behavioral", Kind::Behavioral),
    ("motivation", Kind::Motivation),
    ("definition", Kind::Definition),
    ("logistics", Kind::Logistics),
];

fn union(field: &str) -> String {
    LEXICON
        .iter()
        .filter_map(|(_, entry)| entry.iter().find(|(name, _)| *name == field))
        .map(|(_, pattern)| format!("(?:{pattern})"))
        .collect::<Vec<_>>()
        .join("|")
}

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("a lexicon pattern does not compile: {error}"))
}

static PATTERNS: LazyLock<Vec<(Kind, Regex)>> =
    LazyLock::new(|| KINDS.iter().map(|(field, kind)| (*kind, compile(&format!("(?i){}", union(field))))).collect());
/// A question may begin with a discourse word: "okay, tell me about...", "and how do you...".
static QUESTION_START: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(r"(?i)^(?:(?:and|so|okay|ok|well|now|then|alright)[ ,]+)?(?:{})\b", union("start")))
});
static SMALL_TALK: LazyLock<Regex> =
    LazyLock::new(|| compile(&format!(r"(?i)^(?:{})\b[\s,.!]*$", union("small_talk"))));

fn matches(regex: &Regex, text: &str) -> bool {
    regex.is_match(text).unwrap_or(false)
}

fn collapse(text: &str, limit: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(limit).collect()
}

fn has_lexicon(language: &str) -> bool {
    LEXICON.iter().any(|(code, _)| *code == language)
}

/// True when an interviewer utterance expects the candidate to answer. `language` is the interview language
/// when it is known; one with no lexicon entry is answered when the utterance has a question mark or is at
/// least [`FALLBACK_WORDS`] words long, so a dialogue in it does not stay silent.
pub fn is_question(text: &str, language: Option<&str>) -> bool {
    let value = collapse(text, 2000);
    if value.chars().count() < 5 || matches(&SMALL_TALK, &value) {
        return false;
    }
    if value.contains(QUESTION_MARKS) || matches(&QUESTION_START, &value) {
        return true;
    }
    if PATTERNS.iter().any(|(_, pattern)| matches(pattern, &value)) {
        return true;
    }
    language.is_some_and(|code| !code.is_empty() && !has_lexicon(code)) && value.split(' ').count() >= FALLBACK_WORDS
}

/// Whether an interviewer's words may be something to answer: a question or a task, or enough said that one
/// was probably put. The model has the last word and replies SKIP to what is not for the candidate, so this
/// errs on the side of asking.
pub fn worth_an_answer(text: &str, language: Option<&str>) -> bool {
    let value = collapse(text, usize::MAX);
    if is_question(&value, language) {
        return true;
    }
    value.split(' ').count() >= SENTENCE_WORDS && !matches(&SMALL_TALK, &value)
}

/// Whether a short utterance names a technology of the vacancy or the candidate ("levoe PostgreSQL"): too short
/// to look like a question, but the model may read a question out of it.
pub fn mentions(text: &str, vocabulary: &str) -> bool {
    let value = collapse(text, usize::MAX);
    if value.split(' ').count() < 2 || matches(&SMALL_TALK, &value) {
        return false;
    }
    vocabulary.split([',', '/']).map(str::trim).filter(|term| term.chars().count() > 2).any(|term| {
        Regex::new(&format!(r"(?i)(?<!\w){}(?!\w)", fancy_regex::escape(term))).is_ok_and(|re| matches(&re, &value))
    })
}

pub fn classify(text: &str) -> Kind {
    // a spoken question is short; the cut keeps a hostile one cheap
    let value = collapse(text, 2000);
    if matches(&SMALL_TALK, &value) {
        return Kind::SmallTalk;
    }
    PATTERNS.iter().find(|(_, pattern)| matches(pattern, &value)).map_or(Kind::Technical, |(kind, _)| *kind)
}

static UKRAINIAN_LETTERS: LazyLock<Regex> = LazyLock::new(|| compile(r"[іїєґІЇЄҐ]"));
static RUSSIAN_LETTERS: LazyLock<Regex> = LazyLock::new(|| compile(r"[ыэъёЫЭЪЁ]"));
static UKRAINIAN_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(?i)\b(що|як|чому|де|коли|хто|який|яка|які|чи|будь ласка|дякую|так|ні|розкажіть|поясніть)\b")
});
static RUSSIAN_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)\b(что|как|почему|где|когда|кто|какой|какая|какие|ли|пожалуйста|спасибо|да|нет|расскажите|объясните)\b",
    )
});
static CYRILLIC: LazyLock<Regex> = LazyLock::new(|| compile(r"[а-яёА-ЯЁіїєґІЇЄҐўЎ]"));
static POLISH: LazyLock<Regex> = LazyLock::new(|| compile(r"[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]"));

fn count(regex: &Regex, text: &str) -> usize {
    regex.find_iter(text).filter(Result::is_ok).count()
}

/// `uk`, `be`, `ru`, `pl` or `en`; enough to tell the model which language to answer in.
pub fn detect_language(text: &str) -> &'static str {
    if matches(&CYRILLIC, text) {
        if text.contains(['ў', 'Ў']) {
            return "be";
        }
        let ukrainian = count(&UKRAINIAN_LETTERS, text) * 2 + count(&UKRAINIAN_WORDS, text);
        let russian = count(&RUSSIAN_LETTERS, text) * 2 + count(&RUSSIAN_WORDS, text);
        return if ukrainian > russian { "uk" } else { "ru" };
    }
    if matches(&POLISH, text) { "pl" } else { "en" }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UKRAINIAN: [(&str, Kind); 9] = [
        ("Розкажіть про себе", Kind::Intro),
        ("Чому ви обрали нашу компанію", Kind::Motivation),
        ("Що таке REST", Kind::Definition),
        ("Поясніть різницю між процесом і потоком", Kind::Definition),
        ("Яка у вас очікувана зарплата", Kind::Salary),
        ("Коли ви зможете вийти на роботу", Kind::Logistics),
        ("Наведіть приклад складної ситуації", Kind::Behavioral),
        ("Чи є у вас запитання до нас", Kind::CandidateQuestions),
        ("Як ви працюєте з базами даних", Kind::Technical),
    ];

    #[test]
    fn a_ukrainian_question_is_heard_without_a_question_mark() {
        for (text, kind) in UKRAINIAN {
            assert!(is_question(text, Some("uk")) && is_question(text, None), "{text}");
            assert_eq!(classify(text), kind, "{text}");
        }
    }

    #[test]
    fn a_pleasantry_is_not_a_question() {
        for text in ["Дякую", "Добре", "Так", "Привіт", "Зрозуміло", "Доброго дня", "Thanks!", "okay"]
        {
            assert!(!is_question(text, Some("uk")), "{text}");
            assert_eq!(classify(text), Kind::SmallTalk, "{text}");
        }
    }

    #[test]
    fn the_language_of_a_question_is_told_apart() {
        assert_eq!(detect_language("Розкажіть про себе"), "uk");
        assert_eq!(detect_language("Що таке REST?"), "uk");
        assert_eq!(detect_language("Як ви працюєте з Docker"), "uk");
        assert_eq!(detect_language("Расскажите о себе"), "ru");
        assert_eq!(detect_language("Что такое REST и как это работает?"), "ru");
        assert_eq!(detect_language("Раскажы пра сябе, ўсё добра"), "be");
        assert_eq!(detect_language("Opowiedz o sobie, proszę"), "pl");
        assert_eq!(detect_language("Tell me about yourself"), "en");
    }

    #[test]
    fn questions_are_recognised_in_the_language_they_are_asked_in() {
        for (language, text) in [
            ("de", "Erzählen Sie mir von sich"),
            ("de", "Warum möchten Sie bei uns arbeiten"),
            ("fr", "Parlez-moi de votre expérience"),
            ("fr", "Pourquoi voulez-vous ce poste"),
            ("es", "Cuéntame sobre tu experiencia"),
            ("es", "Por qué quieres trabajar con nosotros"),
            ("ru", "Расскажите о своём опыте"),
            ("pl", "Opowiedz o swoim doświadczeniu"),
        ] {
            assert!(is_question(text, Some(language)), "{language}: {text}");
        }
    }

    #[test]
    fn english_kinds() {
        for (text, kind) in [
            ("Tell me about yourself", Kind::Intro),
            ("Tell me about a time you failed", Kind::Behavioral),
            ("Why do you want this role?", Kind::Motivation),
            ("What are your salary expectations?", Kind::Salary),
            ("What is a left join?", Kind::Definition),
            ("What's the difference between a list and a tuple", Kind::Definition),
            ("When can you start?", Kind::Logistics),
            ("Do you have any questions for us?", Kind::CandidateQuestions),
            ("How would you design a rate limiter?", Kind::Technical),
        ] {
            assert_eq!(classify(text), kind, "{text}");
            assert!(is_question(text, Some("en")), "{text}");
        }
    }

    #[test]
    fn what_is_your_name_is_about_the_candidate_not_a_definition() {
        // the lookahead keeps "what is your ..." out of the definition kind
        assert_ne!(classify("What is your biggest strength"), Kind::Definition);
    }

    #[test]
    fn a_task_worded_as_an_order_is_a_question() {
        for text in
            ["Write a function that reverses a string", "Imagine the queue stalls at night", "Let's talk about indexes"]
        {
            assert!(is_question(text, Some("en")), "{text}");
        }
    }

    #[test]
    fn a_language_without_a_lexicon_is_answered_when_long_enough_or_marked() {
        assert!(is_question("Povězte mi něco o svých zkušenostech s databázemi", Some("cs")));
        assert!(is_question("Jak se máš?", Some("cs")));
        assert!(!is_question("dobře dobře", Some("cs")));
    }

    #[test]
    fn long_talk_that_is_not_a_question_is_still_shown_to_the_model() {
        let said = "We are a team of twelve people working on payments for small shops in Poland";
        assert!(!is_question(said, Some("en")));
        assert!(worth_an_answer(said, Some("en")));
        assert!(!worth_an_answer("Thank you very much", Some("en")));
    }

    #[test]
    fn a_short_mention_of_a_technology_is_worth_a_look() {
        assert!(mentions("left PostgreSQL", "Python, SQL / PostgreSQL, Docker"));
        assert!(mentions("и про docker", "Docker"));
        assert!(!mentions("PostgreSQL", "PostgreSQL")); // one word is not enough
        assert!(!mentions("nothing here", "PostgreSQL"));
        assert!(!mentions("thank you", "SQL"));
    }
}
