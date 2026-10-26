//! The first page of each tab: what you can do here, and what you did lately.

use egui::{RichText, Ui};

use super::theme::{self, MUTED, SNOW};
use super::widgets::{self, option_card};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::session::{Track, ago};

struct Card {
    tag: &'static str,
    title: K,
    text: K,
    go: Nav,
}

fn cards(track: Track) -> Vec<Card> {
    match track {
        Track::English => vec![
            Card { tag: "live", title: K::CardConversation, text: K::CardConversationText, go: Nav::SetupCall },
            Card { tag: "translate", title: K::CardTranslate, text: K::CardTranslateText, go: Nav::Translate },
            Card { tag: "review", title: K::CardPhrases, text: K::CardPhrasesText, go: Nav::Phrases },
        ],
        Track::Technical => vec![
            Card { tag: "live", title: K::CardMock, text: K::CardMockTechnicalText, go: Nav::SetupCall },
            Card { tag: "cards", title: K::CardConcepts, text: K::CardConceptsText, go: Nav::Concepts },
            Card { tag: "prepare", title: K::CardPrepare, text: K::CardPrepareText, go: Nav::Prepare },
        ],
        Track::Interview => vec![
            Card { tag: "live", title: K::CardMock, text: K::CardMockInterviewText, go: Nav::SetupCall },
            Card { tag: "prepare", title: K::CardPrepare, text: K::CardPrepareText, go: Nav::Prepare },
            Card { tag: "translate", title: K::CardTranslate, text: K::CardTranslateText, go: Nav::Translate },
        ],
    }
}

pub fn ui(core: &mut Core, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let track = core.track;
    let mut nav = None;
    scrolling_column(ui, 820.0, |ui| {
        ui.add_space(24.0);
        let (title, intro) = match track {
            Track::English => (K::EnglishTitle, K::EnglishIntro),
            Track::Technical => (K::TechnicalTitle, K::TechnicalIntro),
            Track::Interview => (K::InterviewTitle, K::InterviewIntro),
        };
        ui.label(RichText::new(tr.get(title)).font(theme::serif(42.0)).color(SNOW));
        ui.add_space(4.0);
        ui.label(RichText::new(tr.get(intro)).font(theme::sans(16.5)).color(MUTED));
        if core.brief.is_none() {
            ui.add_space(14.0);
            widgets::card_frame().show(ui, |ui| {
                ui.label(RichText::new(tr.get(K::NoProfile)).color(MUTED));
                ui.add_space(6.0);
                if widgets::ghost_button(ui, tr.get(K::OpenSettings)).clicked() {
                    core.settings_open = true;
                }
            });
        }
        ui.add_space(26.0);
        let width = ui.available_width();
        let gap = 14.0;
        let each = (width - gap) / 2.0;
        let all = cards(track);
        for pair in all.chunks(2) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for card in pair {
                    if option_card(ui, each, card.tag, tr.get(card.title), tr.get(card.text)).clicked() {
                        nav = Some(card.go.clone());
                    }
                }
            });
            ui.add_space(4.0);
        }
        let due = core.deck.due(core.now()).len();
        if due > 0 {
            ui.add_space(10.0);
            ui.label(
                RichText::new(tr.fill(K::DueForReview, &[("n", &due.to_string())]))
                    .font(theme::mono(12.5))
                    .color(theme::BERRY),
            );
        }
        ui.add_space(26.0);
        widgets::caps(ui, tr.get(K::ContinueLabel));
        ui.add_space(4.0);
        let recent = core.recent(track, 5);
        if recent.is_empty() {
            widgets::muted(ui, tr.get(K::NothingYet));
        }
        let now = core.now();
        for session in recent {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&session.title).font(theme::semibold(15.5)).color(SNOW));
                ui.label(RichText::new(ago(session.updated, now)).font(theme::mono(12.0)).color(theme::FAINT));
            });
        }
        ui.add_space(40.0);
    });
    nav
}
