//! The glowing glass sphere in the middle of a call. It is painted pixel by pixel on the CPU, so
//! it looks the same on every machine and can be checked in a test.
//!
//! It swirls slowly while the interviewer thinks, pulses with the interviewer's voice while they
//! speak, and breathes with yours while you speak.

use egui::{Color32, ColorImage};

use super::theme::{BERRY, BLUSH, CRANBERRY};

pub const SIZE: usize = 300;
/// The sphere fills this part of the picture; the rest is glow.
const RADIUS: f32 = 0.60;

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Seconds, any origin.
    pub time: f32,
    /// How loud the voice is, 0 to 1.
    pub level: f32,
    /// How fast the swirl turns; 1 is the resting speed.
    pub spin: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self { time: 0.0, level: 0.0, spin: 1.0 }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn channels(color: Color32) -> [f32; 3] {
    [f32::from(color.r()) / 255.0, f32::from(color.g()) / 255.0, f32::from(color.b()) / 255.0]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// One pixel as premultiplied RGBA; `x` and `y` run from -1 to 1.
pub fn pixel(x: f32, y: f32, p: &Params) -> [u8; 4] {
    let [cr, cg, cb] = channels(CRANBERRY);
    let berry = channels(BERRY);
    let blush = channels(BLUSH);
    let level = p.level.clamp(0.0, 1.0);
    let (qx, qy) = (x / RADIUS, y / RADIUS);
    let r2 = qx * qx + qy * qy;
    let r = r2.sqrt();

    // The glow around the sphere is light added to whatever is behind it. It dies out before the
    // edge of the picture, so there is no visible square.
    let edge = 1.0 / RADIUS;
    let fade = 1.0 - smoothstep(1.15, edge, r);
    let glow = (-(r - 1.0).max(0.0) * 3.4).exp() * (0.14 + 0.50 * level) * fade;
    let glow_color = [berry[0] * glow, berry[1] * glow, berry[2] * glow];

    if r >= 1.0 {
        return premultiplied(glow_color, 0.0);
    }

    let z = (1.0 - r2).sqrt();
    // the glass bends what is inside, more towards the edge
    let bend = 0.55 * (1.0 - z);
    let (mut ux, mut uy) = (qx * (1.0 - bend), qy * (1.0 - bend));
    let t = p.time * p.spin.max(0.05);
    let (sin_a, cos_a) = (t * 0.32).sin_cos();
    (ux, uy) = (ux * cos_a - uy * sin_a, ux * sin_a + uy * cos_a);
    let wx = (2.6 * uy + t * 0.9 + 1.7 * (2.1 * ux - t * 0.55).sin()).sin();
    let wy = (2.9 * ux - t * 0.7 + 1.5 * (2.3 * uy + t * 0.45).sin()).sin();
    let f1 = (6.5 * (ux + 0.32 * wx) + 4.2 * (uy + 0.32 * wy) + t * 1.3 + 3.0 * z).sin();
    let f2 = (4.8 * (ux - 0.28 * wy) - 6.1 * (uy + 0.30 * wx) - t * 1.1).sin();
    // Ribbons of glass where the waves peak: a dark berry body with a bright thin edge.
    let band = (smoothstep(0.55, 0.9, f1.abs()) + smoothstep(0.65, 0.95, f2.abs()) * 0.6).min(1.0);
    let edge =
        |f: f32, lo: f32, hi: f32| (smoothstep(lo, lo + 0.05, f.abs()) - smoothstep(hi, hi + 0.04, f.abs())).max(0.0);
    let outline = (edge(f1, 0.88, 0.96) + edge(f2, 0.90, 0.97) * 0.6).min(1.0);
    let body = 0.10 + 0.14 * (0.5 + 0.5 * (2.0 * ux + 1.3 * uy + t * 0.5).sin());

    let dark = [cr * 0.16, cg * 0.16, cb * 0.16];
    let lit = [berry[0] * 0.15, berry[1] * 0.15, berry[2] * 0.15];
    let inner = lerp3(dark, lit, (body * z * 3.0).min(1.0));
    let band_color = lerp3([cr, cg, cb], berry, band);
    let band_strength = band * (0.36 + 0.55 * level) * (0.5 + 0.5 * z);
    let outline_strength = outline * (0.45 + 0.6 * level) * (0.45 + 0.55 * (1.0 - z));
    let mut color = [0.0f32; 3];
    for i in 0..3 {
        color[i] = inner[i] + band_color[i] * band_strength + blush[i] * outline_strength;
    }

    // light caught along the edge of the glass
    let rim = (1.0 - z).powf(2.6);
    let rim_strength = rim * (0.50 + 0.55 * level);
    // a small highlight in the upper left
    let (hx, hy) = (qx + 0.36, qy + 0.44);
    let highlight = (-(hx * hx * 9.0 + hy * hy * 13.0)).exp() * 0.32;
    // the sphere's own light spilling a little over its edge
    let spill = (0.015 + 0.05 * level) * (1.0 - z);
    for i in 0..3 {
        color[i] += berry[i] * rim_strength + blush[i] * rim * rim * 0.30 + blush[i] * highlight + berry[i] * spill;
    }
    let alpha = smoothstep(1.0, 0.985, r);
    // under the anti-aliased edge the glow still shows
    let color = [color[0] * alpha + glow_color[0], color[1] * alpha + glow_color[1], color[2] * alpha + glow_color[2]];
    premultiplied(color, alpha)
}

fn premultiplied(color: [f32; 3], alpha: f32) -> [u8; 4] {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(color[0]), byte(color[1]), byte(color[2]), byte(alpha)]
}

/// The whole picture, ready to be an egui texture.
pub fn render(params: &Params) -> ColorImage {
    let mut pixels = Vec::with_capacity(SIZE * SIZE);
    for row in 0..SIZE {
        let y = (row as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
        for column in 0..SIZE {
            let x = (column as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
            let [r, g, b, a] = pixel(x, y, params);
            pixels.push(Color32::from_rgba_premultiplied(r, g, b, a));
        }
    }
    ColorImage { size: [SIZE, SIZE], source_size: egui::vec2(SIZE as f32, SIZE as f32), pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(image: &ColorImage) -> f32 {
        image.pixels.iter().map(|p| f32::from(p.r()) + f32::from(p.g()) + f32::from(p.b())).sum::<f32>()
            / image.pixels.len() as f32
    }

    #[test]
    fn the_sphere_is_opaque_in_the_middle_and_clear_at_the_corners() {
        let image = render(&Params::default());
        assert_eq!(image.pixels[SIZE / 2 * SIZE + SIZE / 2].a(), 255);
        assert_eq!(image.pixels[0].a(), 0);
        assert_eq!(image.pixels[SIZE * SIZE - 1].a(), 0);
    }

    #[test]
    fn the_glow_fades_to_nothing_before_the_edge_of_the_picture() {
        let image = render(&Params { level: 1.0, ..Params::default() });
        let corner = image.pixels[0];
        assert!(corner.r() < 3 && corner.g() < 3, "{corner:?}");
        // and it is visible just outside the sphere
        let outside = image.pixels[SIZE / 2 * SIZE + (SIZE as f32 * 0.5 + SIZE as f32 * 0.5 * 0.66) as usize];
        assert!(outside.r() > 20, "{outside:?}");
    }

    #[test]
    fn it_is_cranberry_not_some_other_colour() {
        let image = render(&Params { level: 0.6, time: 3.0, spin: 1.0 });
        let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
        for p in image.pixels.iter().filter(|p| p.a() > 0) {
            r += u64::from(p.r());
            g += u64::from(p.g());
            b += u64::from(p.b());
        }
        assert!(r * 10 > g * 14 && r * 10 > b * 14, "red must dominate: {r} {g} {b}");
    }

    #[test]
    fn a_louder_voice_makes_it_brighter() {
        let quiet = brightness(&render(&Params { level: 0.0, time: 2.0, spin: 1.0 }));
        let loud = brightness(&render(&Params { level: 1.0, time: 2.0, spin: 1.0 }));
        assert!(loud > quiet * 1.15, "{quiet} -> {loud}");
    }

    #[test]
    fn it_moves_with_time_and_is_the_same_for_the_same_time() {
        let a = render(&Params { time: 1.0, ..Params::default() });
        let b = render(&Params { time: 1.0, ..Params::default() });
        let c = render(&Params { time: 2.5, ..Params::default() });
        assert_eq!(a.pixels, b.pixels);
        assert_ne!(a.pixels, c.pixels);
    }

    #[test]
    fn nothing_overflows_for_odd_inputs() {
        for p in [Params { time: -50.0, level: -3.0, spin: 0.0 }, Params { time: 1e6, level: 9.0, spin: 100.0 }] {
            let image = render(&p);
            assert_eq!(image.pixels.len(), SIZE * SIZE);
        }
    }

    #[test]
    fn a_frame_costs_little() {
        let started = std::time::Instant::now();
        for i in 0..5 {
            let _ = render(&Params { time: i as f32, level: 0.5, spin: 1.0 });
        }
        let each = started.elapsed() / 5;
        assert!(each < std::time::Duration::from_millis(250), "{each:?} per frame even without full optimisation");
    }

    #[test]
    fn the_picture_can_be_written_out_for_a_look() {
        if let Ok(dir) = std::env::var("ORB_PNG_DIR") {
            for (name, level, time) in [("rest", 0.0, 1.0), ("talking", 0.8, 4.0)] {
                let image = render(&Params { time, level, spin: 1.0 });
                let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
                for p in &image.pixels {
                    // put it on the window colour so the glow reads
                    let bg = super::super::theme::BG;
                    let blend =
                        |c: u8, b: u8| (u16::from(c) + u16::from(b) * (255 - u16::from(p.a())) / 255).min(255) as u8;
                    rgba.extend_from_slice(&[blend(p.r(), bg.r()), blend(p.g(), bg.g()), blend(p.b(), bg.b()), 255]);
                }
                let buffer = ::image::RgbaImage::from_raw(SIZE as u32, SIZE as u32, rgba).unwrap();
                buffer.save(format!("{dir}/orb-{name}.png")).unwrap();
            }
        }
    }
}
