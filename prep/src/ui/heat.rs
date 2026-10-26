//! The calendar of practice: a column for each week and a square for each day, lit by how much
//! you did, like the contribution graph on GitHub, in the colours of the program. It says at a
//! glance whether you came in this week and how many days in a row.

use egui::{Align2, Color32, CornerRadius, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};

use super::Core;
use super::theme::{self, BERRY, BLUSH, CRANBERRY, FAINT, LINE, MUTED, SNOW, SURFACE};
use crate::i18n::{K, Tr};
use crate::stats::{Cell, duration_label};

const CELL: f32 = 12.0;
const GAP: f32 = 3.0;
const PITCH: f32 = CELL + GAP;
/// Room at the left for the names of the days.
const LABELS: f32 = 30.0;
const MONTHS: f32 = 18.0;

/// The colour of a level, 0 (a day with nothing) to 4 (twice the goal).
pub fn level_color(level: u8) -> Color32 {
    match level {
        0 => theme::mix(SURFACE, LINE, 0.6),
        1 => theme::mix(LINE, CRANBERRY, 0.6),
        2 => CRANBERRY,
        3 => BERRY,
        _ => theme::mix(BERRY, BLUSH, 0.6),
    }
}

fn month_name(tr: Tr, month: u32) -> &'static str {
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const RU: [&str; 12] = ["янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];
    let names = if tr.is_russian() { RU } else { EN };
    names[(month.clamp(1, 12) - 1) as usize]
}

fn weekday_names(tr: Tr) -> [&'static str; 3] {
    if tr.is_russian() { ["Пн", "Ср", "Пт"] } else { ["Mon", "Wed", "Fri"] }
}

/// "25 Oct" or "25 окт" from `YYYY-MM-DD`.
fn short_date(tr: Tr, date: &str) -> String {
    let day = date.get(8..10).and_then(|d| d.parse::<u32>().ok()).unwrap_or(0);
    let month = date.get(5..7).and_then(|m| m.parse::<u32>().ok()).unwrap_or(1);
    format!("{day} {}", month_name(tr, month))
}

fn tooltip(tr: Tr, cell: &Cell) -> String {
    let date = short_date(tr, &cell.date);
    if cell.level == 0 {
        return tr.fill(K::HeatNone, &[("date", &date)]);
    }
    let time = duration_label(cell.day.seconds, tr.get(K::UnitHour), tr.get(K::UnitMinute), tr.get(K::UnitSecond));
    let mut text = format!("{date}: {time}");
    if cell.day.focus_blocks > 0 {
        text.push_str(&format!(" · {} {}", cell.day.focus_blocks, tr.get(K::UsageFocusBlocks).to_lowercase()));
    }
    text
}

/// How many weeks fit in `width`.
pub fn weeks_for(width: f32) -> usize {
    (((width - LABELS) / PITCH).floor() as usize).clamp(8, 53)
}

/// Paints the calendar across `width`: the months above, the days, then a line of words and the
/// legend under it.
pub fn calendar(core: &Core, ui: &mut Ui, width: f32) {
    let tr = core.tr;
    let now = core.now();
    let weeks = weeks_for(width);
    let calendar = core.stats.calendar(now, weeks, core.settings.daily_goal_minutes);
    let grid_width = LABELS + weeks as f32 * PITCH - GAP;
    let grid_height = MONTHS + 7.0 * PITCH - GAP;
    let (rect, response) = ui.allocate_exact_size(vec2(width.max(grid_width), grid_height), Sense::hover());
    let label = tr.get(K::HeatLabel);
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label));
    let left = rect.left() + LABELS;
    let top = rect.top() + MONTHS;
    let painter = ui.painter();
    for (column, month) in &calendar.months {
        painter.text(
            pos2(left + *column as f32 * PITCH, rect.top() + MONTHS / 2.0),
            Align2::LEFT_CENTER,
            month_name(tr, *month),
            theme::mono(11.0),
            FAINT,
        );
    }
    for (name, row) in weekday_names(tr).into_iter().zip([0, 2, 4]) {
        painter.text(
            pos2(rect.left(), top + row as f32 * PITCH + CELL / 2.0),
            Align2::LEFT_CENTER,
            name,
            theme::mono(11.0),
            FAINT,
        );
    }
    let mut hovered: Option<&Cell> = None;
    let pointer = response.hover_pos();
    for (column, days) in calendar.columns.iter().enumerate() {
        for (row, cell) in days.iter().enumerate() {
            let Some(cell) = cell else { continue };
            let square =
                Rect::from_min_size(pos2(left + column as f32 * PITCH, top + row as f32 * PITCH), vec2(CELL, CELL));
            painter.rect_filled(square, CornerRadius::same(3), level_color(cell.level));
            if cell.today {
                painter.rect_stroke(square, CornerRadius::same(3), Stroke::new(1.3, SNOW), StrokeKind::Outside);
            }
            if pointer.is_some_and(|at| square.expand(GAP / 2.0).contains(at)) {
                hovered = Some(cell);
            }
        }
    }
    if let Some(cell) = hovered {
        response.on_hover_text_at_pointer(tooltip(tr, cell));
    }

    // under the squares: what the picture says, and the key to its colours
    ui.add_space(6.0);
    let streak = core.stats.streak(now);
    let mut caption =
        tr.fill(K::HeatCaption, &[("days", &calendar.practised.to_string()), ("weeks", &weeks.to_string())]);
    if streak > 0 {
        caption.push_str(&format!(" · {}", tr.fill(K::BarStreak, &[("n", &streak.to_string())])));
    }
    ui.horizontal(|ui| {
        ui.add_space(LABELS);
        ui.label(egui::RichText::new(caption).font(theme::sans(12.5)).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(tr.get(K::HeatMore)).font(theme::sans(12.0)).color(FAINT));
            for level in (0..=4u8).rev() {
                let (swatch, _) = ui.allocate_exact_size(vec2(CELL, CELL), Sense::hover());
                ui.painter().rect_filled(swatch, CornerRadius::same(3), level_color(level));
            }
            ui.label(egui::RichText::new(tr.get(K::HeatLess)).font(theme::sans(12.0)).color(FAINT));
        });
    });
}
