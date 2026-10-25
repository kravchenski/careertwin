//! Help: what each part is for and the few keys that matter, in a window over the current screen.

use egui::{Align2, Frame, Margin, RichText, Stroke, vec2};

use super::Core;
use super::theme::{self, LINE, MUTED, SNOW, SURFACE};
use super::widgets::{self, Icon, icon_button};
use crate::i18n::K;

pub fn show(core: &mut Core, ctx: &egui::Context) {
    if !core.help_open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        core.help_open = false;
        return;
    }
    let tr = core.tr;
    let mut close = false;
    egui::Window::new(tr.get(K::HelpTitle))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 10.0))
        .fixed_size(vec2(560.0, 0.0))
        .collapsible(false)
        .title_bar(false)
        .frame(
            Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, LINE)).corner_radius(22).inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr.get(K::HelpTitle)).font(theme::serif(30.0)).color(SNOW));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, tr.get(K::Close), 34.0, None).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            for text in [K::HelpTalk, K::HelpShow, K::HelpCode, K::HelpFocus] {
                ui.label(RichText::new(tr.get(text)).font(theme::sans(15.0)).color(MUTED));
                ui.add_space(6.0);
            }
            ui.add_space(8.0);
            widgets::caps(ui, tr.get(K::HelpShortcuts));
            ui.add_space(4.0);
            for (keys, what) in [
                ("Ctrl+Shift+,", K::HelpKeySettings),
                ("Space", K::HelpKeySkip),
                ("Ctrl+Shift+T", K::HelpKeyTimer),
                ("Esc", K::HelpKeyEscape),
            ] {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(keys).font(theme::mono_bold(13.0)).color(SNOW));
                    ui.add_space(8.0);
                    ui.label(RichText::new(tr.get(what)).font(theme::sans(14.5)).color(MUTED));
                });
            }
        });
    if close {
        core.help_open = false;
    }
}
