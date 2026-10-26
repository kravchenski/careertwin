//! Settings: where your profile is, which model, recogniser and voice to use, and which sound
//! devices. Edited as a draft and applied only when you press Save.

use egui::{Align2, Color32, ComboBox, Frame, Margin, RichText, ScrollArea, Stroke, TextEdit, Ui, vec2};

use super::Core;
use super::theme::{self, BERRY, FAINT, GOOD, LINE, MUTED, SNOW, SURFACE};
use super::widgets::{self, chip, ghost_button, primary_button};
use crate::config::{Settings, SttEngine, TtsEngine, UiLanguage};
use crate::i18n::{K, Tr};
use crate::secrets::Secret;

/// Addresses people use, so they do not have to be typed.
const MODEL_PRESETS: [(&str, &str); 6] = [
    ("Switchyard", "http://127.0.0.1:3260"),
    ("Groq", "https://api.groq.com/openai"),
    ("OpenAI", "https://api.openai.com"),
    ("OpenRouter", "https://openrouter.ai/api"),
    ("Ollama", "http://127.0.0.1:11434"),
    ("LM Studio", "http://127.0.0.1:1234"),
];

const STT_PRESETS: [(&str, &str, &str); 3] = [
    ("Groq", "https://api.groq.com/openai", "whisper-large-v3-turbo"),
    ("OpenAI", "https://api.openai.com", "whisper-1"),
    ("Local server", "http://127.0.0.1:8000", "whisper-1"),
];

const VOICE_PRESETS: [(&str, &str, &str, &str); 3] = [
    ("OpenAI", "https://api.openai.com", "gpt-4o-mini-tts", "alloy"),
    ("Local server (Kokoro, Chatterbox…)", "http://127.0.0.1:8880", "kokoro", "af_heart"),
    ("OpenAI tts-1", "https://api.openai.com", "tts-1", "alloy"),
];

/// A settings file being edited. Keys are plain text here and become secrets on Save.
pub struct Draft {
    pub settings: Settings,
    pub llm_key: String,
    pub stt_key: String,
    pub tts_key: String,
    pub new_cv: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub message: Option<(bool, String)>,
}

impl Draft {
    pub fn new(core: &Core) -> Self {
        let settings = core.settings.clone();
        Self {
            llm_key: settings.llm.key.expose().to_owned(),
            stt_key: settings.stt.key.expose().to_owned(),
            tts_key: settings.tts.key.expose().to_owned(),
            settings,
            new_cv: String::new(),
            inputs: crate::audio::friendly_devices(crate::audio::capture::input_devices()),
            outputs: crate::audio::friendly_devices(crate::audio::playback::output_devices()),
            message: None,
        }
    }

    /// The settings with the typed keys put back in.
    pub fn result(&self) -> Settings {
        let mut settings = self.settings.clone();
        settings.llm.key = Secret::new(self.llm_key.clone());
        settings.stt.key = Secret::new(self.stt_key.clone());
        settings.tts.key = Secret::new(self.tts_key.clone());
        settings
    }
}

fn field(ui: &mut Ui, label: &str, text: &mut String, hint: &str, password: bool) {
    widgets::caps(ui, label);
    ui.add(
        TextEdit::singleline(text)
            .hint_text(hint)
            .password(password)
            .desired_width(f32::INFINITY)
            .margin(vec2(12.0, 8.0)),
    );
    ui.add_space(4.0);
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(16.0);
    ui.label(RichText::new(title).font(theme::serif(21.0)).color(SNOW));
    ui.add_space(4.0);
}

fn device_picker(
    ui: &mut Ui,
    id: &str,
    current: &mut String,
    devices: &[String],
    default_label: &str,
    label: egui::Id,
) {
    let shown = if current.is_empty() { default_label.to_owned() } else { current.clone() };
    ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(current, String::new(), default_label);
            for device in devices {
                ui.selectable_value(current, device.clone(), device);
            }
        })
        .response
        .labelled_by(label);
    ui.add_space(4.0);
}

enum Action {
    Save,
    Cancel,
    LoadModels,
    TestVoice,
}

pub fn show(core: &mut Core, draft: &mut Option<Draft>, ctx: &egui::Context) {
    if !core.settings_open {
        *draft = None;
        return;
    }
    if draft.is_none() {
        *draft = Some(Draft::new(core));
    }
    let Some(edit) = draft.as_mut() else { return };
    let tr = core.tr;
    let mut action: Option<Action> = None;
    let mut open = true;
    let height = (ctx.content_rect().height() - 90.0).clamp(380.0, 760.0);
    egui::Window::new(tr.get(K::SettingsTitle))
        .open(&mut open)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 10.0))
        .fixed_size(vec2(660.0, height))
        .collapsible(false)
        .title_bar(false)
        .frame(
            Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, LINE)).corner_radius(22).inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr.get(K::SettingsTitle)).font(theme::serif(30.0)).color(SNOW));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, widgets::Icon::Close, tr.get(K::Close), 34.0, None).clicked() {
                        action = Some(Action::Cancel);
                    }
                });
            });
            ScrollArea::vertical().auto_shrink([false, false]).max_height(height - 150.0).show(ui, |ui| {
                ui.set_width(ui.available_width() - 12.0);
                body(core, edit, ui, tr, &mut action);
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary_button(ui, tr.get(K::SaveSettings), true).clicked() {
                    action = Some(Action::Save);
                }
                if ghost_button(ui, tr.get(K::Cancel)).clicked() {
                    action = Some(Action::Cancel);
                }
                if let Some((ok, message)) = &edit.message {
                    ui.label(RichText::new(message).font(theme::sans(14.0)).color(if *ok { GOOD } else { BERRY }));
                }
            });
        });
    if !open {
        action = Some(Action::Cancel);
    }
    match action {
        Some(Action::Cancel) => core.settings_open = false,
        Some(Action::Save) => {
            let settings = edit.result();
            match settings.validate() {
                Ok(()) => match core.apply_settings(settings) {
                    Ok(()) => core.settings_open = false,
                    Err(error) => edit.message = Some((false, error.to_string())),
                },
                Err(error) => edit.message = Some((false, error.to_string())),
            }
        }
        Some(Action::LoadModels) => core.load_models(&edit.result()),
        Some(Action::TestVoice) => core.test_voice(&edit.result()),
        None => {}
    }
    if core.settings_open {
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}

fn body(core: &mut Core, edit: &mut Draft, ui: &mut Ui, tr: Tr, action: &mut Option<Action>) {
    section(ui, tr.get(K::SecInterface));
    widgets::caps(ui, tr.get(K::UiLanguageLabel));
    ui.horizontal(|ui| {
        for (language, label) in
            [(UiLanguage::Auto, tr.get(K::Automatic)), (UiLanguage::En, "English"), (UiLanguage::Ru, "Русский")]
        {
            if chip(ui, label, edit.settings.ui_language == language).clicked() {
                edit.settings.ui_language = language;
            }
        }
    });

    section(ui, tr.get(K::SecProfile));
    field(ui, tr.get(K::AboutFile), &mut edit.settings.about_path, "~/Documents/job-search/ABOUT.md", false);
    let cv_label = widgets::caps(ui, tr.get(K::CvFiles));
    let mut remove = None;
    for (index, path) in edit.settings.cv_paths.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(path).font(theme::mono(12.5)).color(MUTED));
            if ghost_button(ui, tr.get(K::Remove)).clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        edit.settings.cv_paths.remove(index);
    }
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut edit.new_cv)
                .hint_text("~/Documents/cv.pdf")
                .desired_width(380.0)
                .margin(vec2(12.0, 8.0)),
        )
        .labelled_by(cv_label.id);
        if ghost_button(ui, tr.get(K::AddCv)).clicked() && !edit.new_cv.trim().is_empty() {
            let path = std::mem::take(&mut edit.new_cv);
            edit.settings.cv_paths.push(path.trim().to_owned());
        }
    });
    if let Some(note) = &core.brief_note {
        ui.label(RichText::new(note).font(theme::sans(13.5)).color(BERRY));
    } else if let Some(brief) = &core.brief {
        ui.label(
            RichText::new(brief.sources().iter().map(|(_, l)| l.clone()).collect::<Vec<_>>().join("  ·  "))
                .font(theme::mono(12.0))
                .color(FAINT),
        );
    }

    section(ui, tr.get(K::SecModel));
    ui.horizontal_wrapped(|ui| {
        for (name, url) in MODEL_PRESETS {
            if chip(ui, name, edit.settings.llm.url == url).clicked() {
                edit.settings.llm.url = url.to_owned();
            }
        }
    });
    field(ui, tr.get(K::Address), &mut edit.settings.llm.url, "http://127.0.0.1:3260", false);
    field(ui, tr.get(K::ApiKey), &mut edit.llm_key, tr.get(K::KeyFromEnv), true);
    let model_label = widgets::caps(ui, tr.get(K::ModelName));
    ui.add(
        TextEdit::singleline(&mut edit.settings.llm.model)
            .hint_text("auto")
            .desired_width(f32::INFINITY)
            .margin(vec2(12.0, 8.0)),
    )
    .labelled_by(model_label.id);
    ui.add_space(4.0);
    field(ui, tr.get(K::FastModel), &mut edit.settings.llm.fast_model, "llama-3.1-8b-instant", false);
    ui.horizontal(|ui| {
        if ghost_button(ui, tr.get(K::LoadModels)).clicked() {
            *action = Some(Action::LoadModels);
        }
        if !core.models.is_empty() {
            ui.label(
                RichText::new(tr.fill(K::ModelsLoaded, &[("n", &core.models.len().to_string())]))
                    .font(theme::mono(12.0))
                    .color(FAINT),
            );
        }
    });
    if !core.models.is_empty() {
        ComboBox::from_id_salt("models")
            .selected_text(&edit.settings.llm.model)
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for model in core.models.clone() {
                    ui.selectable_value(&mut edit.settings.llm.model, model.clone(), &model);
                }
            });
    }
    if let Some(problem) = &core.problems.model {
        ui.label(RichText::new(problem).font(theme::sans(13.5)).color(BERRY));
    }

    section(ui, tr.get(K::SecRecognition));
    ui.horizontal_wrapped(|ui| {
        if chip(ui, tr.get(K::EngineHttp), edit.settings.stt.engine == SttEngine::Http).clicked() {
            edit.settings.stt.engine = SttEngine::Http;
        }
        if cfg!(feature = "local-stt")
            && chip(ui, tr.get(K::EngineLocal), edit.settings.stt.engine == SttEngine::Local).clicked()
        {
            edit.settings.stt.engine = SttEngine::Local;
        }
    });
    if edit.settings.stt.engine == SttEngine::Http {
        ui.horizontal_wrapped(|ui| {
            for (name, url, model) in STT_PRESETS {
                if chip(ui, name, edit.settings.stt.url == url).clicked() {
                    edit.settings.stt.url = url.to_owned();
                    edit.settings.stt.model = model.to_owned();
                }
            }
        });
        field(ui, tr.get(K::Address), &mut edit.settings.stt.url, "https://api.groq.com/openai", false);
        field(ui, tr.get(K::ApiKey), &mut edit.stt_key, tr.get(K::KeyFromEnv), true);
        field(ui, tr.get(K::ModelName), &mut edit.settings.stt.model, "whisper-large-v3-turbo", false);
    }
    widgets::caps(ui, tr.get(K::RecognitionLanguage));
    ui.horizontal(|ui| {
        for (code, label) in [("auto", tr.get(K::Automatic)), ("en", "English"), ("ru", "Русский")] {
            if chip(ui, label, edit.settings.stt.language == code).clicked() {
                edit.settings.stt.language = code.to_owned();
            }
        }
    });
    if let Some(problem) = &core.problems.recognition {
        ui.label(RichText::new(problem).font(theme::sans(13.5)).color(BERRY));
    }

    section(ui, tr.get(K::SecVoice));
    ui.horizontal_wrapped(|ui| {
        for (engine, label) in [
            (TtsEngine::Openai, K::EngineOpenai),
            (TtsEngine::Elevenlabs, K::EngineEleven),
            (TtsEngine::Off, K::EngineOff),
        ] {
            if chip(ui, tr.get(label), edit.settings.tts.engine == engine).clicked() {
                edit.settings.tts.engine = engine;
                if engine == TtsEngine::Elevenlabs {
                    edit.settings.tts.url = crate::speech::tts::ELEVEN_URL.to_owned();
                    edit.settings.tts.model = "eleven_multilingual_v2".to_owned();
                }
            }
        }
    });
    if edit.settings.tts.engine == TtsEngine::Openai {
        ui.horizontal_wrapped(|ui| {
            for (name, url, model, voice) in VOICE_PRESETS {
                if chip(ui, name, edit.settings.tts.url == url && edit.settings.tts.model == model).clicked() {
                    edit.settings.tts.url = url.to_owned();
                    edit.settings.tts.model = model.to_owned();
                    edit.settings.tts.voice_en = voice.to_owned();
                    edit.settings.tts.voice_ru = voice.to_owned();
                }
            }
        });
    }
    if edit.settings.tts.engine != TtsEngine::Off {
        field(ui, tr.get(K::Address), &mut edit.settings.tts.url, "https://api.openai.com", false);
        field(ui, tr.get(K::ApiKey), &mut edit.tts_key, tr.get(K::KeyFromEnv), true);
        field(ui, tr.get(K::ModelName), &mut edit.settings.tts.model, "gpt-4o-mini-tts", false);
        field(ui, tr.get(K::VoiceEnglish), &mut edit.settings.tts.voice_en, "alloy", false);
        field(ui, tr.get(K::VoiceRussian), &mut edit.settings.tts.voice_ru, "alloy", false);
        field(ui, tr.get(K::VoiceTone), &mut edit.settings.tts.instructions, "Calm, friendly, a little brisk.", false);
        let speed_label = widgets::caps(ui, tr.get(K::VoiceSpeed));
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut edit.settings.tts.speed, 0.7..=1.3).step_by(0.05).show_value(false))
                .labelled_by(speed_label.id);
            ui.label(RichText::new(format!("{:.2}×", edit.settings.tts.speed)).font(theme::mono(12.5)).color(MUTED));
        });
        if ghost_button(ui, tr.get(K::TestVoice)).clicked() {
            *action = Some(Action::TestVoice);
        }
    }
    if let Some(problem) = &core.problems.voice {
        ui.label(RichText::new(problem).font(theme::sans(13.5)).color(BERRY));
    }
    ui.add_space(4.0);
    ui.label(RichText::new(tr.get(K::CloudWarning)).font(theme::sans(13.0)).color(FAINT));

    section(ui, tr.get(K::SecAudio));
    let mic_label = widgets::caps(ui, tr.get(K::Microphone));
    device_picker(
        ui,
        "mic",
        &mut edit.settings.audio.input_device,
        &edit.inputs.clone(),
        tr.get(K::SystemDefault),
        mic_label.id,
    );
    let speakers_label = widgets::caps(ui, tr.get(K::Speakers));
    device_picker(
        ui,
        "speakers",
        &mut edit.settings.audio.output_device,
        &edit.outputs.clone(),
        tr.get(K::SystemDefault),
        speakers_label.id,
    );
    ui.checkbox(&mut edit.settings.audio.headphones, RichText::new(tr.get(K::SetupHeadphones)).color(MUTED));
    if let Some(problem) = &core.problems.speakers {
        ui.label(RichText::new(problem).font(theme::sans(13.5)).color(BERRY));
    }
    ui.add_space(8.0);
    let _ = (Color32::TRANSPARENT, SNOW);
}
