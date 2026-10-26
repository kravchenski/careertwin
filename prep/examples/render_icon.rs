//! Draws the program's icon: the glass sphere on a dark rounded square.
//! `cargo run --example render_icon -- packaging/careertwin-prep.png 512`

use careertwin_prep::ui::orb::{self, Params};
use careertwin_prep::ui::theme::{BG, COCOA};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().cloned().unwrap_or_else(|| "careertwin-prep.png".into());
    let size: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(512);
    let mut image = ::image::RgbaImage::new(size, size);
    let radius = size as f32 * 0.22;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // inside the rounded square?
            let (cx, cy) = (fx.clamp(radius, size as f32 - radius), fy.clamp(radius, size as f32 - radius));
            let inside = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() <= radius;
            if !inside {
                continue;
            }
            let t = fy / size as f32;
            let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
            let (mut r, mut g, mut b) = (mix(COCOA.r(), BG.r()), mix(COCOA.g(), BG.g()), mix(COCOA.b(), BG.b()));
            // the sphere fills most of the square, a little above the middle
            let (ox, oy) = ((fx / size as f32 * 2.0 - 1.0) * 1.12, ((fy / size as f32 - 0.02) * 2.0 - 1.0) * 1.12);
            let [pr, pg, pb, pa] = orb::pixel(ox, oy, &Params { time: 2.2, level: 0.55, spin: 1.0 });
            let keep = 255 - u16::from(pa);
            r = (u16::from(pr) + u16::from(r) * keep / 255).min(255) as u8;
            g = (u16::from(pg) + u16::from(g) * keep / 255).min(255) as u8;
            b = (u16::from(pb) + u16::from(b) * keep / 255).min(255) as u8;
            image.put_pixel(x, y, ::image::Rgba([r, g, b, 255]));
        }
    }
    image.save(&out).expect("the icon could not be written");
    println!("{out}");
}
