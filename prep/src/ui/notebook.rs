//! Notes: the few lines the tutor keeps after each call. You read them here; a star keeps one at
//! the top, and you can delete one you do not want.

use egui::{Align, CornerRadius, Layout, RichText, Ui};

use super::theme::{self, BERRY, FAINT, MUTED, SNOW};
use super::widgets::{self, Icon, card_frame, ghost_button, icon_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::{K, Tr};
use crate::notes::{Note, NoteKind};
use crate::session::ago;

#[derive(Default)]
pub struct NotebookView {
    pub copied: bool,
}

pub fn kind_label(tr: Tr, kind: NoteKind) -> &'static str {
    tr.get(match kind {
        NoteKind::Vital => K::KindVital,
        NoteKind::Lecture => K::KindLecture,
        NoteKind::Mistake => K::KindMistake,
        NoteKind::Idea => K::KindIdea,
    })
}

enum Action {
    Star(String),
    Delete(String),
}

pub fn ui(core: &mut Core, view: &mut NotebookView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let now = core.now();
    let mut nav = None;
    let mut action: Option<Action> = None;
    let mut copy = false;
    scrolling_column(ui, 760.0, |ui| {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if ghost_button(ui, tr.get(K::BackHome)).clicked() {
                nav = Some(Nav::Home);
            }
            if !core.notebook.notes.is_empty() {
                let label = if view.copied { tr.get(K::NotesCopied) } else { tr.get(K::NotesCopy) };
                if ghost_button(ui, label).clicked() {
                    copy = true;
                }
            }
        });
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::NotesTitle));
        ui.label(RichText::new(tr.get(K::NotesIntro)).font(theme::sans(16.0)).color(MUTED));
        ui.add_space(14.0);
        let notes: Vec<&Note> = core.notebook.listing("");
        if notes.is_empty() {
            widgets::muted(ui, tr.get(K::NotesEmpty));
        }
        for note in notes {
            ui.add_space(4.0);
            card_frame().corner_radius(CornerRadius::same(14)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 90.0);
                        ui.label(RichText::new(&note.text).font(theme::sans(15.5)).color(SNOW));
                        ui.add_space(2.0);
                        let topic = if note.topic.is_empty() { String::new() } else { format!(" · {}", note.topic) };
                        ui.label(
                            RichText::new(format!(
                                "{}{topic} · {}",
                                kind_label(tr, note.kind).to_uppercase(),
                                ago(note.created, now)
                            ))
                            .font(theme::mono(11.0))
                            .color(FAINT),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if icon_button(ui, Icon::Trash, tr.get(K::NoteDelete), 32.0, None).clicked() {
                            action = Some(Action::Delete(note.id.clone()));
                        }
                        let (icon, label) = if note.starred {
                            (Icon::StarFilled, tr.get(K::NoteUnstar))
                        } else {
                            (Icon::Star, tr.get(K::NoteStar))
                        };
                        if icon_button(ui, icon, label, 32.0, note.starred.then_some(BERRY)).clicked() {
                            action = Some(Action::Star(note.id.clone()));
                        }
                    });
                });
            });
        }
        ui.add_space(40.0);
    });
    match action {
        Some(Action::Star(id)) => {
            core.notebook.toggle_star(&id);
            core.save_notes();
        }
        Some(Action::Delete(id)) => {
            core.notebook.remove(&id);
            core.save_notes();
        }
        None => {}
    }
    if copy {
        ui.ctx().copy_text(core.notebook.markdown());
        view.copied = true;
    }
    nav
}
