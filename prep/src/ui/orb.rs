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

/// The twisting strands: a smooth field over the sphere whose iso-lines are the ribbons. Returns
/// the field and its gradient, which says which way a ribbon faces.
fn field(p: [f32; 3], t: f32) -> (f32, [f32; 3]) {
    /// How tightly the field winds, how far it is bent, and how fast the bend repeats.
    const CARRIER: f32 = 4.7;
    const BEND: f32 = 1.5;
    const REPEAT: f32 = 2.0;
    let (sa, ca) = (REPEAT * p[1] + t * 0.7).sin_cos();
    let (su, cu) = (CARRIER * p[0] + BEND * sa).sin_cos();
    let (sb, cb) = (REPEAT * p[2] - t * 0.55).sin_cos();
    let (sv, cv) = (CARRIER * p[1] + BEND * sb).sin_cos();
    let (sc, cc) = (REPEAT * p[0] + t * 0.9).sin_cos();
    let (sw, cw) = (CARRIER * p[2] + BEND * sc).sin_cos();
    let value = su + sv + sw;
    let pull = BEND * REPEAT;
    let gradient = [CARRIER * cu + pull * cw * cc, CARRIER * cv + pull * cu * ca, CARRIER * cw + pull * cv * cb];
    (value, gradient)
}

/// How one surface of glass looks at a point: how much of a ribbon is there, and its colour.
/// `side` is 1 for the near wall and -1 for the far one.
fn strands(p: [f32; 3], t: f32, level: f32) -> (f32, [f32; 3]) {
    let scale = 1.5;
    let (value, gradient) = field(p, t);
    let phase = value * scale;
    let (s, c) = phase.sin_cos();
    // a ribbon is where the wave is near zero; wider when the voice is louder
    let width = 0.78 + 0.07 * level;
    let across = (s / width).clamp(-1.0, 1.0);
    let body = 1.0 - smoothstep(0.80, 1.0, across.abs());
    if body <= 0.0 {
        return (0.0, [0.0; 3]);
    }
    // the ribbon is a tube: it faces the viewer in the middle and turns away at its edges
    let depth = (1.0 - across * across).max(0.0).sqrt();
    let slope = [gradient[0] * c, gradient[1] * c];
    let length = (slope[0] * slope[0] + slope[1] * slope[1]).sqrt().max(1e-4);
    let facing = [slope[0] / length * across, slope[1] / length * across, depth];
    let norm = (facing[0] * facing[0] + facing[1] * facing[1] + facing[2] * facing[2]).sqrt().max(1e-4);
    let light = [-0.50, -0.62, 0.60];
    let shine = ((facing[0] * light[0] + facing[1] * light[1] + facing[2] * light[2]) / norm).max(0.0);
    let glint = shine.powf(14.0);
    let core = depth * depth;
    let berry = channels(BERRY);
    let cranberry = channels(CRANBERRY);
    let blush = channels(BLUSH);
    let mut color = [0.0; 3];
    for i in 0..3 {
        let base = cranberry[i] + (berry[i] - cranberry[i]) * core;
        color[i] = base * (0.30 + 0.70 * core) + blush[i] * glint * 0.95;
    }
    (body, color)
}

fn rotate(p: [f32; 3], y_angle: f32, x_angle: f32) -> [f32; 3] {
    let (sy, cy) = y_angle.sin_cos();
    let (sx, cx) = x_angle.sin_cos();
    let q = [p[0] * cy + p[2] * sy, p[1], -p[0] * sy + p[2] * cy];
    [q[0], q[1] * cx - q[2] * sx, q[1] * sx + q[2] * cx]
}

/// One pixel as premultiplied RGBA; `x` and `y` run from -1 to 1.
pub fn pixel(x: f32, y: f32, p: &Params) -> [u8; 4] {
    let berry = channels(BERRY);
    let blush = channels(BLUSH);
    let cranberry = channels(CRANBERRY);
    let level = p.level.clamp(0.0, 1.0);
    let (qx, qy) = (x / RADIUS, y / RADIUS);
    let r2 = qx * qx + qy * qy;
    let r = r2.sqrt();

    // The glow around the sphere is light added to whatever is behind it; it dies out before the
    // edge of the picture, so there is no visible square.
    let edge = 1.0 / RADIUS;
    let fade = 1.0 - smoothstep(1.15, edge, r);
    let glow = (-(r - 1.0).max(0.0) * 3.0).exp() * (0.14 + 0.40 * level) * fade;
    let glow_color = [berry[0] * glow, berry[1] * glow, berry[2] * glow];
    if r >= 1.0 {
        return premultiplied(glow_color, 0.0);
    }

    let z = (1.0 - r2).sqrt();
    let t = p.time * p.spin.max(0.05);
    let (spin, tilt) = (t * 0.22, 0.45);
    // the same ribbons seen through the near wall and the far wall of the glass
    let near = rotate([qx, qy, z], spin, tilt);
    let far = rotate([qx, qy, -z], spin, tilt);
    let (near_body, near_color) = strands(near, t, level);
    let (far_body, far_color) = strands(far, t * 0.9 + 4.0, level);

    // inside the glass: dark, with a faint warm depth toward the middle
    let depth_glow = (1.0 - r) * (0.05 + 0.06 * level);
    let mut color = [
        cranberry[0] * 0.10 + berry[0] * depth_glow,
        cranberry[1] * 0.10 + berry[1] * depth_glow,
        cranberry[2] * 0.10 + berry[2] * depth_glow,
    ];
    let boost = 0.80 + 0.40 * level;
    for i in 0..3 {
        // the far ribbons show dimmer and are hidden where a near ribbon covers them
        color[i] += far_color[i] * far_body * 0.55 * (1.0 - near_body * 0.8) * boost;
        color[i] += near_color[i] * near_body * boost;
    }
    // light caught along the edge of the glass
    let rim = (1.0 - z).powf(2.4);
    let rim_strength = rim * (0.50 + 0.35 * level);
    // a small soft highlight in the upper left, where a lamp would reflect
    let (hx, hy) = (qx + 0.38, qy + 0.44);
    let highlight = (-(hx * hx * 11.0 + hy * hy * 15.0)).exp() * 0.35;
    for i in 0..3 {
        color[i] += berry[i] * rim_strength + blush[i] * rim * rim * 0.30 + blush[i] * highlight;
    }
    let alpha = smoothstep(1.0, 0.985, r);
    let color = [color[0] * alpha + glow_color[0], color[1] * alpha + glow_color[1], color[2] * alpha + glow_color[2]];
    premultiplied(color, alpha)
}

fn premultiplied(color: [f32; 3], alpha: f32) -> [u8; 4] {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(color[0]), byte(color[1]), byte(color[2]), byte(alpha)]
}

/// The whole picture, ready to be an egui texture. The rows are shared out among the cores, so
/// the sphere can turn smoothly without taking the window's time.
pub fn render(params: &Params) -> ColorImage {
    let threads = std::thread::available_parallelism().map_or(2, std::num::NonZero::get).clamp(1, 8);
    let band = SIZE.div_ceil(threads);
    let mut pixels = vec![Color32::TRANSPARENT; SIZE * SIZE];
    std::thread::scope(|scope| {
        for (index, rows) in pixels.chunks_mut(band * SIZE).enumerate() {
            scope.spawn(move || {
                for (offset, row) in rows.chunks_mut(SIZE).enumerate() {
                    let y = ((index * band + offset) as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                    for (column, target) in row.iter_mut().enumerate() {
                        let x = (column as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                        let [r, g, b, a] = pixel(x, y, params);
                        *target = Color32::from_rgba_premultiplied(r, g, b, a);
                    }
                }
            });
        }
    });
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
