//! The words of the window: English and Russian here, Polish, Ukrainian, German, French and Spanish in `text_more`.

use super::text_more as more;

/// (key, English, Russian). A missing translation falls back to English.
const TABLE: &[(&str, &str, &str)] = &[
    ("get_answer", "Get answer", "Получить ответ"),
    ("manual_hint", "Type a question by hand…", "Введите вопрос вручную…"),
    ("pause_listening", "Pause listening", "Поставить слушание на паузу"),
    ("resume_listening", "Resume listening", "Продолжить слушание"),
    ("start_listening", "Start listening", "Начать слушать"),
    ("ask_hint", "Answer the last thing the interviewer said", "Ответ на последнее, что сказал интервьюер"),
    ("nothing_heard", "Nothing heard yet. Type a question below.", "Пока ничего не слышно. Введите вопрос ниже."),
    ("menu", "Menu", "Меню"),
    ("prev_answer", "Previous answer", "Предыдущий ответ"),
    ("next_answer", "Next answer", "Следующий ответ"),
    ("hide_taskbar", "Hide the window from the taskbar", "Скрыть окно с панели задач"),
    (
        "taskbar_note",
        "Bring the window forward, or quit, from the arrow in the notification area. It takes effect the next time the window starts.",
        "Вынести окно вперёд или выйти можно из стрелки в области уведомлений. Применится при следующем запуске окна.",
    ),
    ("tray_show", "Show the window", "Показать окно"),
    ("parallel", "Several accounts at once", "Несколько аккаунтов сразу"),
    ("parallel_off", "One at a time", "По одному"),
    ("parallel_hedged", "The next one when the first is slow", "Следующий, если первый медлит"),
    ("parallel_all", "All at once (quickest)", "Все сразу (быстрее всего)"),
    ("first_word", "Give up on a silent account after", "Бросать молчащий аккаунт через"),
    (
        "parallel_note",
        "When an account is slow, limited or down, the next one answers instead, and the first to answer wins. All at once is the quickest and spends a request on every account. 0 seconds never gives up early.",
        "Если аккаунт медлит, упёрся в лимит или недоступен, отвечает следующий, а побеждает тот, кто ответил первым. «Все сразу» быстрее всего и тратит запрос на каждом аккаунте. 0 секунд: не бросать раньше времени.",
    ),
    ("tray_icon", "Show an icon in the notification area", "Показывать значок в области уведомлений"),
    (
        "tray_note",
        "A blue arrow like the one for location in use: an outline while the window waits, solid while it listens or writes an answer. It appears the next time the window starts.",
        "Синяя стрелка, как у «геолокация используется»: контур, пока окно ждёт, и заливка, пока оно слушает или пишет ответ. Появится при следующем запуске окна.",
    ),
    ("tray_listening", "Listening", "Слушаю"),
    ("tray_paused", "Paused", "Пауза"),
    ("tray_quit", "Quit", "Выйти"),
    ("ready", "Ready", "Готово"),
    ("you_said", "You:", "Вы:"),
    ("screenshot_key", "Screenshot key", "Клавиша скриншота"),
    (
        "screenshot_key_now",
        "The screenshot key works in any program: {key}",
        "Клавиша скриншота работает в любой программе: {key}",
    ),
    (
        "screenshot_key_restart",
        "{key} will be the screenshot key the next time the window starts",
        "{key} станет клавишей скриншота при следующем запуске окна",
    ),
    (
        "screenshot_key_bad",
        "Use a modifier and a key, like Ctrl+Shift+Enter",
        "Нужны модификатор и клавиша, например Ctrl+Shift+Enter",
    ),
    ("hide_from_sharing", "Hide this window from screen sharing", "Скрывать окно при показе экрана"),
    (
        "hide_note",
        "Zoom, Teams, Meet, OBS and screenshots do not see the window; you still do. Older Windows shows a black box instead. Check the rules of each employer: many forbid help during an interview.",
        "Zoom, Teams, Meet, OBS и скриншоты не видят окно, вы его видите. На старых Windows вместо него чёрный прямоугольник. Проверьте правила работодателя: многие запрещают помощь на собеседовании.",
    ),
    ("heard", "Heard", "Слышу"),
    ("listening_to", "Listening to {name}", "Слушаю {name}"),
    ("hearing", "Listening", "Слух"),
    ("listen_call", "Listen to the call", "Слушать звонок"),
    ("call_source", "Call sound", "Звук звонка"),
    ("auto", "Automatic", "Автоматически"),
    ("default_mic", "Default microphone", "Микрофон по умолчанию"),
    ("use_mic", "Also listen to my microphone", "Слушать и мой микрофон"),
    ("mic_source", "Microphone", "Микрофон"),
    ("refresh", "Refresh", "Обновить"),
    (
        "hearing_note",
        "Automatic follows the output that plays the call, so a headset plugged in later is heard too.",
        "Автоматически идёт за выходом, который играет звонок, поэтому наушники, подключённые позже, тоже слышны.",
    ),
    (
        "project_note",
        "Choose the folder above, then read it: the answers will know the project.",
        "Укажите папку выше и прочитайте её: ответы будут знать проект.",
    ),
    ("project_read", "Read the project", "Прочитать проект"),
    ("project_reread", "Read it again", "Прочитать заново"),
    ("project_reading", "Reading the files…", "Читаю файлы…"),
    ("project_asking", "Writing the dossier from {n} files…", "Пишу досье по {n} файлам…"),
    (
        "project_not_read",
        "Not read yet. It takes about a minute and one request.",
        "Ещё не прочитан. Это около минуты и один запрос.",
    ),
    (
        "project_summary",
        "{name}: {files} files, {size}k characters, read {when}",
        "{name}: {files} файлов, {size}k символов, прочитан {when}",
    ),
    ("project_ready", "{name} is read: the answers know it now", "{name} прочитан: ответы теперь его знают"),
    ("dossier", "Dossier (you can correct it)", "Досье (можно поправить)"),
    ("dossier_save", "Save changes", "Сохранить правки"),
    ("project_forget", "Forget the project", "Забыть проект"),
    (
        "dossier_note",
        "Questions about the project get this dossier and the few pieces of code that match the question.",
        "На вопросы о проекте уходит это досье и несколько фрагментов кода, подходящих к вопросу.",
    ),
    ("thinking", "Thinking…", "Думаю…"),
    ("looking", "Looking at the screen…", "Смотрю на экран…"),
    ("ask", "Ask", "Спросить"),
    ("stop", "Stop", "Стоп"),
    ("screenshot", "Screenshot", "Скриншот"),
    ("settings", "Settings", "Настройки"),
    ("close", "Close", "Закрыть"),
    ("copy", "Copy", "Копировать"),
    ("details", "Details", "Подробнее"),
    ("appearance", "Appearance", "Оформление"),
    ("theme", "Theme", "Тема"),
    ("theme_midnight", "Midnight", "Полночь"),
    ("theme_graphite", "Graphite", "Графит"),
    ("theme_forest", "Forest", "Лес"),
    ("theme_amber", "Amber", "Янтарь"),
    ("theme_rose", "Rose", "Роза"),
    ("theme_ocean", "Ocean", "Океан"),
    ("theme_paper", "Paper", "Бумага"),
    ("theme_custom", "Custom", "Своя"),
    ("theme_background", "Background", "Фон"),
    ("theme_text", "Text", "Текст"),
    ("theme_accent", "Accent", "Акцент"),
    ("equalizer", "Equalizer", "Эквалайзер"),
    ("eq_both", "Both", "И то и другое"),
    ("eq_dots", "Dots at the top", "Точки сверху"),
    ("eq_background", "Background", "Фоном"),
    ("eq_off", "Off", "Выкл"),
    ("animate", "Animate", "Анимация"),
    (
        "eq_note",
        "Follows the call audio, breathes when it is quiet and runs a wave while an answer is written.",
        "Следует за звуком звонка, дышит в тишине и бежит волной, пока пишется ответ.",
    ),
    ("icon_theme", "Theme colour", "Цвет темы"),
    ("hide_details", "Hide details", "Скрыть подробности"),
    ("empty_title", "Nothing asked yet", "Пока нет вопросов"),
    (
        "empty_body",
        "Type a question above, or press Screenshot to answer what is on the screen.",
        "Введите вопрос выше или нажмите «Скриншот», чтобы ответить на то, что на экране.",
    ),
    (
        "no_context",
        "No facts about you yet. Put your ABOUT.md next to the data folder, or open Settings → Interview.",
        "Пока нет фактов о вас. Положите ABOUT.md рядом с папкой данных или откройте Настройки → Собеседование.",
    ),
    ("answered_by", "Answered by {model}", "Ответила модель {model}"),
    ("key_terms", "Key points", "Главное"),
    ("context", "Where it is used", "Где применяется"),
    ("next_questions", "Likely next", "Дальше спросят"),
    // tabs
    ("tab_interview", "Interview", "Собеседование"),
    ("tab_practice", "Practice", "Подготовка"),
    ("tab_ai", "AI", "ИИ"),
    ("tab_general", "General", "Общие"),
    // interview
    ("interview", "Interview", "Собеседование"),
    ("new", "New", "Новое"),
    ("rename", "Rename", "Переименовать"),
    ("delete", "Delete", "Удалить"),
    (
        "interview_note",
        "Each interview keeps its own vacancy, CV and notes. ABOUT.md is shared.",
        "У каждого собеседования своя вакансия, CV и заметки. ABOUT.md общий.",
    ),
    ("vacancy", "Vacancy", "Вакансия"),
    ("vacancy_title", "Title", "Должность"),
    ("vacancy_company", "Company", "Компания"),
    ("vacancy_text", "Paste the text of the vacancy", "Вставьте текст вакансии"),
    ("notes", "Notes for this interview", "Заметки к этому собеседованию"),
    (
        "notes_hint",
        "Facts the answers should know: availability, studies, what you are learning",
        "Что нужно знать ответам: когда можете начать, учёба, что изучаете",
    ),
    ("documents", "CV and documents", "CV и документы"),
    ("add_file", "Add", "Добавить"),
    ("file_path", "Path to a PDF, Markdown or text file", "Путь к PDF, Markdown или текстовому файлу"),
    ("file_added", "Added {name}", "Добавлен {name}"),
    ("about_file", "Facts about you (ABOUT.md)", "Факты о вас (ABOUT.md)"),
    ("project_folder", "Project to defend (folder)", "Проект для защиты (папка)"),
    ("saved", "Saved", "Сохранено"),
    // practice
    ("practice_title", "Get ready for the technical interview", "Подготовка к техническому собеседованию"),
    (
        "practice_intro",
        "Work down the list. Each step makes the answers in the call more exact.",
        "Идите по списку. Каждый шаг делает ответы на звонке точнее.",
    ),
    ("step_vacancy", "The vacancy", "Вакансия"),
    ("step_cv", "The CV you sent", "CV, которое вы отправили"),
    ("step_about", "Facts about you", "Факты о вас"),
    ("step_project", "A project to defend", "Проект для защиты"),
    ("step_bank", "Likely questions", "Вероятные вопросы"),
    ("step_review", "CV against the vacancy", "CV против вакансии"),
    ("optional", "optional", "по желанию"),
    ("missing_vacancy", "Add it in Interview", "Добавьте во вкладке «Собеседование»"),
    ("missing_cv", "Add the CV in Interview", "Добавьте CV во вкладке «Собеседование»"),
    ("missing_about", "ABOUT.md was not found", "ABOUT.md не найден"),
    ("missing_project", "Choose a folder in Interview", "Выберите папку во вкладке «Собеседование»"),
    ("how_many", "How many", "Сколько"),
    ("focus", "Focus (optional)", "Упор (по желанию)"),
    ("focus_hint", "Python, SQL, system design, behavioral…", "Python, SQL, проектирование, поведенческие…"),
    ("build_bank", "Build the questions", "Составить вопросы"),
    ("review_cv", "Review my CV", "Разобрать моё CV"),
    ("working", "Working…", "Работаю…"),
    ("nothing_yet", "Nothing yet.", "Пока ничего."),
    ("say", "What to say", "Что сказать"),
    ("strengths", "Lead with", "С чего начать"),
    ("gaps", "Gaps and how to answer", "Пробелы и как отвечать"),
    ("likely", "Questions the gaps invite", "Вопросы, которые вызовут пробелы"),
    ("fixes", "Changes to the CV", "Правки в CV"),
    ("past_questions", "Questions from your interviews", "Вопросы с ваших собеседований"),
    (
        "past_questions_body",
        "Every question the window heard or was given, with its kind, language and the answer shown. Take them to a model or a spreadsheet to see what is asked and what you missed.",
        "Каждый вопрос, который услышало окно или который ввели, с типом, языком и показанным ответом. Отдайте их модели или в таблицу, чтобы увидеть, о чём спрашивают и что вы упустили.",
    ),
    ("export_questions", "Export the questions", "Выгрузить вопросы"),
    ("export_done", "Saved {n} files in {folder}", "Сохранено файлов: {n}, папка {folder}"),
    ("export_none", "No saved interviews found in {folder}", "Сохранённых собеседований нет в {folder}"),
    ("open_folder", "Open the folder", "Открыть папку"),
    ("questions_found", "{n} questions in {m} interviews", "Вопросов: {n}, собеседований: {m}"),
    // ai
    ("provider", "Provider", "Провайдер"),
    ("base_url", "Base URL", "Базовый URL"),
    ("accounts", "Accounts", "Аккаунты"),
    ("paste_key", "Paste the API key", "Вставьте API-ключ"),
    ("save_key", "Save key", "Сохранить ключ"),
    ("use", "Use", "Использовать"),
    ("in_use", "In use", "Используется"),
    ("remove", "Remove", "Убрать"),
    ("no_keys", "No key saved for this address yet.", "Для этого адреса ключей пока нет."),
    (
        "keys_note",
        "Keys stay in a file only you can read. When the key in use is rate limited or rejected, the next one for the same address is tried before another model.",
        "Ключи хранятся в файле, который читаете только вы. Если используемый ключ упёрся в лимит или отклонён, пробуется следующий для того же адреса, и только потом другая модель.",
    ),
    ("models_title", "Models", "Модели"),
    ("model_ask", "Answers (Ask)", "Ответы (Спросить)"),
    ("model_screenshot", "Screenshots", "Скриншоты"),
    ("model_audio", "Audio: speech recognition", "Аудио: распознавание речи"),
    (
        "model_ask_note",
        "Writes what to say. Empty or auto lets the gateway choose.",
        "Пишет, что сказать. Пусто или auto: выберет шлюз.",
    ),
    (
        "model_shot_note",
        "Must understand images. Empty uses the answers model.",
        "Должна понимать картинки. Пусто: модель для ответов.",
    ),
    ("fallbacks", "If it is down or limited, try", "Если недоступна или в лимите, пробовать"),
    ("fallbacks_hint", "model, model, auto", "модель, модель, auto"),
    ("load_models", "Load models", "Загрузить список"),
    ("test_connection", "Test connection", "Проверить соединение"),
    ("connected", "Connected: {n} models", "Подключено, моделей: {n}"),
    ("engine", "Engine", "Движок"),
    ("engine_whisper", "Whisper on this computer", "Whisper на этом компьютере"),
    ("engine_deepgram", "Deepgram (cloud)", "Deepgram (облако)"),
    ("speech_model", "Speech model", "Модель речи"),
    ("deepgram_key", "Deepgram API key", "API-ключ Deepgram"),
    (
        "audio_note",
        "The call is heard through this engine. Whisper downloads its model once, the first time; the progress shows under the header.",
        "Звонок слушается через этот движок. Whisper скачивает модель один раз, при первом запуске; ход загрузки виден под заголовком.",
    ),
    (
        "cloud_note",
        "{name} gets your CV, your ABOUT text and each question with every request.",
        "{name} получает ваше CV, текст ABOUT и каждый вопрос с каждым запросом.",
    ),
    // general
    ("language", "Languages", "Языки"),
    ("interview_language", "Interview language", "Язык собеседования"),
    ("window_language", "Window language", "Язык окна"),
    ("window", "Window", "Окно"),
    ("keep_on_top", "Keep the window on top", "Держать окно поверх других"),
    ("frameless", "Frameless window", "Окно без рамки"),
    ("opacity", "Opacity", "Прозрачность"),
    ("text_size", "Text size", "Размер текста"),
    ("answer_style", "Answers", "Ответы"),
    ("style", "Style", "Стиль"),
    ("layout", "Layout", "Вид"),
    ("layout_main", "Main answer only", "Только главный ответ"),
    ("layout_lines", "Lines to say, then details", "Реплики, затем подробности"),
    ("style_concise", "Concise", "Коротко"),
    ("style_standard", "Standard", "Обычно"),
    ("style_detailed", "Detailed", "Подробно"),
    ("screenshot_section", "Screenshot", "Скриншот"),
    ("screenshot_enabled", "Enable the Screenshot button", "Включить кнопку «Скриншот»"),
    ("screenshot_tool", "Capture tool", "Чем снимать"),
    (
        "screenshot_note",
        "The window turns transparent while the screen is captured. The image goes to the screenshot model through the gateway.",
        "Пока снимается экран, окно становится прозрачным. Картинка уходит модели для скриншотов через шлюз.",
    ),
    ("app_icon", "App icon", "Иконка приложения"),
    ("icon_default", "Default", "Стандартная"),
    ("icon_custom", "Your own image (PNG or JPEG)", "Своя картинка (PNG или JPEG)"),
    ("icon_path", "Path to the image", "Путь к картинке"),
    ("icon_apply", "Use this image", "Использовать картинку"),
    ("icon_bad", "Could not use that image: {why}", "Не удалось использовать картинку: {why}"),
    (
        "icon_note",
        "Some desktops (Wayland) take the icon from the desktop file instead of the window.",
        "Некоторые окружения (Wayland) берут иконку из файла .desktop, а не из окна.",
    ),
    ("updates", "Updates", "Обновления"),
    ("check_updates", "Look for a new version once a day", "Раз в день искать новую версию"),
    ("check_now", "Check now", "Проверить сейчас"),
    ("up_to_date", "You have the latest version ({version}).", "У вас последняя версия ({version})."),
    ("update_failed", "Could not check: {why}", "Не удалось проверить: {why}"),
    (
        "update_note",
        "Asks GitHub for the latest release. Nothing else is sent.",
        "Спрашивает GitHub о последнем релизе. Больше ничего не отправляется.",
    ),
    ("data", "Data", "Данные"),
    ("save_transcripts", "Save transcripts of interviews", "Сохранять записи собеседований"),
    ("data_folder", "Data folder: {path}", "Папка данных: {path}"),
    ("reset", "Reset all settings", "Сбросить все настройки"),
    // update window
    ("update_title", "Version {version} is available", "Доступна версия {version}"),
    ("update_have", "You have {version}.", "У вас {version}."),
    ("update_download", "Download the installer", "Скачать установщик"),
    ("update_page", "Open the release page", "Открыть страницу релиза"),
    ("update_later", "Later", "Позже"),
    ("update_skip", "Skip this version", "Пропустить эту версию"),
    ("update_downloading", "Downloading… {done} of {total}", "Скачиваю… {done} из {total}"),
    (
        "update_verified",
        "Downloaded and checked. Saved as {path}. It is not started: run it when you are ready.",
        "Скачано и проверено. Сохранено как {path}. Установщик не запущен: запустите, когда будете готовы.",
    ),
    (
        "update_linux",
        "This system updates from the repository: git pull, or uv tool upgrade.",
        "В этой системе обновляются из репозитория: git pull или uv tool upgrade.",
    ),
    ("update_error", "Download failed: {why}", "Скачивание не удалось: {why}"),
    ("saved_as", "Key saved as {name}", "Ключ сохранён как {name}"),
    (
        "saved_unchecked",
        "Key saved as {name}. The provider could not be asked to check it.",
        "Ключ сохранён как {name}. Провайдер не смог его проверить.",
    ),
    ("bad_value", "Not saved: {error}", "Не сохранено: {error}"),
];

/// The text for `key` in `language` (`ru`, else English), with `{name}` holes filled from `values`.
pub fn t(language: &str, key: &str, values: &[(&str, &str)]) -> String {
    let text = match TABLE.iter().find(|(name, _, _)| *name == key) {
        None => key,
        Some((_, en, ru)) => match language {
            "ru" => ru,
            other => other_language(other, key).unwrap_or(en),
        },
    };
    values.iter().fold(text.to_owned(), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

fn other_language(language: &str, key: &str) -> Option<&'static str> {
    let table = match language {
        "pl" => more::PL,
        "uk" => more::UK,
        "de" => more::DE,
        "fr" => more::FR,
        "es" => more::ES,
        _ => return None,
    };
    table.iter().find(|(name, _)| *name == key).map(|(_, text)| *text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_both_languages_and_the_same_holes() {
        for (key, en, ru) in TABLE {
            assert!(!en.is_empty() && !ru.is_empty(), "{key}");
            let holes = |text: &str| {
                let mut found: Vec<String> =
                    text.split('{').skip(1).filter_map(|rest| rest.split('}').next()).map(str::to_owned).collect();
                found.sort();
                found
            };
            assert_eq!(holes(en), holes(ru), "{key}: the holes differ");
        }
        let mut keys: Vec<_> = TABLE.iter().map(|(key, _, _)| *key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "a key is there twice");
    }

    #[test]
    fn every_other_language_has_every_key_with_the_same_holes() {
        let holes = |text: &str| {
            let mut found: Vec<String> =
                text.split('{').skip(1).filter_map(|rest| rest.split('}').next()).map(str::to_owned).collect();
            found.sort();
            found
        };
        for (language, table) in
            [("pl", more::PL), ("uk", more::UK), ("de", more::DE), ("fr", more::FR), ("es", more::ES)]
        {
            assert_eq!(table.len(), TABLE.len(), "{language} has a different number of keys");
            for (key, en, _) in TABLE {
                let text =
                    table.iter().find(|(name, _)| name == key).unwrap_or_else(|| panic!("{language} lacks {key}")).1;
                assert!(!text.is_empty(), "{language} {key}");
                assert_eq!(holes(en), holes(text), "{language} {key}: the holes differ");
            }
            for code in crate::languages::UI_LANGUAGES.iter().map(|(code, _)| *code) {
                assert!(code == "en" || code == "ru" || matches!(code, "pl" | "uk" | "de" | "fr" | "es"));
            }
        }
    }

    #[test]
    fn the_language_picks_the_text_and_the_holes_are_filled() {
        assert_eq!(t("en", "ask", &[]), "Ask");
        assert_eq!(t("ru", "ask", &[]), "Спросить");
        assert_eq!(t("de", "ask", &[]), "Fragen");
        assert_eq!(t("pl", "ask", &[]), "Zapytaj");
        assert_eq!(t("xx", "ask", &[]), "Ask"); // a language the window does not know: English
        assert_eq!(t("en", "update_title", &[("version", "0.6.0")]), "Version 0.6.0 is available");
        assert_eq!(t("en", "no such key", &[]), "no such key");
    }
}
