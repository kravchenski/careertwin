//! Every word on the screen, in English and Russian.

use crate::config::UiLanguage;

macro_rules! strings {
    ($($name:ident => ($en:expr, $ru:expr)),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum K { $($name),* }

        impl K {
            pub const ALL: &'static [K] = &[$(K::$name),*];

            pub fn en(self) -> &'static str {
                match self { $(K::$name => $en),* }
            }

            pub fn ru(self) -> &'static str {
                match self { $(K::$name => $ru),* }
            }
        }
    };
}

strings! {
    // the three tabs
    TabEnglish => ("English", "Английский"),
    TabTechnical => ("Technical", "Техническое"),
    TabInterview => ("Interview", "Собеседование"),

    // home of each tab
    EnglishTitle => ("English practice", "Практика английского"),
    EnglishIntro => (
        "Talk it through out loud with a partner who answers like a person. The review afterwards shows what to fix.",
        "Говорите вслух с собеседником, который отвечает как человек. Разбор после покажет, что исправить."
    ),
    TechnicalTitle => ("Technical interview", "Техническое собеседование"),
    TechnicalIntro => (
        "Be asked about the real work in your profile, then about the stack. Follow-up questions, no hints, a review at the end.",
        "Вас спросят о реальной работе из профиля, потом о стеке. Уточняющие вопросы, без подсказок, разбор в конце."
    ),
    InterviewTitle => ("Job interview", "Собеседование"),
    InterviewIntro => (
        "Introductions, motivation, behavioural questions and salary, with someone who listens to the answer.",
        "Знакомство, мотивация, поведенческие вопросы и зарплата с тем, кто слушает ответ."
    ),
    CardConversation => ("Conversation", "Разговор"),
    CardConversationText => ("A relaxed talk about your work and plans, in spoken English.", "Спокойный разговор о вашей работе и планах на английском."),
    CardMock => ("Mock interview", "Пробное собеседование"),
    CardMockTechnicalText => ("A spoken technical interview with follow-ups, like the real one.", "Устное техническое собеседование с уточнениями, как настоящее."),
    CardMockInterviewText => ("A spoken interview with a recruiter or a hiring manager.", "Устное собеседование с рекрутером или нанимающим менеджером."),
    CardTranslate => ("Say it in English", "Скажи по-английски"),
    CardTranslateText => ("Say a thought in Russian, hear it in natural English, then say it back.", "Скажите мысль по-русски, услышьте её по-английски и повторите."),
    CardPhrases => ("Phrases to review", "Фразы для повторения"),
    CardPhrasesText => ("Phrases from your reviews, brought back just before you forget them.", "Фразы из ваших разборов: возвращаются перед тем, как вы их забудете."),
    CardConcepts => ("Concept checks", "Проверка понимания"),
    CardConceptsText => ("Questions with your confidence: sure and wrong comes back soon.", "Вопросы с вашей уверенностью: уверен и ошибся — вернётся скоро."),
    CardPrepare => ("Prepare for a vacancy", "Подготовка к вакансии"),
    CardPrepareText => ("Your CV against the vacancy, and the questions it invites.", "Ваше резюме против вакансии и вопросы, которые она вызывает."),
    ContinueLabel => ("Recent", "Недавнее"),
    DueForReview => ("{n} to review today", "К повторению сегодня: {n}"),
    NothingYet => ("Nothing here yet.", "Здесь пока пусто."),

    // setting up a call
    SetupInterviewer => ("Interviewer", "Интервьюер"),
    SetupLevel => ("Their English", "Их английский"),
    SetupLanguage => ("Language", "Язык"),
    SetupLength => ("Length", "Длительность"),
    SetupFocus => ("Focus (optional)", "Фокус (необязательно)"),
    SetupFocusHint => ("for example: PostgreSQL indexes, async, system design", "например: индексы PostgreSQL, async, дизайн систем"),
    SetupHeadphones => ("I wear headphones (lets you interrupt)", "Я в наушниках (можно перебивать)"),
    SetupVoiceOff => ("Voice is off: the interviewer only writes", "Голос выключен: интервьюер только пишет"),
    StartCall => ("Start call", "Начать звонок"),
    Minutes => ("{n} min", "{n} мин"),
    LangEnglish => ("English", "Английский"),
    LangRussian => ("Russian", "Русский"),
    RoleTechLead => ("Tech lead", "Тимлид"),
    RoleArchitect => ("Architect", "Архитектор"),
    RoleHiringManager => ("Hiring manager", "Нанимающий менеджер"),
    RoleRecruiter => ("Recruiter", "Рекрутер"),
    RoleTutor => ("Conversation partner", "Собеседник"),
    NoProfile => (
        "No profile found. Open Settings and choose your ABOUT.md, so the interviewer asks about your real experience.",
        "Профиль не найден. Откройте настройки и выберите ABOUT.md, чтобы интервьюер спрашивал о вашем реальном опыте."
    ),
    OpenSettings => ("Open Settings", "Открыть настройки"),

    // the call
    StatusStarting => ("Connecting…", "Соединяю…"),
    StatusThinking => ("{name} is thinking…", "{name} думает…"),
    StatusSpeaking => ("{name} is speaking", "{name} говорит"),
    StatusListening => ("Your turn. I'm listening.", "Ваша очередь. Я слушаю."),
    StatusMuted => ("Microphone is off", "Микрофон выключен"),
    StatusFinished => ("The call has ended", "Звонок завершён"),
    Live => ("LIVE", "ЭФИР"),
    Transcript => ("Transcript", "Стенограмма"),
    TypeAnswer => ("Type your answer…", "Напишите ответ…"),
    Send => ("Send", "Отправить"),
    Mute => ("Mute", "Выключить"),
    Unmute => ("Talk", "Говорить"),
    Interrupt => ("Skip", "Пропустить"),
    EndCall => ("End call", "Завершить"),
    Retry => ("Try again", "Повторить"),
    You => ("YOU", "ВЫ"),
    TimeLeft => ("left", "осталось"),
    EndConfirm => ("End the call and get your review?", "Завершить звонок и получить разбор?"),
    EndAndReview => ("End and review", "Завершить и разобрать"),
    KeepGoing => ("Keep going", "Продолжить"),

    // the review
    ReviewTitle => ("Review", "Разбор"),
    ReviewPreparing => ("Preparing your review…", "Готовлю ваш разбор…"),
    ReviewFailed => ("The review could not be made", "Разбор не получился"),
    Scores => ("Scores", "Оценки"),
    Priorities => ("What to work on", "Над чем работать"),
    YourAnswers => ("Your answers", "Ваши ответы"),
    YouSaid => ("You said", "Вы сказали"),
    WentWell => ("Went well", "Получилось"),
    ToFix => ("To fix", "Исправить"),
    SayItLike => ("Say it like this", "Скажите так"),
    MissedPoints => ("A strong answer also says", "Сильный ответ ещё говорит"),
    LanguageNotes => ("Language notes", "Заметки по языку"),
    PhrasesToLearn => ("Phrases to learn", "Фразы для заучивания"),
    AddToDeck => ("Add to review", "В повторение"),
    Added => ("Added", "Добавлено"),
    Listen => ("Listen", "Слушать"),
    SayItYourself => ("Say it yourself", "Повторить самому"),
    Listening => ("Listening…", "Слушаю…"),
    Heard => ("Heard", "Распознано"),
    WordsMatched => ("{n}% of the words matched", "Совпало слов: {n}%"),
    MissedWords => ("Missed: {words}", "Пропущено: {words}"),
    PractiseAgain => ("Practise again", "Ещё раз"),
    BackHome => ("Back", "Назад"),
    NothingToReview => ("Nothing was said, so there is nothing to review.", "Ничего не было сказано, разбирать нечего."),

    // concept checks
    CardsTopic => ("Topic", "Тема"),
    CardsCount => ("Questions", "Вопросов"),
    CardsMake => ("Make questions", "Составить вопросы"),
    CardsMaking => ("Writing questions…", "Составляю вопросы…"),
    CardsLabel => ("CONCEPT CHECK", "ПРОВЕРКА ПОНИМАНИЯ"),
    CardsDontKnow => ("I don't know", "Не знаю"),
    CardsConfidence => ("CONFIDENCE", "УВЕРЕННОСТЬ"),
    CardsThinking => ("YOUR THINKING", "ВАШИ РАССУЖДЕНИЯ"),
    CardsThinkingHint => ("Why this answer? Whatever comes to mind.", "Почему этот ответ? Всё, что приходит в голову."),
    CardsSubmitHint => ("Enter sends · Shift+Enter new line", "Enter — отправить · Shift+Enter — новая строка"),
    CardsSubmit => ("Submit answer", "Ответить"),
    CardsNext => ("Next", "Дальше"),
    CardsFinish => ("Finish", "Закончить"),
    CardsCorrect => ("Correct", "Верно"),
    CardsWrong => ("Not quite", "Не совсем"),
    CardsOverconfident => ("You were sure and wrong. This one comes back soon.", "Вы были уверены и ошиблись. Этот вопрос скоро вернётся."),
    CardsUnderconfident => ("Right, but you doubted it.", "Верно, но вы сомневались."),
    CardsWellJudged => ("Your confidence matched the answer.", "Уверенность совпала с ответом."),
    CardsProgress => ("{n} of {total}", "{n} из {total}"),
    CardsScore => ("{right} of {total} right", "Верно {right} из {total}"),
    CardsBrier => ("Calibration {score} (0 is perfect, 0.25 is guessing)", "Калибровка {score} (0 — идеально, 0.25 — наугад)"),
    CardsSaved => ("{n} added to your review", "Добавлено в повторение: {n}"),
    Easy => ("Fundamentals", "Основы"),
    Medium => ("Interview", "Собеседование"),
    Hard => ("Hard", "Сложно"),

    // say it in English
    TranslateLabel => ("SAY IT IN RUSSIAN, OR WRITE IT", "СКАЖИТЕ ПО-РУССКИ ИЛИ НАПИШИТЕ"),
    TranslateHint => ("A thought, a sentence, an answer you want to give…", "Мысль, предложение, ответ, который хотите дать…"),
    TranslateGo => ("Say it in English", "Сказать по-английски"),
    TranslateBusy => ("Finding the words…", "Подбираю слова…"),
    Dictate => ("Dictate", "Продиктовать"),
    Natural => ("Natural", "Естественно"),
    Formal => ("Formal", "Официально"),
    Short => ("Short", "Коротко"),
    Notes => ("Notes", "Заметки"),

    ShowAnswer => ("Show the answer", "Показать ответ"),
    GradeAgain => ("Again", "Снова"),
    GradeHard => ("Hard", "Трудно"),
    GradeGood => ("Good", "Хорошо"),
    GradeEasy => ("Easy", "Легко"),

    // preparing for a vacancy
    PrepareTitle => ("Prepare for a vacancy", "Подготовка к вакансии"),
    PrepareIntro => (
        "Paste the vacancy. The interviewer will ask about it, and here you can see how your CV fits.",
        "Вставьте вакансию. Интервьюер будет спрашивать по ней, а здесь видно, как подходит ваше резюме."
    ),
    JobTitle => ("Job title", "Должность"),
    Company => ("Company", "Компания"),
    JobDescription => ("The vacancy text", "Текст вакансии"),
    CompareCv => ("Compare my CV", "Сравнить моё резюме"),
    MakeQuestions => ("Likely questions", "Вероятные вопросы"),
    Strengths => ("Strengths to lead with", "С чего начинать"),
    Gaps => ("Gaps", "Пробелы"),
    HowToAddress => ("How to answer", "Как отвечать"),
    CvFixes => ("CV fixes", "Правки в резюме"),
    LikelyQuestions => ("Questions to practise", "Вопросы для тренировки"),
    PractiseThis => ("Practise this", "Потренировать"),
    Working => ("Working…", "Работаю…"),
    VacancySaved => ("The vacancy is saved and the interviewer will use it.", "Вакансия сохранена, интервьюер будет её использовать."),

    // history and background work
    Sessions => ("SESSIONS", "СЕССИИ"),
    Search => ("Search…", "Поиск…"),
    NewSession => ("New session", "Новая сессия"),
    NoSessions => ("No sessions yet.", "Сессий пока нет."),
    Background => ("BACKGROUND", "В ФОНЕ"),
    Delete => ("Delete", "Удалить"),
    Open => ("Open", "Открыть"),
    StatusActive => ("In progress", "Идёт"),
    StatusDone => ("Done", "Готово"),

    // settings
    SettingsTitle => ("Settings", "Настройки"),
    SecInterface => ("Interface", "Интерфейс"),
    SecProfile => ("Your profile", "Ваш профиль"),
    SecModel => ("Language model", "Языковая модель"),
    SecRecognition => ("Speech recognition", "Распознавание речи"),
    SecVoice => ("Interviewer's voice", "Голос интервьюера"),
    SecAudio => ("Audio", "Звук"),
    UiLanguageLabel => ("Interface language", "Язык интерфейса"),
    Automatic => ("Automatic", "Авто"),
    AboutFile => ("ABOUT.md", "ABOUT.md"),
    Browse => ("Choose…", "Выбрать…"),
    CvFiles => ("CV files (PDF, Markdown or text)", "Файлы резюме (PDF, Markdown или текст)"),
    AddCv => ("Add a CV", "Добавить резюме"),
    Remove => ("Remove", "Убрать"),
    Address => ("Address", "Адрес"),
    ApiKey => ("Key", "Ключ"),
    KeyFromEnv => ("empty: taken from the environment", "пусто: берётся из окружения"),
    ModelName => ("Model", "Модель"),
    FastModel => ("Fast model for the interviewer (optional)", "Быстрая модель для интервьюера (необязательно)"),
    LoadModels => ("Load the list", "Загрузить список"),
    ModelsLoaded => ("{n} models", "Моделей: {n}"),
    Engine => ("Engine", "Движок"),
    EngineHttp => ("Cloud or local server", "Облако или локальный сервер"),
    EngineLocal => ("In this program (offline)", "В этой программе (офлайн)"),
    EngineOff => ("Off (the interviewer only writes)", "Выкл. (интервьюер только пишет)"),
    EngineOpenai => ("OpenAI-style speech address", "Адрес речи в стиле OpenAI"),
    EngineEleven => ("ElevenLabs", "ElevenLabs"),
    RecognitionLanguage => ("Language of your speech", "Язык вашей речи"),
    VoiceEnglish => ("English voice", "Английский голос"),
    VoiceRussian => ("Russian voice", "Русский голос"),
    VoiceTone => ("Tone (for models that take instructions)", "Тон (для моделей с инструкциями)"),
    VoiceSpeed => ("Speaking speed", "Скорость речи"),
    TestVoice => ("Test the voice", "Проверить голос"),
    Microphone => ("Microphone", "Микрофон"),
    Speakers => ("Speakers", "Колонки"),
    SystemDefault => ("System default", "По умолчанию в системе"),
    CloudWarning => (
        "Cloud engines receive what they speak or hear. Use a local address to keep it on this computer.",
        "Облачные движки получают то, что произносят или слышат. Локальный адрес оставляет всё на этом компьютере."
    ),
    SaveSettings => ("Save", "Сохранить"),
    Cancel => ("Cancel", "Отмена"),
    Saved => ("Saved", "Сохранено"),
    Close => ("Close", "Закрыть"),
    TestPhrase => ("Hello, I'm your interviewer. Let's begin.", "Hello, I'm your interviewer. Let's begin."),
    Dismiss => ("Dismiss", "Скрыть"),
}

/// The strings of one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tr {
    russian: bool,
}

impl Tr {
    pub fn new(language: UiLanguage) -> Self {
        Self { russian: language.resolve() == UiLanguage::Ru }
    }

    pub fn is_russian(self) -> bool {
        self.russian
    }

    pub fn get(self, key: K) -> &'static str {
        if self.russian { key.ru() } else { key.en() }
    }

    /// A string with `{name}` places filled in.
    pub fn fill(self, key: K, values: &[(&str, &str)]) -> String {
        let mut text = self.get(key).to_owned();
        for (name, value) in values {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(text: &str) -> Vec<String> {
        let mut found: Vec<String> =
            text.split('{').skip(1).filter_map(|rest| rest.split('}').next()).map(str::to_owned).collect();
        found.sort();
        found
    }

    #[test]
    fn nothing_is_left_untranslated_or_empty() {
        for key in K::ALL {
            assert!(!key.en().trim().is_empty() && !key.ru().trim().is_empty(), "{key:?}");
        }
    }

    #[test]
    fn both_languages_fill_the_same_places() {
        for key in K::ALL {
            assert_eq!(places(key.en()), places(key.ru()), "{key:?}");
        }
    }

    #[test]
    fn the_russian_text_is_really_russian_where_it_should_be() {
        let russian = K::ALL
            .iter()
            .filter(|k| k.ru().chars().any(|c| ('а'..='я').contains(&c.to_ascii_lowercase()) || c == 'ё'))
            .count();
        assert!(russian * 100 >= K::ALL.len() * 90, "{russian} of {} contain Russian letters", K::ALL.len());
    }

    #[test]
    fn filling_replaces_every_named_place() {
        let tr = Tr::new(UiLanguage::En);
        assert_eq!(tr.fill(K::StatusThinking, &[("name", "Daniel")]), "Daniel is thinking…");
        assert_eq!(tr.fill(K::CardsProgress, &[("n", "2"), ("total", "5")]), "2 of 5");
        let ru = Tr::new(UiLanguage::Ru);
        assert_eq!(ru.fill(K::Minutes, &[("n", "20")]), "20 мин");
    }

    #[test]
    fn the_language_follows_the_setting() {
        assert!(Tr::new(UiLanguage::Ru).is_russian());
        assert!(!Tr::new(UiLanguage::En).is_russian());
        assert_eq!(Tr::new(UiLanguage::En).get(K::TabInterview), "Interview");
        assert_eq!(Tr::new(UiLanguage::Ru).get(K::TabInterview), "Собеседование");
    }
}
