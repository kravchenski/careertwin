//! "Say it in English": give a thought in Russian, hear it in natural English in three registers,
//! and say it back to see which words you got.

use std::collections::HashMap;

use egui::text::{LayoutJob, TextFormat};
use egui::{RichText, Stroke, TextEdit, Ui, vec2};

use super::review::Drill;
use super::theme::{self, BERRY, FAINT, GOOD, MUTED, SNOW};
use super::widgets::{self, Icon, card_frame, ghost_button, icon_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::Lang;
use crate::prompts::translate::Translation;
use crate::session::{Kind, Session};
use crate::srs::{Card, CardKind};
use crate::textdiff::Mark;

/// The slot a dictation uses; the three registers use 0, 1 and 2.
pub const DICTATE: u32 = 100;

pub struct TranslateView {
    pub input: String,
    pub result: Option<Translation>,
    pub working: bool,
    pub dictating: bool,
    pub error: Option<String>,
    pub drills: HashMap<usize, Drill>,
    pub listening_slot: Option<usize>,
    pub added: Vec<String>,
    pub session: Option<Session>,
}

impl TranslateView {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            result: None,
            working: false,
            dictating: false,
            error: None,
            drills: HashMap::new(),
            listening_slot: None,
            added: Vec::new(),
            session: None,
        }
    }

    pub fn from_session(session: Session) -> Self {
        let result: Option<Translation> = serde_json::from_value(session.payload["translation"].clone()).ok();
        let mut view = Self::new();
        view.input = session.payload["input"].as_str().unwrap_or_default().to_owned();
        view.result = result;
        view.session = Some(session);
        view
    }

    /// The target of a drill: slot 0 natural, 1 formal, 2 short.
    pub fn target(&self, slot: usize) -> Option<String> {
        let result = self.result.as_ref()?;
        Some(result.all().get(slot)?.1.to_owned())
    }

    /// Keeps the translation with the other sessions.
    pub fn keep(&mut self, core: &Core) {
        let Some(result) = &self.result else { return };
        let title: String = self.input.chars().take(48).collect();
        let mut session =
            self.session.take().unwrap_or_else(|| Session::new(core.track, Kind::Translate, title.trim()));
        session.payload = serde_json::json!({ "input": self.input, "translation": result });
        session.finish();
        let _ = core.store.save(&session);
        self.session = Some(session);
    }
}

impl Default for TranslateView {
    fn default() -> Self {
        Self::new()
    }
}

fn drill_job(drill: &Drill) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (index, mark) in drill.comparison.marks.iter().enumerate() {
        let format = match mark.mark {
            Mark::Match => TextFormat { font_id: theme::sans(15.5), color: GOOD, ..Default::default() },
            Mark::Missed => TextFormat {
                font_id: theme::sans(15.5),
                color: BERRY,
                underline: Stroke::new(1.5, BERRY),
                ..Default::default()
            },
            Mark::Extra => TextFormat {
                font_id: theme::sans(15.5),
                color: FAINT,
                strikethrough: Stroke::new(1.0, FAINT),
                ..Default::default()
            },
        };
        job.append(&if index == 0 { mark.word.clone() } else { format!(" {}", mark.word) }, 0.0, format);
    }
    job
}

pub fn ui(core: &mut Core, view: &mut TranslateView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut go = false;
    let mut dictate = false;
    let mut speak: Option<String> = None;
    let mut hear: Option<usize> = None;
    let mut add: Option<(String, String)> = None;
    scrolling_column(ui, 760.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::CardTranslate));
        ui.add_space(10.0);
        widgets::caps(ui, tr.get(K::TranslateLabel));
        ui.add(
            TextEdit::multiline(&mut view.input)
                .hint_text(tr.get(K::TranslateHint))
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .margin(vec2(14.0, 12.0)),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let busy = view.working;
            if primary_button(ui, tr.get(K::TranslateGo), !busy && !view.input.trim().is_empty()).clicked() {
                go = true;
            }
            let label = if view.dictating { tr.get(K::Listening) } else { tr.get(K::Dictate) };
            if ghost_button(ui, label).clicked() && !view.dictating {
                dictate = true;
            }
            if view.working {
                ui.spinner();
                ui.label(RichText::new(tr.get(K::TranslateBusy)).font(theme::serif_italic(17.0)).color(MUTED));
            }
        });
        if let Some(error) = &view.error {
            ui.add_space(6.0);
            ui.label(RichText::new(error).font(theme::sans(14.5)).color(BERRY));
        }
        let Some(result) = view.result.clone() else { return };
        ui.add_space(22.0);
        for (slot, (_, text)) in result.all().into_iter().enumerate() {
            ui.add_space(6.0);
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                let name = match slot {
                    0 => tr.get(K::Natural),
                    1 => tr.get(K::Formal),
                    _ => tr.get(K::Short),
                };
                widgets::caps(ui, name);
                ui.label(RichText::new(text).font(theme::serif(21.0)).color(SNOW));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if icon_button(ui, Icon::Play, tr.get(K::Listen), 36.0, None).clicked() {
                        speak = Some(text.to_owned());
                    }
                    let busy = view.listening_slot == Some(slot);
                    if ghost_button(ui, if busy { tr.get(K::Listening) } else { tr.get(K::SayItYourself) }).clicked()
                        && !busy
                    {
                        hear = Some(slot);
                    }
                    if view.added.iter().any(|a| a == text) {
                        ui.label(RichText::new(tr.get(K::Added)).font(theme::mono(12.5)).color(GOOD));
                    } else if ghost_button(ui, tr.get(K::AddToDeck)).clicked() {
                        add = Some((text.to_owned(), view.input.trim().to_owned()));
                    }
                });
                if let Some(drill) = view.drills.get(&slot) {
                    ui.add_space(6.0);
                    widgets::caps(ui, tr.get(K::Heard));
                    ui.label(drill_job(drill));
                    let percent = (drill.comparison.accuracy * 100.0).round() as i32;
                    ui.label(
                        RichText::new(tr.fill(K::WordsMatched, &[("n", &percent.to_string())]))
                            .font(theme::mono(12.5))
                            .color(MUTED),
                    );
                    let missed = drill.comparison.missed();
                    if !missed.is_empty() {
                        ui.label(
                            RichText::new(tr.fill(K::MissedWords, &[("words", &missed.join(", "))]))
                                .font(theme::sans(14.0))
                                .color(BERRY),
                        );
                    }
                }
            });
        }
        if !result.notes.is_empty() {
            ui.add_space(14.0);
            widgets::caps(ui, tr.get(K::Notes));
            for note in &result.notes {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new("•").color(BERRY));
                    ui.label(RichText::new(note).font(theme::sans(15.0)).color(SNOW));
                });
            }
        }
        ui.add_space(40.0);
    });
    if go || dictate || speak.is_some() || hear.is_some() || add.is_some() {
        ui.ctx().request_repaint();
    }
    if go {
        view.working = true;
        view.error = None;
        view.drills.clear();
        core.translate(view.input.clone());
    }
    if dictate {
        view.dictating = true;
        view.error = None;
        core.hear_in(DICTATE, None);
    }
    if let Some(text) = speak {
        core.speak(text, Lang::En);
    }
    if let Some(slot) = hear {
        view.listening_slot = Some(slot);
        core.hear(slot as u32);
    }
    if let Some((english, russian)) = add {
        let card = Card::new(CardKind::Phrase, &english, &russian, "", core.now());
        core.deck.add(card);
        let _ = core.deck.save(&core.paths.deck_file());
        view.added.push(english);
    }
    nav
}
