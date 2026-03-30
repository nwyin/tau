use ruse::prelude::*;

use super::theme;

pub enum FocusHint {
    Editor,
    Chat,
    Permission,
}

pub fn render_status_bar(width: usize, hint: FocusHint) -> String {
    let help = match hint {
        FocusHint::Editor => "enter send | shift+enter newline | ctrl+p cmds | tab chat",
        FocusHint::Chat => "j/k scroll | J/K messages | space expand | tab editor",
        FocusHint::Permission => "a allow | s session | d deny | esc cancel",
    };

    let styled = theme::half_muted_style().render(&[help]);
    Style::new().width(width as u16).render(&[&styled])
}
