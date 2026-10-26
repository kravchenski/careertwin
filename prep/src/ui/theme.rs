//! How the program looks: the cranberry palette on dark cocoa, a serif for titles, a clean sans for
//! text and a monospace for small capital labels.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// The five colours of the palette.
pub const CRANBERRY: Color32 = rgb(0x7F1D1D); // Deep Cranberry
pub const BERRY: Color32 = rgb(0xBE3A3A); // Winter Berry
pub const BLUSH: Color32 = rgb(0xF4D6D6); // Berry Snow
pub const SNOW: Color32 = rgb(0xFFF8F2); // Warm Snow: the text colour
pub const COCOA: Color32 = rgb(0x2A1C1C); // Dark Cocoa

// Tints made from them for backgrounds and borders.
pub const BG: Color32 = rgb(0x181010);
pub const SURFACE: Color32 = rgb(0x211515);
pub const RAISED: Color32 = rgb(0x362424);
pub const LINE: Color32 = rgb(0x432E2E);
pub const MUTED: Color32 = rgb(0xBDA7A4);
pub const FAINT: Color32 = rgb(0x8A7572);
pub const GOOD: Color32 = rgb(0x8FBF9A);

pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}
pub fn serif_italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif_italic".into()))
}
pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("sans_semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("sans_bold".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono".into()))
}
pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono_bold".into()))
}

const FONTS: [(&str, &[u8]); 8] = [
    ("manrope", include_bytes!("../../assets/fonts/Manrope-Regular.ttf")),
    ("manrope_semibold", include_bytes!("../../assets/fonts/Manrope-SemiBold.ttf")),
    ("manrope_bold", include_bytes!("../../assets/fonts/Manrope-ExtraBold.ttf")),
    ("ptserif", include_bytes!("../../assets/fonts/PTSerif-Regular.ttf")),
    ("ptserif_italic", include_bytes!("../../assets/fonts/PTSerif-Italic.ttf")),
    ("ptserif_bold", include_bytes!("../../assets/fonts/PTSerif-Bold.ttf")),
    ("jetbrains", include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf")),
    ("jetbrains_bold", include_bytes!("../../assets/fonts/JetBrainsMono-SemiBold.ttf")),
];

pub fn fonts() -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    for (name, bytes) in FONTS {
        definitions.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's own fonts stay behind ours, for symbols and anything ours lack
    let fallback: Vec<String> = definitions.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut family = |name: &str, first: &str| {
        let mut list = vec![first.to_owned()];
        list.extend(fallback.iter().cloned());
        let key = if name.is_empty() { FontFamily::Proportional } else { FontFamily::Name(name.into()) };
        definitions.families.insert(key, list);
    };
    family("", "manrope");
    family("sans_semibold", "manrope_semibold");
    family("sans_bold", "manrope_bold");
    family("serif", "ptserif");
    family("serif_italic", "ptserif_italic");
    family("mono", "jetbrains");
    family("mono_bold", "jetbrains_bold");
    definitions
}

/// Fonts, colours and spacing for the whole program.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, serif(30.0)),
            (TextStyle::Body, sans(16.0)),
            (TextStyle::Button, semibold(15.0)),
            (TextStyle::Small, sans(12.5)),
            (TextStyle::Monospace, mono(13.0)),
        ]
        .into();
        let visuals = &mut style.visuals;
        visuals.dark_mode = true;
        visuals.panel_fill = BG;
        visuals.window_fill = SURFACE;
        visuals.window_stroke = Stroke::new(1.0, LINE);
        visuals.window_corner_radius = CornerRadius::same(18);
        visuals.window_shadow =
            egui::Shadow { offset: [0, 10], blur: 40, spread: 0, color: Color32::from_black_alpha(140) };
        visuals.extreme_bg_color = COCOA;
        visuals.faint_bg_color = SURFACE;
        visuals.code_bg_color = COCOA;
        visuals.hyperlink_color = BLUSH;
        visuals.selection.bg_fill = CRANBERRY;
        visuals.selection.stroke = Stroke::new(1.0, SNOW);
        visuals.weak_text_color = Some(MUTED);
        let radius = CornerRadius::same(12);
        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = SURFACE;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, SNOW);
        widgets.noninteractive.corner_radius = radius;
        widgets.inactive.bg_fill = COCOA;
        widgets.inactive.weak_bg_fill = COCOA;
        widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        widgets.inactive.fg_stroke = Stroke::new(1.0, SNOW);
        widgets.inactive.corner_radius = radius;
        widgets.hovered.bg_fill = RAISED;
        widgets.hovered.weak_bg_fill = RAISED;
        widgets.hovered.bg_stroke = Stroke::new(1.0, BERRY);
        widgets.hovered.fg_stroke = Stroke::new(1.5, SNOW);
        widgets.hovered.corner_radius = radius;
        widgets.active.bg_fill = CRANBERRY;
        widgets.active.weak_bg_fill = CRANBERRY;
        widgets.active.bg_stroke = Stroke::new(1.0, BERRY);
        widgets.active.fg_stroke = Stroke::new(1.5, SNOW);
        widgets.active.corner_radius = radius;
        widgets.open.bg_fill = RAISED;
        widgets.open.weak_bg_fill = RAISED;
        widgets.open.corner_radius = radius;
        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(10.0, 10.0);
        spacing.button_padding = egui::vec2(16.0, 9.0);
        spacing.window_margin = Margin::same(20);
        spacing.interact_size.y = 36.0;
        spacing.slider_width = 260.0;
    });
}

/// A colour with its opacity set.
pub fn alpha(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (opacity.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Blends `a` towards `b` by `t`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_is_the_one_from_the_picture() {
        assert_eq!(CRANBERRY, Color32::from_rgb(0x7F, 0x1D, 0x1D));
        assert_eq!(BERRY, Color32::from_rgb(0xBE, 0x3A, 0x3A));
        assert_eq!(BLUSH, Color32::from_rgb(0xF4, 0xD6, 0xD6));
        assert_eq!(SNOW, Color32::from_rgb(0xFF, 0xF8, 0xF2));
        assert_eq!(COCOA, Color32::from_rgb(0x2A, 0x1C, 0x1C));
    }

    #[test]
    fn every_font_family_has_a_real_font_in_front() {
        let definitions = fonts();
        for name in ["serif", "serif_italic", "sans_semibold", "sans_bold", "mono", "mono_bold"] {
            let list = definitions.families.get(&FontFamily::Name(name.into())).unwrap();
            assert!(definitions.font_data.contains_key(&list[0]), "{name}");
            assert!(list.len() > 1, "{name} keeps egui's fonts behind it");
        }
    }

    #[test]
    fn mixing_and_alpha() {
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 5.0), Color32::WHITE);
        assert_eq!(alpha(BERRY, 0.0).a(), 0);
        assert_eq!(alpha(BERRY, 1.0).a(), 255);
    }
}
