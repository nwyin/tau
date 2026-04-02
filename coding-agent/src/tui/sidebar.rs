use ruse::prelude::*;

use super::theme;
use crate::tools::TodoItem;

/// Thread entry for sidebar display.
pub struct SidebarThread<'a> {
    pub alias: &'a str,
    pub status: SidebarThreadStatus,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SidebarThreadStatus {
    Running,
    Completed,
    Failed,
}

pub struct SidebarState<'a> {
    pub width: usize,
    pub height: usize,
    pub session_id: Option<&'a str>,
    pub model_id: &'a str,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub context_window: u64,
    pub total_cost: f64,
    pub thinking_level: &'a str,
    pub active_tools: &'a [String],
    pub todos: &'a [TodoItem],
    pub cwd: &'a str,
    /// Thread entries for sidebar navigation.
    pub threads: &'a [SidebarThread<'a>],
    /// Currently selected thread index (when sidebar is focused).
    pub selected_thread: Option<usize>,
}

pub fn render_sidebar(s: &SidebarState) -> String {
    let inner_w = s.width.saturating_sub(3); // left border + padding
    let mut lines = Vec::new();

    // Session identifier
    let session_label = match s.session_id {
        Some(id) => format!("Session {}", id),
        None => "Ephemeral".to_string(),
    };
    lines.push(theme::subtle_style().render(&[&session_label]));
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

    // Active tools — deduplicate and count, cap at max_tools lines
    if !s.active_tools.is_empty() {
        lines.push(String::new());
        lines.push(section_header("Active", inner_w));

        // Count occurrences of each tool name
        let mut tool_counts: Vec<(String, usize)> = Vec::new();
        for tool in s.active_tools {
            if let Some(entry) = tool_counts.iter_mut().find(|(t, _)| t == tool) {
                entry.1 += 1;
            } else {
                tool_counts.push((tool.clone(), 1));
            }
        }

        let max_tools = 8;
        for (i, (name, count)) in tool_counts.iter().enumerate() {
            if i >= max_tools {
                let remaining: usize = tool_counts[i..].iter().map(|(_, c)| c).sum();
                let more_line =
                    theme::half_muted_style().render(&[&format!("  +{} more", remaining)]);
                lines.push(more_line);
                break;
            }
            let label = if *count > 1 {
                format!("{} x{}", name, count)
            } else {
                name.clone()
            };
            // Truncate to sidebar width (account for "● " prefix)
            let max_label = inner_w.saturating_sub(2);
            let display = if label.len() > max_label {
                format!("{}…", &label[..max_label.saturating_sub(1)])
            } else {
                label
            };
            let tool_line = format!(
                "{} {}",
                theme::green_dark_style().render(&[theme::TOOL_PENDING]),
                theme::half_muted_style().render(&[&display]),
            );
            lines.push(tool_line);
        }
    }

    // Thread entries for navigation
    if !s.threads.is_empty() {
        lines.push(String::new());
        lines.push(section_header("Threads", inner_w));

        for (i, thread) in s.threads.iter().enumerate() {
            let is_selected = s.selected_thread == Some(i);
            let (icon, icon_style) = match thread.status {
                SidebarThreadStatus::Running => (theme::TOOL_PENDING, theme::green_dark_style()),
                SidebarThreadStatus::Completed => (theme::TOOL_SUCCESS, theme::green_style()),
                SidebarThreadStatus::Failed => (theme::TOOL_ERROR, theme::red_style()),
            };
            // Truncate alias to fit sidebar
            let max_alias = inner_w.saturating_sub(4); // "  X " prefix
            let alias = if thread.alias.len() > max_alias {
                format!("{}…", &thread.alias[..max_alias.saturating_sub(1)])
            } else {
                thread.alias.to_string()
            };
            let label_style = if is_selected {
                theme::subtle_style().bold(true)
            } else {
                theme::half_muted_style()
            };
            let prefix = if is_selected { ">" } else { " " };
            let line = format!(
                "{}{} {}",
                theme::primary_style().render(&[prefix]),
                icon_style.render(&[icon]),
                label_style.render(&[&alias]),
            );
            lines.push(line);

            if lines.len() >= s.height.saturating_sub(1) {
                break;
            }
        }
    }

    // Todo progress
    if !s.todos.is_empty() {
        lines.push(String::new());
        lines.push(section_header("Progress", inner_w));

        let total = s.todos.len();
        let done = s.todos.iter().filter(|t| t.status == "completed").count();
        let counter = format!("[{}/{}]", done, total);
        lines.push(theme::half_muted_style().render(&[&counter]));

        for item in s.todos {
            let (icon, icon_style) = match item.status.as_str() {
                "completed" => ("\u{2713}", theme::green_style()), // ✓
                "in_progress" => ("\u{2192}", theme::green_dark_style()), // →
                _ => ("\u{25CB}", theme::half_muted_style()),      // ○
            };
            let content_style = if item.status == "in_progress" {
                theme::subtle_style()
            } else {
                theme::half_muted_style()
            };
            // Truncate content to fit sidebar
            let max_content = inner_w.saturating_sub(4); // "  X " prefix
            let content = if item.content.len() > max_content {
                format!("{}…", &item.content[..max_content.saturating_sub(1)])
            } else {
                item.content.clone()
            };
            let line = format!(
                "  {} {}",
                icon_style.render(&[icon]),
                content_style.render(&[&content]),
            );
            lines.push(line);

            // Stop if we'd overflow the sidebar height
            if lines.len() >= s.height.saturating_sub(1) {
                break;
            }
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
