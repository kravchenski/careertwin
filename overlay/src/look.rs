//! The colours a theme is made of, kept apart from the window so that the settings can be read without it.

use crate::settings::AppSettings;

/// Red, green and blue.
pub type Rgb = [u8; 3];

/// (name, panel, text, accent): a theme that comes with the window.
pub type Preset = (&'static str, Rgb, Rgb, Rgb);

pub const PRESETS: [Preset; 7] = [
    ("midnight", [19, 20, 30], [215, 218, 232], [188, 180, 255]),
    ("graphite", [24, 24, 26], [225, 225, 228], [232, 232, 236]),
    ("forest", [16, 26, 22], [214, 232, 222], [132, 214, 160]),
    ("amber", [28, 22, 14], [236, 224, 206], [255, 181, 58]),
    ("rose", [30, 18, 24], [236, 214, 224], [255, 140, 176]),
    ("ocean", [14, 24, 34], [212, 228, 240], [110, 196, 255]),
    ("paper", [246, 244, 238], [40, 40, 48], [98, 84, 214]),
];

pub const DEFAULT_THEME: &str = "midnight";
/// The theme that is dark or light as the system is.
pub const SYSTEM_THEME: &str = "system";
/// The themes the system's dark and light mode choose.
pub const SYSTEM_DARK: &str = "midnight";
pub const SYSTEM_LIGHT: &str = "paper";

pub fn is_preset(name: &str) -> bool {
    PRESETS.iter().any(|(known, ..)| *known == name)
}

/// Whether a saved theme name is one the window knows: a preset, `custom` or `system`.
pub fn is_theme(name: &str) -> bool {
    name == "custom" || name == SYSTEM_THEME || is_preset(name)
}

/// A colour as `#rrggbb`, the way the person types it and the file keeps it.
pub fn hex(value: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", value[0], value[1], value[2])
}

pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// (panel, text, accent) of the theme the settings choose; a custom colour that cannot be read is the midnight one.
/// `system_dark` is whether the system is in dark mode, which the `system` theme follows. An accent chosen apart from
/// the theme replaces the theme's own.
pub fn colors_for(app: &AppSettings, system_dark: bool) -> ([u8; 3], [u8; 3], [u8; 3]) {
    let midnight = PRESETS[0];
    let theme = if app.theme == SYSTEM_THEME {
        if system_dark { SYSTEM_DARK } else { SYSTEM_LIGHT }
    } else {
        app.theme.as_str()
    };
    let (panel, text, mut accent) = if theme == "custom" {
        (
            parse_hex(&app.custom_panel).unwrap_or(midnight.1),
            parse_hex(&app.custom_text).unwrap_or(midnight.2),
            parse_hex(&app.custom_accent).unwrap_or(midnight.3),
        )
    } else {
        let (_, panel, text, accent) = PRESETS.iter().find(|(name, ..)| *name == theme).copied().unwrap_or(midnight);
        (panel, text, accent)
    };
    if let Some(chosen) = parse_hex(&app.accent) {
        accent = chosen;
    }
    (panel, text, accent)
}

/// As [`colors_for`] with the system in dark mode: for where the system is not asked (the window is not open yet).
pub fn colors(app: &AppSettings) -> ([u8; 3], [u8; 3], [u8; 3]) {
    colors_for(app, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_written_and_read_as_hex() {
        assert_eq!(hex([188, 180, 255]), "#bcb4ff");
        assert_eq!(parse_hex("#bcb4ff"), Some([188, 180, 255]));
        assert_eq!(parse_hex(" BCB4FF "), Some([188, 180, 255]));
        for bad in ["", "#fff", "#bcb4f", "#gggggg", "#bcb4ff00"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_theme_is_a_preset_or_the_three_custom_colours() {
        let mut app = AppSettings::default();
        assert_eq!(colors(&app).2, [188, 180, 255]);
        app.theme = "forest".into();
        assert_eq!(colors(&app).2, [132, 214, 160]);
        app.theme = "custom".into();
        app.custom_accent = "#ff0080".into();
        app.custom_panel = "garbage".into();
        let (panel, _, accent) = colors(&app);
        assert_eq!((panel, accent), ([19, 20, 30], [255, 0, 128])); // a colour that cannot be read is the midnight one
        assert!(is_preset("paper") && !is_preset("custom") && !is_preset("nope"));
    }

    #[test]
    fn the_system_theme_follows_dark_and_light_and_an_accent_of_its_own_wins_over_any_theme() {
        let mut app = AppSettings { theme: "system".into(), ..AppSettings::default() };
        assert_eq!(colors_for(&app, true).0, PRESETS[0].1);
        assert_eq!(colors_for(&app, false).0, PRESETS[6].1); // paper
        app.accent = "#ff0080".into();
        assert_eq!(colors_for(&app, true).2, [255, 0, 128]);
        assert_eq!(colors_for(&app, false).2, [255, 0, 128]);
        assert_eq!(colors_for(&app, false).0, PRESETS[6].1); // only the accent changed
        app.accent = "not a colour".into();
        assert_eq!(colors_for(&app, true).2, PRESETS[0].3); // an accent that cannot be read is the theme's own
        assert!(is_theme("system") && is_theme("custom") && is_theme("amber") && !is_theme("neon"));
    }
}
