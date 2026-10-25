//! Usage: how long you have practised today and over the months, and the streak that keeps you
//! coming back. It is a window over the current screen, like Settings.

use egui::{Align, Align2, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};

use super::Core;
use super::heat;
use super::theme::{self, LINE, MUTED, SNOW, SURFACE};
use super::widgets::{self, Icon, icon_button};
use crate::i18n::{K, Tr};
use crate::stats::duration_label;

fn length(tr: Tr, seconds: u32) -> String {
    duration_label(seconds, tr.get(K::UnitHour), tr.get(K::UnitMinute), tr.get(K::UnitSecond))
}

/// A line of the list: what it is at the left, the number at the right.
fn line(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).font(theme::sans(15.0)).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).font(theme::semibold(16.0)).color(SNOW));
        });
    });
    super::rule(ui);
}

pub fn show(core: &mut Core, ctx: &egui::Context) {
    if !core.usage_open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        core.usage_open = false;
        return;
    }
    let tr = core.tr;
    let now = core.now();
    let today = core.stats.today(now);
    let total = core.stats.totals();
    let streak = core.stats.streak(now);
    let goal = core.settings.daily_goal_minutes.max(1) * 60;
    let mut close = false;
    egui::Window::new(tr.get(K::UsageTitle))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 10.0))
        .fixed_size(vec2(600.0, 0.0))
        .collapsible(false)
        .title_bar(false)
        .frame(
            Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, LINE)).corner_radius(22).inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr.get(K::UsageTitle)).font(theme::heading(30.0)).color(SNOW));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, tr.get(K::Close), 34.0, None).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(length(tr, today.seconds)).font(theme::heading(34.0)).color(SNOW));
                ui.label(
                    RichText::new(tr.fill(K::UsageGoal, &[("n", &(goal / 60).to_string())]))
                        .font(theme::sans(14.0))
                        .color(MUTED),
                );
            });
            widgets::progress(ui, ui.available_width(), today.seconds as f32 / goal as f32);
            if total.seconds == 0 && total.focus_blocks == 0 {
                ui.add_space(6.0);
                widgets::muted(ui, tr.get(K::UsageNothing));
            }
            ui.add_space(18.0);
            heat::calendar(core, ui, ui.available_width());
            ui.add_space(16.0);
            let week = length(tr, core.stats.week_seconds(now));
            let blocks = format!("{} / {}", today.focus_blocks, total.focus_blocks);
            ui.columns(2, |columns| {
                line(&mut columns[0], tr.get(K::UsageStreak), &streak.to_string());
                line(&mut columns[0], tr.get(K::UsageSessions), &total.sessions.to_string());
                line(&mut columns[0], tr.get(K::UsageFocusBlocks), &blocks);
                line(&mut columns[1], tr.get(K::UsagePractised), &length(tr, total.seconds));
                line(&mut columns[1], tr.get(K::UsageWeek), &week);
                line(&mut columns[1], tr.get(K::UsageWords), &total.words.to_string());
            });
        });
    if close {
        core.usage_open = false;
    }
}
