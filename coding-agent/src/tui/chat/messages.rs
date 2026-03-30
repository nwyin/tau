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

#[allow(dead_code)]
pub struct AssistantMessage {
    pub thinking: Option<String>,
    pub thinking_expanded: bool,
    pub content: String,
    pub rendered_content: Option<String>,
    pub model_name: String,
}

pub struct ToolCallMessage {
    pub tool_name: String,
    pub header: String,
    pub body: String,
    pub status: ToolStatus,
    pub expanded: bool,
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

    // Thinking block (collapsible)
    if let Some(ref thinking) = msg.thinking {
        if msg.thinking_expanded {
            let thinking_styled = theme::half_muted_style().render(&[thinking]);
            parts.push(thinking_styled);
        } else {
            let line_count = thinking.lines().count();
            let summary = theme::half_muted_style().render(&[&format!(
                "Thought ({} lines) {}",
                line_count,
                theme::SECTION_SEP
            )]);
            parts.push(summary);
        }
    }

    // Main content
    let content = msg.rendered_content.as_deref().unwrap_or(&msg.content);
    if !content.is_empty() {
        parts.push(content.to_string());
    }

    let body = parts.join("\n");

    if focused {
        theme::assistant_focused(width).render(&[&body])
    } else {
        theme::assistant_blurred().render(&[&body])
    }
}

fn render_tool(msg: &ToolCallMessage, width: usize, focused: bool) -> String {
    let icon = match msg.status {
        ToolStatus::Pending => theme::green_dark_style().render(&[theme::TOOL_PENDING]),
        ToolStatus::Success => theme::green_style().render(&[theme::TOOL_SUCCESS]),
        ToolStatus::Error => theme::red_style().render(&[theme::TOOL_ERROR]),
    };
    let name = theme::half_muted_style().render(&[&msg.tool_name]);
    let header = theme::base_style().render(&[&msg.header]);
    let line = format!("{} {} {}", icon, name, header);

    let mut output = line;
    if msg.expanded && !msg.body.is_empty() {
        let body = theme::muted_style().render(&[&msg.body]);
        output = format!("{}\n{}", output, body);
    }

    if focused {
        theme::tool_focused(width).render(&[&output])
    } else {
        theme::tool_blurred().render(&[&output])
    }
}
