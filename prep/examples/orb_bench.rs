//! How long one picture of the sphere takes: `cargo run --release --example orb_bench`

use std::time::Instant;

use careertwin_prep::ui::orb::{Params, render_at};

fn main() {
    for size in [170usize, 256] {
        let frames = 40;
        let started = Instant::now();
        let mut checksum = 0u64;
        for i in 0..frames {
            let image = render_at(&Params { time: i as f32 * 0.1, level: 0.5, spin: 1.0 }, size);
            checksum += u64::from(image.pixels[size * size / 2].r());
        }
        let each = started.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
        println!("{size}x{size}: {each:.2} ms per picture (check {checksum})");
    }
}
