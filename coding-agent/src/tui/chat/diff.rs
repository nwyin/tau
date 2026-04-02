use ruse::prelude::*;
use serde_json::Value;

use crate::tui::theme;

/// Render a file_edit diff from the tool result details JSON.
///
/// Expects fields: old_string, new_string, start_line (1-based),
/// context_before (array), context_after (array).
pub fn render_edit_diff(details: &Value, width: usize) -> Option<String> {
    let success = details.get("success")?.as_bool()?;
    if !success {
        return None;
    }
    let old_string = details.get("old_string")?.as_str()?;
    let new_string = details.get("new_string")?.as_str()?;
    let start_line = details.get("start_line")?.as_u64()? as usize;

    let ctx_before: Vec<&str> = details
        .get("context_before")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let ctx_after: Vec<&str> = details
        .get("context_after")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let old_lines: Vec<&str> = old_string.lines().collect();
    let new_lines: Vec<&str> = new_string.lines().collect();

    // Summary header
    let summary = diff_summary(old_lines.len(), new_lines.len());
    let summary_styled = format!(
        "{} {}",
        theme::half_muted_style().render(&["\u{2514}"]),
        theme::half_muted_style().render(&[&summary]),
    );

    let mut lines = vec![summary_styled];

    // Line number gutter width
    let max_line = start_line + new_lines.len() + ctx_after.len();
    let gutter_w = max_line.to_string().len().max(4);
    // Content width after gutter + marker + spaces
    let content_w = width.saturating_sub(gutter_w + 5);

    // Context before
    let ctx_start = start_line.saturating_sub(ctx_before.len());
    for (i, line) in ctx_before.iter().enumerate() {
        let ln = ctx_start + i;
        let truncated = truncate_line(line, content_w);
        lines.push(format!(
            "  {} {}",
            theme::muted_style().render(&[&format!("{:>gutter_w$}", ln)]),
            theme::half_muted_style().render(&[&truncated]),
        ));
    }

    // Removed lines (old)
    let removed_style = Style::new()
        .foreground(Color::parse(theme::RED))
        .strikethrough(true);
    let removed_bg = Style::new().foreground(Color::parse(theme::RED));
    for (i, line) in old_lines.iter().enumerate() {
        let ln = start_line + i;
        let truncated = truncate_line(line, content_w);
        lines.push(format!(
            "  {} {} {}",
            removed_bg.render(&[&format!("{:>gutter_w$}", ln)]),
            removed_bg.render(&["-"]),
            removed_style.render(&[&truncated]),
        ));
    }

    // Added lines (new)
    let added_style = Style::new().foreground(Color::parse(theme::GREEN));
    for (i, line) in new_lines.iter().enumerate() {
        let ln = start_line + i;
        let truncated = truncate_line(line, content_w);
        lines.push(format!(
            "  {} {} {}",
            added_style.render(&[&format!("{:>gutter_w$}", ln)]),
            added_style.render(&["+"]),
            added_style.render(&[&truncated]),
        ));
    }

    // Context after
    let after_start = start_line + new_lines.len();
    for (i, line) in ctx_after.iter().enumerate() {
        let ln = after_start + i;
        let truncated = truncate_line(line, content_w);
        lines.push(format!(
            "  {} {}",
            theme::muted_style().render(&[&format!("{:>gutter_w$}", ln)]),
            theme::half_muted_style().render(&[&truncated]),
        ));
    }

    Some(lines.join("\n"))
}

/// Render a file_write new-file creation as an all-green diff.
pub fn render_create_diff(details: &Value, width: usize) -> Option<String> {
    let created = details.get("created")?.as_bool()?;
    if !created {
        return None;
    }
    let content = details.get("new_content")?.as_str()?;
    let total_lines = details
        .get("total_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let file_lines: Vec<&str> = content.lines().collect();

    // Summary
    let summary = format!("Created ({} lines)", total_lines);
    let summary_styled = format!(
        "{} {}",
        theme::half_muted_style().render(&["\u{2514}"]),
        theme::half_muted_style().render(&[&summary]),
    );

    let mut lines = vec![summary_styled];

    let gutter_w = total_lines.to_string().len().max(4);
    let content_w = width.saturating_sub(gutter_w + 5);

    let added_style = Style::new().foreground(Color::parse(theme::GREEN));
    for (i, line) in file_lines.iter().enumerate() {
        let ln = i + 1;
        let truncated = truncate_line(line, content_w);
        lines.push(format!(
            "  {} {} {}",
            added_style.render(&[&format!("{:>gutter_w$}", ln)]),
            added_style.render(&["+"]),
            added_style.render(&[&truncated]),
        ));
    }

    if total_lines > file_lines.len() {
        lines.push(format!(
            "  {} ... +{} more lines",
            theme::half_muted_style().render(&[&format!("{:>gutter_w$}", "")]),
            total_lines - file_lines.len()
        ));
    }

    Some(lines.join("\n"))
}

fn diff_summary(old_count: usize, new_count: usize) -> String {
    if old_count == 0 {
        format!(
            "Added {} line{}",
            new_count,
            if new_count == 1 { "" } else { "s" }
        )
    } else if new_count == 0 {
        format!(
            "Removed {} line{}",
            old_count,
            if old_count == 1 { "" } else { "s" }
        )
    } else if old_count == new_count {
        format!(
            "Changed {} line{}",
            old_count,
            if old_count == 1 { "" } else { "s" }
        )
    } else {
        format!("{} \u{2192} {} lines", old_count, new_count)
    }
}

fn truncate_line(line: &str, max_w: usize) -> String {
    if max_w == 0 || line.len() <= max_w {
        line.to_string()
    } else {
        format!("{}…", &line[..max_w.saturating_sub(1)])
    }
}
