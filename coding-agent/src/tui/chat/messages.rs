use ruse::prelude::*;

use crate::tui::theme;

#[derive(Clone, Copy, PartialEq)]
pub enum ToolStatus {
    Pending,
    Success,
    Error,
}

pub struct UserMessage {
    pub text: String,
}

pub struct AssistantMessage {
    pub thinking: Option<String>,
    pub thinking_expanded: bool,
    pub content: String,
    pub rendered_content: Option<String>,
    #[allow(dead_code)]
    pub model_name: String,
    /// True while this message is still being streamed
    pub is_streaming: bool,
}

pub struct ToolCallMessage {
    /// Unique tool call ID from the API (used for matching start/end events).
    pub tool_call_id: Option<String>,
    pub tool_name: String,
    pub header: String,
    pub body: String,
    pub status: ToolStatus,
    pub expanded: bool,
    /// Pre-styled diff content (shown below header, not wrapped in muted style).
    pub diff_body: Option<String>,
}

pub enum ChatMessage {
    User(UserMessage),
    Assistant(AssistantMessage),
    ToolCall(ToolCallMessage),
}

impl ChatMessage {
    pub fn render(&self, width: usize, focused: bool) -> String {
        match self {
            ChatMessage::User(msg) => render_user(msg, width, focused),
            ChatMessage::Assistant(msg) => render_assistant(msg, width, focused),
            ChatMessage::ToolCall(msg) => render_tool(msg, width, focused),
        }
    }
}

fn render_user(msg: &UserMessage, width: usize, focused: bool) -> String {
    let style = if focused {
        theme::user_focused(width)
    } else {
        theme::user_blurred(width)
    };
    style.render(&[&msg.text])
}

fn render_assistant(msg: &AssistantMessage, width: usize, focused: bool) -> String {
    let mut parts = Vec::new();
    // Inner width accounts for left padding/border (~3 chars)
    let inner_w = width.saturating_sub(4).max(20);

    // Thinking block
    if let Some(ref thinking) = msg.thinking {
        if !thinking.is_empty() {
            if msg.is_streaming || msg.thinking_expanded {
                // Show live thinking during streaming, or when manually expanded.
                // During streaming, show last 10 lines for readability.
                let lines: Vec<&str> = thinking.lines().collect();
                let display = if msg.is_streaming && lines.len() > 10 {
                    let shown = &lines[lines.len() - 10..];
                    format!(
                        "... ({} more lines)\n{}",
                        lines.len() - 10,
                        shown.join("\n")
                    )
                } else {
                    thinking.clone()
                };
                // Word-wrap to fit within the chat area
                let wrapped = ruse::ansi::wordwrap(&display, inner_w);
                let thinking_styled = Style::new()
                    .foreground(Color::parse(theme::FG_HALF_MUTED))
                    .italic(true)
                    .render(&[&wrapped]);
                parts.push(thinking_styled);
            } else {
                // Show full thinking content, styled as muted italic
                let wrapped = ruse::ansi::wordwrap(thinking, inner_w);
                let thinking_styled = Style::new()
                    .foreground(Color::parse(theme::FG_HALF_MUTED))
                    .italic(true)
                    .render(&[&wrapped]);
                parts.push(thinking_styled);
            }
        }
    }

    // Main content — rendered markdown already handles wrapping via glamour
    let content = msg.rendered_content.as_deref().unwrap_or(&msg.content);
    if !content.trim().is_empty() {
        // Word-wrap raw streaming content (rendered markdown is already wrapped)
        if msg.rendered_content.is_some() {
            parts.push(content.to_string());
        } else {
            let wrapped = ruse::ansi::wordwrap(content, inner_w);
            parts.push(wrapped);
        }
    }

    // If streaming with no content yet, show nothing extra (spinner handles it)
    if parts.is_empty() {
        return String::new();
    }

    let body = parts.join("\n");

    if focused {
        theme::assistant_focused(width).render(&[&body])
    } else {
        theme::assistant_blurred().render(&[&body])
    }
}

fn render_tool(msg: &ToolCallMessage, _width: usize, focused: bool) -> String {
    // Status icon — use brighter colors for visibility
    let icon = match msg.status {
        ToolStatus::Pending => Style::new()
            .foreground(Color::parse(theme::GREEN_DARK))
            .bold(true)
            .render(&[theme::TOOL_PENDING]),
        ToolStatus::Success => Style::new()
            .foreground(Color::parse(theme::GREEN))
            .bold(true)
            .render(&[theme::TOOL_SUCCESS]),
        ToolStatus::Error => Style::new()
            .foreground(Color::parse(theme::RED))
            .bold(true)
            .render(&[theme::TOOL_ERROR]),
    };

    // Tool name in readable color
    let name = Style::new()
        .foreground(Color::parse(theme::FG_HALF_MUTED))
        .render(&[&msg.tool_name]);

    // Header detail in base text color
    let header = theme::base_style().render(&[&msg.header]);

    let line = if msg.tool_name.is_empty() {
        // System message (from push_system_msg) — just show the header
        header
    } else {
        format!("{} {} {}", icon, name, header)
    };

    let mut output = line;

    // Diff view (always shown, not toggleable)
    if let Some(ref diff) = msg.diff_body {
        output = format!("{}\n{}", output, diff);
    }

    // Show body when expanded
    if msg.expanded && !msg.body.is_empty() {
        let body_lines: Vec<&str> = msg.body.lines().collect();
        let truncated = if body_lines.len() > 20 {
            let shown: String = body_lines[..20].join("\n");
            format!("{}\n  ... ({} more lines)", shown, body_lines.len() - 20)
        } else {
            msg.body.clone()
        };
        let body_styled = theme::half_muted_style().render(&[&truncated]);
        output = format!("{}\n{}", output, body_styled);
    }

    // Left padding for alignment (2 spaces to match assistant message indent)
    if focused {
        theme::tool_focused(_width).render(&[&output])
    } else {
        // Use base style with padding instead of muted for readability
        Style::new()
            .foreground(Color::parse(theme::FG_BASE))
            .padding_left(2)
            .render(&[&output])
    }
}
