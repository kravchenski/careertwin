//! The pictures of a visual explanation: panels that float and glow around the sphere, painted
//! with lines and text only, so they look the same everywhere and can be checked in a test.
//!
//! Each kind of panel paints itself from three numbers: how far it has appeared (0 to 1), how far
//! through its scene the narration is (so items come in as they are spoken) and the time (for
//! the float and the pulses).

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

use super::theme::{self, BERRY, BLUSH, COCOA, CRANBERRY, FAINT, LINE, MUTED, SNOW, SURFACE, alpha, mix};
use crate::explain::{Panel, PanelKind, item_visibility};

/// What a panel needs to know about the moment it is painted at.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    /// How far the panel has appeared, 0 to 1.
    pub appear: f32,
    /// How far into the scene the narration is, 0 to 1.
    pub progress: f32,
    /// Seconds, any origin, for the pulses.
    pub time: f32,
    /// Items that were already there in the last scene, shown at once.
    pub base: usize,
}

/// Where each of `count` panels goes inside `area`, with a gap between them.
pub fn layout(count: usize, area: Rect) -> Vec<Rect> {
    const GAP: f32 = 16.0;
    let (w, h) = (area.width(), area.height());
    let cell =
        |x: f32, y: f32, cw: f32, ch: f32| Rect::from_min_size(pos2(area.left() + x, area.top() + y), vec2(cw, ch));
    match count {
        0 => Vec::new(),
        1 => {
            let (cw, ch) = (w.min(760.0), h.min(420.0));
            vec![cell((w - cw) / 2.0, (h - ch) / 2.0, cw, ch)]
        }
        2 => {
            let cw = (w - GAP) / 2.0;
            let ch = h.min(360.0);
            let y = (h - ch) / 2.0;
            vec![cell(0.0, y, cw, ch), cell(cw + GAP, y, cw, ch)]
        }
        3 => {
            let big = (w - GAP) * 0.56;
            let small = w - GAP - big;
            let half = (h - GAP) / 2.0;
            vec![cell(0.0, 0.0, big, h), cell(big + GAP, 0.0, small, half), cell(big + GAP, half + GAP, small, half)]
        }
        _ => {
            let cw = (w - GAP) / 2.0;
            let ch = (h - GAP) / 2.0;
            vec![
                cell(0.0, 0.0, cw, ch),
                cell(cw + GAP, 0.0, cw, ch),
                cell(0.0, ch + GAP, cw, ch),
                cell(cw + GAP, ch + GAP, cw, ch),
            ]
        }
    }
}

/// Ease out: fast at first, then settling.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

/// How big the text should be for a panel of this size, as a factor of the base size.
fn scale_for(rect: Rect) -> f32 {
    (rect.width() / 340.0).min(rect.height() / 200.0).clamp(0.72, 1.4)
}

fn serif(size: f32) -> FontId {
    theme::serif(size)
}

/// The glowing frame, the title and the faint scan line.
fn frame(painter: &Painter, rect: Rect, clock: &Clock, title: &str, scale: f32) {
    let a = clock.appear.clamp(0.0, 1.0);
    let radius = CornerRadius::same(20);
    for (grow, strength) in [(14.0, 0.035), (9.0, 0.06), (5.0, 0.10), (2.0, 0.16)] {
        painter.rect_stroke(
            rect.expand(grow / 2.0),
            CornerRadius::same(20 + (grow / 2.0) as u8),
            Stroke::new(grow, alpha(BERRY, strength * a)),
            StrokeKind::Outside,
        );
    }
    painter.rect(
        rect,
        radius,
        alpha(theme::mix(COCOA, SURFACE, 0.4), 0.80 * a),
        Stroke::new(1.8, alpha(mix(BERRY, BLUSH, 0.30), a)),
        StrokeKind::Inside,
    );
    // a line of light drifting down the glass
    let travel = (clock.time * 0.18).fract();
    let y = rect.top() + 16.0 + travel * (rect.height() - 32.0);
    painter.line_segment(
        [pos2(rect.left() + 14.0, y), pos2(rect.right() - 14.0, y)],
        Stroke::new(1.0, alpha(BLUSH, 0.07 * a)),
    );
    // the corners, like the edges of a lens
    let tick = 14.0;
    let color = alpha(BLUSH, 0.65 * a);
    for (corner, dx, dy) in [
        (rect.left_top(), 1.0, 1.0),
        (rect.right_top(), -1.0, 1.0),
        (rect.left_bottom(), 1.0, -1.0),
        (rect.right_bottom(), -1.0, -1.0),
    ] {
        let c = corner + vec2(dx * 6.0, dy * 6.0);
        painter.line_segment([c, c + vec2(dx * tick, 0.0)], Stroke::new(1.6, color));
        painter.line_segment([c, c + vec2(0.0, dy * tick)], Stroke::new(1.6, color));
    }
    if !title.is_empty() {
        let at = pos2(rect.left() + 20.0 * scale, rect.top() + 16.0 * scale);
        painter.circle_filled(at + vec2(0.0, 7.0 * scale), 3.2 * scale, alpha(BERRY, a));
        painter.text(
            at + vec2(12.0 * scale, 0.0),
            Align2::LEFT_TOP,
            title.to_uppercase(),
            theme::mono_bold(11.5 * scale),
            alpha(mix(BLUSH, BERRY, 0.4), a),
        );
    }
}

/// The room inside the frame under the title.
fn content(rect: Rect, scale: f32, titled: bool) -> Rect {
    let top = if titled { 42.0 * scale } else { 18.0 * scale };
    Rect::from_min_max(
        pos2(rect.left() + 22.0 * scale, rect.top() + top),
        pos2(rect.right() - 22.0 * scale, rect.bottom() - 18.0 * scale),
    )
}

/// Paints one panel in `rect`.
pub fn paint_panel(painter: &Painter, rect: Rect, panel: &Panel, clock: &Clock) {
    let scale = scale_for(rect);
    frame(painter, rect, clock, &panel.title, scale);
    let inner = content(rect, scale, !panel.title.is_empty());
    let painter = painter.with_clip_rect(rect.shrink(2.0));
    match panel.kind {
        PanelKind::Text => text(&painter, inner, panel, clock, scale),
        PanelKind::Flow => flow(&painter, inner, panel, clock, scale),
        PanelKind::Cells => cells(&painter, inner, panel, clock, scale),
        PanelKind::Code => code(&painter, inner, panel, clock, scale),
        PanelKind::Compare => compare(&painter, inner, panel, clock, scale),
        PanelKind::Steps => steps(&painter, inner, panel, clock, scale),
        PanelKind::Analogy => analogy(&painter, inner, panel, clock, scale),
        PanelKind::Layers => layers(&painter, inner, panel, clock, scale),
    }
}

fn vis(panel: &Panel, clock: &Clock, index: usize) -> f32 {
    ease(item_visibility(clock.progress, index, clock.base, panel.count())) * clock.appear.clamp(0.0, 1.0)
}

/// Text laid out in `width`, drawn at `pos` with `anchor`; returns the size it took.
fn words(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> egui::Vec2 {
    let galley = painter.layout(text.to_owned(), font, color, width);
    let size = galley.size();
    let rect = anchor.anchor_size(pos, size);
    painter.galley(rect.min, galley, color);
    size
}

fn text(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let mut y = area.top();
    let font = theme::sans(18.0 * scale);
    for (index, line) in panel.lines.iter().enumerate() {
        let v = vis(panel, clock, index);
        let x = area.left() + (1.0 - v) * 16.0;
        painter.circle_filled(pos2(x + 4.0 * scale, y + 11.0 * scale), 3.0 * scale, alpha(BERRY, v));
        let size = words(
            painter,
            pos2(x + 18.0 * scale, y),
            Align2::LEFT_TOP,
            line,
            font.clone(),
            alpha(SNOW, v),
            area.width() - 18.0 * scale,
        );
        y += size.y + 12.0 * scale;
    }
}

/// A rounded box with centred words; `lit` makes it glow.
fn node(painter: &Painter, rect: Rect, label: &str, font: FontId, v: f32, lit: f32, time: f32) {
    if lit > 0.0 {
        let pulse = 0.5 + 0.5 * (time * 4.0).sin();
        for (grow, strength) in [(10.0, 0.05), (6.0, 0.10), (3.0, 0.18)] {
            painter.rect_stroke(
                rect.expand(grow / 2.0),
                CornerRadius::same(14 + (grow / 2.0) as u8),
                Stroke::new(grow, alpha(BERRY, strength * lit * (0.6 + 0.4 * pulse) * v)),
                StrokeKind::Outside,
            );
        }
    }
    let fill = mix(SURFACE, CRANBERRY, 0.25 + 0.55 * lit);
    painter.rect(
        rect,
        CornerRadius::same(14),
        alpha(fill, 0.92 * v),
        Stroke::new(1.4 + lit, alpha(mix(BERRY, BLUSH, 0.25 + 0.4 * lit), v)),
        StrokeKind::Inside,
    );
    words(painter, rect.center(), Align2::CENTER_CENTER, label, font, alpha(SNOW, v), rect.width() - 14.0);
}

fn arrow(painter: &Painter, from: Pos2, to: Pos2, v: f32) {
    let color = alpha(mix(BERRY, BLUSH, 0.3), v);
    let tip = from + (to - from) * v.clamp(0.0, 1.0);
    painter.line_segment([from, tip], Stroke::new(2.0, color));
    if v > 0.85 {
        let direction = (to - from).normalized();
        let side = vec2(-direction.y, direction.x);
        painter.line_segment([to, to - direction * 9.0 + side * 5.0], Stroke::new(2.0, color));
        painter.line_segment([to, to - direction * 9.0 - side * 5.0], Stroke::new(2.0, color));
    }
}

fn flow(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let count = panel.items.len().max(1);
    let per_row = if count <= 4 || area.width() > 120.0 * count as f32 { count } else { count.div_ceil(2) };
    let rows = count.div_ceil(per_row);
    let gap = 34.0 * scale;
    let box_w = ((area.width() - gap * (per_row as f32 - 1.0)) / per_row as f32).clamp(60.0, 190.0);
    let box_h = (58.0 * scale).min(area.height() / rows as f32 - 10.0).max(40.0);
    let row_h = box_h + 28.0 * scale;
    let total_h = row_h * rows as f32 - 28.0 * scale;
    let top = area.top() + ((area.height() - total_h) / 2.0).max(0.0);
    let font = theme::semibold(16.0 * scale);
    let rects: Vec<Rect> = (0..panel.items.len())
        .map(|i| {
            let (row, col) = (i / per_row, i % per_row);
            let in_row = per_row.min(panel.items.len() - row * per_row);
            let row_w = in_row as f32 * box_w + (in_row as f32 - 1.0) * gap;
            let left = area.center().x - row_w / 2.0 + col as f32 * (box_w + gap);
            Rect::from_min_size(pos2(left, top + row as f32 * row_h), vec2(box_w, box_h))
        })
        .collect();
    for (i, label) in panel.items.iter().enumerate() {
        let v = vis(panel, clock, i);
        let lit = if panel.active == Some(i) { 1.0 } else { 0.0 };
        let lift = (1.0 - v) * 10.0;
        node(painter, rects[i].translate(vec2(0.0, lift)), label, font.clone(), v, lit, clock.time);
        if i > 0 && i % per_row != 0 {
            let (a, b) = (rects[i - 1], rects[i]);
            arrow(painter, pos2(a.right() + 4.0, a.center().y), pos2(b.left() - 4.0, b.center().y), v);
        }
    }
}

fn cells(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let count = panel.items.len().max(1);
    let gap = 8.0 * scale;
    let size = ((area.width() - gap * (count as f32 - 1.0)) / count as f32).clamp(34.0, 86.0 * scale);
    let row_w = size * count as f32 + gap * (count as f32 - 1.0);
    let note_h = if panel.note.is_empty() { 0.0 } else { 34.0 * scale };
    let top = area.top() + ((area.height() - size - 26.0 * scale - note_h) / 2.0).max(0.0);
    for (i, label) in panel.items.iter().enumerate() {
        let v = vis(panel, clock, i);
        let left = area.center().x - row_w / 2.0 + i as f32 * (size + gap);
        let rect = Rect::from_min_size(pos2(left, top + (1.0 - v) * 12.0), vec2(size, size));
        let lit = if panel.highlight.contains(&i) { 1.0 } else { 0.0 };
        let font =
            if label.chars().count() > 4 { theme::semibold(14.0 * scale) } else { theme::semibold(20.0 * scale) };
        node(painter, rect, label, font, v, lit, clock.time);
        painter.text(
            pos2(rect.center().x, rect.bottom() + 12.0 * scale),
            Align2::CENTER_CENTER,
            i.to_string(),
            theme::mono(11.5 * scale),
            alpha(FAINT, v),
        );
    }
    if !panel.note.is_empty() {
        let v = ease((clock.progress - 0.55) / 0.3) * clock.appear;
        words(
            painter,
            pos2(area.center().x, top + size + 38.0 * scale),
            Align2::CENTER_TOP,
            &panel.note,
            theme::mono(14.0 * scale),
            alpha(BLUSH, v),
            area.width(),
        );
    }
}

fn code(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let line_h = 26.0 * scale;
    let height = (line_h * panel.lines.len() as f32 + 16.0).min(area.height());
    let sheet = Rect::from_min_size(area.min, vec2(area.width(), height));
    painter.rect_filled(sheet, CornerRadius::same(10), alpha(theme::mix(theme::BG, COCOA, 0.6), 0.9 * clock.appear));
    let font = theme::mono(15.0 * scale);
    for (i, line) in panel.lines.iter().enumerate() {
        let v = vis(panel, clock, i);
        let y = sheet.top() + 8.0 + i as f32 * line_h;
        let row = Rect::from_min_size(pos2(sheet.left() + 4.0, y), vec2(sheet.width() - 8.0, line_h));
        if panel.highlight.contains(&i) {
            let pulse = 0.75 + 0.25 * (clock.time * 3.0).sin();
            painter.rect_filled(row, CornerRadius::same(6), alpha(BERRY, 0.26 * v * pulse));
            painter.rect_filled(
                Rect::from_min_size(row.min, vec2(3.0, row.height())),
                CornerRadius::same(2),
                alpha(BLUSH, v),
            );
        }
        painter.text(
            pos2(sheet.left() + 14.0, y + line_h / 2.0),
            Align2::LEFT_CENTER,
            (i + 1).to_string(),
            theme::mono(12.0 * scale),
            alpha(FAINT, v),
        );
        painter.text(
            pos2(sheet.left() + 40.0 * scale, y + line_h / 2.0),
            Align2::LEFT_CENTER,
            line,
            font.clone(),
            alpha(SNOW, v),
        );
    }
}

fn compare(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let half = area.width() / 2.0 - 12.0;
    let columns = [(&panel.left, area.left()), (&panel.right, area.left() + half + 24.0)];
    painter.line_segment(
        [pos2(area.center().x, area.top()), pos2(area.center().x, area.bottom())],
        Stroke::new(1.2, alpha(BERRY, 0.5 * clock.appear)),
    );
    for (column, (side, left)) in columns.into_iter().enumerate() {
        let v = vis(panel, clock, 0);
        let mut y = area.top();
        let size = words(
            painter,
            pos2(left, y),
            Align2::LEFT_TOP,
            &side.title,
            serif(21.0 * scale),
            alpha(if column == 0 { BLUSH } else { SNOW }, v),
            half,
        );
        y += size.y + 10.0 * scale;
        for (i, line) in side.lines.iter().enumerate() {
            let v = vis(panel, clock, i + 1);
            painter.circle_filled(pos2(left + 4.0, y + 10.0 * scale), 2.8 * scale, alpha(BERRY, v));
            let size = words(
                painter,
                pos2(left + 16.0 * scale, y),
                Align2::LEFT_TOP,
                line,
                theme::sans(16.0 * scale),
                alpha(SNOW, v),
                half - 16.0,
            );
            y += size.y + 8.0 * scale;
        }
    }
}

fn steps(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let count = panel.lines.len().max(1);
    let current = panel.active.unwrap_or_else(|| ((clock.progress * count as f32) as usize).min(count - 1));
    let radius = 13.0 * scale;
    let step_h = ((area.height() / count as f32).min(52.0 * scale)).max(radius * 2.0 + 6.0);
    for (i, line) in panel.lines.iter().enumerate() {
        let v = vis(panel, clock, i);
        let center = pos2(area.left() + radius, area.top() + radius + i as f32 * step_h);
        if i + 1 < count {
            let next = center + vec2(0.0, step_h);
            painter.line_segment(
                [center + vec2(0.0, radius), next - vec2(0.0, radius)],
                Stroke::new(1.5, alpha(if i < current { BERRY } else { LINE }, v)),
            );
        }
        let (fill, text_color) = if i == current {
            (BERRY, SNOW)
        } else if i < current {
            (CRANBERRY, MUTED)
        } else {
            (SURFACE, FAINT)
        };
        if i == current {
            let pulse = 0.5 + 0.5 * (clock.time * 4.0).sin();
            painter.circle_stroke(center, radius + 3.0, Stroke::new(3.0, alpha(BERRY, 0.25 * pulse * v)));
        }
        painter.circle(center, radius, alpha(fill, v), Stroke::new(1.4, alpha(BLUSH, 0.6 * v)));
        painter.text(
            center,
            Align2::CENTER_CENTER,
            (i + 1).to_string(),
            theme::mono_bold(13.0 * scale),
            alpha(SNOW, v),
        );
        words(
            painter,
            pos2(center.x + radius + 14.0, center.y),
            Align2::LEFT_CENTER,
            line,
            if i == current { theme::semibold(17.0 * scale) } else { theme::sans(16.0 * scale) },
            alpha(text_color, v),
            area.width() - radius * 2.0 - 14.0,
        );
    }
}

fn analogy(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let box_w = (area.width() - 90.0 * scale) / 2.0;
    let box_h = (area.height() * 0.8).clamp(60.0, 150.0 * scale);
    let top = area.top() + (area.height() - box_h) / 2.0;
    let left = Rect::from_min_size(pos2(area.left(), top), vec2(box_w, box_h));
    let right = Rect::from_min_size(pos2(area.right() - box_w, top), vec2(box_w, box_h));
    let (v0, v1) = (vis(panel, clock, 0), vis(panel, clock, 1));
    let font = serif(19.0 * scale);
    node(painter, left, panel.lines.first().map_or("", String::as_str), font.clone(), v0, 0.0, clock.time);
    node(painter, right, panel.lines.get(1).map_or("", String::as_str), font, v1, v1, clock.time);
    // a dashed link that "travels" between them
    let y = left.center().y;
    let (from, to) = (pos2(left.right() + 8.0, y), pos2(right.left() - 8.0, y));
    let reach = from + (to - from) * v1;
    painter.add(Shape::dashed_line(&[from, reach], Stroke::new(2.0, alpha(BLUSH, 0.8 * v1)), 7.0, 6.0));
    painter.text(
        pos2((from.x + to.x) / 2.0, y - 16.0),
        Align2::CENTER_CENTER,
        "≈",
        theme::semibold(22.0 * scale),
        alpha(BLUSH, v1),
    );
}

fn layers(painter: &Painter, area: Rect, panel: &Panel, clock: &Clock, scale: f32) {
    let count = panel.lines.len().max(1);
    let gap = 8.0 * scale;
    let height = ((area.height() - gap * (count as f32 - 1.0)) / count as f32).clamp(28.0, 60.0 * scale);
    let total = height * count as f32 + gap * (count as f32 - 1.0);
    let top = area.top() + ((area.height() - total) / 2.0).max(0.0);
    for (i, line) in panel.lines.iter().enumerate() {
        let v = vis(panel, clock, i);
        let shade = if count > 1 { i as f32 / (count - 1) as f32 } else { 0.0 };
        let rect = Rect::from_min_size(
            pos2(area.left() + (1.0 - v) * 24.0, top + i as f32 * (height + gap)),
            vec2(area.width(), height),
        );
        let lit = if panel.active == Some(i) { 1.0 } else { 0.0 };
        painter.rect(
            rect,
            CornerRadius::same(12),
            alpha(mix(CRANBERRY, BERRY, shade * 0.7 + 0.2 * lit), 0.55 * v),
            Stroke::new(1.3, alpha(mix(BERRY, BLUSH, 0.35 + 0.3 * lit), v)),
            StrokeKind::Inside,
        );
        words(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            line,
            theme::semibold(16.5 * scale),
            alpha(SNOW, v),
            rect.width() - 20.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain::Side;

    fn area() -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(900.0, 420.0))
    }

    #[test]
    fn the_panels_fit_the_stage_and_do_not_overlap() {
        let area = area();
        for count in 0..=4 {
            let rects = layout(count, area);
            assert_eq!(rects.len(), count);
            for rect in &rects {
                assert!(area.expand(0.5).contains_rect(*rect), "{rect:?} outside {area:?}");
                assert!(rect.width() > 150.0 && rect.height() > 100.0, "{rect:?} is too small to read");
            }
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(!a.shrink(1.0).intersects(*b), "{a:?} overlaps {b:?} with {count}");
                }
            }
        }
        assert_eq!(layout(9, area).len(), 4, "more than four are laid out as four");
    }

    #[test]
    fn easing_runs_from_zero_to_one() {
        assert_eq!(ease(-1.0), 0.0);
        assert_eq!(ease(2.0), 1.0);
        assert!(ease(0.3) > 0.3, "it is quick at the start");
        assert!(ease(0.5) < ease(0.6));
    }

    #[test]
    fn every_kind_of_panel_paints_without_trouble_at_any_size_and_moment() {
        let ctx = egui::Context::default();
        let panels = vec![
            Panel {
                kind: PanelKind::Text,
                title: "Facts".into(),
                lines: vec!["one".into(), "two".into()],
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Flow,
                title: "Path".into(),
                items: (0..7).map(|i| format!("step {i}")).collect(),
                active: Some(2),
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Cells,
                title: "Cells".into(),
                items: (0..8).map(|i| i.to_string()).collect(),
                highlight: vec![3],
                note: "i = hash % 8".into(),
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Code,
                title: "Python".into(),
                lines: vec!["d = {}".into(), "d['a'] = 1".into()],
                highlight: vec![1],
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Compare,
                title: "List vs set".into(),
                left: Side { title: "List".into(), lines: vec!["ordered".into()] },
                right: Side { title: "Set".into(), lines: vec!["unique".into(), "fast".into()] },
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Steps,
                title: "Steps".into(),
                lines: vec!["a".into(), "b".into(), "c".into()],
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Analogy,
                title: "Like".into(),
                lines: vec!["A post office".into(), "A message queue".into()],
                ..Panel::default()
            },
            Panel {
                kind: PanelKind::Layers,
                title: "Layers".into(),
                lines: vec!["UI".into(), "API".into(), "DB".into()],
                ..Panel::default()
            },
            Panel { kind: PanelKind::Flow, ..Panel::default() },
            Panel { kind: PanelKind::Text, ..Panel::default() },
        ];
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for panel in &panels {
                for size in [vec2(200.0, 130.0), vec2(420.0, 260.0), vec2(760.0, 420.0)] {
                    for progress in [0.0, 0.3, 1.0] {
                        for appear in [0.0, 0.5, 1.0] {
                            let clock = Clock { appear, progress, time: 3.7, base: 1 };
                            let rect = Rect::from_min_size(pos2(0.0, 0.0), size);
                            paint_panel(ui.painter(), rect, panel, &clock);
                        }
                    }
                }
            }
        });
        output.textures_delta.clear();
    }
}
