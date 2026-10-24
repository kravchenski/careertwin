//! The first page of each tab: what you can do here, and what you did lately.

use egui::{Rect, RichText, Ui, Vec2};

use super::orb;
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
        // the sphere sits at the right of the heading when there is room for it
        let column = ui.max_rect();
        let wide = column.width() >= 700.0;
        let sphere = 200.0;
        let header_top = ui.cursor().top();
        ui.scope(|ui| {
            if wide {
                ui.set_max_width(column.width() - sphere - 24.0);
            }
            ui.label(RichText::new(tr.get(title)).font(theme::serif(42.0)).color(SNOW));
            ui.add_space(4.0);
            ui.label(RichText::new(tr.get(intro)).font(theme::sans(16.5)).color(MUTED));
        });
        if wide {
            let seconds = core.started.elapsed().as_secs_f32();
            let breath = 0.10 + 0.06 * (seconds * 1.4).sin();
            let rect = Rect::from_min_size(egui::pos2(column.right() - sphere, header_top - 8.0), Vec2::splat(sphere));
            core.paint_orb(ui, rect, &orb::Params { time: seconds, level: breath, spin: 0.8 });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            let used = ui.cursor().top() - header_top;
            if used < sphere - 24.0 {
                ui.add_space(sphere - 24.0 - used);
            }
        }
        for download in core.downloading() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr.fill(
                    K::Downloading,
                    &[("name", &download.name), ("percent", &download.progress.percent().to_string())],
                ))
                .font(theme::mono(12.5))
                .color(theme::BERRY),
            );
        }
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
