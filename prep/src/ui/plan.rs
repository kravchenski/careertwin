//! Your plan: the vacancy ranked by the 80/20 rule. The few topics that hold most of the questions
//! come first, each with its terms in plain words, an everyday comparison and an answer to say in
//! English.

use std::sync::atomic::Ordering;

use egui::{CornerRadius, RichText, Sense, Stroke, Ui, vec2};

use super::prepare::vacancy_fields;
use super::theme::{self, BERRY, CRANBERRY, FAINT, GOOD, LINE, MUTED, SNOW};
use super::widgets::{self, Icon, card_frame, ghost_button, icon_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::{K, Tr};
use crate::prompts::Lang;
use crate::prompts::tutor::Mode;
use crate::syllabus::{Progress, Syllabus, Topic, TopicKind};

#[derive(Default)]
pub struct PlanView {
    /// The topic whose terms are shown.
    pub open: Option<usize>,
    /// Showing the vacancy form even though there is a plan.
    pub editing: bool,
    /// The topic whose terms are being written.
    pub writing: Option<usize>,
    pub error: Option<String>,
}

/// A row of ten small marks, filled up to `value`, with a caption.
fn meter(ui: &mut Ui, caption: &str, value: u8, color: egui::Color32) {
    ui.label(RichText::new(caption).font(theme::mono(11.5)).color(FAINT));
    let (rect, _) = ui.allocate_exact_size(vec2(70.0, 8.0), Sense::hover());
    let each = rect.width() / 10.0;
    for i in 0..10 {
        let cell = egui::Rect::from_min_size(
            egui::pos2(rect.left() + i as f32 * each + 0.5, rect.top()),
            vec2(each - 1.5, rect.height()),
        );
        ui.painter().rect_filled(
            cell,
            CornerRadius::same(2),
            if i < usize::from(value.min(10)) { color } else { LINE },
        );
    }
}

enum Action {
    Open(usize),
    WriteTerms(usize),
    Listen(String),
    Practise(usize),
    Show(String, String),
}

pub fn ui(core: &mut Core, view: &mut PlanView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut research = false;
    let mut action: Option<Action> = None;
    let mut changed = false;
    let plan = core.syllabus.take();
    let running = core.research.clone();
    scrolling_column(ui, 800.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::PlanTitle));
        ui.label(RichText::new(tr.get(K::PlanIntro)).font(theme::sans(16.0)).color(MUTED));
        ui.add_space(12.0);

        let stale = plan.as_ref().is_some_and(|p| core.plan_is_stale(p));
        if plan.is_none() || view.editing || stale {
            if plan.is_none() {
                ui.label(RichText::new(tr.get(K::PlanNone)).font(theme::serif_italic(18.0)).color(MUTED));
                ui.add_space(6.0);
            } else if stale {
                ui.label(RichText::new(tr.get(K::PlanStale)).font(theme::sans(14.5)).color(BERRY));
                ui.add_space(6.0);
            }
            changed |= vacancy_fields(core, ui);
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            let ready = !core.vacancy.description.trim().is_empty() && running.is_none();
            let label = if plan.is_some() { K::PlanAgain } else { K::PlanResearch };
            if primary_button(ui, tr.get(label), ready).clicked() {
                research = true;
            }
            if plan.is_some() && ghost_button(ui, tr.get(K::PlanChangeVacancy)).clicked() {
                view.editing = !view.editing;
            }
            if let Some(progress) = &running {
                ui.spinner();
                let text = tr.fill(
                    K::PlanResearching,
                    &[
                        ("done", &progress.done.load(Ordering::Relaxed).to_string()),
                        ("total", &progress.total.load(Ordering::Relaxed).to_string()),
                    ],
                );
                ui.label(RichText::new(text).font(theme::serif_italic(17.0)).color(MUTED));
            }
        });
        if let Some(error) = &view.error {
            ui.add_space(6.0);
            ui.label(RichText::new(error).font(theme::sans(14.5)).color(BERRY));
        }

        if let Some(plan) = &plan {
            ui.add_space(20.0);
            summary(ui, tr, plan, &core.progress);
            let vital = plan.vital();
            let order = plan.study_order();
            ui.add_space(18.0);
            widgets::caps(ui, tr.get(K::PlanVital));
            for index in order.iter().copied().filter(|i| vital.contains(i)) {
                topic_card(ui, tr, plan, index, true, &core.progress, view, &mut action);
            }
            let later: Vec<usize> = order.iter().copied().filter(|i| !vital.contains(i)).collect();
            if !later.is_empty() {
                ui.add_space(18.0);
                widgets::caps(ui, tr.get(K::PlanOthers));
                for index in later {
                    topic_card(ui, tr, plan, index, false, &core.progress, view, &mut action);
                }
            }
        }
        ui.add_space(40.0);
    });
    core.syllabus = plan;
    if changed {
        core.save_vacancy();
    }
    if research {
        view.error = None;
        view.editing = false;
        core.start_research();
        ui.ctx().request_repaint();
    }
    if running.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
    }
    match action {
        Some(Action::Open(index)) => view.open = if view.open == Some(index) { None } else { Some(index) },
        Some(Action::WriteTerms(index)) => {
            view.writing = Some(index);
            view.error = None;
            core.write_terms(index);
            ui.ctx().request_repaint();
        }
        Some(Action::Listen(text)) => {
            core.speak(text, Lang::En);
            ui.ctx().request_repaint();
        }
        Some(Action::Show(topic, hint)) => nav = Some(Nav::Show { topic: Some(topic), hint }),
        Some(Action::Practise(index)) => nav = Some(Nav::Tutor { mode: Mode::Terms, topic: Some(index) }),
        None => {}
    }
    nav
}

fn summary(ui: &mut Ui, tr: Tr, plan: &Syllabus, progress: &Progress) {
    card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let covered = (plan.coverage(progress) * 100.0).round() as u32;
        let vital = plan.vital();
        let known = (plan.vital_coverage(progress) * 100.0).round() as u32;
        ui.label(
            RichText::new(tr.fill(K::PlanCoverage, &[("n", &covered.to_string())]))
                .font(theme::heading(24.0))
                .color(SNOW),
        );
        widgets::progress(ui, ui.available_width(), plan.coverage(progress));
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr.fill(
                K::PlanVitalLine,
                &[
                    ("vital", &vital.len().to_string()),
                    ("all", &plan.topics.len().to_string()),
                    ("known", &known.to_string()),
                ],
            ))
            .font(theme::sans(14.5))
            .color(MUTED),
        );
        if !plan.summary.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(&plan.summary).font(theme::sans(14.5)).color(MUTED));
        }
        if let Some(next) = plan.next_topic(progress) {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                widgets::caps(ui, tr.get(K::PlanStart));
                ui.label(RichText::new(&plan.topics[next].name).font(theme::semibold(15.5)).color(SNOW));
            });
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn topic_card(
    ui: &mut Ui,
    tr: Tr,
    plan: &Syllabus,
    index: usize,
    vital: bool,
    progress: &Progress,
    view: &PlanView,
    action: &mut Option<Action>,
) {
    let topic = &plan.topics[index];
    let open = view.open == Some(index);
    ui.add_space(6.0);
    card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let name = ui.add(
                egui::Label::new(RichText::new(&topic.name).font(theme::semibold(18.0)).color(SNOW))
                    .sense(Sense::click()),
            );
            if name.clicked() {
                *action = Some(Action::Open(index));
            }
            let kind = match topic.kind {
                TopicKind::Concept => K::TopicConcept,
                TopicKind::Framework => K::TopicFramework,
                TopicKind::Language => K::TopicLanguage,
                TopicKind::Practice => K::TopicPractice,
                TopicKind::Behaviour => K::TopicBehaviour,
            };
            ui.label(RichText::new(tr.get(kind).to_uppercase()).font(theme::mono(10.5)).color(FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                meter(ui, &tr.fill(K::PlanEffort, &[("n", &topic.effort.to_string())]), topic.effort, MUTED);
                ui.add_space(8.0);
                meter(
                    ui,
                    &tr.fill(K::PlanLikely, &[("n", &topic.likelihood.to_string())]),
                    topic.likelihood,
                    if vital { BERRY } else { CRANBERRY },
                );
            });
        });
        if !topic.why.is_empty() {
            ui.label(RichText::new(&topic.why).font(theme::sans(14.0)).color(MUTED));
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            widgets::progress(ui, 120.0, progress.topic_mastery(topic));
            if topic.terms.is_empty() {
                if view.writing == Some(index) {
                    ui.spinner();
                } else if ghost_button(ui, tr.get(K::PlanWriteTerms)).clicked() {
                    *action = Some(Action::WriteTerms(index));
                }
            } else {
                ui.label(
                    RichText::new(tr.fill(K::PlanTermsCount, &[("n", &topic.terms.len().to_string())]))
                        .font(theme::mono(12.0))
                        .color(FAINT),
                );
                if ghost_button(ui, tr.get(K::PlanPractise)).clicked() {
                    *action = Some(Action::Practise(index));
                }
            }
        });
        if open {
            for term in 0..topic.terms.len() {
                ui.add_space(10.0);
                term_block(ui, tr, topic, term, progress, action);
            }
        }
    });
}

fn term_block(ui: &mut Ui, tr: Tr, topic: &Topic, index: usize, progress: &Progress, action: &mut Option<Action>) {
    let term = &topic.terms[index];
    let levels = progress.get(&topic.name, &term.name);
    egui::Frame::new()
        .fill(theme::mix(theme::SURFACE, theme::COCOA, 0.5))
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&term.name).font(theme::serif(19.0)).color(SNOW));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(tr.fill(
                            K::PlanLevels,
                            &[("native", &levels.native.to_string()), ("english", &levels.english.to_string())],
                        ))
                        .font(theme::mono(11.0))
                        .color(if levels.mastery() > 0.8 { GOOD } else { FAINT }),
                    );
                });
            });
            ui.label(RichText::new(&term.plain).font(theme::sans(15.0)).color(SNOW));
            if !term.analogy.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{}: {}", tr.get(K::PlanAnalogy), term.analogy))
                        .font(theme::serif_italic(15.5))
                        .color(MUTED),
                );
            }
            if !term.english.is_empty() {
                ui.add_space(6.0);
                widgets::caps(ui, tr.get(K::PlanSayIt));
                ui.horizontal_top(|ui| {
                    if icon_button(ui, Icon::Play, tr.get(K::Listen), 30.0, None).clicked() {
                        *action = Some(Action::Listen(term.english.clone()));
                    }
                    ui.add(widgets::wrapping(RichText::new(&term.english).font(theme::sans(15.0)).color(SNOW)));
                });
            }
            ui.add_space(6.0);
            if ghost_button(ui, tr.get(K::ShowItBtn)).clicked() {
                let hint = format!("{} Comparison: {} A picture could show: {}", term.plain, term.analogy, term.visual);
                *action = Some(Action::Show(format!("{} ({})", term.name, topic.name), hint));
            }
            if !term.follow_ups.is_empty() {
                ui.add_space(4.0);
                widgets::caps(ui, tr.get(K::PlanFollowUps));
                for question in &term.follow_ups {
                    ui.label(RichText::new(format!("• {question}")).font(theme::sans(14.0)).color(MUTED));
                }
            }
        });
}
