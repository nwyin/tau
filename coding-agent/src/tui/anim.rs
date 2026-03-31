use std::time::Instant;

use ruse::prelude::*;

use super::theme;

pub struct GradientSpinner {
    start: Instant,
    frame: usize,
    label: String,
}

impl GradientSpinner {
    pub fn new(label: &str) -> Self {
        Self {
            start: Instant::now(),
            frame: 0,
            label: label.to_string(),
        }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
    }

    pub fn tick_duration() -> std::time::Duration {
        std::time::Duration::from_millis(150)
    }

    pub fn view(&self) -> String {
        // Simple animated dots spinner — avoids per-character ANSI that
        // can get mangled by the cellbuf screen parser.
        let dots = match self.frame % 4 {
            0 => "   ",
            1 => ".  ",
            2 => ".. ",
            _ => "...",
        };

        let elapsed = self.start.elapsed();
        let secs = elapsed.as_secs();

        let label = Style::new()
            .foreground(Color::parse(theme::FG_SUBTLE))
            .italic(true)
            .render(&[&self.label]);

        let timing = if secs > 0 {
            let time_str = if secs >= 60 {
                format!(" {}m{}s", secs / 60, secs % 60)
            } else {
                format!(" {}s", secs)
            };
            theme::half_muted_style().render(&[&time_str])
        } else {
            String::new()
        };

        format!("{}{}{}", label, dots, timing)
    }
}
