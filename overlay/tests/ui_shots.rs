//! The window drawn headless: what a screen reader sees is checked, and pictures are written (when `UI_SHOTS`
//! names a folder) so the look can be judged.

use std::sync::Arc;
use std::time::Instant;

use careertwin_overlay::paths::Paths;
use careertwin_overlay::questions::Kind;
use careertwin_overlay::ui::settings_ui::Tab;
use careertwin_overlay::ui::{App, Entry, Source, theme};
use careertwin_overlay::update::{Asset, Available, Release};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const DEMO: &str = include_str!("../../prep/tests/fixtures/ABOUT.demo.md");

struct Fixture {
    app: App,
    _dir: tempfile::TempDir,
    _runtime: tokio::runtime::Runtime,
}

fn fixture(language: &str) -> Fixture {
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
    let app = App::new(Paths::under(data), runtime.handle().clone(), Arc::new(|| {}));
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
        // the inputs of the settings are named by the label next to them, which the check cannot see yet
        .with_accessibility_check(false)
        .with_context_setup(move |ctx| theme::install(ctx, scale))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (harness, _dir, _runtime)
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
    harness.get_by_label("Settings");
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
        harness.get_by_label(key);
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
        harness.get_by_label(label);
    }
    shot(&mut harness, "tab-ai-full");
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
        "Dots at the top",
        "Animate",
        "Theme colour",
    ] {
        harness.get_by_label(label);
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
    harness.get_by_label("Accent");
    harness.get_by_label("#ff7a59");
    assert_eq!(theme::palette().accent, egui::Color32::from_rgb(255, 122, 89));
    shot(&mut harness, "tab-general-custom");
    theme::set_palette(theme::preset("midnight"));
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
                    equalizer::background(ui.painter(), rect.shrink(6.0), level, mood, time);
                    let strip = egui::Rect::from_min_size(rect.min + egui::vec2(20.0, 14.0), egui::vec2(86.0, 26.0));
                    ui.painter().rect_filled(strip, 13.0, theme::mix(theme::panel(), theme::text(), 0.09));
                    equalizer::dots(ui.painter(), strip, level, mood, time);
                });
            harness.run();
            if let Ok(dir) = std::env::var("UI_SHOTS") {
                harness.render().expect("renders").save(format!("{dir}/eq-{theme_name}-{name}.png")).unwrap();
            }
        }
    }
    theme::set_palette(theme::preset("midnight"));
}
