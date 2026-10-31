//! The window drawn headless: what a screen reader sees is checked, and pictures are written (when `UI_SHOTS`
//! names a folder) so the look can be judged.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use careertwin_overlay::coach::{CoachError, Reply, Request};
use careertwin_overlay::paths::Paths;
use careertwin_overlay::questions::Kind;
use careertwin_overlay::session::{Event as Heard, Model, State};
use careertwin_overlay::ui::settings_ui::Tab;
use careertwin_overlay::ui::{App, Entry, Event, Source, theme};
use careertwin_overlay::update::{Asset, Available, Release};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use image::ImageEncoder;
use tokio_util::sync::CancellationToken;

const DEMO: &str = include_str!("../../prep/tests/fixtures/ABOUT.demo.md");

struct Fixture {
    app: App,
    _dir: tempfile::TempDir,
    _runtime: tokio::runtime::Runtime,
}

fn fixture(language: &str) -> Fixture {
    fixture_model(language, None)
}

/// A model that answers at once (or when stopped, for a slow one) and remembers what it was asked.
struct Answers {
    asked: Mutex<Vec<String>>,
    slow: bool,
}

impl Answers {
    fn new(slow: bool) -> Arc<Self> {
        Arc::new(Self { asked: Mutex::new(Vec::new()), slow })
    }

    fn count(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

impl Model for Answers {
    fn ask(
        &self,
        request: &Request,
        cancel: &CancellationToken,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<Reply, CoachError> {
        self.asked.lock().unwrap().push(request.prompt.clone());
        on_chunk("ANSWER:\nfrom the model");
        if self.slow {
            for _ in 0..500 {
                if cancel.is_cancelled() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(Reply { text: "ANSWER:\nfrom the model".into(), finish: Some("stop".into()), fell_back: None })
    }

    fn supports_listening(&self) -> bool {
        false
    }
}

fn fixture_model(language: &str, model: Option<Arc<dyn Model>>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("interview")).unwrap();
    std::fs::write(dir.path().join("ABOUT.md"), DEMO).unwrap();
    std::fs::write(data.join("interview").join("settings.json"), format!(r#"{{"ui_language": "{language}", "gateway_url": "https://api.groq.com/openai", "model": "groq/openai/gpt-oss-120b", "vision_model": "groq/qwen/qwen3.8-27b", "accounts": [{{"name": "default", "url": "https://api.groq.com/openai", "key": "gsk_demoDEMOdemo0001aaaa"}}, {{"name": "default-2", "url": "https://api.groq.com/openai", "key": "gsk_demoDEMOdemo0002bbbb"}}], "gateway_key": "gsk_demoDEMOdemo0001aaaa"}}"#)).unwrap();
    std::fs::write(
        data.join("interview").join("context.json"),
        r#"{"active": "v1", "interviews": [{"id": "v1", "name": "Vention: Data Engineering Internship (Gdańsk)", "documents": [], "notes": "I study in the evenings", "vacancy": {"type": "text", "title": "Data Engineering Intern", "company": "Vention", "text": "Python, SQL and Airflow pipelines."}, "project": ""}]}"#,
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let mut app = App::with_model(Paths::under(data), runtime.handle().clone(), Arc::new(|| {}), model);
    // the AI tab would ask Groq for its models with the demo keys: a test turns it on when it has a server of its own
    app.settings_ui.auto_models = false;
    Fixture { app, _dir: dir, _runtime: runtime }
}

fn harness(
    fixture: Fixture,
    width: f32,
    height: f32,
) -> (Harness<'static, App>, tempfile::TempDir, tokio::runtime::Runtime) {
    let Fixture { app, _dir, _runtime } = fixture;
    let scale = app.settings.font_scale as f32 / 100.0;
    let harness = Harness::builder()
        .with_size(egui::vec2(width, height))
        .with_context_setup(move |ctx| theme::install(ctx, scale))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (harness, _dir, _runtime)
}

/// A label that is on the screen at least once: an input and the text next to it carry the same name.
fn see(harness: &Harness<'_, App>, label: &str) {
    assert!(harness.query_all_by_label(label).next().is_some(), "{label} is not on the screen");
}

fn shot(harness: &mut Harness<'_, App>, name: &str) {
    if let Ok(dir) = std::env::var("UI_SHOTS") {
        harness.run();
        let image = harness.render().expect("the window renders headless");
        image.save(format!("{dir}/{name}.png")).unwrap();
    }
}

fn answered(question: &str, raw: &str) -> Entry {
    Entry {
        id: 1,
        question: question.into(),
        kind: Kind::Definition,
        source: Source::Typed,
        raw: raw.into(),
        done: true,
        error: None,
        fell_back: Some("ollama-cloud/gpt-oss:120b".into()),
        first_ms: Some(900),
        started: Instant::now(),
    }
}

const REPLY: &str = "POINTS:\n- A **LEFT JOIN** keeps every row of the left table\n- Where there is no match the right columns are **NULL**\n- I use it to keep rows with `no activity` in reports\nANSWER:\nA left join returns all rows from the left table and the matching rows from the right one; where nothing matches, the right side is NULL.\n```sql\nselect u.id, o.total\nfrom users u\nleft join orders o on o.user_id = u.id;\n```\nKEY:\n- **INNER JOIN**: only rows that match on both sides\nCONTEXT:\nUsed to keep records that have no activity in reports.\nNEXT:\n- What is the difference with a full join? - a full join also keeps unmatched right rows.";

#[test]
fn the_window_starts_empty_and_says_what_to_do() {
    let (mut harness, _dir, _runtime) = harness(fixture("en"), 900.0, 420.0);
    harness.run();
    harness.get_by_label("Nothing asked yet");
    harness.get_by_label("Ask");
    harness.get_by_label("Screenshot");
    harness.get_by_label("Menu");
    harness.get_by_label("Get answer");
    shot(&mut harness, "overlay-empty");
}

#[test]
fn an_answer_shows_the_lines_to_say_and_opens_its_details() {
    let mut fixture = fixture("en");
    fixture.app.entries.push(answered("What is a left join?", REPLY));
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
    harness.run();
    harness.get_by_label_contains("keeps every row");
    harness.get_by_label_contains("Details").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Where it is used");
    harness.get_by_label_contains("Answered by ollama-cloud/gpt-oss:120b");
    shot(&mut harness, "overlay-answer");
}

const MARKDOWN_REPLY: &str = "POINTS:\n- A **left join** keeps every row of the *left* table\n- See [the docs](https://example.com/join) for `NULL` rules\nANSWER:\n## Joins in short\n\nA left join returns all rows of the left table.\n\n1. Pick the **driving** table\n2. Join the other one\n   - on a key\n   - never on a float\n\n> NULL is not equal to NULL.\n\n| join | keeps |\n|------|-------|\n| inner | matches |\n| left | all left rows |\n\n- [x] indexed\n- [ ] explained\n\n---\n\n```sql\nselect u.id from users u left join orders o on o.user_id = u.id;\n```\nKEY:\n- **INNER JOIN**: only matches\nCONTEXT:\nUsed in reports.\nNEXT:\n- Full join? - keeps both sides.";

#[test]
fn an_answer_in_markdown_is_drawn_as_headings_lists_quotes_tables_links_and_code() {
    let mut fixture = fixture("en");
    fixture.app.details_open = true;
    fixture.app.entries.push(answered("Explain a left join", MARKDOWN_REPLY));
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 900.0);
    harness.run();
    for text in [
        "Joins in short",
        "Pick the",
        "never on a float",
        "NULL is not equal to NULL.",
        "all left rows",
        "indexed",
        "the docs",
    ] {
        assert!(harness.query_all_by_label_contains(text).next().is_some(), "{text} is not drawn");
    }
    // none of the marks is left in the text
    for mark in ["## ", "**", "```", "|---", "> NULL"] {
        assert!(harness.query_all_by_label_contains(mark).next().is_none(), "{mark} shows as text");
    }
    shot(&mut harness, "overlay-markdown");
}

#[test]
fn the_main_answer_layout_shows_only_the_paragraph() {
    let mut fixture = fixture("en");
    fixture.app.settings.answer_layout = "main".into();
    fixture
        .app
        .entries
        .push(answered("What is a left join?", "ANSWER:\nA left join keeps all rows of the left table."));
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 320.0);
    harness.run();
    harness.get_by_label_contains("A left join keeps all rows");
    assert!(harness.query_by_label_contains("Details").is_none());
    shot(&mut harness, "overlay-main-layout");
}

#[test]
fn the_settings_have_four_tabs_and_each_draws() {
    for (tab, key, shot_name) in [
        (Tab::Interview, "Interview", "tab-interview"),
        (Tab::Practice, "Practice", "tab-practice"),
        (Tab::Ai, "AI", "tab-ai"),
        (Tab::General, "General", "tab-general"),
    ] {
        let mut fixture = fixture("en");
        fixture.app.settings_open = true;
        fixture.app.settings_ui.tab = tab;
        let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 760.0);
        harness.run();
        see(&harness, key);
        shot(&mut harness, shot_name);
    }
}

#[test]
fn the_ai_tab_has_the_three_models_and_the_keys_of_the_address() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    harness.run();
    for label in [
        "Answers (Ask)",
        "Screenshots",
        "Audio: speech recognition",
        "Accounts",
        "default",
        "default-2",
        "In use",
        "Save key",
        "Base URL",
    ] {
        see(&harness, label);
    }
    shot(&mut harness, "tab-ai-full");
}

#[test]
fn nvidia_and_tokenharbor_are_in_the_provider_menu_and_choosing_one_sets_its_address() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    harness.run();
    harness.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Provider").click();
    harness.run_steps(3);
    harness.get_by_label("TokenHarbor");
    shot(&mut harness, "provider-menu");
    harness.get_by_label("NVIDIA").click();
    harness.run_steps(3);
    let app = harness.state();
    assert_eq!(app.settings.gateway(), "https://integrate.api.nvidia.com");
    assert!(app.settings.model.is_empty(), "a model id belongs to one provider: it is cleared");
    harness.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Provider");
    harness.get_by_label("NVIDIA gets your CV, your ABOUT text and each question with every request.");
}

/// A server that lists `ids` as its models, answering however `status` says, and counts what it was asked.
fn models_server(runtime: &tokio::runtime::Runtime, status: u16, ids: &[&str]) -> wiremock::MockServer {
    runtime.block_on(async {
        let server = wiremock::MockServer::start().await;
        let data: Vec<_> = ids.iter().map(|id| serde_json::json!({ "id": id })).collect();
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/v1/models"))
            .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(serde_json::json!({ "data": data })))
            .mount(&server)
            .await;
        server
    })
}

fn asked(runtime: &tokio::runtime::Runtime, server: &wiremock::MockServer) -> usize {
    runtime.block_on(server.received_requests()).unwrap().len()
}

/// Frames until `done` holds, or fails after a few seconds: an answer comes from another thread.
fn until(harness: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..300 {
        harness.run_steps(2);
        if done(harness.state()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("{what} did not happen");
}

fn ai_tab_asking_for_models() -> Fixture {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    fixture.app.settings_ui.auto_models = true;
    fixture
}

#[test]
fn the_list_of_models_loads_by_itself_and_follows_the_provider() {
    let probe = tokio::runtime::Runtime::new().unwrap();
    let first = models_server(&probe, 200, &["alpha/one", "alpha/two"]);
    let second = models_server(&probe, 200, &["beta/only"]);
    let mut fixture = ai_tab_asking_for_models();
    fixture.app.settings.set_gateway(&first.uri()).unwrap();
    fixture.app.apply_coach();
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    until(&mut harness, "the first list", |app| app.settings_ui.models.len() == 2);
    harness.get_by_label("Answers (Ask): 2");
    // other frames do not ask again
    harness.run_steps(20);
    assert_eq!(asked(&probe, &first), 1);
    // another address: the old list is gone and the new one comes
    harness.state_mut().settings.set_gateway(&second.uri()).unwrap();
    harness.state_mut().apply_coach();
    until(&mut harness, "the second list", |app| app.settings_ui.models == ["beta/only"]);
    harness.get_by_label("Answers (Ask): 1");
    assert_eq!((asked(&probe, &first), asked(&probe, &second)), (1, 1));
}

#[test]
fn a_cloud_provider_without_a_key_is_not_asked_and_the_old_list_is_dropped() {
    let probe = tokio::runtime::Runtime::new().unwrap();
    let server = models_server(&probe, 200, &["alpha/one"]);
    let mut fixture = ai_tab_asking_for_models();
    fixture.app.settings.set_gateway(&server.uri()).unwrap();
    fixture.app.apply_coach();
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    until(&mut harness, "the list", |app| app.settings_ui.models.len() == 1);
    harness.state_mut().settings.set_gateway("https://integrate.api.nvidia.com").unwrap();
    harness.state_mut().apply_coach();
    harness.run_steps(20);
    let state = &harness.state().settings_ui;
    assert!(state.models.is_empty(), "a list from another provider is not offered");
    assert!(state.models_error.is_none() && !state.models_loading && state.models_for.is_none());
    // the key is saved: the list comes
    harness.state_mut().settings.set_gateway(&server.uri()).unwrap();
    harness.state_mut().apply_coach();
    until(&mut harness, "the list again", |app| app.settings_ui.models.len() == 1);
}

#[test]
fn a_list_that_failed_is_not_asked_for_again_until_the_button_is() {
    let probe = tokio::runtime::Runtime::new().unwrap();
    let broken = models_server(&probe, 500, &[]);
    let mut fixture = ai_tab_asking_for_models();
    fixture.app.settings.set_gateway(&broken.uri()).unwrap();
    fixture.app.apply_coach();
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    until(&mut harness, "the error", |app| app.settings_ui.models_error.is_some());
    harness.run_steps(40);
    assert_eq!(asked(&probe, &broken), 1, "no loop of requests");
    // the button is at the bottom of a tab that scrolls: bring it into view first
    harness.get_by_label("Load models").scroll_to_me();
    harness.run_steps(3);
    harness.get_by_label("Load models").click();
    until(&mut harness, "the second ask", |app| {
        !app.settings_ui.models_loading && app.settings_ui.models_error.is_some()
    });
    assert_eq!(asked(&probe, &broken), 2);
}

#[test]
fn the_model_menu_can_be_searched_and_a_click_picks_the_model() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    fixture.app.settings_ui.models =
        ["meta/llama-3.3-70b-instruct", "meta/llama-3.1-8b-instruct", "nvidia/nemotron-70b", "mistralai/mixtral-8x7b"]
            .map(str::to_owned)
            .to_vec();
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    harness.run();
    harness.get_by_label("Answers (Ask): 4").click();
    harness.run_steps(3);
    harness.get_by_label("meta/llama-3.1-8b-instruct"); // the whole list to begin with
    harness.get_by_label("4 / 4");
    let search = harness.get_by_label("Search models");
    search.focus();
    search.type_text("LLAMA 70b");
    harness.run_steps(3);
    harness.get_by_label("meta/llama-3.3-70b-instruct");
    harness.get_by_label("1 / 4");
    shot(&mut harness, "model-search");
    assert!(harness.query_by_label("nvidia/nemotron-70b").is_none(), "a model that does not match is hidden");
    assert!(harness.query_by_label("meta/llama-3.1-8b-instruct").is_none());
    harness.get_by_label("meta/llama-3.3-70b-instruct").click();
    harness.run_steps(3);
    assert_eq!(harness.state().settings.model, "meta/llama-3.3-70b-instruct");
    assert!(harness.query_by_label("Search models").is_none(), "choosing a model closes the menu");
    // opened again, the search starts clear
    harness.get_by_label("Answers (Ask): 4").click();
    harness.run_steps(3);
    harness.get_by_label("4 / 4");
}

#[test]
fn the_model_menu_says_so_when_no_list_has_been_loaded() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    harness.run();
    harness.get_by_label("Answers (Ask): 0").click();
    harness.run_steps(3);
    harness.get_by_label("No list yet: press Load models below");
    assert!(harness.query_by_label("Search models").is_none());
}

#[test]
fn the_practice_tab_is_a_checklist() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Practice;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    harness.run();
    harness.get_by_label("Get ready for the technical interview");
    harness.get_by_label_contains("1. The vacancy");
    harness.get_by_label("Data Engineering Intern at Vention");
    harness.get_by_label_contains("Build the questions");
    harness.get_by_label("Export the questions");
    shot(&mut harness, "tab-practice-full");
}

#[test]
fn a_new_version_opens_a_window_with_what_to_do() {
    let mut fixture = fixture("en");
    let asset = |name: &str| Asset {
        name: name.into(),
        url: format!("https://github.com/kravchenski/careertwin/releases/download/v0.6.0/{name}"),
        size: 100,
    };
    fixture.app.update_ui.available = Some(Available {
        current: "0.5.0".into(),
        release: Release {
            tag: "v0.6.0".into(),
            notes: "## What is new\n- Several API keys per provider\n- The window is written in Rust".into(),
            page: "https://github.com/kravchenski/careertwin/releases/tag/v0.6.0".into(),
            assets: vec![
                asset("CareerTwin-Interview-v0.6.0-windows-x64-setup.exe"),
                asset("CareerTwin-Interview-v0.6.0-macos-arm64.dmg"),
                asset("SHA256SUMS.txt"),
            ],
        },
    });
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
    harness.run();
    harness.get_by_label("Version 0.6.0 is available");
    harness.get_by_label("You have 0.5.0.");
    harness.get_by_label("Open the release page");
    harness.get_by_label("Later");
    harness.get_by_label("Skip this version");
    shot(&mut harness, "update-window");
    harness.get_by_label("Later").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Later").is_none());
}

#[test]
fn the_window_speaks_russian_when_asked() {
    let (mut harness, _dir, _runtime) = harness(fixture("ru"), 900.0, 420.0);
    harness.run();
    harness.get_by_label("Пока нет вопросов");
    harness.get_by_label("Спросить");
    harness.get_by_label("Скриншот");
    shot(&mut harness, "overlay-ru");
}

#[test]
fn the_general_tab_has_the_appearance_section_with_themes_the_equalizer_and_icons() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::General;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 1000.0);
    harness.run();
    for label in [
        "Appearance",
        "Theme",
        "Midnight",
        "Graphite",
        "Forest",
        "Amber",
        "Rose",
        "Ocean",
        "Paper",
        "Custom",
        "Equalizer",
        "Background",
        "Animate",
        "Theme colour",
    ] {
        see(&harness, label);
    }
    shot(&mut harness, "tab-general-appearance");
}

#[test]
fn a_theme_changes_every_colour_and_a_light_one_stays_readable() {
    for (name, shot_name) in [("paper", "overlay-paper"), ("forest", "overlay-forest"), ("amber", "overlay-amber")] {
        let mut fixture = fixture("en");
        fixture.app.app.theme = name.into();
        fixture.app.entries.push(answered("What is a left join?", REPLY));
        let palette = theme::from_settings(&fixture.app.app);
        theme::set_palette(palette);
        let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
        harness.run();
        harness.get_by_label_contains("keeps every row");
        assert_eq!(theme::palette(), palette);
        shot(&mut harness, shot_name);
    }
    theme::set_palette(theme::preset("midnight"));
}

#[test]
fn a_custom_theme_uses_the_three_colours() {
    let mut fixture = fixture("en");
    fixture.app.app.theme = "custom".into();
    fixture.app.app.custom_panel = "#101820".into();
    fixture.app.app.custom_text = "#e8eef4".into();
    fixture.app.app.custom_accent = "#ff7a59".into();
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::General;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 1000.0);
    harness.run();
    see(&harness, "Accent");
    harness.get_by_label("#ff7a59");
    assert_eq!(theme::palette().accent, egui::Color32::from_rgb(255, 122, 89));
    shot(&mut harness, "tab-general-custom");
    theme::set_palette(theme::preset("midnight"));
}

fn appearance_window() -> (Harness<'static, App>, tempfile::TempDir, tokio::runtime::Runtime) {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::General;
    let (mut harness, dir, runtime) = harness(fixture, 1000.0, 1000.0);
    harness.run();
    (harness, dir, runtime)
}

#[test]
fn the_language_of_the_window_is_chosen_in_appearance_and_kept() {
    let (mut harness, _dir, _runtime) = appearance_window();
    for (name, code, heading) in [
        ("Русский", "ru", "Оформление"),
        ("Українська", "uk", "Вигляд"),
        ("Polski", "pl", "Wygląd"),
        ("English", "en", "Appearance"),
    ] {
        harness.get_by_label(name).click();
        harness.run_steps(3);
        assert_eq!(harness.state().settings.ui_language, code);
        see(&harness, heading);
        let saved = std::fs::read_to_string(harness.state().paths.settings_file()).unwrap();
        assert!(saved.contains(&format!(r#""ui_language": "{code}""#)), "{code} is saved: {saved}");
    }
    harness.get_by_label("Automatic").click();
    harness.run_steps(3);
    assert_eq!(harness.state().settings.ui_language, "auto");
}

#[test]
fn the_accent_is_chosen_apart_from_the_theme_and_can_go_back_to_the_themes_own() {
    use careertwin_overlay::look;
    let (mut harness, _dir, _runtime) = appearance_window();
    let amber = look::PRESETS[3].3;
    harness.get_by_label(&look::hex(amber)).click();
    harness.run_steps(3);
    assert_eq!(harness.state().app.accent, look::hex(amber));
    assert_eq!(theme::palette().accent, egui::Color32::from_rgb(amber[0], amber[1], amber[2]));
    assert_eq!(harness.state().app.theme, "midnight", "the theme itself stays");
    shot(&mut harness, "tab-general-accent");

    harness.get_by_label("The theme's own").click();
    harness.run_steps(3);
    assert_eq!(harness.state().app.accent, "");
    let own = look::PRESETS[0].3;
    assert_eq!(theme::palette().accent, egui::Color32::from_rgb(own[0], own[1], own[2]));
    theme::set_palette(theme::preset("midnight"));
}

#[test]
fn the_system_theme_is_a_choice_that_follows_the_system() {
    use careertwin_overlay::look;
    let (mut harness, _dir, _runtime) = appearance_window();
    harness.get_by_label("System").click();
    harness.run_steps(3);
    assert_eq!(harness.state().app.theme, look::SYSTEM_THEME);
    let saved = std::fs::read_to_string(harness.state().paths.app_file()).unwrap();
    assert!(saved.contains(r#""theme": "system""#), "{saved}");
    theme::set_palette(theme::preset("midnight"));
}

/// Scroll the settings down until `label` is inside the window (the icons are below its fold) and let it come to rest:
/// the position a node reports is that of the frame before, so a click must wait for the scrolling to stop.
fn reveal(harness: &mut Harness<'_, App>, label: &str) {
    for _ in 0..20 {
        if harness.get_by_label(label).rect().max.y < 640.0 {
            harness.run_steps(4);
            let rect = harness.get_by_label(label).rect();
            assert!(rect.min.y > 200.0 && rect.max.y < 800.0, "{label} is not inside the window: {rect:?}");
            return;
        }
        harness.get_by_label("Appearance").scroll_down();
        harness.run_steps(2);
    }
    panic!("{label} did not come into view");
}

/// An ICO with one picture, as Windows keeps its icons.
fn ico_file(dir: &std::path::Path, name: &str) -> String {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[200u8; 32 * 32 * 4], 32, 32, image::ExtendedColorType::Rgba8)
        .unwrap();
    let frame = image::codecs::ico::IcoFrame::with_encoded(png, 32, 32, image::ExtendedColorType::Rgba8).unwrap();
    let path = dir.join(name);
    image::codecs::ico::IcoEncoder::new(std::fs::File::create(&path).unwrap()).encode_images(&[frame]).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn an_ico_is_taken_as_the_icon_and_a_file_that_is_not_a_picture_is_refused() {
    let (mut harness, dir, _runtime) = appearance_window();
    let ico = ico_file(dir.path(), "app.ico");
    harness.state_mut().settings_ui.icon_path = ico.clone();
    harness.run_steps(2);
    reveal(&mut harness, "Use this image");
    harness.get_by_label("Use this image").click();
    harness.run_steps(3);
    assert_eq!(harness.state().app.app_icon, ico);
    assert!(harness.state().settings_ui.icon_error.is_none());
    see(&harness, "app.ico");
    shot(&mut harness, "tab-general-icon-own");

    // a script with a picture's name, and a file that is not there: refused, the icon in use stays
    let fake = dir.path().join("evil.png");
    std::fs::write(&fake, b"#!/bin/sh\necho hello\n").unwrap();
    for bad in [fake.to_string_lossy().into_owned(), dir.path().join("missing.ico").to_string_lossy().into_owned()] {
        harness.state_mut().settings_ui.icon_path = bad;
        harness.run_steps(2);
        reveal(&mut harness, "Use this image");
        harness.get_by_label("Use this image").click();
        harness.run_steps(3);
        assert_eq!(harness.state().app.app_icon, ico, "a refused file does not replace the icon");
        harness.get_by_label_contains("Could not use that image");
    }
}

/// A file the system hands to the window, as the windowing backend does.
#[derive(Debug)]
struct Dropped(std::path::PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

#[test]
fn a_picture_dropped_on_the_window_becomes_the_icon() {
    let (mut harness, dir, _runtime) = appearance_window();
    let ico = ico_file(dir.path(), "dropped.ico");
    harness.input_mut().dropped_files.push(Arc::new(Dropped(ico.clone().into())));
    harness.run_steps(3);
    assert_eq!(harness.state().app.app_icon, ico);
}

fn tell(app: &App, events: Vec<Heard>) {
    for event in events {
        app.tx.send(Event::Heard(event)).unwrap();
    }
}

#[test]
fn a_question_heard_on_the_call_is_answered_in_the_window() {
    let fixture = fixture("en");
    tell(
        &fixture.app,
        vec![
            Heard::State(State::Listening),
            Heard::Partial("what is a left jo".into()),
            Heard::Turn { speaker: "Interviewer".into(), text: "What is a left join?".into() },
            Heard::SuggestionStart { question: "What is a left join?".into() },
            Heard::Chunk("ANSWER:\nIt keeps ".into()),
            Heard::Chunk("every left row.".into()),
            Heard::Done("ANSWER:\nIt keeps every left row.".into()),
            Heard::Status("The first model is rate limited or down: answered by backup/model".into()),
        ],
    );
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 420.0);
    harness.run();
    harness.get_by_label_contains("It keeps every left row.");
    harness.get_by_label_contains("Answered by backup/model");
    let app = harness.state();
    assert_eq!((app.entries.len(), app.entries[0].source, app.entries[0].done), (1, Source::Heard, true));
    assert_eq!(app.state, State::Listening);
    shot(&mut harness, "overlay-heard");
}

#[test]
fn words_still_being_said_show_as_a_line_and_a_cancelled_early_answer_goes_away() {
    let mut fixture = fixture("en");
    fixture.app.settings.listen = true; // listening is off by default on a Mac, and the line shows only while it is on
    tell(
        &fixture.app,
        vec![
            Heard::State(State::Listening),
            Heard::SuggestionStart { question: "What is a left".into() },
            Heard::Chunk("ANSWER:\nA join".into()),
            Heard::Discard,
            Heard::Partial("what is a left join and when".into()),
        ],
    );
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 420.0);
    harness.run();
    assert!(harness.state().entries.is_empty(), "the answer to half a question is dropped");
    harness.get_by_label_contains("what is a left join and when");
    harness.get_by_label("Heard");
    shot(&mut harness, "overlay-hearing");
}

#[test]
fn an_error_in_the_middle_of_an_answer_shows_in_its_place_and_a_stop_ends_it() {
    let fixture = fixture("en");
    tell(
        &fixture.app,
        vec![
            Heard::State(State::Thinking),
            Heard::SuggestionStart { question: "Why Rust?".into() },
            Heard::Error("429: too many requests".into()),
        ],
    );
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 420.0);
    harness.run();
    harness.get_by_label_contains("429: too many requests");
    assert!(harness.state().entries[0].done);
}

#[test]
fn the_general_tab_says_what_is_listened_to() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::General;
    fixture.app.settings.listen = true;
    fixture.app.settings.use_mic = true;
    fixture.app.settings_ui.sources = Some(vec!["alsa_output.pci.monitor".into()]);
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    harness.run();
    see(&harness, "Listen to the call");
    see(&harness, "Also listen to my microphone");
    see(&harness, "Call sound");
    see(&harness, "Microphone");
    shot(&mut harness, "tab-general-hearing");
}

#[test]
fn the_window_and_every_settings_tab_draw_in_each_language_without_missing_letters() {
    for (language, ask) in
        [("pl", "Zapytaj"), ("uk", "Запитати"), ("de", "Fragen"), ("fr", "Demander"), ("es", "Preguntar")]
    {
        for tab in [Tab::Interview, Tab::Practice, Tab::Ai, Tab::General] {
            let mut fixture = fixture(language);
            fixture.app.settings_open = true;
            fixture.app.settings_ui.tab = tab;
            fixture.app.settings.listen = true;
            fixture.app.settings.use_mic = true;
            let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 1100.0);
            harness.run();
            see(&harness, ask);
        }
    }
}

#[test]
fn the_equalizer_can_be_switched_off() {
    let mut fixture = fixture("en");
    fixture.app.app.equalizer = "off".into();
    fixture.app.app.animate = false;
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 420.0);
    harness.run();
    harness.get_by_label("Ready");
    shot(&mut harness, "overlay-no-equalizer");
}

/// The equalizer at a few moments, to judge how it looks: `UI_SHOTS=dir cargo test --test ui_shots the_equalizer_moves`.
#[test]
fn the_equalizer_moves_with_the_level_and_the_mood() {
    use careertwin_overlay::ui::equalizer::{self, Mood};
    let cases = [
        ("listening-loud", Mood::Listening, 0.85, 3.2),
        ("listening-soft", Mood::Listening, 0.3, 4.1),
        ("thinking", Mood::Thinking, 0.0, 1.0),
        ("idle", Mood::Idle, 0.0, 2.0),
    ];
    for theme_name in ["midnight", "amber"] {
        for (name, mood, level, time) in cases {
            theme::set_palette(theme::preset(theme_name));
            let mut harness = Harness::builder()
                .with_size(egui::vec2(900.0, 360.0))
                .with_accessibility_check(false)
                .with_context_setup(|ctx| theme::install(ctx, 1.0))
                .build_ui(move |ui| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 14.0, theme::panel());
                    let area = rect.shrink(6.0);
                    let (columns, _) = equalizer::grid(area.width(), area.height());
                    let bars = equalizer::heights(columns, time, level, mood);
                    equalizer::background(ui.painter(), area, &bars, equalizer::mood_color(mood), 1.0);
                });
            harness.run();
            if let Ok(dir) = std::env::var("UI_SHOTS") {
                harness.render().expect("renders").save(format!("{dir}/eq-{theme_name}-{name}.png")).unwrap();
            }
        }
    }
    theme::set_palette(theme::preset("midnight"));
}

// ---- every button of the window is pressed, the way a pointer presses it --------------------------------------------

/// Run frames until `done` holds: what the threads of the session say reaches the window a moment later.
fn settle(harness: &mut Harness<'_, App>, done: impl Fn(&App) -> bool) {
    for _ in 0..600 {
        harness.run_steps(1);
        if done(harness.state()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the window never got where the test waits for it");
}

fn window_with(model: Arc<Answers>) -> (Harness<'static, App>, tempfile::TempDir, tokio::runtime::Runtime) {
    let mut fixture = fixture_model("en", Some(model));
    fixture.app.settings.listen = true;
    fixture.app.settings.screenshot_tool = "no-such-tool".into(); // a press must never capture the real screen
    harness(fixture, 900.0, 520.0)
}

#[test]
fn the_round_button_pauses_and_resumes_listening() {
    let (mut harness, _dir, _runtime) = window_with(Answers::new(false));
    harness.run();
    assert!(!harness.state().paused);
    harness.get_by_label("Pause listening").click();
    settle(&mut harness, |app| app.paused);
    harness.run_steps(2);
    harness.get_by_label("Resume listening").click();
    settle(&mut harness, |app| !app.paused);
    harness.run_steps(2);
    harness.get_by_label("Pause listening");
}

#[test]
fn the_round_button_switches_listening_on_when_nothing_is_set_to_listen() {
    let mut fixture = fixture_model("en", Some(Answers::new(false)));
    fixture.app.settings.listen = false;
    fixture.app.settings.use_mic = false;
    fixture.app.settings.stt_engine = "deepgram".into(); // with no key: nothing is recorded, and it says why
    fixture.app.settings.deepgram_key.clear();
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
    harness.run();
    harness.get_by_label("Start listening").click();
    settle(&mut harness, |app| app.settings.listen);
    harness.run();
    assert!(harness.state().notice.as_ref().is_some_and(|(text, error)| *error && text.contains("Deepgram")));
}

#[test]
fn the_ask_pill_answers_the_last_thing_heard_and_says_so_when_nothing_was() {
    let model = Answers::new(false);
    let (mut harness, _dir, _runtime) = window_with(model.clone());
    harness.run();
    harness.get_by_label("Ask").click();
    harness.run();
    assert!(harness.state().notice.as_ref().is_some_and(|(text, _)| text.contains("Nothing heard yet")));
    assert_eq!(model.count(), 0);
    harness.state().session.with_log(|log| log.add("Interviewer", "What is a left join?"));
    harness.get_by_label("Ask").click();
    settle(&mut harness, |app| app.entries.first().is_some_and(|entry| entry.done));
    assert_eq!(model.count(), 1);
    assert!(model.asked.lock().unwrap()[0].contains("What is a left join?"));
    harness.get_by_label_contains("from the model");
}

/// What the window says when the capture tool it was told to use is not there: the tool's name when the computer has
/// some other capture tool, and the general "no screenshot tool" when it has none (a bare CI machine).
fn capture_refused(text: &str) -> bool {
    text.contains("no-such-tool") || text.starts_with("No screenshot tool")
}

#[test]
fn the_screenshot_pill_starts_a_capture_and_a_missing_tool_is_said_out_loud() {
    let (mut harness, _dir, _runtime) = window_with(Answers::new(false));
    harness.run();
    harness.get_by_label("Screenshot").click();
    settle(&mut harness, |app| app.notice.is_some());
    let notice = harness.state().notice.clone().unwrap();
    assert!(notice.1 && capture_refused(&notice.0), "{notice:?}");
    assert!(!harness.state().busy(), "the window is free again after a failed capture");
}

#[test]
fn a_pill_that_cannot_be_pressed_does_nothing() {
    let model = Answers::new(true);
    let (mut harness, _dir, _runtime) = window_with(model.clone());
    harness.run();
    harness.state_mut().settings.screenshot_enabled = false;
    harness.run();
    harness.get_by_label("Screenshot").click();
    harness.run();
    assert!(!harness.state().busy() && harness.state().notice.is_none());
}

#[test]
fn the_menu_opens_and_each_of_its_items_does_what_it_says() {
    let mut fixture = fixture_model("en", Some(Answers::new(false)));
    fixture.app.library.new_interview("Second interview");
    fixture.app.library_changed();
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 620.0);
    harness.run_steps(3);
    let second = harness.state().library.active.clone();
    // Settings
    harness.get_by_label("Menu").click();
    harness.run_steps(3);
    for label in ["Settings", "Interview", "Second interview", "Keep the window on top", "Check now", "Quit"] {
        assert!(harness.query_all_by_label(label).next().is_some(), "the menu lacks {label}");
    }
    harness.get_by_label("Settings").click();
    harness.run_steps(3);
    assert!(harness.state().settings_open);
    harness.state_mut().settings_open = false;
    harness.run_steps(3);
    // another interview
    harness.get_by_label("Menu").click();
    harness.run_steps(3);
    harness.get_by_label("Vention: Data Engineering Internship (Gdańsk)").click();
    harness.run_steps(3);
    assert_ne!(harness.state().library.active, second);
    // on top, and back
    let pinned = harness.state().settings.pinned;
    harness.get_by_label("Menu").click();
    harness.run_steps(3);
    harness.get_by_label("Keep the window on top").click();
    harness.run_steps(3);
    assert_eq!(harness.state().settings.pinned, !pinned);
    // quit
    harness.get_by_label("Menu").click();
    harness.run_steps(3);
    harness.get_by_label("Quit").click();
    harness.run_steps(3);
    assert!(harness.state().quitting());
}

#[test]
fn a_question_typed_by_hand_is_answered_by_the_button_and_by_enter_and_stopped_by_stop() {
    let model = Answers::new(false);
    let (mut harness, _dir, _runtime) = window_with(model.clone());
    harness.run();
    // empty: nothing to get
    harness.get_by_label("Get answer").click();
    harness.run();
    assert_eq!(model.count(), 0);
    // the button
    harness.get_by_label("Type a question by hand…").click();
    harness.event(egui::Event::Text("What is a left join?".into()));
    harness.run();
    harness.get_by_label("Get answer").click();
    settle(&mut harness, |app| app.entries.first().is_some_and(|entry| entry.done));
    assert_eq!((model.count(), harness.state().input.as_str()), (1, ""));
    assert_eq!(harness.state().entries[0].source, Source::Typed);
    // Enter
    harness.get_by_label("Type a question by hand…").click();
    harness.event(egui::Event::Text("And a right join?".into()));
    harness.run();
    harness.key_press(egui::Key::Enter);
    settle(&mut harness, |app| app.entries.len() == 2 && app.entries[1].done);
    assert_eq!(model.count(), 2);
    // a slow answer is stopped
    let slow = Answers::new(true);
    let (mut harness, _dir, _runtime) = window_with(slow.clone());
    harness.run();
    harness.get_by_label("Type a question by hand…").click();
    harness.event(egui::Event::Text("Explain indexes".into()));
    harness.run();
    harness.get_by_label("Get answer").click();
    settle(&mut harness, |app| app.thinking());
    harness.run();
    harness.get_by_label("Stop").click();
    settle(&mut harness, |app| !app.thinking());
    assert!(harness.state().entries[0].done);
}

#[test]
fn the_keys_on_the_pills_do_what_the_pills_do() {
    let model = Answers::new(false);
    let (mut harness, _dir, _runtime) = window_with(model.clone());
    harness.run();
    harness.state().session.with_log(|log| log.add("Interviewer", "What is an index?"));
    let both = egui::Modifiers { ctrl: true, command: true, ..egui::Modifiers::NONE };
    harness.key_press_modifiers(both, egui::Key::Enter); // Ask
    settle(&mut harness, |app| app.entries.first().is_some_and(|entry| entry.done));
    assert_eq!(model.count(), 1);
    let with_shift = egui::Modifiers { shift: true, ..both };
    harness.key_press_modifiers(with_shift, egui::Key::Enter); // the screenshot, not Ask
    settle(&mut harness, |app| app.notice.as_ref().is_some_and(|(text, _)| capture_refused(text)));
    assert_eq!(model.count(), 1, "Ctrl+Shift+Enter must not also ask");
}

#[test]
fn the_answers_are_stepped_through_by_the_buttons_and_the_keys_and_the_counter_says_where() {
    let mut fixture = fixture("en");
    for number in 1..=3 {
        let mut entry = answered(&format!("Question {number}"), &format!("ANSWER:\nAnswer {number}"));
        entry.id = number;
        fixture.app.entries.push(entry);
    }
    fixture.app.shown = 2;
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
    harness.run();
    harness.get_by_label("3/3");
    harness.get_by_label_contains("Answer 3");
    harness.get_by_label("Previous answer").click();
    harness.run();
    harness.get_by_label("2/3");
    harness.get_by_label_contains("Answer 2");
    harness.get_by_label("Next answer").click();
    harness.run();
    harness.get_by_label("3/3");
    // the ends: pressing past them does nothing
    harness.get_by_label("Next answer").click();
    harness.run();
    harness.get_by_label("3/3");
    // the keys, while the line to type on is empty
    let both = egui::Modifiers { ctrl: true, command: true, ..egui::Modifiers::NONE };
    harness.key_press_modifiers(both, egui::Key::ArrowLeft);
    harness.run();
    harness.get_by_label("2/3");
    harness.key_press_modifiers(both, egui::Key::ArrowLeft);
    harness.run();
    harness.get_by_label("1/3");
    harness.key_press_modifiers(both, egui::Key::ArrowLeft);
    harness.run();
    harness.get_by_label("1/3");
    harness.key_press_modifiers(both, egui::Key::ArrowRight);
    harness.run();
    harness.get_by_label("2/3");
    shot(&mut harness, "overlay-steps");
}

#[test]
fn copy_puts_the_answer_on_the_clipboard() {
    let mut fixture = fixture("en");
    fixture.app.entries.push(answered("What is a left join?", "ANSWER:\nIt keeps the left rows."));
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 420.0);
    harness.run();
    harness.get_by_label("Copy").click();
    harness.run_steps(1); // the frame that handles the press is the one whose output has the copy
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == "It keeps the left rows."));
    assert!(copied, "the answer was not copied: {:?}", harness.output().platform_output.commands);
}

#[test]
fn the_window_in_a_narrow_size_drops_the_keys_from_the_pills_and_keeps_every_button() {
    let (mut harness, _dir, _runtime) = window_with(Answers::new(false));
    harness.set_size(egui::vec2(520.0, 300.0));
    harness.run();
    for label in ["Pause listening", "Ask", "Screenshot", "Menu", "Get answer"] {
        harness.get_by_label(label);
    }
    shot(&mut harness, "overlay-narrow");
}

#[test]
fn nothing_floats_over_the_window_to_take_its_clicks() {
    // the corner to pull a frameless window by was once a layer of its own that covered most of the window
    let mut fixture = fixture("en");
    fixture.app.settings.frameless = true;
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
    harness.run();
    for y in (0..520).step_by(10) {
        for x in (0..900).step_by(10) {
            let layer = harness.ctx.layer_id_at(egui::pos2(x as f32, y as f32));
            assert!(
                layer.is_none_or(|layer| layer.order == egui::Order::Background),
                "({x}, {y}) is covered by {layer:?}"
            );
        }
    }
}

#[test]
fn the_account_modes_are_shown_for_a_provider_with_accounts_and_not_for_a_local_gateway() {
    // Groq has accounts saved here
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    let (mut groq, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    groq.run();
    see(&groq, "Several accounts at once");
    see(&groq, "Accounts");
    // Switchyard spreads the load over its own accounts, so there is nothing to choose here
    let mut fixture = fixture_model("en", None);
    fixture.app.settings_open = true;
    fixture.app.settings_ui.tab = Tab::Ai;
    fixture.app.settings.set_gateway("http://127.0.0.1:3260").unwrap();
    let (mut local, _dir, _runtime) = harness(fixture, 1000.0, 900.0);
    local.run();
    assert!(local.query_all_by_label("Several accounts at once").next().is_none());
    assert!(local.query_all_by_label("Give up on a silent account after").next().is_none());
    assert!(local.query_all_by_label("Accounts").next().is_none());
    see(&local, "Provider");
}

#[test]
fn a_status_word_shows_no_text_cursor_but_an_answer_can_be_selected() {
    let mut fixture = fixture("en");
    fixture.app.entries.push(answered("What is a left join?", REPLY));
    let (mut harness, _dir, _runtime) = harness(fixture, 900.0, 520.0);
    harness.run_steps(3);
    let status = harness.get_by_label("Ready").rect().center();
    harness.hover_at(status);
    harness.run_steps(2);
    assert_ne!(
        harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Text,
        "the status word is not text to pick"
    );
    let answer = harness.get_by_label_contains("keeps every row").rect();
    harness.hover_at(egui::pos2(answer.left() + 30.0, answer.center().y));
    harness.run_steps(2);
    assert_eq!(harness.output().platform_output.cursor_icon, egui::CursorIcon::Text, "the answer is");
}

#[test]
fn the_edges_and_corners_of_the_settings_window_are_not_resize_handles() {
    let mut fixture = fixture("en");
    fixture.app.settings_open = true;
    let (mut harness, _dir, _runtime) = harness(fixture, 1000.0, 760.0);
    harness.run_steps(3);
    // the window is 720 x 640 in the middle: its corners are at (140, 60) and (860, 700)
    for (x, y) in [
        (860.0, 700.0),
        (858.0, 698.0),
        (862.0, 702.0),
        (140.0, 60.0),
        (860.0, 60.0),
        (140.0, 700.0),
        (500.0, 701.0),
        (861.0, 380.0),
    ] {
        harness.hover_at(egui::pos2(x, y));
        harness.run_steps(2);
        let cursor = harness.output().platform_output.cursor_icon;
        assert!(
            !matches!(
                cursor,
                egui::CursorIcon::ResizeNwSe
                    | egui::CursorIcon::ResizeNeSw
                    | egui::CursorIcon::ResizeHorizontal
                    | egui::CursorIcon::ResizeVertical
                    | egui::CursorIcon::ResizeNorth
                    | egui::CursorIcon::ResizeSouth
                    | egui::CursorIcon::ResizeEast
                    | egui::CursorIcon::ResizeWest
                    | egui::CursorIcon::ResizeNorthEast
                    | egui::CursorIcon::ResizeNorthWest
                    | egui::CursorIcon::ResizeSouthEast
                    | egui::CursorIcon::ResizeSouthWest
            ),
            "({x}, {y}) shows {cursor:?}"
        );
    }
}
