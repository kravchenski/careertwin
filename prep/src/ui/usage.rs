//! Usage: how long you have practised today, over the week and in total, and the streak that
//! keeps you coming back. It is a window over the current screen, like Settings.

use egui::{Align2, CornerRadius, Frame, Margin, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Core;
use super::theme::{self, BERRY, CRANBERRY, FAINT, LINE, MUTED, SNOW, SURFACE};
use super::widgets::{self, Icon, icon_button};
use crate::i18n::{K, Tr};
use crate::stats::duration_label;

fn length(tr: Tr, seconds: u32) -> String {
    duration_label(seconds, tr.get(K::UnitHour), tr.get(K::UnitMinute), tr.get(K::UnitSecond))
}

fn tile(ui: &mut Ui, width: f32, caption: &str, value: &str) {
    Frame::new()
        .fill(theme::mix(SURFACE, theme::COCOA, 0.5))
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.vertical(|ui| {
                widgets::caps(ui, caption);
                ui.label(RichText::new(value).font(theme::serif(26.0)).color(SNOW));
            });
        });
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
    let week = core.stats.last_days(now, 7);
    let streak = core.stats.streak(now);
    let goal = core.settings.daily_goal_minutes.max(1) * 60;
    let mut close = false;
    egui::Window::new(tr.get(K::UsageTitle))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 10.0))
        .fixed_size(vec2(560.0, 0.0))
        .collapsible(false)
        .title_bar(false)
        .frame(
            Frame::new().fill(SURFACE).stroke(Stroke::new(1.0, LINE)).corner_radius(22).inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr.get(K::UsageTitle)).font(theme::serif(30.0)).color(SNOW));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, tr.get(K::Close), 34.0, None).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            widgets::caps(ui, tr.get(K::UsageToday));
            ui.horizontal(|ui| {
                ui.label(RichText::new(length(tr, today.seconds)).font(theme::serif(34.0)).color(SNOW));
                ui.label(
                    RichText::new(tr.fill(K::UsageGoal, &[("n", &(goal / 60).to_string())]))
                        .font(theme::mono(12.5))
                        .color(MUTED),
                );
            });
            widgets::progress(ui, ui.available_width(), today.seconds as f32 / goal as f32);
            if total.seconds == 0 && total.focus_blocks == 0 {
                ui.add_space(6.0);
                widgets::muted(ui, tr.get(K::UsageNothing));
            }

            ui.add_space(16.0);
            widgets::caps(ui, tr.get(K::UsageWeek));
            week_bars(ui, &week, now);

            ui.add_space(14.0);
            let each = (ui.available_width() - 20.0) / 3.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                tile(ui, each, tr.get(K::UsageStreak), &streak.to_string());
                tile(ui, each, tr.get(K::UsageSessions), &total.sessions.to_string());
                tile(
                    ui,
                    each,
                    tr.get(K::UsageFocusBlocks),
                    &format!("{} / {}", today.focus_blocks, total.focus_blocks),
                );
            });
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                tile(ui, each, tr.get(K::UsagePractised), &length(tr, total.seconds));
                tile(ui, each, tr.get(K::UsageWords), &total.words.to_string());
                tile(ui, each, tr.get(K::UsageWeek), &length(tr, core.stats.week_seconds(now)));
            });
        });
    if close {
        core.usage_open = false;
    }
}

/// Seven bars, the oldest first and today at the right, with the day of the month under each.
fn week_bars(ui: &mut Ui, days: &[(String, u32)], now: u64) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::hover());
    let most = days.iter().map(|(_, s)| *s).max().unwrap_or(0).max(15 * 60) as f32;
    let slot = rect.width() / days.len().max(1) as f32;
    let plot_height = rect.height() - 20.0;
    let today = crate::session::date(now);
    for (index, (day, seconds)) in days.iter().enumerate() {
        let x = rect.left() + slot * (index as f32 + 0.5);
        let height = (*seconds as f32 / most * plot_height).max(3.0);
        let bar = egui::Rect::from_min_max(
            pos2(x - slot * 0.28, rect.top() + plot_height - height),
            pos2(x + slot * 0.28, rect.top() + plot_height),
        );
        let is_today = *day == today;
        let fill = if *seconds == 0 {
            LINE
        } else if is_today {
            BERRY
        } else {
            CRANBERRY
        };
        ui.painter().rect(bar, CornerRadius::same(5), fill, Stroke::NONE, StrokeKind::Inside);
        let label = day.rsplit('-').next().unwrap_or_default().trim_start_matches('0');
        ui.painter().text(
            pos2(x, rect.bottom() - 6.0),
            Align2::CENTER_CENTER,
            label,
            theme::mono(11.0),
            if is_today { SNOW } else { FAINT },
        );
    }
}
