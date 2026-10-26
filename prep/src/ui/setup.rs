//! Setting up a call: who interviews you, how hard their English is, how long, what about.

use egui::{RichText, TextEdit, Ui};

use super::theme::{self, FAINT, MUTED, SNOW};
use super::widgets::{self, chip, ghost_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::config::TtsEngine;
use crate::i18n::K;
use crate::prompts::Lang;
use crate::prompts::interviewer::{InterviewerSpec, Level, Persona};
use crate::session::Track;

pub struct SetupView {
    pub spec: InterviewerSpec,
    pub headphones: bool,
}

impl SetupView {
    pub fn new(core: &Core) -> Self {
        let mut spec = InterviewerSpec::new(core.track);
        spec.language = Lang::En;
        Self { spec, headphones: core.settings.audio.headphones }
    }
}

pub fn persona_label(core: &Core, persona: Persona) -> String {
    let tr = core.tr;
    let role = match persona {
        Persona::TechLead => K::RoleTechLead,
        Persona::Architect => K::RoleArchitect,
        Persona::HiringManager => K::RoleHiringManager,
        Persona::Recruiter => K::RoleRecruiter,
        Persona::Tutor => K::RoleTutor,
        Persona::Mentor => K::RoleMentor,
    };
    format!("{} · {}", persona.first_name(), tr.get(role))
}

pub fn title_for(core: &Core) -> &'static str {
    let tr = core.tr;
    tr.get(match core.track {
        Track::English => K::CardConversation,
        _ => K::CardMock,
    })
}

pub fn ui(core: &mut Core, view: &mut SetupView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let track = core.track;
    let mut nav = None;
    scrolling_column(ui, 760.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, title_for(core));
        ui.add_space(2.0);
        let sources = core.brief.as_ref().map(|b| b.sources()).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            for (_, label) in &sources {
                ui.label(RichText::new(label).font(theme::mono(12.0)).color(FAINT));
            }
        });
        if core.brief.is_none() && track != Track::English {
            ui.add_space(10.0);
            widgets::card_frame().show(ui, |ui| {
                ui.label(RichText::new(tr.get(K::NoProfile)).color(MUTED));
                ui.add_space(6.0);
                if ghost_button(ui, tr.get(K::OpenSettings)).clicked() {
                    core.settings_open = true;
                }
            });
        }
        ui.add_space(22.0);
        widgets::caps(ui, tr.get(K::SetupInterviewer));
        ui.horizontal_wrapped(|ui| {
            for persona in Persona::for_track(track) {
                if chip(ui, &persona_label(core, persona), view.spec.persona == persona).clicked() {
                    view.spec.persona = persona;
                }
            }
        });
        ui.add_space(14.0);
        if track == Track::English {
            widgets::caps(ui, tr.get(K::SetupLevel));
            ui.horizontal_wrapped(|ui| {
                for level in Level::ALL {
                    if chip(ui, level.label(), view.spec.level == level).clicked() {
                        view.spec.level = level;
                    }
                }
            });
        } else {
            widgets::caps(ui, tr.get(K::SetupLanguage));
            ui.horizontal_wrapped(|ui| {
                for (language, key) in [(Lang::En, K::LangEnglish), (Lang::Ru, K::LangRussian)] {
                    if chip(ui, tr.get(key), view.spec.language == language).clicked() {
                        view.spec.language = language;
                    }
                }
            });
        }
        ui.add_space(14.0);
        widgets::caps(ui, tr.get(K::SetupLength));
        ui.horizontal_wrapped(|ui| {
            let options: [u32; 3] = if track == Track::English { [5, 10, 15] } else { [10, 20, 30] };
            for minutes in options {
                if chip(ui, &tr.fill(K::Minutes, &[("n", &minutes.to_string())]), view.spec.minutes == minutes)
                    .clicked()
                {
                    view.spec.minutes = minutes;
                }
            }
        });
        ui.add_space(14.0);
        widgets::caps(ui, tr.get(K::SetupFocus));
        ui.add(
            TextEdit::singleline(&mut view.spec.focus)
                .hint_text(tr.get(K::SetupFocusHint))
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(12.0, 9.0)),
        );
        ui.add_space(12.0);
        ui.checkbox(&mut view.headphones, RichText::new(tr.get(K::SetupHeadphones)).color(MUTED));
        if core.settings.tts.engine == TtsEngine::Off {
            ui.label(RichText::new(tr.get(K::SetupVoiceOff)).font(theme::sans(13.5)).color(FAINT));
        }
        if let Some(problem) = core.problems.voice.as_deref().or(core.problems.model.as_deref()) {
            ui.add_space(6.0);
            ui.label(RichText::new(problem).font(theme::sans(13.5)).color(theme::BERRY));
        }
        ui.add_space(26.0);
        let downloading = !core.downloading().is_empty();
        if downloading {
            ui.label(RichText::new(tr.get(K::ModelsNotReady)).font(theme::sans(13.5)).color(theme::BERRY));
            for download in core.downloading() {
                ui.label(
                    RichText::new(tr.fill(
                        K::Downloading,
                        &[("name", &download.name), ("percent", &download.progress.percent().to_string())],
                    ))
                    .font(theme::mono(12.5))
                    .color(MUTED),
                );
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        let ready = (core.brief.is_some() || track == Track::English) && !downloading;
        if primary_button(ui, tr.get(K::StartCall), ready).clicked() {
            nav = Some(Nav::StartCall { spec: Box::new(view.spec.clone()), headphones: view.headphones });
        }
        ui.add_space(40.0);
    });
    let _ = SNOW;
    nav
}
