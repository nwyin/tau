use ruse::prelude::*;

use super::theme;

pub enum FocusHint {
    Editor,
    Chat,
    Sidebar,
    ThreadModal,
    Permission,
}

pub fn render_status_bar(width: usize, hint: FocusHint, warning: Option<&str>) -> String {
    if let Some(msg) = warning {
        let warn = Style::new()
            .foreground(Color::parse(theme::YELLOW))
            .render(&[msg]);
        return format!(
            "  {}",
            Style::new()
                .width(width.saturating_sub(2) as u16)
                .render(&[&warn])
        );
    }

    let help = match hint {
        FocusHint::Editor => "enter send | shift+enter newline | ctrl+p cmds | tab chat",
        FocusHint::Chat => "j/k scroll | J/K messages | space expand | tab sidebar",
        FocusHint::Sidebar => "j/k navigate | enter inspect | esc editor | tab editor",
        FocusHint::ThreadModal => "j/k scroll | esc close | g/G top/bottom",
        FocusHint::Permission => "a allow | s session | d deny | esc cancel",
    };

    // Indent to align with chat content (2 spaces matching input prompt indent)
    let styled = theme::half_muted_style().render(&[help]);
    format!(
        "  {}",
        Style::new()
            .width(width.saturating_sub(2) as u16)
            .render(&[&styled])
    )
}
