//! The controls of the top bar and the foot of the window: pills that show the key that does the same, a round
//! button to pause listening, the mark of the menu, the keys to move through the answers.
//!
//! They are drawn here because the window's fonts have no arrow or key glyphs (an empty box would show in their
//! place), and each one is still a button with a name, for the keyboard and for a screen reader.

use egui::{Color32, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use super::theme::{accent, mix, muted, on_accent, panel, sans, semibold, strong, text};

/// A key as it is drawn in a small box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cap {
    Word(String),
    Enter,
    Left,
    Right,
}

/// What the key of commands is called here: Ctrl, or Cmd on a Mac.
pub fn command_word() -> &'static str {
    if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" }
}

/// The keys of an in-window shortcut: the command key, the extra modifiers, and the key itself.
pub fn shortcut(extra: &[&str], key: Cap) -> Vec<Cap> {
    let mut caps = vec![Cap::Word(command_word().to_owned())];
    caps.extend(extra.iter().map(|word| Cap::Word((*word).to_owned())));
    caps.push(key);
    caps
}

/// A key written as `Ctrl+Shift+Enter` (the global screenshot key) as keys to draw.
pub fn caps_of(keys: &str) -> Vec<Cap> {
    keys.split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_lowercase().as_str() {
            "enter" | "return" => Cap::Enter,
            "left" => Cap::Left,
            "right" => Cap::Right,
            _ => Cap::Word(part.to_owned()),
        })
        .collect()
}

const PILL_HEIGHT: f32 = 34.0;
const CAP_HEIGHT: f32 = 18.0;

fn word_width(ui: &Ui, word: &str) -> f32 {
    ui.painter().layout_no_wrap(word.to_owned(), sans(11.0), text()).size().x + 10.0
}

fn cap_width(ui: &Ui, cap: &Cap) -> f32 {
    match cap {
        Cap::Word(word) => word_width(ui, word),
        _ => 20.0,
    }
}

fn caps_width(ui: &Ui, caps: &[Cap]) -> f32 {
    if caps.is_empty() {
        return 0.0;
    }
    caps.iter().map(|cap| cap_width(ui, cap)).sum::<f32>() + 3.0 * (caps.len() - 1) as f32
}

fn paint_cap(ui: &Ui, rect: Rect, cap: &Cap, color: Color32) {
    let painter = ui.painter();
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, color), StrokeKind::Inside);
    let c = rect.center();
    let line = Stroke::new(1.3, color);
    match cap {
        Cap::Word(word) => {
            painter.text(c, egui::Align2::CENTER_CENTER, word, sans(11.0), color);
        }
        Cap::Enter => {
            let (a, b, d) = (c + vec2(4.0, -4.0), c + vec2(4.0, 2.0), c + vec2(-4.0, 2.0));
            painter.line_segment([a, b], line);
            painter.line_segment([b, d], line);
            painter.line_segment([d, d + vec2(3.0, -3.0)], line);
            painter.line_segment([d, d + vec2(3.0, 3.0)], line);
        }
        Cap::Left => {
            painter.line_segment([c + vec2(4.5, 0.0), c + vec2(-4.5, 0.0)], line);
            painter.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.5, -3.0)], line);
            painter.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.0)], line);
        }
        Cap::Right => {
            painter.line_segment([c + vec2(-4.5, 0.0), c + vec2(4.5, 0.0)], line);
            painter.line_segment([c + vec2(4.5, 0.0), c + vec2(1.5, -3.0)], line);
            painter.line_segment([c + vec2(4.5, 0.0), c + vec2(1.5, 3.0)], line);
        }
    }
}

fn paint_caps(ui: &Ui, mut x: f32, y: f32, caps: &[Cap], color: Color32) {
    for cap in caps {
        let width = cap_width(ui, cap);
        paint_cap(ui, Rect::from_min_size(Pos2::new(x, y - CAP_HEIGHT / 2.0), vec2(width, CAP_HEIGHT)), cap, color);
        x += width + 3.0;
    }
}

/// How wide `pill` will be, so the bar can centre a pair of them.
pub fn pill_width(ui: &Ui, label: &str, caps: &[Cap], show_caps: bool) -> f32 {
    let label = ui.painter().layout_no_wrap(label.to_owned(), semibold(14.0), text()).size().x;
    let keys = if show_caps && !caps.is_empty() { 8.0 + caps_width(ui, caps) } else { 0.0 };
    (16.0 + label + keys + 12.0).max(72.0)
}

fn edge(ui: &Ui, enabled: bool, hot: bool) -> Color32 {
    let _ = ui;
    if !enabled {
        mix(panel(), text(), 0.22)
    } else if hot {
        accent()
    } else {
        mix(panel(), strong(), 0.5)
    }
}

/// A button shaped as a pill, with the keys that do the same beside its name.
pub fn pill(ui: &mut Ui, label: &str, caps: &[Cap], show_caps: bool, enabled: bool, tooltip: &str) -> Response {
    let width = pill_width(ui, label, caps, show_caps);
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, PILL_HEIGHT), if enabled { Sense::click() } else { Sense::hover() });
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    let hot = enabled && response.hovered();
    let painter = ui.painter();
    if enabled && response.is_pointer_button_down_on() {
        painter.rect_filled(rect, PILL_HEIGHT / 2.0, mix(panel(), accent(), 0.22));
    } else if hot {
        painter.rect_filled(rect, PILL_HEIGHT / 2.0, mix(panel(), accent(), 0.10));
    }
    painter.rect_stroke(rect, PILL_HEIGHT / 2.0, Stroke::new(1.2, edge(ui, enabled, hot)), StrokeKind::Inside);
    let color = if enabled { strong() } else { muted() };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), semibold(14.0), color);
    let label_width = galley.size().x;
    ui.painter().galley(Pos2::new(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0), galley, color);
    if show_caps && !caps.is_empty() {
        let cap_color = if enabled { mix(panel(), text(), 0.7) } else { mix(panel(), text(), 0.3) };
        paint_caps(ui, rect.left() + 16.0 + label_width + 8.0, rect.center().y, caps, cap_color);
    }
    if tooltip.is_empty() { response } else { response.on_hover_text(tooltip) }
}

/// What the round button shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Pause,
    Play,
}

/// The round button at the left of the bar: filled while it can be paused, an outline while it would start or resume.
pub fn round_button(ui: &mut Ui, glyph: Glyph, name: &str) -> Response {
    let size = 40.0;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name));
    let hot = response.hovered();
    let filled = glyph == Glyph::Pause;
    let painter = ui.painter();
    let c = rect.center();
    if filled {
        painter.circle_filled(c, size / 2.0, if hot { mix(accent(), strong(), 0.18) } else { accent() });
    } else {
        if hot {
            painter.circle_filled(c, size / 2.0, mix(panel(), accent(), 0.14));
        }
        painter.circle_stroke(c, size / 2.0 - 0.6, Stroke::new(1.4, accent()));
    }
    let ink = if filled { on_accent() } else { accent() };
    match glyph {
        Glyph::Pause => {
            for dx in [-4.5, 4.5] {
                painter.rect_filled(Rect::from_center_size(c + vec2(dx, 0.0), vec2(4.0, 14.0)), 1.5, ink);
            }
        }
        Glyph::Play => {
            let points = vec![c + vec2(-4.0, -7.5), c + vec2(-4.0, 7.5), c + vec2(8.0, 0.0)];
            painter.add(egui::Shape::convex_polygon(points, ink, Stroke::NONE));
        }
    }
    response.on_hover_text(name)
}

/// The three lines that open the menu.
pub fn menu_mark(ui: &mut Ui, name: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(40.0, PILL_HEIGHT), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name));
    let color = if response.hovered() { strong() } else { muted() };
    let c = rect.center();
    for dy in [-5.0, 0.0, 5.0] {
        ui.painter().line_segment([c + vec2(-8.0, dy), c + vec2(8.0, dy)], Stroke::new(1.8, color));
    }
    response.on_hover_text(name)
}

/// How wide `counter` will be.
pub fn counter_width(ui: &Ui, text_: &str) -> f32 {
    ui.painter().layout_no_wrap(text_.to_owned(), sans(12.0), text()).size().x + 28.0
}

/// A small pill with `2/5`: which answer is shown out of how many.
pub fn counter(ui: &mut Ui, shown: &str) -> Response {
    let width = counter_width(ui, shown);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    ui.painter().rect_stroke(rect, 13.0, Stroke::new(1.0, mix(panel(), text(), 0.3)), StrokeKind::Inside);
    // a label of its own, so a screen reader reads it
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
        |ui| ui.label(egui::RichText::new(shown).font(sans(12.0)).color(muted())),
    )
    .inner
}

/// How wide `step` will be.
pub fn step_width(ui: &Ui, caps: &[Cap]) -> f32 {
    caps_width(ui, caps) + 8.0
}

/// The keys that move through the answers, drawn as keys and pressed as a button.
pub fn step(ui: &mut Ui, caps: &[Cap], enabled: bool, name: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(step_width(ui, caps), PILL_HEIGHT),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
    let color = if !enabled {
        mix(panel(), text(), 0.2)
    } else if response.hovered() {
        accent()
    } else {
        mix(panel(), text(), 0.55)
    };
    paint_caps(ui, rect.left() + 4.0, rect.center().y, caps, color);
    if enabled { response.on_hover_text(name) } else { response }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_key_becomes_the_keys_to_draw() {
        assert_eq!(caps_of("Ctrl+Shift+Enter"), [Cap::Word("Ctrl".into()), Cap::Word("Shift".into()), Cap::Enter]);
        assert_eq!(caps_of("Alt + Left"), [Cap::Word("Alt".into()), Cap::Left]);
        assert!(caps_of("").is_empty());
    }

    #[test]
    fn a_shortcut_starts_with_the_command_key_of_the_system() {
        let keys = shortcut(&["Shift"], Cap::Enter);
        assert_eq!(keys[0], Cap::Word(command_word().to_owned()));
        assert_eq!((keys.len(), keys[2].clone()), (3, Cap::Enter));
    }
}
