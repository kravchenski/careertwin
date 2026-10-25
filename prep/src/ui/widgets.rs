//! The small pieces every screen is made of: the pill of tabs, round icon buttons, chips, cards and
//! the icons themselves (drawn with lines, so there is no icon font to ship).

use std::collections::VecDeque;

use egui::epaint::PathShape;
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Pos2, Rect, Response, RichText, Role, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2, WidgetInfo, pos2, vec2,
};

use super::theme::{self, BERRY, BG, BLUSH, COCOA, CRANBERRY, FAINT, LINE, MUTED, RAISED, SNOW, SURFACE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Back,
    Mic,
    MicOff,
    Wave,
    Close,
    Plus,
    List,
    Sliders,
    Refresh,
    Play,
    Check,
    Send,
    Trash,
    Speaker,
    Search,
    Gear,
    Clock,
    Globe,
    Help,
    Exit,
    Chevron,
    Pause,
    Skip,
    Prev,
    Star,
    StarFilled,
}

pub(crate) fn arc(center: Pos2, radius: f32, from: f32, to: f32, steps: usize) -> Vec<Pos2> {
    (0..=steps)
        .map(|i| {
            let angle = from + (to - from) * i as f32 / steps as f32;
            pos2(center.x + radius * angle.cos(), center.y + radius * angle.sin())
        })
        .collect()
}

pub fn paint_icon(painter: &egui::Painter, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let stroke = Stroke::new((size / 11.0).clamp(1.3, 2.4), color);
    let s = size;
    let line = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], stroke);
    };
    let path = |points: Vec<Pos2>| {
        painter.add(Shape::line(points, stroke));
    };
    let at = |x: f32, y: f32| pos2(center.x + x * s, center.y + y * s);
    match icon {
        Icon::Back => path(vec![at(0.12, -0.34), at(-0.2, 0.0), at(0.12, 0.34)]),
        Icon::Close => {
            line(at(-0.26, -0.26), at(0.26, 0.26));
            line(at(0.26, -0.26), at(-0.26, 0.26));
        }
        Icon::Plus => {
            line(at(0.0, -0.3), at(0.0, 0.3));
            line(at(-0.3, 0.0), at(0.3, 0.0));
        }
        Icon::List => {
            for y in [-0.24, 0.0, 0.24] {
                line(at(-0.3, y), at(0.3, y));
            }
        }
        Icon::Mic | Icon::MicOff => {
            let body = Rect::from_center_size(at(0.0, -0.1), vec2(0.30 * s, 0.50 * s));
            painter.rect_stroke(body, CornerRadius::same((0.15 * s) as u8), stroke, StrokeKind::Middle);
            path(arc(at(0.0, -0.02), 0.27 * s, 0.0, std::f32::consts::PI, 14));
            line(at(0.0, 0.25), at(0.0, 0.40));
            line(at(-0.12, 0.40), at(0.12, 0.40));
            if icon == Icon::MicOff {
                painter.line_segment([at(-0.34, -0.34), at(0.34, 0.34)], Stroke::new(stroke.width + 0.4, BERRY));
            }
        }
        Icon::Wave => {
            for (i, h) in [0.18, 0.42, 0.62, 0.34, 0.2].into_iter().enumerate() {
                let x = -0.28 + 0.14 * i as f32;
                line(at(x, -h / 2.0), at(x, h / 2.0));
            }
        }
        Icon::Sliders => {
            for (y, knob) in [(-0.22, -0.12), (0.0, 0.14), (0.22, -0.02)] {
                line(at(-0.32, y), at(0.32, y));
                painter.circle_filled(at(knob, y), 0.075 * s, color);
            }
        }
        Icon::Refresh => {
            path(arc(center, 0.30 * s, -2.2, 3.4, 18));
            let tip = at(0.30 * (-2.2f32).cos(), 0.30 * (-2.2f32).sin());
            line(tip, tip + vec2(0.14 * s, -0.02 * s));
            line(tip, tip + vec2(0.02 * s, 0.14 * s));
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![at(-0.18, -0.28), at(0.30, 0.0), at(-0.18, 0.28)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Check => path(vec![at(-0.3, 0.02), at(-0.1, 0.22), at(0.32, -0.24)]),
        Icon::Send => {
            line(at(0.0, 0.3), at(0.0, -0.3));
            path(vec![at(-0.22, -0.08), at(0.0, -0.32), at(0.22, -0.08)]);
        }
        Icon::Trash => {
            line(at(-0.3, -0.2), at(0.3, -0.2));
            line(at(-0.1, -0.2), at(-0.1, -0.32));
            line(at(0.1, -0.2), at(0.1, -0.32));
            line(at(-0.1, -0.32), at(0.1, -0.32));
            path(vec![at(-0.22, -0.12), at(-0.17, 0.34), at(0.17, 0.34), at(0.22, -0.12)]);
        }
        Icon::Speaker => {
            painter.add(Shape::convex_polygon(
                vec![
                    at(-0.34, -0.12),
                    at(-0.16, -0.12),
                    at(0.04, -0.3),
                    at(0.04, 0.3),
                    at(-0.16, 0.12),
                    at(-0.34, 0.12),
                ],
                color,
                Stroke::NONE,
            ));
            path(arc(at(0.04, 0.0), 0.18 * s, -0.9, 0.9, 8));
            path(arc(at(0.04, 0.0), 0.32 * s, -0.9, 0.9, 10));
        }
        Icon::Search => {
            painter.circle_stroke(at(-0.06, -0.06), 0.22 * s, stroke);
            line(at(0.1, 0.1), at(0.32, 0.32));
        }
        Icon::Gear => {
            painter.circle_stroke(center, 0.12 * s, stroke);
            painter.circle_stroke(center, 0.26 * s, stroke);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let (sin, cos) = angle.sin_cos();
                line(at(0.26 * cos, 0.26 * sin), at(0.37 * cos, 0.37 * sin));
            }
        }
        Icon::Clock => {
            painter.circle_stroke(center, 0.32 * s, stroke);
            line(at(0.0, 0.0), at(0.0, -0.19));
            line(at(0.0, 0.0), at(0.14, 0.08));
        }
        Icon::Globe => {
            painter.circle_stroke(center, 0.32 * s, stroke);
            line(at(-0.32, 0.0), at(0.32, 0.0));
            let ellipse: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / 24.0;
                    at(0.15 * angle.cos(), 0.32 * angle.sin())
                })
                .collect();
            path(ellipse);
        }
        Icon::Help => {
            painter.circle_stroke(center, 0.32 * s, stroke);
            painter.text(center, Align2::CENTER_CENTER, "?", theme::semibold(s * 0.5), color);
        }
        Icon::Exit => {
            path(vec![at(0.0, -0.3), at(-0.3, -0.3), at(-0.3, 0.3), at(0.0, 0.3)]);
            line(at(-0.06, 0.0), at(0.34, 0.0));
            path(vec![at(0.2, -0.14), at(0.34, 0.0), at(0.2, 0.14)]);
        }
        Icon::Chevron => path(vec![at(-0.1, -0.22), at(0.12, 0.0), at(-0.1, 0.22)]),
        Icon::Pause => {
            let thick = Stroke::new(stroke.width * 1.7, color);
            painter.line_segment([at(-0.12, -0.24), at(-0.12, 0.24)], thick);
            painter.line_segment([at(0.12, -0.24), at(0.12, 0.24)], thick);
        }
        Icon::Skip => {
            painter.add(Shape::convex_polygon(
                vec![at(-0.26, -0.24), at(0.1, 0.0), at(-0.26, 0.24)],
                color,
                Stroke::NONE,
            ));
            painter.line_segment([at(0.24, -0.24), at(0.24, 0.24)], Stroke::new(stroke.width * 1.5, color));
        }
        Icon::Star | Icon::StarFilled => {
            let points: Vec<Pos2> = (0..10)
                .map(|i| {
                    let radius = if i % 2 == 0 { 0.36 } else { 0.15 };
                    let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    at(radius * angle.cos(), radius * angle.sin())
                })
                .collect();
            if icon == Icon::StarFilled {
                // a star is not convex, so it is filled as ten triangles from its middle
                for i in 0..10 {
                    painter.add(Shape::convex_polygon(
                        vec![center, points[i], points[(i + 1) % 10]],
                        color,
                        Stroke::NONE,
                    ));
                }
            }
            let mut outline = points.clone();
            outline.push(points[0]);
            path(outline);
        }
        Icon::Prev => {
            painter.add(Shape::convex_polygon(
                vec![at(0.26, -0.24), at(-0.1, 0.0), at(0.26, 0.24)],
                color,
                Stroke::NONE,
            ));
            painter.line_segment([at(-0.24, -0.24), at(-0.24, 0.24)], Stroke::new(stroke.width * 1.5, color));
        }
    }
}

/// A small capital label in the monospace font, for headings of a block.
pub fn caps(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text.to_uppercase()).font(theme::mono(11.5)).color(FAINT))
}

pub fn muted(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED));
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::heading(34.0)).color(SNOW));
}

pub fn card_frame() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
}

pub fn pointer(response: Response) -> Response {
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn pill_widths(ui: &Ui, labels: &[&str]) -> Vec<f32> {
    let font = theme::semibold(14.5);
    labels
        .iter()
        .map(|label| ui.painter().layout_no_wrap((*label).to_owned(), font.clone(), SNOW).size().x + 44.0)
        .collect()
}

/// How wide the pill of tabs is, so it can be centred.
pub fn pill_width(ui: &Ui, labels: &[&str]) -> f32 {
    pill_widths(ui, labels).iter().sum::<f32>() + 8.0
}

/// The pill of tabs. Returns the index that was clicked, if any.
pub fn pill_tabs(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let font = theme::semibold(14.5);
    let height = 38.0;
    let padding = 4.0;
    let widths = pill_widths(ui, labels);
    let total = widths.iter().sum::<f32>() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, height + padding * 2.0), Sense::hover());
    let radius = CornerRadius::same(((height + padding * 2.0) / 2.0) as u8);
    ui.painter().rect(rect, radius, SURFACE, Stroke::new(1.0, LINE), StrokeKind::Inside);
    let mut clicked = None;
    let mut x = rect.left() + padding;
    for (index, label) in labels.iter().enumerate() {
        let segment = Rect::from_min_size(pos2(x, rect.top() + padding), vec2(widths[index], height));
        x += widths[index];
        let response = pointer(ui.interact(segment, ui.scope_id().with(("tab", index)), Sense::click()));
        response.widget_info(|| WidgetInfo::selected(Role::Tab, true, index == selected, *label));
        let active = index == selected;
        if active {
            ui.painter().rect(
                segment,
                CornerRadius::same((height / 2.0) as u8),
                RAISED,
                Stroke::new(1.0, theme::mix(LINE, BERRY, 0.35)),
                StrokeKind::Inside,
            );
        }
        let color = if active || response.hovered() { SNOW } else { MUTED };
        ui.painter().text(segment.center(), Align2::CENTER_CENTER, label, font.clone(), color);
        if response.clicked() && !active {
            clicked = Some(index);
        }
    }
    clicked
}

/// A round button with an icon.
pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str, diameter: f32, tint: Option<Color32>) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    let response = pointer(response).on_hover_text(label);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), label));
    let hovered = response.hovered();
    let fill =
        tint.map_or(if hovered { RAISED } else { COCOA }, |t| if hovered { theme::mix(t, BLUSH, 0.12) } else { t });
    let stroke = if tint.is_some() { Stroke::NONE } else { Stroke::new(1.0, if hovered { BERRY } else { LINE }) };
    ui.painter().circle(rect.center(), diameter / 2.0, fill, stroke);
    paint_icon(ui.painter(), icon, rect.center(), diameter * 0.52, SNOW);
    response
}

/// Whether the person clicked somewhere outside `rect` in this frame.
pub fn clicked_outside(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|at| !rect.contains(at)))
}

/// An icon with nothing behind it, for the way out beside something that matters more.
pub fn quiet_icon(ui: &mut Ui, icon: Icon, label: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let response = pointer(response).on_hover_text(label);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), label));
    let color = if response.hovered() { SNOW } else { MUTED };
    paint_icon(ui.painter(), icon, rect.center(), size * 0.5, color);
    response
}

/// The main action: a berry pill.
pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let font = theme::semibold(15.5);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), SNOW);
    let size = vec2(galley.size().x + 52.0, 46.0);
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let response = if enabled { pointer(response) } else { response };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, text));
    let fill = if !enabled {
        RAISED
    } else if response.is_pointer_button_down_on() {
        CRANBERRY
    } else if response.hovered() {
        theme::mix(BERRY, BLUSH, 0.14)
    } else {
        BERRY
    };
    ui.painter().rect_filled(rect, CornerRadius::same(23), fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, if enabled { SNOW } else { FAINT });
    response
}

/// A quiet button with an outline.
pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    let font = theme::semibold(14.5);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), SNOW);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 36.0, 38.0), Sense::click());
    let response = pointer(response);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), text));
    let hovered = response.hovered();
    ui.painter().rect(
        rect,
        CornerRadius::same(19),
        if hovered { RAISED } else { Color32::TRANSPARENT },
        Stroke::new(1.0, if hovered { BERRY } else { LINE }),
        StrokeKind::Inside,
    );
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, SNOW);
    response
}

/// A choice among a few, shown as a rounded tag.
pub fn chip(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let font = theme::semibold(13.5);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), SNOW);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 28.0, 32.0), Sense::click());
    let response = pointer(response);
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, selected, text));
    let hovered = response.hovered();
    let (fill, stroke, color) = if selected {
        (CRANBERRY, BERRY, SNOW)
    } else {
        (if hovered { RAISED } else { COCOA }, if hovered { BERRY } else { LINE }, if hovered { SNOW } else { MUTED })
    };
    ui.painter().rect(rect, CornerRadius::same(16), fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    response
}

/// A big clickable card with a tag, a title and a line of explanation.
pub fn option_card(ui: &mut Ui, width: f32, tag: &str, title_text: &str, text: &str) -> Response {
    let wrap = width - 40.0;
    let title_galley = ui.painter().layout(title_text.to_owned(), theme::semibold(18.0), SNOW, wrap);
    let text_galley = ui.painter().layout(text.to_owned(), theme::sans(14.5), MUTED, wrap);
    let height = 20.0 + 16.0 + title_galley.size().y + 6.0 + text_galley.size().y + 20.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height.max(140.0)), Sense::click());
    let response = pointer(response);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, title_text));
    let hovered = response.hovered();
    ui.painter().rect(
        rect,
        CornerRadius::same(18),
        if hovered { RAISED } else { SURFACE },
        Stroke::new(1.0, if hovered { BERRY } else { LINE }),
        StrokeKind::Inside,
    );
    let mut y = rect.top() + 18.0;
    ui.painter().text(
        pos2(rect.left() + 20.0, y),
        Align2::LEFT_TOP,
        tag.to_uppercase(),
        theme::mono(11.0),
        if hovered { BERRY } else { FAINT },
    );
    y += 24.0;
    let title_height = title_galley.size().y;
    ui.painter().galley(pos2(rect.left() + 20.0, y), title_galley, SNOW);
    y += title_height + 6.0;
    ui.painter().galley(pos2(rect.left() + 20.0, y), text_galley, MUTED);
    response
}

/// A row of bars that follows the loudness of the voice, newest on the right.
pub fn level_bars(ui: &mut Ui, size: Vec2, levels: &VecDeque<f32>, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let step = 6.0;
    let count = (size.x / step) as usize;
    let skip = levels.len().saturating_sub(count);
    let center = rect.center().y;
    for (i, level) in levels.iter().skip(skip).enumerate() {
        let x = rect.right() - (levels.len() - skip - i) as f32 * step;
        let half = (level.clamp(0.0, 1.0) * size.y / 2.0).max(1.5);
        let faded = theme::alpha(color, 0.35 + 0.65 * (i as f32 / count.max(1) as f32));
        ui.painter().line_segment([pos2(x, center - half), pos2(x, center + half)], Stroke::new(2.4, faded));
    }
}

/// A thin progress line, for the time left or the question number.
pub fn progress(ui: &mut Ui, width: f32, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), LINE);
    let filled = Rect::from_min_size(rect.min, vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()));
    ui.painter().rect_filled(filled, CornerRadius::same(2), BERRY);
}

/// Five segments, filled up to `value`.
pub fn score_bar(ui: &mut Ui, value: u8) {
    let (rect, _) = ui.allocate_exact_size(vec2(150.0, 10.0), Sense::hover());
    let gap = 4.0;
    let each = (rect.width() - gap * 4.0) / 5.0;
    for i in 0..5 {
        let x = rect.left() + i as f32 * (each + gap);
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(each, rect.height()));
        ui.painter().rect_filled(cell, CornerRadius::same(3), if i < usize::from(value) { BERRY } else { LINE });
    }
}

/// The window colour behind everything, for the screens that paint their own background.
pub fn window_fill() -> Color32 {
    BG
}

pub fn path_shape(points: Vec<Pos2>, stroke: Stroke) -> Shape {
    Shape::Path(PathShape::line(points, stroke))
}
