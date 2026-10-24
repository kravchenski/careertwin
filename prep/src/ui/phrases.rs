//! Cards that come back just before you would forget them: phrases from your reviews and the
//! questions you got wrong.

use egui::{RichText, Ui};

use super::theme::{self, BERRY, FAINT, MUTED, SNOW};
use super::widgets::{self, Icon, card_frame, ghost_button, icon_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::Lang;
use crate::session::{Track, date};
use crate::srs::{CardKind, Grade};

pub struct PhrasesView {
    pub shown: bool,
}

impl PhrasesView {
    pub fn new() -> Self {
        Self { shown: false }
    }
}

impl Default for PhrasesView {
    fn default() -> Self {
        Self::new()
    }
}

/// The kind of card a tab reviews.
pub fn kind_for(track: Track) -> CardKind {
    if track == Track::Technical { CardKind::Question } else { CardKind::Phrase }
}

pub fn ui(core: &mut Core, view: &mut PhrasesView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let now = core.now();
    let kind = kind_for(core.track);
    let due: Vec<_> = core.deck.due(now).into_iter().filter(|c| c.kind == kind).cloned().collect();
    let mut grade: Option<(String, Grade)> = None;
    let mut speak: Option<String> = None;
    scrolling_column(ui, 720.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(if kind == CardKind::Question { K::CardConcepts } else { K::CardPhrases }));
        ui.add_space(8.0);
        let Some(card) = due.first() else {
            widgets::muted(ui, tr.get(K::NothingYet));
            let next = core.deck.cards.iter().filter(|c| c.kind == kind).map(|c| c.due).min();
            if let Some(next) = next {
                ui.label(RichText::new(date(next)).font(theme::mono(12.5)).color(FAINT));
            }
            return;
        };
        ui.label(
            RichText::new(tr.fill(K::DueForReview, &[("n", &due.len().to_string())]))
                .font(theme::mono(12.5))
                .color(BERRY),
        );
        ui.add_space(8.0);
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&card.front).font(theme::serif(26.0)).color(SNOW));
            ui.add_space(8.0);
            if icon_button(ui, Icon::Play, tr.get(K::Listen), 38.0, None).clicked() {
                speak = Some(card.front.clone());
            }
            ui.add_space(8.0);
            if view.shown {
                crate::ui::rule(ui);
                ui.add_space(8.0);
                ui.label(RichText::new(&card.back).font(theme::sans(16.5)).color(MUTED));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    for (label, value) in [
                        (K::GradeAgain, Grade::Again),
                        (K::GradeHard, Grade::Hard),
                        (K::GradeGood, Grade::Good),
                        (K::GradeEasy, Grade::Easy),
                    ] {
                        if ghost_button(ui, tr.get(label)).clicked() {
                            grade = Some((card.id.clone(), value));
                        }
                    }
                });
            } else if primary_button(ui, tr.get(K::ShowAnswer), true).clicked() {
                view.shown = true;
            }
        });
        ui.add_space(40.0);
    });
    if let Some((id, value)) = grade {
        core.deck.review(&id, value, now);
        let _ = core.deck.save(&core.paths.deck_file());
        view.shown = false;
        ui.ctx().request_repaint();
    }
    if let Some(text) = speak {
        core.speak(text, Lang::En);
    }
    nav
}
