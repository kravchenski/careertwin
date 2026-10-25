//! The first page of each tab. It looks like the call itself: the sphere, a line from the tutor and
//! and the one thing to do now. The numbers live in Your activity and what you did last in the menu.

use egui::{RichText, Ui, vec2};

use super::orb;
use super::theme::{self, BERRY, MUTED, SNOW};
use super::widgets::{self, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::tutor::Mode;
use crate::session::Track;

pub struct Card {
    pub title: K,
    pub text: K,
    pub go: Nav,
}

/// The screens under a tab. They are not listed anywhere: the tutor leads, and opens the one it needs.
pub fn ways(track: Track) -> Vec<Card> {
    let chat = Card { title: K::CardChat, text: K::CardChatText, go: Nav::Tutor { mode: Mode::Chat, topic: None } };
    let show = Card { title: K::CardShow, text: K::CardShowText, go: Nav::Show { topic: None, hint: String::new() } };
    match track {
        Track::English => vec![
            Card { title: K::CardConversation, text: K::CardConversationText, go: Nav::SetupCall },
            Card { title: K::CardTranslate, text: K::CardTranslateText, go: Nav::Translate },
            Card { title: K::CardPhrases, text: K::CardPhrasesText, go: Nav::Phrases },
            chat,
        ],
        Track::Technical => vec![
            Card { title: K::CardMock, text: K::CardMockTechnicalText, go: Nav::SetupCall },
            Card { title: K::CardConcepts, text: K::CardConceptsText, go: Nav::Concepts },
            Card { title: K::CardCode, text: K::CardCodeText, go: Nav::Code },
            Card { title: K::CardPlan, text: K::CardPlanText, go: Nav::Plan },
            Card { title: K::CardPrepare, text: K::CardPrepareText, go: Nav::Prepare },
            show,
            chat,
        ],
        Track::Interview => vec![
            Card { title: K::CardMock, text: K::CardMockInterviewText, go: Nav::SetupCall },
            Card { title: K::CardPrepare, text: K::CardPrepareText, go: Nav::Prepare },
            Card { title: K::CardTranslate, text: K::CardTranslateText, go: Nav::Translate },
            chat,
        ],
    }
}

/// What the main button says and does, and the tutor's line above it.
fn main_action(core: &Core, track: Track) -> (String, String, Nav) {
    let tr = core.tr;
    match track {
        Track::English => (tr.get(K::HomeStart).to_owned(), tr.get(K::HomeLineEnglish).to_owned(), Nav::QuickCall),
        Track::Interview => (
            tr.get(K::HomeIntroStart).to_owned(),
            tr.get(K::HomeLineInterview).to_owned(),
            Nav::Tutor { mode: Mode::Intro, topic: None },
        ),
        Track::Technical => match &core.syllabus {
            None => (tr.get(K::PlanResearch).to_owned(), tr.get(K::HomeLineNoPlan).to_owned(), Nav::Plan),
            Some(plan) => match plan.next_topic(&core.progress) {
                Some(index) => {
                    let topic = &plan.topics[index];
                    (
                        tr.get(K::HomeStartRun).to_owned(),
                        tr.fill(
                            K::HomeLineRun,
                            &[
                                ("topic", &topic.name),
                                ("lang", &crate::i18n::language_phrase(tr, core.settings.practice_name())),
                            ],
                        ),
                        Nav::FullInterview,
                    )
                }
                None if plan.topics.iter().all(|t| t.terms.is_empty()) => {
                    (tr.get(K::CardPlan).to_owned(), tr.get(K::HomeLineNoTerms).to_owned(), Nav::Plan)
                }
                None => {
                    (tr.get(K::HomeStartRun).to_owned(), tr.get(K::HomeLineAllLearned).to_owned(), Nav::FullInterview)
                }
            },
        },
    }
}

pub fn ui(core: &mut Core, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let track = core.track;
    let mut nav = None;
    scrolling_column(ui, 820.0, |ui| {
        ui.add_space(6.0);
        // the sphere, breathing
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 230.0), egui::Sense::hover());
        let seconds = core.started.elapsed().as_secs_f32();
        let breath = 0.10 + 0.06 * (seconds * 1.4).sin();
        let rect = egui::Rect::from_center_size(area.center(), egui::Vec2::splat(230.0));
        core.paint_orb(ui, rect, &orb::Params { time: seconds, level: breath, spin: 0.8 }, 10.0);

        let (title, _) = match track {
            Track::English => (K::EnglishTitle, K::EnglishIntro),
            Track::Technical => (K::TechnicalTitle, K::TechnicalIntro),
            Track::Interview => (K::InterviewTitle, K::InterviewIntro),
        };
        let (button, line, go) = main_action(core, track);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr.get(title)).font(theme::heading(36.0)).color(SNOW));
            ui.add_space(2.0);
            ui.set_max_width(600.0);
            ui.label(RichText::new(line).font(theme::serif_italic(19.0)).color(MUTED));
            ui.add_space(14.0);
            if primary_button(ui, &button, true).clicked() {
                nav = Some(go.clone());
            }
        });

        for download in core.downloading() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr.fill(
                    K::Downloading,
                    &[("name", &download.name), ("percent", &download.progress.percent().to_string())],
                ))
                .font(theme::mono(12.5))
                .color(BERRY),
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

        let due = core.deck.due(core.now()).len();
        if due > 0 {
            ui.add_space(10.0);
            ui.label(
                RichText::new(tr.fill(K::DueForReview, &[("n", &due.to_string())]))
                    .font(theme::mono(12.5))
                    .color(BERRY),
            );
        }
        ui.add_space(40.0);
    });
    nav
}
