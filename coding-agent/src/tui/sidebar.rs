use ruse::prelude::*;

use super::theme;

pub struct SidebarState<'a> {
    pub width: usize,
    pub height: usize,
    pub model_id: &'a str,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub context_window: u64,
    pub total_cost: f64,
    pub thinking_level: &'a str,
    pub active_tools: &'a [String],
    pub cwd: &'a str,
}

pub fn render_sidebar(s: &SidebarState) -> String {
    let inner_w = s.width.saturating_sub(3); // left border + padding
    let mut lines = Vec::new();

    // Session name (placeholder)
    lines.push(theme::subtle_style().render(&["New Session"]));
    lines.push(String::new());

    // CWD — truncate from the left if too long, show with ~ for home
    let cwd_display = shorten_cwd(s.cwd, inner_w);
    lines.push(theme::half_muted_style().render(&[&cwd_display]));
    lines.push(String::new());

    // Model block
    let model_line = format!(
        "{} {}",
        theme::primary_style().render(&[theme::MODEL_ICON]),
        Style::new()
            .foreground(Color::parse(theme::FG_SUBTLE))
            .bold(true)
            .render(&[s.model_id]),
    );
    lines.push(model_line);

    // Thinking level (if not off)
    if s.thinking_level != "off" {
        let think_line = format!("  Reasoning {}", capitalize(s.thinking_level));
        lines.push(theme::half_muted_style().render(&[&think_line]));
    }

    // Context + cost on one line
    let ctx_pct = if s.context_window > 0 {
        ((s.tokens_in + s.tokens_out) as f64 / s.context_window as f64 * 100.0) as u64
    } else {
        0
    };
    let tokens_total = s.tokens_in + s.tokens_out;
    let ctx_cost = format!(
        "  {}% ({}) ${:.2}",
        ctx_pct,
        format_tokens(tokens_total),
        s.total_cost
    );
    lines.push(theme::half_muted_style().render(&[&ctx_cost]));

    // Active tools
    if !s.active_tools.is_empty() {
        lines.push(String::new());
        lines.push(section_header("Active", inner_w));
        for tool in s.active_tools {
            let tool_line = format!(
                "{} {}",
                theme::green_dark_style().render(&[theme::TOOL_PENDING]),
                theme::half_muted_style().render(&[tool.as_str()]),
            );
            lines.push(tool_line);
        }
    }

    // Pad to height
    while lines.len() < s.height {
        lines.push(String::new());
    }
    lines.truncate(s.height);

    let body = lines.join("\n");
    Style::new()
        .padding_left(1)
        .padding_right(1)
        .border(NORMAL_BORDER, &[false, false, false, true])
        .border_foreground(Color::parse(theme::FG_MUTED))
        .width(s.width as u16)
        .render(&[&body])
}

fn section_header(label: &str, width: usize) -> String {
    let label_styled = theme::half_muted_style().render(&[label]);
    let label_w = label.len() + 1; // +1 for space
    let rule_w = width.saturating_sub(label_w);
    let rule = theme::muted_style().render(&[&theme::SECTION_SEP.repeat(rule_w)]);
    format!("{} {}", label_styled, rule)
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn shorten_cwd(cwd: &str, max_w: usize) -> String {
    // Replace home dir with ~
    let home = std::env::var("HOME").unwrap_or_default();
    let display = if !home.is_empty() && cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_string()
    };

    if display.len() <= max_w {
        display
    } else {
        // Truncate from left, keeping the rightmost path segments
        let truncated = &display[display.len().saturating_sub(max_w.saturating_sub(1))..];
        format!("…{}", truncated)
    }
}
