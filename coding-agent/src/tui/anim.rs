use std::time::Instant;

use ruse::prelude::*;

use super::theme;

const CHARSET: &[u8] = b"0123456789abcdefABCDEF~!@#$%^&*()+=/";
const ANIM_WIDTH: usize = 15;
const ELLIPSIS_CYCLE: usize = 8;

fn pseudo_rand(seed: u64) -> u64 {
    let mut s = seed;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    s
}

pub struct GradientSpinner {
    start: Instant,
    frame: usize,
    label: String,
    birth_offsets: [usize; ANIM_WIDTH],
    color_a: (u8, u8, u8),
    color_b: (u8, u8, u8),
}

impl GradientSpinner {
    pub fn new(label: &str) -> Self {
        let now = Instant::now();
        let mut offsets = [0usize; ANIM_WIDTH];
        let seed = now.elapsed().subsec_nanos() as u64;
        for (i, offset) in offsets.iter_mut().enumerate() {
            *offset = (pseudo_rand(seed.wrapping_add(i as u64 * 7)) % 20) as usize;
        }

        Self {
            start: now,
            frame: 0,
            label: label.to_string(),
            birth_offsets: offsets,
            color_a: (0x9d, 0x84, 0xb7), // Charple
            color_b: (0xd4, 0xee, 0x04), // Dolly
        }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
    }

    pub fn tick_duration() -> std::time::Duration {
        std::time::Duration::from_millis(50)
    }

    #[allow(dead_code)]
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    pub fn view(&self) -> String {
        let mut chars = String::new();

        for i in 0..ANIM_WIDTH {
            if self.frame < self.birth_offsets[i] {
                chars.push(' ');
                continue;
            }

            let age = self.frame - self.birth_offsets[i];
            let char_seed = pseudo_rand((age as u64).wrapping_mul(31).wrapping_add(i as u64 * 17));
            let ch = CHARSET[(char_seed % CHARSET.len() as u64) as usize] as char;

            let cycle_offset = (self.frame as f64 * 0.15) % 1.0;
            let pos = ((i as f64 / ANIM_WIDTH as f64) + cycle_offset) % 1.0;

            let (r, g, b) = lerp_color(self.color_a, self.color_b, pos);
            let color_str = format!("#{:02x}{:02x}{:02x}", r, g, b);

            let styled = Style::new()
                .foreground(Color::parse(&color_str))
                .bold(true)
                .render(&[&ch.to_string()]);
            chars.push_str(&styled);
        }

        let ellipsis_phase = (self.frame / ELLIPSIS_CYCLE) % 4;
        let dots = match ellipsis_phase {
            0 => "",
            1 => ".",
            2 => "..",
            _ => "...",
        };

        let label = Style::new()
            .foreground(Color::parse(theme::FG_BASE))
            .render(&[&self.label]);

        format!("  {} {}{}", chars, label, dots)
    }
}

fn lerp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let r = (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8;
    let g = (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8;
    let bl = (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8;
    (r, g, bl)
}
