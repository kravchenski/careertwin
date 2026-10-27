//! How the window looks: the dark panel and the single lavender accent of the Python overlay, one font.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle};

pub const PANEL: Color32 = Color32::from_rgb(19, 20, 30);
pub const TEXT: Color32 = Color32::from_rgb(215, 218, 232);
pub const STRONG: Color32 = Color32::WHITE;
pub const MUTED: Color32 = Color32::from_rgb(142, 146, 164);
pub const ACCENT: Color32 = Color32::from_rgb(188, 180, 255);
pub const ON_ACCENT: Color32 = Color32::from_rgb(25, 22, 48);
pub const WARN: Color32 = Color32::from_rgb(255, 207, 122);
pub const DANGER: Color32 = Color32::from_rgb(255, 154, 154);
pub const GOOD: Color32 = Color32::from_rgb(143, 191, 154);
pub const FIELD: Color32 = Color32::from_rgb(38, 39, 50);
pub const LINE: Color32 = Color32::from_rgb(52, 54, 68);
pub const CODE: Color32 = Color32::from_rgb(12, 13, 20);

pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

const MANROPE: &[u8] = include_bytes!("../../../prep/assets/fonts/Manrope-Regular.ttf");
const MANROPE_SEMIBOLD: &[u8] = include_bytes!("../../../prep/assets/fonts/Manrope-SemiBold.ttf");

pub fn install(ctx: &egui::Context, scale: f32) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("manrope".into(), Arc::new(FontData::from_static(MANROPE)));
    fonts.font_data.insert("manrope_semibold".into(), Arc::new(FontData::from_static(MANROPE_SEMIBOLD)));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "manrope".into());
    fonts.families.insert(FontFamily::Name("semibold".into()), vec!["manrope_semibold".into(), "manrope".into()]);
    ctx.set_fonts(fonts);
    apply(ctx, scale);
}

/// The text sizes and colours; called again when the text size setting changes.
pub fn apply(ctx: &egui::Context, scale: f32) {
    let mut style = (*ctx.global_style()).clone();
    let size = |points: f32| points * scale;
    style.text_styles = [
        (TextStyle::Heading, sans(size(20.0))),
        (TextStyle::Body, sans(size(15.0))),
        (TextStyle::Button, semibold(size(14.0))),
        (TextStyle::Small, sans(size(12.5))),
        (TextStyle::Monospace, mono(size(14.0))),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size.y = 32.0;
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = Color32::from_rgb(24, 25, 36);
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = Color32::from_rgb(28, 29, 40);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = Color32::from_rgb(88, 80, 160);
    visuals.selection.stroke = Stroke::new(1.0, STRONG);
    let radius = CornerRadius::same(8);
    for widget in [&mut visuals.widgets.noninteractive, &mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active, &mut visuals.widgets.open] {
        widget.corner_radius = radius;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.inactive.bg_fill = FIELD;
    visuals.widgets.inactive.weak_bg_fill = FIELD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 54, 72);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 54, 72);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_fill = Color32::from_rgb(66, 62, 110);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(66, 62, 110);
    visuals.widgets.open.bg_fill = Color32::from_rgb(30, 31, 44);
    visuals.widgets.open.weak_bg_fill = Color32::from_rgb(30, 31, 44);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, LINE);
    ctx.set_global_style(style);
}

pub fn panel_frame(opacity: f32) -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(PANEL.r(), PANEL.g(), PANEL.b(), (opacity.clamp(0.35, 1.0) * 232.0) as u8))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(18, 14))
        .stroke(Stroke::new(1.0, LINE))
}

/// A check mark or an empty circle, painted: the fonts of the window have no such glyphs.
pub fn step_mark(ui: &mut egui::Ui, done: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let painter = ui.painter();
    let centre = rect.center();
    if done {
        painter.circle_filled(centre, 8.0, GOOD);
        let stroke = Stroke::new(2.0, PANEL);
        painter.line_segment([centre + egui::vec2(-3.8, 0.4), centre + egui::vec2(-1.0, 3.2)], stroke);
        painter.line_segment([centre + egui::vec2(-1.0, 3.2), centre + egui::vec2(4.0, -3.0)], stroke);
    } else {
        painter.circle_stroke(centre, 7.5, Stroke::new(1.5, MUTED));
    }
}

/// Two short diagonal lines in a corner, to show where a frameless window can be pulled.
pub fn grip(painter: &egui::Painter, corner: egui::Pos2) {
    let stroke = Stroke::new(1.5, LINE);
    painter.line_segment([corner + egui::vec2(-4.0, -12.0), corner + egui::vec2(-12.0, -4.0)], stroke);
    painter.line_segment([corner + egui::vec2(-4.0, -7.0), corner + egui::vec2(-7.0, -4.0)], stroke);
}
