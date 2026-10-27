//! The app icon: the one that ships, the same in other colours, or an image of the person's own.

use std::path::Path;
use std::sync::Arc;

use egui::IconData;

const DEFAULT_PNG: &[u8] = include_bytes!("../../../packaging/careertwin-interview.png");
/// The icon sent to the window: big enough for any taskbar, small enough to be cheap.
const SIZE: u32 = 256;

/// The colour variants: (name, how far the hue turns in degrees; `None` is grey).
pub const VARIANTS: [(&str, Option<f32>); 5] = [("amber", Some(0.0)), ("sky", Some(165.0)), ("mint", Some(105.0)), ("rose", Some(305.0)), ("graphite", None)];

pub fn is_builtin(setting: &str) -> bool {
    setting.strip_prefix("builtin:").is_some_and(|name| VARIANTS.iter().any(|(known, _)| *known == name))
}

pub fn default_icon() -> IconData {
    decode(DEFAULT_PNG).unwrap_or_default()
}

fn decode(bytes: &[u8]) -> Result<IconData, String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    Ok(square(&image))
}

/// A square of [`SIZE`] pixels cut from the middle of the image (an icon is square).
fn square(image: &image::DynamicImage) -> IconData {
    let (width, height) = (image.width(), image.height());
    let side = width.min(height).max(1);
    let cropped = image.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    let resized = cropped.resize_exact(SIZE, SIZE, image::imageops::FilterType::Triangle);
    IconData { rgba: resized.into_rgba8().into_raw(), width: SIZE, height: SIZE }
}

/// Turn the hue of every pixel by `degrees`, or take its colour away for `None`; the alpha stays.
pub fn recolour(icon: &IconData, turn: Option<f32>) -> IconData {
    let mut rgba = icon.rgba.clone();
    for pixel in rgba.chunks_exact_mut(4) {
        let (h, s, l) = rgb_to_hsl(pixel[0], pixel[1], pixel[2]);
        let (r, g, b) = match turn {
            Some(degrees) => hsl_to_rgb((h + degrees).rem_euclid(360.0), s, l),
            None => hsl_to_rgb(h, 0.0, l),
        };
        pixel[..3].copy_from_slice(&[r, g, b]);
    }
    IconData { rgba, width: icon.width, height: icon.height }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f32::EPSILON {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s == 0.0 {
        let value = (l * 255.0).round() as u8;
        return (value, value, value);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let value = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (value * 255.0).round() as u8
    };
    (channel(h / 360.0 + 1.0 / 3.0), channel(h / 360.0), channel(h / 360.0 - 1.0 / 3.0))
}

/// The icon a setting asks for: `""` the default, `builtin:<name>`, or the path of an image. A bad image is an
/// error the window can show; the caller then keeps the default.
pub fn load(setting: &str) -> Result<IconData, String> {
    let setting = setting.trim();
    if setting.is_empty() {
        return Ok(default_icon());
    }
    if let Some(name) = setting.strip_prefix("builtin:") {
        let (_, turn) = VARIANTS.iter().find(|(known, _)| *known == name).ok_or_else(|| format!("there is no icon named {name}"))?;
        return Ok(recolour(&default_icon(), *turn));
    }
    let path = Path::new(setting);
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("the file is over 20 MB".into());
    }
    decode(&bytes)
}

pub fn shared(setting: &str) -> Arc<IconData> {
    Arc::new(load(setting).unwrap_or_else(|_| default_icon()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(icon: &IconData, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * icon.width + x) * 4) as usize;
        icon.rgba[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn the_icon_that_ships_is_square_and_has_its_pixels() {
        let icon = default_icon();
        assert_eq!((icon.width, icon.height, icon.rgba.len()), (SIZE, SIZE, (SIZE * SIZE * 4) as usize));
        assert!(icon.rgba.chunks_exact(4).any(|px| px[3] > 200)); // not blank
    }

    #[test]
    fn a_colour_variant_changes_the_colour_and_keeps_the_shape() {
        let original = default_icon();
        let amber = original.rgba.chunks_exact(4).filter(|px| px[3] > 200 && px[0] > 200 && px[1] > 140 && px[2] < 90).count();
        assert!(amber > 500, "the amber crescent should be there: {amber}");
        for (name, turn) in VARIANTS.iter().skip(1) {
            let other = recolour(&original, *turn);
            assert_ne!(other.rgba, original.rgba, "{name}");
            assert!(other.rgba.chunks_exact(4).zip(original.rgba.chunks_exact(4)).all(|(a, b)| a[3] == b[3]), "{name}: alpha");
        }
        let grey = recolour(&original, None);
        assert!(grey.rgba.chunks_exact(4).all(|px| px[0] == px[1] && px[1] == px[2]));
        assert_eq!(recolour(&original, Some(0.0)).rgba.chunks_exact(4).zip(original.rgba.chunks_exact(4)).filter(|(a, b)| a[..3].iter().zip(b[..3].iter()).any(|(x, y)| x.abs_diff(*y) > 2)).count(), 0);
    }

    #[test]
    fn hue_maths_round_trips() {
        for (r, g, b) in [(255, 180, 40), (10, 200, 90), (120, 120, 250), (0, 0, 0), (255, 255, 255)] {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r2, g2, b2) = hsl_to_rgb(h, s, l);
            assert!(r.abs_diff(r2) <= 1 && g.abs_diff(g2) <= 1 && b.abs_diff(b2) <= 1, "{r},{g},{b} -> {r2},{g2},{b2}");
        }
    }

    #[test]
    fn a_setting_names_the_default_a_variant_or_a_file() {
        assert_eq!(load("").unwrap(), default_icon());
        assert_ne!(load("builtin:sky").unwrap(), default_icon());
        assert!(load("builtin:nope").is_err() && is_builtin("builtin:mint") && !is_builtin("builtin:nope") && !is_builtin(""));
        assert!(load("/no/such/file.png").is_err());
    }

    #[test]
    fn an_image_of_your_own_is_cut_square_and_sized() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        let mut wide = image::RgbaImage::new(600, 200);
        for (x, _, px) in wide.enumerate_pixels_mut() {
            *px = if (200..400).contains(&x) { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) };
        }
        wide.save(&path).unwrap();
        let icon = load(path.to_str().unwrap()).unwrap();
        assert_eq!((icon.width, icon.height), (SIZE, SIZE));
        assert_eq!(pixel(&icon, SIZE / 2, SIZE / 2), [255, 0, 0, 255]); // the middle third is what is kept
        assert_eq!(pixel(&icon, 2, 2), [255, 0, 0, 255]);
        std::fs::write(dir.path().join("bad.png"), b"not an image").unwrap();
        assert!(load(dir.path().join("bad.png").to_str().unwrap()).is_err());
    }
}
