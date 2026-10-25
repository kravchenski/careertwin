//! Study together: bring a text, your notes, new words or a topic, and the tutor makes a lesson of
//! it. It goes through every item in your own words first, then again in English, and fixes your
//! grammar there.

use egui::{DroppedFile, RichText, TextEdit, Ui, vec2};

use super::theme::{self, BERRY, FAINT, MUTED};
use super::widgets::{self, ghost_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;

#[derive(Default)]
pub struct StudyView {
    pub material: String,
    pub working: bool,
    pub error: Option<String>,
}

/// The kinds of file that are read as they are.
const READABLE: [&str; 6] = ["txt", "md", "csv", "json", "srt", "text"];
const LARGEST_FILE: u64 = 400_000;

/// The text of a file dropped on the window, if it is a text file of a sensible size.
fn read_dropped(file: &dyn DroppedFile) -> Option<String> {
    let extension = file.path().extension()?.to_string_lossy().to_lowercase();
    if !READABLE.contains(&extension.as_str()) {
        return None;
    }
    if std::fs::metadata(file.path()).is_ok_and(|meta| meta.len() > LARGEST_FILE) {
        return None;
    }
    let bytes = file.bytes().ok()?;
    (bytes.len() as u64 <= LARGEST_FILE).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

pub fn ui(core: &mut Core, view: &mut StudyView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut go = false;
    for file in ui.ctx().input(|input| input.raw.dropped_files.clone()) {
        match read_dropped(file.as_ref()) {
            Some(text) => {
                if !view.material.trim().is_empty() {
                    view.material.push_str("\n\n");
                }
                view.material.push_str(text.trim());
                view.error = None;
            }
            None => view.error = Some(tr.get(K::StudyOnlyText).to_owned()),
        }
    }
    scrolling_column(ui, 760.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::StudyTitle));
        ui.label(RichText::new(tr.get(K::HomeLineStudy)).font(theme::sans(16.0)).color(MUTED));
        ui.add_space(12.0);
        ui.add(
            TextEdit::multiline(&mut view.material)
                .hint_text(tr.get(K::StudyHint))
                .desired_rows(10)
                .desired_width(f32::INFINITY)
                .margin(vec2(14.0, 12.0)),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(tr.get(K::StudyDrop)).font(theme::sans(13.5)).color(FAINT));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if primary_button(ui, tr.get(K::StudyStart), !view.working && !view.material.trim().is_empty()).clicked() {
                go = true;
            }
            if ghost_button(ui, tr.get(K::StudyTalk)).clicked() {
                nav = Some(Nav::QuickCall);
            }
            if view.working {
                ui.spinner();
                ui.label(RichText::new(tr.get(K::StudyBusy)).font(theme::serif_italic(17.0)).color(MUTED));
            }
        });
        if let Some(error) = &view.error {
            ui.add_space(6.0);
            ui.label(RichText::new(error).font(theme::sans(14.5)).color(BERRY));
        }
        ui.add_space(40.0);
    });
    if go {
        view.working = true;
        view.error = None;
        core.study(view.material.clone());
    }
    nav
}
