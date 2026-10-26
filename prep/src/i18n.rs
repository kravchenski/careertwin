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
    EngineLocalVoice => ("In this program (offline, English)", "В этой программе (офлайн, английский)"),
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
    SecOffline => ("Offline on this computer", "Автономно на этом компьютере"),
    OfflineHardware => ("{ram} GB of memory, {cores} cores{gpu}", "Памяти: {ram} ГБ, ядер: {cores}{gpu}"),
    OfflineGpu => (", NVIDIA GPU", ", видеокарта NVIDIA"),
    OfflineHearing => ("Hearing: {model} ({size} MB)", "Слух: {model} ({size} МБ)"),
    OfflineInterviewer => (
        "Interviewer: ollama pull {tag} (about {size} GB), then choose Ollama above",
        "Интервьюер: ollama pull {tag} (около {size} ГБ), затем выберите Ollama выше"
    ),
    OfflineWeak => (
        "This much memory is too little for an interviewer that follows up well: use a cloud model for it and keep hearing and voice offline.",
        "Для интервьюера, который хорошо уточняет, этой памяти мало: берите облачную модель, а слух и голос оставьте автономными."
    ),
    OfflineNotBuilt => (
        "This build has no offline engines. Install without LIGHT=1 to get them.",
        "В этой сборке нет автономных движков. Установите без LIGHT=1, чтобы они появились."
    ),
    OfflineModel => ("Hearing model", "Модель слуха"),
    OfflineVoiceName => ("Voice", "Голос"),
    OfflineVoiceNote => (
        "The offline voice speaks English only. Russian prompts need a voice server or the cloud.",
        "Автономный голос говорит только по-английски. Для русского нужен голосовой сервер или облако."
    ),
    Recommended => ("recommended", "рекомендуется"),
    UseModel => ("Use {tag}", "Взять {tag}"),
    Downloading => ("Downloading {name}… {percent}%", "Скачиваю {name}… {percent}%"),
    DownloadFailed => ("Could not download {name}: {why}", "Не удалось скачать {name}: {why}"),
    ModelsNotReady => ("The offline models are still downloading.", "Автономные модели ещё скачиваются."),
    SaveSettings => ("Save", "Сохранить"),
    Cancel => ("Cancel", "Отмена"),
    Saved => ("Saved", "Сохранено"),
    Close => ("Close", "Закрыть"),
    TestPhrase => ("Hello, I'm your interviewer. Let's begin.", "Hello, I'm your interviewer. Let's begin."),
    Dismiss => ("Dismiss", "Скрыть"),

    // the panel at the bottom of the window
    AccountMenu => ("Account menu", "Меню профиля"),
    AppName => ("CareerTwin Prep", "CareerTwin Prep"),
    MenuUsage => ("Usage", "Использование"),
    MenuLanguage => ("Language", "Язык"),
    MenuHelp => ("Help", "Помощь"),
    MenuQuit => ("Quit", "Выйти"),
    BarToday => ("Today {done} of {goal} min", "Сегодня {done} из {goal} мин"),
    BarStreak => ("{n}-day streak", "Серия: {n} дн."),

    // usage
    UsageTitle => ("Usage", "Использование"),
    UsageToday => ("Today", "Сегодня"),
    UsageWeek => ("Last 7 days", "Последние 7 дней"),
    UsageTotal => ("In total", "Всего"),
    UsageStreak => ("Day streak", "Дней подряд"),
    UsageSessions => ("Sessions", "Занятий"),
    UsageFocusBlocks => ("Focus blocks", "Блоков фокуса"),
    UsageWords => ("Words you said", "Сказано слов"),
    UsagePractised => ("Practised", "Занимались"),
    UsageGoal => ("Goal for the day: {n} min", "Цель на день: {n} мин"),
    UsageNothing => ("Nothing yet. The first minutes count too.", "Пока ничего. Первые минуты тоже считаются."),
    UnitHour => ("h", "ч"),
    UnitMinute => ("min", "мин"),
    UnitSecond => ("s", "с"),

    // help
    HelpTitle => ("How to use it", "Как пользоваться"),
    HelpShortcuts => ("Keys", "Клавиши"),
    HelpKeySettings => ("Settings", "Настройки"),
    HelpKeySkip => ("Skip what the tutor is saying", "Перебить репетитора"),
    HelpKeyEscape => ("Close a panel", "Закрыть панель"),
    HelpKeyTimer => ("Start or pause the focus timer", "Запустить или остановить таймер"),
    HelpTalk => (
        "Pick a tab and press Start. Talk like on a call; the tutor answers like a person. You can type instead.",
        "Выберите вкладку и нажмите «Начать». Говорите, как на созвоне: репетитор отвечает как человек. Можно писать текстом."
    ),
    HelpFocus => (
        "The timer at the top right is a Pomodoro: work for a block, then rest. Click it to start or pause.",
        "Таймер справа вверху — помодоро: блок работы, потом отдых. Нажмите на него, чтобы запустить или остановить."
    ),

    // the focus timer
    TimerLabel => ("Focus timer", "Таймер фокуса"),
    TimerStart => ("Start the timer", "Запустить таймер"),
    TimerPause => ("Pause the timer", "Остановить таймер"),
    TimerSkip => ("Skip to the next phase", "К следующему этапу"),
    TimerReset => ("Reset the timer", "Сбросить таймер"),
    PhaseFocus => ("Focus", "Фокус"),
    PhaseShortBreak => ("Break", "Перерыв"),
    PhaseLongBreak => ("Long break", "Долгий перерыв"),
    FocusDone => (
        "Focus block done. Rest for {n} minutes; the timer is running.",
        "Блок фокуса закончен. Отдохните {n} мин: таймер уже идёт."
    ),
    BreakDone => ("The break is over. Ready for the next block?", "Перерыв закончился. Начнём следующий блок?"),

    // settings: about you and the timer
    SecAccount => ("About you", "О вас"),
    YourName => ("Your name", "Ваше имя"),
    YourEmail => ("E-mail (shown in the panel below)", "Почта (видна в панели внизу)"),
    SecFocus => ("Focus timer and goal", "Таймер фокуса и цель"),
    FocusMinutes => ("Focus, min", "Фокус, мин"),
    ShortMinutes => ("Break, min", "Перерыв, мин"),
    LongMinutes => ("Long break, min", "Долгий перерыв, мин"),
    LongEvery => ("Long break after", "Долгий после"),
    GoalMinutes => ("Goal per day, min", "Цель на день, мин"),

    // the plan: the vacancy ranked by the 80/20 rule
    CardPlan => ("Your plan", "Ваш план"),
    CardPlanText => (
        "The topics that cover 80% of the questions for your vacancy, with every term in plain words.",
        "Темы, которые закрывают 80% вопросов по вашей вакансии, и каждый термин простыми словами."
    ),
    PlanTitle => ("Your plan", "Ваш план"),
    PlanIntro => (
        "The vacancy ranked by the 80/20 rule: a few topics hold most of the questions. Learn those first.",
        "Вакансия по правилу 80/20: несколько тем содержат большую часть вопросов. Учите их первыми."
    ),
    PlanNone => (
        "There is no plan yet. Paste the vacancy and I will work out what you are likely to be asked.",
        "Плана пока нет. Вставьте вакансию, и я выясню, о чём вас, скорее всего, спросят."
    ),
    PlanResearch => ("Research the vacancy", "Изучить вакансию"),
    PlanResearching => ("Researching… {done} of {total}", "Изучаю… {done} из {total}"),
    PlanAgain => ("Research again", "Изучить заново"),
    PlanStale => (
        "The vacancy has changed since this plan was made.",
        "Вакансия изменилась с тех пор, как был составлен план."
    ),
    PlanCoverage => ("{n}% of the likely questions covered", "Закрыто вероятных вопросов: {n}%"),
    PlanVitalLine => (
        "{vital} of {all} topics hold 80% of the questions. You know {known}% of those.",
        "{vital} из {all} тем дают 80% вопросов. Из них вы знаете {known}%."
    ),
    PlanVital => ("The 20% that matters", "Те самые 20%"),
    PlanOthers => ("Later", "Потом"),
    PlanLikely => ("likely {n}/10", "вероятность {n}/10"),
    PlanEffort => ("effort {n}/10", "усилия {n}/10"),
    PlanWriteTerms => ("Write the terms", "Составить термины"),
    PlanTermsCount => ("Terms: {n}", "Терминов: {n}"),
    PlanPlain => ("In plain words", "Простыми словами"),
    PlanAnalogy => ("Think of it like this", "Как это понять"),
    PlanSayIt => ("Say it in English", "Скажите по-английски"),
    PlanFollowUps => ("They may ask next", "Могут спросить дальше"),
    PlanStart => ("Start here", "Начните отсюда"),
    PlanChangeVacancy => ("Change the vacancy", "Изменить вакансию"),
    PlanPractise => ("Practise this topic", "Потренировать тему"),
    PlanLevels => (
        "In your words {native}/3 · in English {english}/3",
        "Своими словами {native}/3 · по-английски {english}/3"
    ),

    // notes
    NotesTitle => ("Notes", "Заметки"),
    NotesIntro => (
        "Short, one idea each. After a lesson the tutor keeps the 20% worth remembering; you can add your own.",
        "Коротко, по одной мысли. После занятия репетитор сохраняет те самые 20%, которые стоит помнить; свои можно добавить."
    ),
    NotesWrite => ("Write a note…", "Напишите заметку…"),
    NotesAdd => ("Add", "Добавить"),
    NotesEmpty => ("No notes yet. The first lesson fills this page.", "Заметок пока нет. Первое же занятие наполнит эту страницу."),
    NotesCopy => ("Copy as Markdown", "Скопировать как Markdown"),
    NotesCopied => ("Copied", "Скопировано"),
    NoteStar => ("Star this note", "Отметить заметку"),
    NoteUnstar => ("Remove the star", "Снять отметку"),
    NoteDelete => ("Delete this note", "Удалить заметку"),
    KindVital => ("Key", "Главное"),
    KindLecture => ("Lecture", "Лекция"),
    KindMistake => ("Slip", "Ошибка"),
    KindIdea => ("Idea", "Идея"),
    FilterAll => ("All", "Все"),

    // after a lesson
    DigestTitle => ("The 20% to keep", "Те самые 20%"),
    DigestNext => ("Next time", "В следующий раз"),
    DigestSaved => ("Saved to your notes", "Сохранено в заметки"),
    OpenNotes => ("Open notes", "Открыть заметки"),
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
