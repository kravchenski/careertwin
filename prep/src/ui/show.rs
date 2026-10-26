//! "Explain it with pictures" on its own: name a topic and watch it told as an animated story.
//! The same stage is used inside a call, when the tutor draws something.

use egui::{RichText, TextEdit, Ui, vec2};

use super::stage::{self, Outcome, Picture};
use super::theme::{self, MUTED};
use super::widgets::{self, chip, ghost_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;

#[derive(Default)]
pub struct ShowView {
    /// What is typed in the box.
    pub topic: String,
    /// Background for the model, such as the term the picture is for.
    pub hint: String,
    pub stage: Option<Picture>,
}

impl ShowView {
    /// Starts at once on `topic`.
    pub fn start(core: &mut Core, topic: &str, hint: &str) -> Self {
        let stage = core.begin_stage(topic, hint, false);
        Self { topic: topic.to_owned(), hint: hint.to_owned(), stage: Some(stage) }
    }
}

/// Topics to try: the terms of the plan, or a few that everybody meets.
fn suggestions(core: &Core) -> Vec<String> {
    let mut names: Vec<String> = core
        .syllabus
        .as_ref()
        .map(|plan| {
            plan.study_order().into_iter().flat_map(|i| plan.topics[i].terms.iter().map(|t| t.name.clone())).collect()
        })
        .unwrap_or_default();
    names.truncate(5);
    if names.is_empty() {
        names = ["How the internet works", "What an API is", "Git branches", "Big O in plain words"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
    }
    names
}

pub fn ui(core: &mut Core, view: &mut ShowView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    if let Some(mut live) = view.stage.take() {
        let area = ui.available_rect_before_wrap().shrink2(vec2(28.0, 8.0));
        match stage::paint(core, &mut live, ui, area) {
            Outcome::Closed | Outcome::Finished => {
                core.services.speaker.stop();
            }
            Outcome::Simpler => {
                core.services.speaker.stop();
                view.stage = Some(core.begin_stage(&live.topic, &live.context, true));
            }
            Outcome::Nothing => view.stage = Some(live),
        }
        return None;
    }
    let mut nav = None;
    let mut go: Option<String> = None;
    scrolling_column(ui, 720.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::ShowTitle));
        ui.label(RichText::new(tr.get(K::ShowIntro)).font(theme::sans(16.0)).color(MUTED));
        ui.add_space(14.0);
        let label = widgets::caps(ui, tr.get(K::ShowHint));
        let edit = ui
            .add(TextEdit::singleline(&mut view.topic).desired_width(f32::INFINITY).margin(vec2(12.0, 10.0)))
            .labelled_by(label.id);
        let submit = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(8.0);
        let ready = !view.topic.trim().is_empty();
        if (primary_button(ui, tr.get(K::ShowGo), ready).clicked() || submit) && ready {
            go = Some(view.topic.clone());
        }
        ui.add_space(18.0);
        widgets::caps(ui, tr.get(K::ShowTry));
        ui.horizontal_wrapped(|ui| {
            for name in suggestions(core) {
                if chip(ui, &name, false).clicked() {
                    go = Some(name);
                }
            }
        });
    });
    if let Some(topic) = go {
        view.topic = topic.clone();
        view.stage = Some(core.begin_stage(&topic, &view.hint, false));
        ui.ctx().request_repaint();
    }
    nav
}
