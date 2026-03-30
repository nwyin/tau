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
}

pub fn render_sidebar(s: &SidebarState) -> String {
    let mut lines = Vec::new();

    // Model info
    let model_line = format!(
        "{} {}",
        theme::primary_style().render(&[theme::MODEL_ICON]),
        theme::subtle_style().render(&[s.model_id]),
    );
    lines.push(model_line);
    lines.push(String::new());

    // Context usage
    let ctx_pct = if s.context_window > 0 {
        ((s.tokens_in + s.tokens_out) as f64 / s.context_window as f64 * 100.0) as u64
    } else {
        0
    };
    let ctx_line = format!(
        "ctx {}%  {} / {}",
        ctx_pct,
        format_tokens(s.tokens_in + s.tokens_out),
        format_tokens(s.context_window),
    );
    lines.push(theme::half_muted_style().render(&[&ctx_line]));

    // Cost
    if s.total_cost > 0.0 {
        let cost_line = format!("${:.2}", s.total_cost);
        lines.push(theme::half_muted_style().render(&[&cost_line]));
    }

    // Thinking level
    if s.thinking_level != "off" {
        let think_line = format!("think: {}", s.thinking_level);
        lines.push(theme::half_muted_style().render(&[&think_line]));
    }

    // Active tools
    if !s.active_tools.is_empty() {
        lines.push(String::new());
        lines.push(
            theme::muted_style().render(&[&theme::SECTION_SEP.repeat(s.width.saturating_sub(2))]),
        );
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

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
