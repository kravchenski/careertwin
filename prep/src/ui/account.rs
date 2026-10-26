//! The strip along the bottom: a panel with your name and address at the left, how today is going
//! at the right (it opens Your activity), and a menu that opens upwards from the panel with
//! Settings, Your activity, Language, Help and Quit.

use egui::{
    Align2, Color32, CornerRadius, Frame, Id, Margin, Order, Pos2, Rect, RichText, Role, Sense, Shape, Stroke,
    StrokeKind, Ui, WidgetInfo, pos2, vec2,
};

use super::theme::{self, BERRY, CRANBERRY, FAINT, LINE, MUTED, RAISED, SNOW, SURFACE};
use super::widgets::{Icon, chip, paint_icon, pointer};
use super::{Core, rule};
use crate::config::UiLanguage;
use crate::i18n::K;

const MENU_WIDTH: f32 = 300.0;
const CHIP_HEIGHT: f32 = 46.0;
const CHIP_MIN_WIDTH: f32 = 150.0;
const CHIP_MAX_WIDTH: f32 = 380.0;
/// The height of the strip along the bottom that holds the panel.
pub const BAR_HEIGHT: f32 = 64.0;

fn initial(core: &Core) -> String {
    let name = core.display_name();
    let source = if name.is_empty() { core.display_email() } else { name };
    source.chars().find(|c| c.is_alphanumeric()).map_or_else(|| "?".to_owned(), |c| c.to_uppercase().to_string())
}

/// The strip along the bottom: the panel with your name at the left, and how today is going at
/// the right. The menu that opens from the panel is [`menu`].
pub fn bar(core: &mut Core, ui: &mut Ui) {
    let tr = core.tr;
    egui::Panel::bottom("account")
        .exact_size(BAR_HEIGHT)
        .frame(Frame::new().fill(theme::BG).inner_margin(Margin::symmetric(16, 9)))
        .show(ui, |ui| {
            let full = ui.max_rect();
            let name = short_name(&core.display_name());
            let email = core.display_email();
            let headline = if name.is_empty() { email.clone() } else { name.clone() };
            let headline = if headline.is_empty() { tr.get(K::AppName).to_owned() } else { headline };
            let subline = if name.is_empty() { String::new() } else { email };
            let letter = initial(core);
            let head_font = theme::display(15.0);
            let sub_font = theme::mono(11.0);
            // as wide as the words need, up to a limit
            let words = text_width(ui, &headline, &head_font).max(text_width(ui, &subline, &sub_font));
            let width = (CHIP_HEIGHT + 2.0 + words + 18.0).clamp(CHIP_MIN_WIDTH, CHIP_MAX_WIDTH);
            let rect =
                Rect::from_min_size(pos2(full.left(), full.center().y - CHIP_HEIGHT / 2.0), vec2(width, CHIP_HEIGHT));
            let response = pointer(ui.interact(rect, ui.scope_id().with("account-chip"), Sense::click()));
            response.widget_info(|| WidgetInfo::labeled(Role::Button, true, tr.get(K::AccountMenu)));
            core.account_chip = rect;
            let hovered = response.hovered() || core.account_open;
            let painter = ui.painter();
            // at rest there is only the letter and the words; the outline appears when it is a button you can press
            if hovered {
                painter.rect(
                    rect,
                    CornerRadius::same(23),
                    RAISED,
                    Stroke::new(1.0, theme::mix(LINE, BERRY, 0.4)),
                    StrokeKind::Inside,
                );
            }
            let center = pos2(rect.left() + CHIP_HEIGHT / 2.0, rect.center().y);
            painter.circle_filled(center, 16.0, CRANBERRY);
            painter.text(center, Align2::CENTER_CENTER, &letter, theme::bold(15.0), SNOW);
            let x = rect.left() + CHIP_HEIGHT + 2.0;
            let room = rect.right() - x - 12.0;
            let top = if subline.is_empty() { rect.center().y } else { rect.center().y - 8.0 };
            let headline = elide(ui, &headline, head_font.clone(), room);
            ui.painter().text(pos2(x, top), Align2::LEFT_CENTER, headline, head_font, SNOW);
            if !subline.is_empty() {
                let subline = elide(ui, &subline, sub_font.clone(), room);
                ui.painter().text(pos2(x, rect.center().y + 9.0), Align2::LEFT_CENTER, subline, sub_font, FAINT);
            }
            if response.clicked() {
                core.account_open = !core.account_open;
                core.language_open = false;
            }
            today(core, ui, full);
        });
}

/// "Maksim Krauchyk" from "Maksim Krauchyk (Polish citizen)": what stands in brackets is for a
/// profile page, not for a name tag.
fn short_name(name: &str) -> String {
    name.split(" (").next().unwrap_or(name).trim().to_owned()
}

fn text_width(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font.clone(), SNOW).size().x
}

/// A ring that fills as the day's goal is met.
fn ring(painter: &egui::Painter, center: Pos2, radius: f32, fraction: f32) {
    let width = 3.0;
    painter.circle_stroke(center, radius, Stroke::new(width, LINE));
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return;
    }
    let steps = ((fraction * 48.0).ceil() as usize).max(2);
    let points: Vec<Pos2> = (0..=steps)
        .map(|step| {
            let angle = -std::f32::consts::FRAC_PI_2 + fraction * std::f32::consts::TAU * step as f32 / steps as f32;
            center + radius * vec2(angle.cos(), angle.sin())
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(width, BERRY)));
}

/// How much of today's goal is done, at the right of the strip. It is also the way into Your
/// activity: the calendar and the numbers behind this one.
fn today(core: &mut Core, ui: &mut Ui, full: Rect) {
    let tr = core.tr;
    let now = core.now();
    let done = core.stats.today(now).seconds;
    let goal = core.settings.daily_goal_minutes.max(1);
    let streak = core.stats.streak(now);
    let text = tr.fill(K::BarToday, &[("done", &(done / 60).to_string()), ("goal", &goal.to_string())]);
    let streak_text = (streak > 0).then(|| tr.fill(K::BarStreak, &[("n", &streak.to_string())]));
    let text_font = theme::display(13.0);
    let streak_font = theme::display(12.0);
    let words =
        text_width(ui, &text, &text_font).max(streak_text.as_deref().map_or(0.0, |t| text_width(ui, t, &streak_font)));
    let ring_size = 30.0;
    let width = 12.0 + ring_size + 10.0 + words + 14.0;
    let rect =
        Rect::from_min_size(pos2(full.right() - width, full.center().y - CHIP_HEIGHT / 2.0), vec2(width, CHIP_HEIGHT));
    let response = pointer(ui.interact(rect, ui.scope_id().with("activity-chip"), Sense::click()));
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &text));
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect(
            rect,
            CornerRadius::same(23),
            RAISED,
            Stroke::new(1.0, theme::mix(LINE, BERRY, 0.4)),
            StrokeKind::Inside,
        );
    }
    let fraction = done as f32 / (goal * 60) as f32;
    ring(painter, pos2(rect.left() + 12.0 + ring_size / 2.0, rect.center().y), ring_size / 2.0 - 2.0, fraction);
    let x = rect.left() + 12.0 + ring_size + 10.0;
    match streak_text {
        Some(streak_text) => {
            painter.text(pos2(x, rect.center().y - 8.0), Align2::LEFT_CENTER, text, text_font, MUTED);
            painter.text(pos2(x, rect.center().y + 9.0), Align2::LEFT_CENTER, streak_text, streak_font, BERRY);
        }
        None => {
            painter.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, text, text_font, MUTED);
        }
    }
    if response.clicked() {
        core.usage_open = true;
        core.account_open = false;
        core.language_open = false;
    }
}

/// The menu that opens upwards from the panel while it is open.
pub fn menu(core: &mut Core, ctx: &egui::Context) {
    let tr = core.tr;
    if core.account_open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        core.account_open = false;
    }
    if !core.account_open {
        return;
    }
    let name = core.display_name();
    let email = core.display_email();
    let who = if !email.is_empty() {
        email
    } else if !name.is_empty() {
        name
    } else {
        tr.get(K::AppName).to_owned()
    };
    let chip_rect = core.account_chip;
    let mut menu_rect = Rect::NOTHING;
    let mut close = false;
    egui::Area::new(Id::unique("account-menu"))
        .order(Order::Foreground)
        .anchor(Align2::LEFT_BOTTOM, vec2(16.0, -(BAR_HEIGHT + 4.0)))
        .show(ctx, |ui| {
            let inner = Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, LINE))
                .corner_radius(CornerRadius::same(18))
                .inner_margin(Margin::same(10))
                .shadow(egui::Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_black_alpha(150) })
                .show(ui, |ui| {
                    ui.set_width(MENU_WIDTH);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add_space(4.0);
                    ui.label(RichText::new(who).font(theme::sans(14.0)).color(MUTED));
                    ui.add_space(6.0);
                    rule(ui);
                    ui.add_space(4.0);
                    if row(ui, Icon::Gear, tr.get(K::SettingsTitle), Trailing::Hint("Ctrl+Shift+,")).clicked() {
                        core.settings_open = true;
                        close = true;
                    }
                    if row(ui, Icon::Clock, tr.get(K::MenuUsage), Trailing::None).clicked() {
                        core.usage_open = true;
                        close = true;
                    }
                    let language = match core.settings.ui_language {
                        UiLanguage::Auto => tr.get(K::Automatic),
                        UiLanguage::En => "English",
                        UiLanguage::Ru => "Русский",
                    };
                    if row(ui, Icon::Globe, tr.get(K::MenuLanguage), Trailing::Value(language)).clicked() {
                        core.language_open = !core.language_open;
                    }
                    if core.language_open {
                        ui.horizontal(|ui| {
                            ui.add_space(34.0);
                            for (choice, label) in [
                                (UiLanguage::Auto, tr.get(K::Automatic)),
                                (UiLanguage::En, "English"),
                                (UiLanguage::Ru, "Русский"),
                            ] {
                                if chip(ui, label, core.settings.ui_language == choice).clicked() {
                                    let mut settings = core.settings.clone();
                                    settings.ui_language = choice;
                                    if let Err(error) = core.apply_settings(settings) {
                                        core.notify(error.to_string());
                                    }
                                }
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if row(ui, Icon::Help, tr.get(K::MenuHelp), Trailing::None).clicked() {
                        core.help_open = true;
                        close = true;
                    }
                    ui.add_space(4.0);
                    rule(ui);
                    ui.add_space(4.0);
                    if row(ui, Icon::Exit, tr.get(K::MenuQuit), Trailing::None).clicked() {
                        core.quit = true;
                        close = true;
                    }
                });
            menu_rect = inner.response.rect;
        });
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_click()
            && i.pointer.interact_pos().is_some_and(|at| !menu_rect.contains(at) && !chip_rect.contains(at))
    });
    if close || clicked_outside {
        core.account_open = false;
        core.language_open = false;
    }
}

/// `text` shortened with an ellipsis so it fits in `room`.
fn elide(ui: &Ui, text: &str, font: egui::FontId, room: f32) -> String {
    let width = |s: &str| ui.painter().layout_no_wrap(s.to_owned(), font.clone(), SNOW).size().x;
    if width(text) <= room {
        return text.to_owned();
    }
    let mut kept: String = text.to_owned();
    while !kept.is_empty() && width(&format!("{kept}…")) > room {
        kept.pop();
    }
    format!("{kept}…")
}

enum Trailing<'a> {
    None,
    /// A keyboard shortcut, in small type.
    Hint(&'a str),
    /// The current value of a choice, with an arrow after it.
    Value(&'a str),
}

/// One line of the menu: an icon, a label and, at the right, a hint or the current choice.
fn row(ui: &mut Ui, icon: Icon, label: &str, trailing: Trailing<'_>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let response = pointer(response);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(10), RAISED);
    }
    paint_icon(painter, icon, pos2(rect.left() + 22.0, rect.center().y), 20.0, SNOW);
    painter.text(pos2(rect.left() + 46.0, rect.center().y), Align2::LEFT_CENTER, label, theme::semibold(15.0), SNOW);
    match trailing {
        Trailing::None => {}
        Trailing::Hint(hint) => {
            painter.text(
                pos2(rect.right() - 12.0, rect.center().y),
                Align2::RIGHT_CENTER,
                hint,
                theme::mono(11.5),
                FAINT,
            );
        }
        Trailing::Value(value) => {
            paint_icon(painter, Icon::Chevron, pos2(rect.right() - 16.0, rect.center().y), 16.0, FAINT);
            painter.text(
                pos2(rect.right() - 30.0, rect.center().y),
                Align2::RIGHT_CENTER,
                value,
                theme::sans(13.5),
                MUTED,
            );
        }
    }
    response
}
