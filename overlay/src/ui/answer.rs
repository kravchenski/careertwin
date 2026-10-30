//! Showing an answer: the lines to say with their key terms, the Markdown text, code blocks and details.

use egui::{Frame, Margin, RichText, ScrollArea, Ui};

use super::theme::{accent, code, line, mono, muted, sans, semibold, text};
use crate::prompts::Sections;

/// One line with Markdown in it (`**bold**`, `code`, a link): the lines to say and the key terms.
pub fn rich_line(ui: &mut Ui, text: &str, size: f32, color: egui::Color32) {
    super::markdown::inline(ui, text, size, color);
}

/// The answer text: Markdown, with fenced code drawn as code blocks.
pub fn answer_text(ui: &mut Ui, body: &str, size: f32) {
    super::markdown::show(ui, body, size);
}

pub fn code_block(ui: &mut Ui, language: &str, body: &str, size: f32) {
    Frame::new()
        .fill(code())
        .stroke(egui::Stroke::new(1.0, line()))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !language.is_empty() {
                ui.label(RichText::new(language).font(sans(size * 0.8)).color(muted()));
            }
            ScrollArea::horizontal().id_salt(("code", body.len(), language)).show(ui, |ui| {
                ui.label(RichText::new(body).font(mono(size * 0.94)).color(text()));
            });
        });
}

/// The whole reply: lines to say (when the layout has them), the answer, and the details.
pub fn show(
    ui: &mut Ui,
    sections: &Sections,
    main_only: bool,
    details_open: &mut bool,
    size: f32,
    t: &dyn Fn(&str) -> String,
) {
    if !main_only && !sections.points.is_empty() {
        for (number, point) in sections.points.iter().enumerate() {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(format!("{}", number + 1)).font(semibold(size * 0.9)).color(accent()));
                ui.vertical(|ui| rich_line(ui, point, size + 1.0, text()));
            });
        }
        ui.add_space(6.0);
    }
    let details = sections.has_details();
    if main_only || *details_open || sections.points.is_empty() {
        if !sections.answer.is_empty() {
            answer_text(ui, &sections.answer, size);
        }
        if !main_only && *details_open {
            heading_and_lines(ui, &t("key_terms"), &sections.key, size);
            if !sections.context.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(t("context")).font(semibold(size * 0.9)).color(muted()));
                rich_line(ui, &sections.context, size, text());
            }
            heading_and_lines(ui, &t("next_questions"), &sections.follow_ups, size);
        }
    }
    if !main_only && details && !sections.points.is_empty() {
        let label = t(if *details_open { "hide_details" } else { "details" });
        if ui.add(egui::Button::new(RichText::new(label).color(accent())).frame(false)).clicked() {
            *details_open = !*details_open;
        }
    }
}

fn heading_and_lines(ui: &mut Ui, heading: &str, lines: &[String], size: f32) {
    if lines.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(RichText::new(heading).font(semibold(size * 0.9)).color(muted()));
    for line in lines {
        rich_line(ui, line, size, text());
    }
}
