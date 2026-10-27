//! How the window looks: a palette (a preset or the person's own three colours), one font.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle};

/// Every colour of the window. A theme is these, nothing else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// A dark window: the dark base of the widgets and light text.
    pub dark: bool,
    pub panel: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub field: Color32,
    pub line: Color32,
    pub code: Color32,
    pub window: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub danger: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub use crate::look::{DEFAULT_THEME, PRESETS, hex, parse_hex};

/// The mix of two colours: `t` of `b` over `a`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(channel(a.r(), b.r()), channel(a.g(), b.g()), channel(a.b(), b.b()))
}

/// How light a colour looks, 0 (black) to 1 (white).
pub fn luminance(color: Color32) -> f32 {
    (0.2126 * f32::from(color.r()) + 0.7152 * f32::from(color.g()) + 0.0722 * f32::from(color.b())) / 255.0
}

fn color(value: [u8; 3]) -> Color32 {
    rgb(value[0], value[1], value[2])
}

/// A palette from three colours: the rest (fields, lines, muted text, code blocks) is worked out from them, so a
/// custom theme cannot come out with text that cannot be read.
pub fn derive(panel: [u8; 3], text: [u8; 3], accent: [u8; 3]) -> Palette {
    let (panel, text, accent) = (color(panel), color(text), color(accent));
    let dark = luminance(panel) < 0.5;
    let (white, black) = (Color32::WHITE, Color32::BLACK);
    // text that is too close to the background in lightness is pushed away from it
    let text = if (luminance(text) - luminance(panel)).abs() < 0.35 {
        if dark { mix(text, white, 0.7) } else { mix(text, black, 0.7) }
    } else {
        text
    };
    Palette {
        dark,
        panel,
        text,
        strong: if dark { white } else { black },
        muted: mix(text, panel, 0.45),
        accent,
        on_accent: if luminance(accent) > 0.55 { rgb(24, 22, 40) } else { white },
        field: mix(panel, text, 0.09),
        line: mix(panel, text, 0.18),
        code: if dark { mix(panel, black, 0.4) } else { mix(panel, text, 0.06) },
        window: mix(panel, text, 0.05),
        good: if dark { rgb(143, 191, 154) } else { rgb(40, 130, 70) },
        warn: if dark { rgb(255, 207, 122) } else { rgb(170, 110, 0) },
        danger: if dark { rgb(255, 154, 154) } else { rgb(190, 50, 50) },
    }
}

/// The palette the settings choose: a preset, or the three colours of the custom theme.
pub fn from_settings(app: &crate::settings::AppSettings) -> Palette {
    let (panel, text, accent) = crate::look::colors(app);
    derive(panel, text, accent)
}

/// A colour as `#rrggbb`.
pub fn hex_of(color: Color32) -> String {
    crate::look::hex([color.r(), color.g(), color.b()])
}

pub fn preset(name: &str) -> Palette {
    let (_, panel, text, accent) = PRESETS.iter().find(|(known, ..)| *known == name).copied().unwrap_or(PRESETS[0]);
    derive(panel, text, accent)
}

// The palette belongs to the thread that draws the window (the main one), so two windows drawn at once, as
// the tests do, never take each other's colours.
thread_local! {
    static CURRENT: std::cell::Cell<Option<Palette>> = const { std::cell::Cell::new(None) };
}

pub fn palette() -> Palette {
    CURRENT.with(std::cell::Cell::get).unwrap_or_else(|| preset(DEFAULT_THEME))
}

pub fn set_palette(palette: Palette) {
    CURRENT.with(|current| current.set(Some(palette)));
}

pub fn panel() -> Color32 {
    palette().panel
}
pub fn text() -> Color32 {
    palette().text
}
pub fn strong() -> Color32 {
    palette().strong
}
pub fn muted() -> Color32 {
    palette().muted
}
pub fn accent() -> Color32 {
    palette().accent
}
pub fn on_accent() -> Color32 {
    palette().on_accent
}
pub fn field() -> Color32 {
    palette().field
}
pub fn line() -> Color32 {
    palette().line
}
pub fn code() -> Color32 {
    palette().code
}
pub fn good() -> Color32 {
    palette().good
}
pub fn warn() -> Color32 {
    palette().warn
}
pub fn danger() -> Color32 {
    palette().danger
}

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

/// The text sizes and the colours of the current palette; called again when either changes.
pub fn apply(ctx: &egui::Context, scale: f32) {
    let p = palette();
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
    *visuals = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.override_text_color = Some(p.text);
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = p.window;
    visuals.window_stroke = Stroke::new(1.0, p.line);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.extreme_bg_color = p.field;
    visuals.faint_bg_color = mix(p.panel, p.text, 0.05);
    visuals.hyperlink_color = p.accent;
    visuals.selection.bg_fill = mix(p.panel, p.accent, 0.45);
    visuals.selection.stroke = Stroke::new(1.0, p.strong);
    let radius = CornerRadius::same(8);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = radius;
        widget.fg_stroke.color = p.text;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    visuals.widgets.inactive.bg_fill = p.field;
    visuals.widgets.inactive.weak_bg_fill = p.field;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.line);
    let hover = mix(p.field, p.accent, 0.18);
    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.hovered.fg_stroke.color = p.strong;
    let pressed = mix(p.field, p.accent, 0.35);
    visuals.widgets.active.bg_fill = pressed;
    visuals.widgets.active.weak_bg_fill = pressed;
    visuals.widgets.active.fg_stroke.color = p.strong;
    visuals.widgets.open.bg_fill = p.window;
    visuals.widgets.open.weak_bg_fill = p.window;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, p.line);
    ctx.set_global_style(style);
}

pub fn panel_frame(opacity: f32) -> egui::Frame {
    let p = palette();
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            p.panel.r(),
            p.panel.g(),
            p.panel.b(),
            (opacity.clamp(0.35, 1.0) * 232.0) as u8,
        ))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(18, 14))
        .stroke(Stroke::new(1.0, p.line))
}

/// A check mark or an empty circle, painted: the fonts of the window have no such glyphs.
pub fn step_mark(ui: &mut egui::Ui, done: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let painter = ui.painter();
    let centre = rect.center();
    if done {
        painter.circle_filled(centre, 8.0, good());
        let stroke = Stroke::new(2.0, panel());
        painter.line_segment([centre + egui::vec2(-3.8, 0.4), centre + egui::vec2(-1.0, 3.2)], stroke);
        painter.line_segment([centre + egui::vec2(-1.0, 3.2), centre + egui::vec2(4.0, -3.0)], stroke);
    } else {
        painter.circle_stroke(centre, 7.5, Stroke::new(1.5, muted()));
    }
}

/// Two short diagonal lines in a corner, to show where a frameless window can be pulled.
pub fn grip(painter: &egui::Painter, corner: egui::Pos2) {
    let stroke = Stroke::new(1.5, line());
    painter.line_segment([corner + egui::vec2(-4.0, -12.0), corner + egui::vec2(-12.0, -4.0)], stroke);
    painter.line_segment([corner + egui::vec2(-4.0, -7.0), corner + egui::vec2(-7.0, -4.0)], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far apart two colours are in lightness: below about 0.35 text is hard to read.
    fn contrast(a: Color32, b: Color32) -> f32 {
        (luminance(a) - luminance(b)).abs()
    }

    #[test]
    fn every_preset_has_readable_text_and_an_accent_that_stands_out() {
        for (name, ..) in PRESETS {
            let p = preset(name);
            assert!(contrast(p.text, p.panel) > 0.5, "{name}: text on the panel");
            assert!(contrast(p.muted, p.panel) > 0.25, "{name}: muted text");
            assert!(contrast(p.accent, p.panel) > 0.25, "{name}: accent");
            assert!(contrast(p.on_accent, p.accent) > 0.35, "{name}: text on the accent");
            assert!(contrast(p.strong, p.panel) > 0.7, "{name}: strong text");
        }
        assert!(preset("midnight").dark && !preset("paper").dark);
        assert_eq!(preset("nope"), preset("midnight"));
    }

    #[test]
    fn a_custom_theme_cannot_come_out_unreadable() {
        // text almost the colour of the background is pushed away from it
        let p = derive([20, 20, 30], [30, 30, 44], [255, 0, 128]);
        assert!(contrast(p.text, p.panel) > 0.35);
        let light = derive([250, 250, 250], [240, 240, 240], [0, 90, 200]);
        assert!(!light.dark && contrast(light.text, light.panel) > 0.35);
        // a pale accent gets dark text, a deep one light text
        assert!(luminance(derive([20, 20, 30], [220, 220, 230], [250, 250, 120]).on_accent) < 0.3);
        assert!(luminance(derive([20, 20, 30], [220, 220, 230], [40, 40, 200]).on_accent) > 0.7);
    }

    #[test]
    fn mixing_goes_from_one_colour_to_the_other() {
        let (a, b) = (rgb(0, 0, 0), rgb(200, 100, 50));
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 0.5), rgb(100, 50, 25));
    }
}
