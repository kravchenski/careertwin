//! The app icon: the one that ships, the same in other colours, or an image of the person's own (PNG, JPEG or ICO).
//!
//! A file chosen by the person is untrusted input however kindly it was meant: it may be damaged, it may be a file that
//! only claims to be an image, or a small file that decodes to gigabytes. So it is read with a ceiling on its size, only
//! a regular file is opened, the kind of image is taken from its content (not its name), and the decoder is given limits
//! on the picture's sides and on the memory it may take, so a bomb fails before it is built.

use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

use egui::IconData;

const DEFAULT_PNG: &[u8] = include_bytes!("../../../packaging/careertwin-interview.png");
/// The icon sent to the window: big enough for any taskbar, small enough to be cheap.
const SIZE: u32 = 256;

/// The colour variants: (name, how far the hue turns in degrees; `None` is grey).
pub const VARIANTS: [(&str, Option<f32>); 5] =
    [("amber", Some(0.0)), ("sky", Some(165.0)), ("mint", Some(105.0)), ("rose", Some(305.0)), ("graphite", None)];

pub fn is_builtin(setting: &str) -> bool {
    setting.strip_prefix("builtin:").is_some_and(|name| VARIANTS.iter().any(|(known, _)| *known == name))
}

pub fn default_icon() -> IconData {
    decode(DEFAULT_PNG).unwrap_or_default()
}

/// The largest file that is read as an icon.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// The largest side of a picture that is decoded. An icon needs 256; 4096 leaves room for a photograph.
pub const MAX_SIDE: u32 = 4096;
/// The most memory the decoder may ask for.
const MAX_ALLOC: u64 = 128 * 1024 * 1024;

/// The kinds of image an icon may be, whatever the file is called.
fn allowed(format: image::ImageFormat) -> bool {
    matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::Ico)
}

fn decode(bytes: &[u8]) -> Result<IconData, String> {
    let mut reader =
        image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|error| error.to_string())?;
    match reader.format() {
        Some(format) if allowed(format) => {}
        Some(_) => return Err("only PNG, JPEG and ICO images can be used".into()),
        None => return Err("this is not a PNG, JPEG or ICO image".into()),
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| error.to_string())?;
    Ok(square(&image))
}

/// The bytes of a regular file of at most [`MAX_FILE_BYTES`]: a folder, a device such as /dev/zero or a pipe is refused
/// before anything is read from it, and a file that grows while it is read is cut off at the ceiling.
fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let meta = file.metadata().map_err(|error| error.to_string())?;
    if !meta.is_file() {
        return Err("this is not a file".into());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("the file is over {} MB", MAX_FILE_BYTES / (1024 * 1024)));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("the file is over {} MB", MAX_FILE_BYTES / (1024 * 1024)));
    }
    Ok(bytes)
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
    for pixel in rgba.as_chunks_mut::<4>().0 {
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

/// The hue of the crescent of the icon that ships, in degrees.
const BASE_HUE: f32 = 38.9;

/// The icon in the colour of a theme's accent; grey for an accent with no colour of its own.
pub fn in_accent(accent: egui::Color32) -> IconData {
    let (hue, saturation, _) = rgb_to_hsl(accent.r(), accent.g(), accent.b());
    recolour(&default_icon(), if saturation < 0.12 { None } else { Some((hue - BASE_HUE).rem_euclid(360.0)) })
}

/// The icon a setting asks for: `""` the default, `theme` the default in the accent of the theme, `builtin:<name>`,
/// or the path of an image. A bad image is an error the window can show; the caller then keeps the default.
pub fn load(setting: &str, accent: egui::Color32) -> Result<IconData, String> {
    let setting = setting.trim();
    if setting.is_empty() {
        return Ok(default_icon());
    }
    if setting == "theme" {
        return Ok(in_accent(accent));
    }
    if let Some(name) = setting.strip_prefix("builtin:") {
        let (_, turn) = VARIANTS
            .iter()
            .find(|(known, _)| *known == name)
            .ok_or_else(|| format!("there is no icon named {name}"))?;
        return Ok(recolour(&default_icon(), *turn));
    }
    decode(&read_file(Path::new(setting))?)
}

pub fn shared(setting: &str, accent: egui::Color32) -> Arc<IconData> {
    Arc::new(load(setting, accent).unwrap_or_else(|_| default_icon()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCENT: egui::Color32 = egui::Color32::from_rgb(188, 180, 255);

    fn pixel(icon: &IconData, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * icon.width + x) * 4) as usize;
        icon.rgba[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn the_icon_that_ships_is_square_and_has_its_pixels() {
        let icon = default_icon();
        assert_eq!((icon.width, icon.height, icon.rgba.len()), (SIZE, SIZE, (SIZE * SIZE * 4) as usize));
        assert!(icon.rgba.as_chunks::<4>().0.iter().any(|px| px[3] > 200)); // not blank
    }

    #[test]
    fn a_colour_variant_changes_the_colour_and_keeps_the_shape() {
        let original = default_icon();
        let amber = original
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 200 && px[0] > 200 && px[1] > 140 && px[2] < 90)
            .count();
        assert!(amber > 500, "the amber crescent should be there: {amber}");
        for (name, turn) in VARIANTS.iter().skip(1) {
            let other = recolour(&original, *turn);
            assert_ne!(other.rgba, original.rgba, "{name}");
            assert!(
                other
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(original.rgba.as_chunks::<4>().0.iter())
                    .all(|(a, b)| a[3] == b[3]),
                "{name}: alpha"
            );
        }
        let grey = recolour(&original, None);
        assert!(grey.rgba.as_chunks::<4>().0.iter().all(|px| px[0] == px[1] && px[1] == px[2]));
        assert_eq!(
            recolour(&original, Some(0.0))
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .zip(original.rgba.as_chunks::<4>().0.iter())
                .filter(|(a, b)| a[..3].iter().zip(b[..3].iter()).any(|(x, y)| x.abs_diff(*y) > 2))
                .count(),
            0
        );
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
        assert_eq!(load("", ACCENT).unwrap(), default_icon());
        assert_ne!(load("builtin:sky", ACCENT).unwrap(), default_icon());
        assert!(
            load("builtin:nope", ACCENT).is_err()
                && is_builtin("builtin:mint")
                && !is_builtin("builtin:nope")
                && !is_builtin("")
        );
        assert!(load("/no/such/file.png", ACCENT).is_err());
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
        let icon = load(path.to_str().unwrap(), ACCENT).unwrap();
        assert_eq!((icon.width, icon.height), (SIZE, SIZE));
        assert_eq!(pixel(&icon, SIZE / 2, SIZE / 2), [255, 0, 0, 255]); // the middle third is what is kept
        assert_eq!(pixel(&icon, 2, 2), [255, 0, 0, 255]);
        std::fs::write(dir.path().join("bad.png"), b"not an image").unwrap();
        assert!(load(dir.path().join("bad.png").to_str().unwrap(), ACCENT).is_err());
    }

    #[test]
    fn the_icon_can_take_the_colour_of_the_theme() {
        let themed = load("theme", ACCENT).unwrap();
        assert_ne!(themed, default_icon());
        let grey = load("theme", egui::Color32::from_rgb(200, 200, 200)).unwrap();
        assert!(grey.rgba.as_chunks::<4>().0.iter().all(|px| px[0] == px[1] && px[1] == px[2]));
        let amber = in_accent(egui::Color32::from_rgb(255, 181, 58));
        let differing = amber
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(default_icon().rgba.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a[..3].iter().zip(b[..3].iter()).any(|(x, y)| x.abs_diff(*y) > 8))
            .count();
        assert!(differing < 200, "the base hue changes (almost) nothing: {differing}");
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::new(width, height).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn png_jpeg_and_ico_files_all_become_icons_and_an_ico_gives_its_largest_picture() {
        let dir = tempfile::tempdir().unwrap();
        let mut red = image::RgbaImage::new(64, 64);
        red.pixels_mut().for_each(|px| *px = image::Rgba([220, 20, 20, 255]));
        let jpeg = dir.path().join("a.jpeg");
        image::DynamicImage::ImageRgba8(red.clone()).to_rgb8().save(&jpeg).unwrap();
        assert_eq!(load(jpeg.to_str().unwrap(), ACCENT).unwrap().width, SIZE);
        // an ICO with a small picture and a large one: the large one is used
        let ico = dir.path().join("app.ico");
        let mut small = image::RgbaImage::new(16, 16);
        small.pixels_mut().for_each(|px| *px = image::Rgba([0, 0, 255, 255]));
        let mut large = image::RgbaImage::new(128, 128);
        large.pixels_mut().for_each(|px| *px = image::Rgba([0, 255, 0, 255]));
        let mut frames = Vec::new();
        for picture in [&small, &large] {
            frames.push(
                image::codecs::ico::IcoFrame::as_png(
                    picture.as_raw(),
                    picture.width(),
                    picture.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap(),
            );
        }
        image::codecs::ico::IcoEncoder::new(std::fs::File::create(&ico).unwrap()).encode_images(&frames).unwrap();
        let icon = load(ico.to_str().unwrap(), ACCENT).unwrap();
        assert_eq!((icon.width, pixel(&icon, SIZE / 2, SIZE / 2)), (SIZE, [0, 255, 0, 255]));
        // the content decides, not the name: a PNG called .ico and an ICO called .png are both read
        std::fs::copy(&ico, dir.path().join("really-ico.png")).unwrap();
        std::fs::write(dir.path().join("really-png.ico"), png_bytes(32, 32)).unwrap();
        assert!(load(dir.path().join("really-ico.png").to_str().unwrap(), ACCENT).is_ok());
        assert!(load(dir.path().join("really-png.ico").to_str().unwrap(), ACCENT).is_ok());
    }

    #[test]
    fn a_picture_that_is_too_big_or_not_an_allowed_kind_is_refused_before_it_is_built() {
        let dir = tempfile::tempdir().unwrap();
        // 5000 x 5000 of one colour is a small file that would take 100 MB: over the limit on the sides
        let bomb = dir.path().join("bomb.png");
        std::fs::write(&bomb, png_bytes(MAX_SIDE + 904, MAX_SIDE + 904)).unwrap();
        assert!(std::fs::metadata(&bomb).unwrap().len() < 1_000_000);
        let started = std::time::Instant::now();
        let refused = load(bomb.to_str().unwrap(), ACCENT).unwrap_err();
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{refused}");
        // a kind that is not on the list, even when the name says PNG
        let mut gif = Vec::new();
        image::RgbaImage::new(8, 8).write_to(&mut Cursor::new(&mut gif), image::ImageFormat::Bmp).unwrap();
        let named = dir.path().join("bitmap.png");
        std::fs::write(&named, &gif).unwrap();
        assert!(load(named.to_str().unwrap(), ACCENT).unwrap_err().contains("only PNG, JPEG and ICO"));
        // text, an empty file and a file that is only a signature
        for (name, bytes) in
            [("text.png", b"hello".to_vec()), ("empty.png", Vec::new()), ("sig.png", b"\x89PNG\r\n\x1a\n".to_vec())]
        {
            std::fs::write(dir.path().join(name), bytes).unwrap();
            assert!(load(dir.path().join(name).to_str().unwrap(), ACCENT).is_err(), "{name}");
        }
    }

    #[test]
    fn only_a_regular_file_of_a_sane_size_is_read() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path().to_str().unwrap(), ACCENT).unwrap_err().contains("not a file"));
        #[cfg(unix)]
        for device in ["/dev/zero", "/dev/null"] {
            if Path::new(device).exists() {
                let started = std::time::Instant::now();
                assert!(load(device, ACCENT).is_err(), "{device}");
                assert!(started.elapsed() < std::time::Duration::from_secs(1), "{device} was read");
            }
        }
        let big = dir.path().join("big.png");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap(); // sparse: costs no disk
        assert!(load(big.to_str().unwrap(), ACCENT).unwrap_err().contains("over 8 MB"));
    }

    #[test]
    fn damaged_pictures_fail_cleanly_and_never_panic() {
        // every prefix and a few corruptions of real files: an error or a picture, never a crash
        let png = png_bytes(40, 40);
        let mut jpeg = Vec::new();
        image::RgbImage::new(40, 40).write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        for original in [&png, &jpeg] {
            for cut in (0..original.len()).step_by((original.len() / 40).max(1)) {
                let _ = decode(&original[..cut]);
            }
            for position in (0..original.len()).step_by((original.len() / 60).max(1)) {
                let mut damaged = original.clone();
                damaged[position] ^= 0xff;
                let _ = decode(&damaged);
                damaged[position] = 0;
                let _ = decode(&damaged);
            }
        }
    }
}
