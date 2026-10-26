//! Preparing for a vacancy: paste it, see how your CV fits, and get the questions it invites.

use egui::{RichText, TextEdit, Ui, vec2};

use super::theme::{self, BERRY, FAINT, GOOD, MUTED, SNOW};
use super::widgets::{self, Icon, card_frame, ghost_button, icon_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::Lang;
use crate::prompts::prepare::{Analysis, BankQuestion};

pub struct PrepareView {
    pub analysis: Option<Analysis>,
    pub questions: Vec<BankQuestion>,
    pub comparing: bool,
    pub asking: bool,
    pub error: Option<String>,
}

impl PrepareView {
    pub fn new() -> Self {
        Self { analysis: None, questions: Vec::new(), comparing: false, asking: false, error: None }
    }
}

impl Default for PrepareView {
    fn default() -> Self {
        Self::new()
    }
}

/// The job title, the company and the text of the vacancy. Returns whether anything was edited.
pub fn vacancy_fields(core: &mut Core, ui: &mut Ui) -> bool {
    let tr = core.tr;
    let mut changed = false;
    let title_label = widgets::caps(ui, tr.get(K::JobTitle));
    changed |= ui
        .add(TextEdit::singleline(&mut core.vacancy.title).desired_width(f32::INFINITY).margin(vec2(12.0, 9.0)))
        .labelled_by(title_label.id)
        .changed();
    ui.add_space(4.0);
    let company_label = widgets::caps(ui, tr.get(K::Company));
    changed |= ui
        .add(TextEdit::singleline(&mut core.vacancy.company).desired_width(f32::INFINITY).margin(vec2(12.0, 9.0)))
        .labelled_by(company_label.id)
        .changed();
    ui.add_space(4.0);
    let description_label = widgets::caps(ui, tr.get(K::JobDescription));
    changed |= ui
        .add(
            TextEdit::multiline(&mut core.vacancy.description)
                .desired_rows(9)
                .desired_width(f32::INFINITY)
                .margin(vec2(12.0, 10.0)),
        )
        .labelled_by(description_label.id)
        .changed();
    changed
}

pub fn ui(core: &mut Core, view: &mut PrepareView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut compare = false;
    let mut ask = false;
    let mut speak: Option<String> = None;
    let mut changed = false;
    scrolling_column(ui, 760.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::PrepareTitle));
        ui.label(RichText::new(tr.get(K::PrepareIntro)).font(theme::sans(16.0)).color(MUTED));
        ui.add_space(14.0);
        changed |= vacancy_fields(core, ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ready = !core.vacancy.description.trim().is_empty();
            if primary_button(ui, tr.get(K::CompareCv), ready && !view.comparing).clicked() {
                compare = true;
            }
            if ghost_button(ui, tr.get(K::MakeQuestions)).clicked() && !view.asking {
                ask = true;
            }
            if view.comparing || view.asking {
                ui.spinner();
                ui.label(RichText::new(tr.get(K::Working)).font(theme::serif_italic(17.0)).color(MUTED));
            }
        });
        if let Some(error) = &view.error {
            ui.add_space(6.0);
            ui.label(RichText::new(error).font(theme::sans(14.5)).color(BERRY));
        }
        if let Some(analysis) = &view.analysis {
            ui.add_space(22.0);
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&analysis.summary).font(theme::serif(19.0)).color(SNOW));
                if !analysis.strengths.is_empty() {
                    ui.add_space(10.0);
                    widgets::caps(ui, tr.get(K::Strengths));
                    for item in &analysis.strengths {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("•").color(GOOD));
                            ui.add(widgets::wrapping(RichText::new(item).font(theme::sans(15.0)).color(SNOW)));
                        });
                    }
                }
                if !analysis.gaps.is_empty() {
                    ui.add_space(10.0);
                    widgets::caps(ui, tr.get(K::Gaps));
                    for gap in &analysis.gaps {
                        ui.label(RichText::new(&gap.gap).font(theme::semibold(15.5)).color(SNOW));
                        if !gap.how_to_address.is_empty() {
                            ui.label(
                                RichText::new(format!("{}: {}", tr.get(K::HowToAddress), gap.how_to_address))
                                    .font(theme::sans(14.5))
                                    .color(MUTED),
                            );
                        }
                        ui.add_space(4.0);
                    }
                }
                if !analysis.cv_fixes.is_empty() {
                    ui.add_space(6.0);
                    widgets::caps(ui, tr.get(K::CvFixes));
                    for item in &analysis.cv_fixes {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("•").color(BERRY));
                            ui.add(widgets::wrapping(RichText::new(item).font(theme::sans(15.0)).color(SNOW)));
                        });
                    }
                }
            });
        }
        if !view.questions.is_empty() {
            ui.add_space(22.0);
            widgets::caps(ui, tr.get(K::LikelyQuestions));
            for question in &view.questions {
                ui.add_space(6.0);
                card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(question.kind.to_uppercase()).font(theme::mono(11.0)).color(FAINT));
                    });
                    ui.label(RichText::new(&question.question).font(theme::serif(19.0)).color(SNOW));
                    for line in &question.outline {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("•").color(FAINT));
                            ui.add(widgets::wrapping(RichText::new(line).font(theme::sans(14.5)).color(MUTED)));
                        });
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if icon_button(ui, Icon::Play, tr.get(K::Listen), 34.0, None).clicked() {
                            speak = Some(question.outline.join(" "));
                        }
                        if ghost_button(ui, tr.get(K::PractiseThis)).clicked() {
                            nav = Some(Nav::PracticeQuestion(question.question.clone()));
                        }
                    });
                });
            }
        }
        ui.add_space(40.0);
    });
    if changed {
        core.save_vacancy();
    }
    if compare || ask || speak.is_some() {
        ui.ctx().request_repaint();
    }
    if compare {
        view.comparing = true;
        view.error = None;
        core.analyze();
    }
    if ask {
        view.asking = true;
        view.error = None;
        core.question_bank(8);
    }
    if let Some(text) = speak
        && !text.trim().is_empty()
    {
        core.speak(text, if core.vacancy_language_is_russian() { Lang::Ru } else { Lang::En });
    }
    nav
}
