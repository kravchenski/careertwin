//! Writes the tray arrow at 16, 32 and 128 pixels as PNG files, to look at: `cargo run --example tray_preview -- <folder>`.
use careertwin_overlay::tray::{BLUE, Look, arrow_rgba};

fn main() {
    let folder = std::env::args().nth(1).expect("a folder to write the pictures in");
    for (name, look) in [("idle", Look::Idle), ("active", Look::Active)] {
        for size in [16u32, 32, 128] {
            let pixels = arrow_rgba(size, BLUE, look);
            // on a dark taskbar colour, so the transparent parts show as they will
            let mut flat = Vec::with_capacity((size * size * 3) as usize);
            for pixel in pixels.chunks(4) {
                let a = u32::from(pixel[3]);
                for value in &pixel[..3] {
                    flat.push(((u32::from(*value) * a + 32 * (255 - a)) / 255) as u8);
                }
            }
            image::save_buffer(
                format!("{folder}/arrow-{name}-{size}.png"),
                &flat,
                size,
                size,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        }
    }
}
