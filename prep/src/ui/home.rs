//! The first page of each tab. It looks like the call itself: the sphere, a line from the tutor and
//! the one thing to do now; the other ways to practise and how today is going sit below it.

use egui::{RichText, Ui, vec2};

use super::orb;
use super::theme::{self, BERRY, FAINT, MUTED, SNOW};
use super::widgets::{self, option_card, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::tutor::Mode;
use crate::session::{Track, ago};

struct Card {
    tag: &'static str,
    title: K,
    text: K,
    go: Nav,
}

fn cards(track: Track) -> Vec<Card> {
    let chat = Card {
        tag: "write",
        title: K::CardChat,
        text: K::CardChatText,
        go: Nav::Tutor { mode: Mode::Chat, topic: None },
    };
    let show = Card {
        tag: "show",
        title: K::CardShow,
        text: K::CardShowText,
        go: Nav::Show { topic: None, hint: String::new() },
    };
    match track {
        Track::English => vec![
            Card { tag: "live", title: K::CardConversation, text: K::CardConversationText, go: Nav::SetupCall },
            Card { tag: "translate", title: K::CardTranslate, text: K::CardTranslateText, go: Nav::Translate },
            Card { tag: "review", title: K::CardPhrases, text: K::CardPhrasesText, go: Nav::Phrases },
            chat,
        ],
        Track::Technical => vec![
            Card { tag: "live", title: K::CardMock, text: K::CardMockTechnicalText, go: Nav::SetupCall },
            Card { tag: "cards", title: K::CardConcepts, text: K::CardConceptsText, go: Nav::Concepts },
            Card { tag: "code", title: K::CardCode, text: K::CardCodeText, go: Nav::Code },
            Card { tag: "plan", title: K::CardPlan, text: K::CardPlanText, go: Nav::Plan },
            Card { tag: "prepare", title: K::CardPrepare, text: K::CardPrepareText, go: Nav::Prepare },
            show,
            chat,
        ],
        Track::Interview => vec![
            Card { tag: "live", title: K::CardMock, text: K::CardMockInterviewText, go: Nav::SetupCall },
            Card { tag: "prepare", title: K::CardPrepare, text: K::CardPrepareText, go: Nav::Prepare },
            Card { tag: "translate", title: K::CardTranslate, text: K::CardTranslateText, go: Nav::Translate },
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
                        tr.fill(K::HomeStartTopic, &[("topic", &topic.name)]),
                        tr.fill(K::HomeLineTechnical, &[("topic", &topic.name), ("n", &topic.terms.len().to_string())]),
                        Nav::Tutor { mode: Mode::Terms, topic: Some(index) },
                    )
                }
                None => (tr.get(K::CardMock).to_owned(), tr.get(K::HomeLineAllLearned).to_owned(), Nav::SetupCall),
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
            ui.label(RichText::new(tr.get(title)).font(theme::serif(36.0)).color(SNOW));
            ui.add_space(2.0);
            ui.set_max_width(600.0);
            ui.label(RichText::new(line).font(theme::serif_italic(19.0)).color(MUTED));
            ui.add_space(14.0);
            // the two buttons, centred as one row
            let write = tr.get(K::HomeWrite);
            let measure = |text: &str, font: egui::FontId, extra: f32| {
                ui.painter().layout_no_wrap(text.to_owned(), font, SNOW).size().x + extra
            };
            let width =
                measure(&button, theme::semibold(15.5), 52.0) + 10.0 + measure(write, theme::semibold(14.5), 36.0);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                if primary_button(ui, &button, true).clicked() {
                    nav = Some(go.clone());
                }
                if widgets::ghost_button(ui, write).clicked() {
                    nav = Some(Nav::Tutor { mode: Mode::Chat, topic: None });
                }
            });
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

        ui.add_space(22.0);
        today(core, ui);

        ui.add_space(22.0);
        widgets::caps(ui, tr.get(K::HomeMore));
        ui.add_space(6.0);
        let width = ui.available_width();
        let gap = 14.0;
        let each = (width - gap) / 2.0;
        for pair in cards(track).chunks(2) {
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
                    .color(BERRY),
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
                ui.label(RichText::new(ago(session.updated, now)).font(theme::mono(12.0)).color(FAINT));
            });
        }
        ui.add_space(40.0);
    });
    nav
}

/// Today in three numbers: the minutes against the goal, the streak, and how much of the plan
/// you cover. They are the reason to come back tomorrow.
fn today(core: &Core, ui: &mut Ui) {
    let tr = core.tr;
    let now = core.now();
    let done = core.stats.today(now).seconds / 60;
    let goal = core.settings.daily_goal_minutes.max(1);
    let streak = core.stats.streak(now);
    let covered = core.syllabus.as_ref().map(|plan| (plan.coverage(&core.progress) * 100.0).round() as u32);
    let tiles: Vec<(&str, String, f32)> = {
        let mut tiles = vec![
            (tr.get(K::UsageToday), format!("{done} / {goal} min"), done as f32 / goal as f32),
            (tr.get(K::UsageStreak), streak.to_string(), (streak as f32 / 7.0).min(1.0)),
        ];
        if let Some(percent) = covered {
            tiles.push((tr.get(K::TodayCoverage), format!("{percent}%"), percent as f32 / 100.0));
        }
        tiles
    };
    let gap = 12.0;
    let each = (ui.available_width() - gap * (tiles.len() as f32 - 1.0)) / tiles.len() as f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (caption, value, fraction) in tiles {
            widgets::card_frame().inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_width(each - 32.0);
                ui.vertical(|ui| {
                    widgets::caps(ui, caption);
                    ui.label(RichText::new(value).font(theme::serif(24.0)).color(SNOW));
                    widgets::progress(ui, ui.available_width(), fraction);
                });
            });
        }
    });
}
