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
/// The display font for small text such as a name tag; the semibold sans when it is not installed.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
/// Titles and big numbers: the display font when it is on this computer, the serif font otherwise.
pub fn heading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("heading".into()))
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

/// The display fonts the program looks for, the one it likes best first: Ralev001 (RALEV.COM) and
/// SOUTHGHETTO (S. Shebestova), both free with Cyrillic.
const DISPLAY_FONTS: [&str; 2] = ["ralev", "southghetto"];

/// Looks for a display font the person has put on this computer. It is never shipped with the
/// program, because the licences do not say that the files may be passed on; it is found in the
/// font folders or in the program's own `fonts` folder, by the name of its file.
pub fn find_display_font() -> Option<Vec<u8>> {
    let mut folders: Vec<std::path::PathBuf> = Vec::new();
    if let Some(dirs) = directories::BaseDirs::new() {
        folders.push(dirs.data_dir().join("fonts"));
        folders.push(dirs.home_dir().join(".fonts"));
    }
    if let Ok(paths) = crate::config::Paths::discover() {
        folders.push(paths.data.join("fonts"));
    }
    folders.push("/usr/local/share/fonts".into());
    folders.push("/usr/share/fonts".into());
    DISPLAY_FONTS.iter().find_map(|name| folders.iter().find_map(|folder| find_font_in(folder, name, 0)))
}

/// A real font file whose name says `name`, in `folder` or below it.
fn find_font_in(folder: &std::path::Path, name_part: &str, depth: usize) -> Option<Vec<u8>> {
    let entries = std::fs::read_dir(folder).ok()?;
    let mut subfolders = Vec::new();
    // several weights of one family may sit in a folder: the shortest name is the regular one
    let mut best: Option<(usize, Vec<u8>)> = None;
    for entry in entries.flatten().take(2000) {
        let path = entry.path();
        if path.is_dir() {
            subfolders.push(path);
            continue;
        }
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let font_file = name.ends_with(".ttf") || name.ends_with(".otf");
        if font_file
            && name.replace([' ', '-', '_'], "").contains(name_part)
            && let Ok(bytes) = std::fs::read(&path)
            && looks_like_a_font(&bytes)
            && best.as_ref().is_none_or(|(length, _)| name.len() < *length)
        {
            best = Some((name.len(), bytes));
        }
    }
    if let Some((_, bytes)) = best {
        return Some(bytes);
    }
    if depth >= 3 {
        return None;
    }
    subfolders.iter().find_map(|sub| find_font_in(sub, name_part, depth + 1))
}

/// The first bytes of a TrueType or OpenType file: a renamed archive or a broken download is not one.
fn looks_like_a_font(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0, 1, 0, 0] | b"OTTO" | b"true"))
}

pub fn fonts() -> FontDefinitions {
    fonts_with(None)
}

/// The fonts of the program. With `display`, headings are set in it, and any letter it lacks falls
/// back to the serif font; without, headings are set in the serif font.
pub fn fonts_with(display: Option<Vec<u8>>) -> FontDefinitions {
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
    // the display font leads two families, and any letter it lacks falls to ours: the serif for
    // headings, the semibold sans for small labels set in it
    let has_display = display.is_some();
    if let Some(bytes) = display {
        definitions.font_data.insert("display".to_owned(), Arc::new(FontData::from_owned(bytes)));
    }
    for (name, own) in [("heading", "ptserif"), ("display", "manrope_semibold")] {
        let mut list = vec![];
        if has_display {
            list.push("display".to_owned());
        }
        list.push(own.to_owned());
        list.extend(fallback.iter().cloned());
        definitions.families.insert(FontFamily::Name(name.into()), list);
    }
    definitions
}

/// Fonts, colours and spacing for the whole program.
pub fn install(ctx: &egui::Context) {
    install_with(ctx, find_display_font());
}

/// [`install`] with the display font given, so that a test does not depend on the fonts of the machine.
pub fn install_with(ctx: &egui::Context, display: Option<Vec<u8>>) {
    ctx.set_fonts(fonts_with(display));
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
    fn headings_use_the_display_font_when_there_is_one_and_the_serif_when_not() {
        let without = fonts();
        let list = without.families.get(&FontFamily::Name("heading".into())).unwrap();
        assert_eq!(list[0], "ptserif");
        let with = fonts_with(Some(include_bytes!("../../assets/fonts/PTSerif-Bold.ttf").to_vec()));
        let list = with.families.get(&FontFamily::Name("heading".into())).unwrap();
        assert_eq!(&list[..2], ["display", "ptserif"], "a letter the display font lacks falls to the serif");
        let small = &with.families[&FontFamily::Name("display".into())];
        assert_eq!(&small[..2], ["display", "manrope_semibold"]);
        assert_eq!(without.families[&FontFamily::Name("display".into())][0], "manrope_semibold");
        assert!(with.font_data.contains_key("display") && !without.font_data.contains_key("display"));
        assert!(
            !with.families[&FontFamily::Name("serif".into())].contains(&"display".to_owned()),
            "text stays as it was"
        );
    }

    #[test]
    fn the_display_font_is_found_by_its_name_anywhere_below_a_font_folder_and_nothing_else_is() {
        let dir = tempfile::tempdir().unwrap();
        let real = include_bytes!("../../assets/fonts/PTSerif-Bold.ttf");
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("SOUTH GHETTO-Regular.ttf"), real).unwrap();
        std::fs::write(dir.path().join("Other.ttf"), real).unwrap();
        assert_eq!(find_font_in(dir.path(), "southghetto", 0).as_deref(), Some(&real[..]));
        assert!(find_font_in(dir.path(), "ralev", 0).is_none());
        std::fs::write(dir.path().join("Ralev001.otf"), real).unwrap();
        assert!(find_font_in(dir.path(), "ralev", 0).is_some());
        // a download that is not a font is passed over, and so is a file of another name
        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join("southghetto.ttf"), b"PK\x03\x04 not a font").unwrap();
        std::fs::write(broken.path().join("southghetto.zip"), real).unwrap();
        assert!(find_font_in(broken.path(), "southghetto", 0).is_none());
        assert!(find_font_in(&dir.path().join("missing"), "southghetto", 0).is_none());
        assert!(looks_like_a_font(real) && !looks_like_a_font(b"") && !looks_like_a_font(b"PK\x03\x04"));
    }

    #[test]
    fn of_several_weights_in_one_folder_the_regular_one_is_taken() {
        let dir = tempfile::tempdir().unwrap();
        let regular = include_bytes!("../../assets/fonts/PTSerif-Regular.ttf");
        let bold = include_bytes!("../../assets/fonts/PTSerif-Bold.ttf");
        std::fs::write(dir.path().join("Ralev001-ExtraBold.ttf"), bold).unwrap();
        std::fs::write(dir.path().join("Ralev001.ttf"), regular).unwrap();
        assert_eq!(find_font_in(dir.path(), "ralev", 0).as_deref(), Some(&regular[..]));
    }

    #[test]
    fn mixing_and_alpha() {
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 5.0), Color32::WHITE);
        assert_eq!(alpha(BERRY, 0.0).a(), 0);
        assert_eq!(alpha(BERRY, 1.0).a(), 255);
    }
}
