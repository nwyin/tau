use ruse::prelude::*;

use crate::tui::sidebar::SidebarThreadStatus;
use crate::tui::theme;

pub struct ThreadModalPane {
    viewport: Viewport,
    pub thread_id: String,
    alias: String,
    task: String,
    model_name: String,
    status: SidebarThreadStatus,
    width: usize,
    is_focused: bool,
    pending_close: bool,
}

impl ThreadModalPane {
    pub fn new(
        thread_id: String,
        alias: String,
        task: String,
        model_name: String,
        status: SidebarThreadStatus,
        width: usize,
        height: usize,
    ) -> Self {
        // Inner dims = modal - border (2) - padding (2)
        let inner_w = width.saturating_sub(4);
        // Header takes 4 lines (title, task, status, separator), border takes 2
        let viewport_h = height.saturating_sub(6);
        Self {
            viewport: Viewport::new(inner_w, viewport_h),
            thread_id,
            alias,
            task,
            model_name,
            status,
            width,
            is_focused: false,
            pending_close: false,
        }
    }

    pub fn set_content(&mut self, content: &str) {
        self.viewport.set_content(content);
        self.viewport.goto_bottom();
    }

    pub fn set_status(&mut self, status: SidebarThreadStatus) {
        self.status = status;
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        let inner_w = width.saturating_sub(4);
        let viewport_h = height.saturating_sub(6);
        self.viewport.set_width(inner_w);
        self.viewport.set_height(viewport_h);
    }

    pub fn should_close(&self) -> bool {
        self.pending_close
    }
}

impl Pane for ThreadModalPane {
    fn update(&mut self, msg: &Msg) -> Cmd {
        if let Msg::KeyPress(key) = msg {
            match key.code {
                KeyCode::Escape => {
                    self.pending_close = true;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.viewport.line_down(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.viewport.line_up(1);
                }
                KeyCode::Char('d') => {
                    self.viewport.half_page_down();
                }
                KeyCode::Char('u') => {
                    self.viewport.half_page_up();
                }
                KeyCode::Char('G') => {
                    self.viewport.goto_bottom();
                }
                KeyCode::Char('g') => {
                    self.viewport.goto_top();
                }
                _ => {}
            }
        }
        None
    }

    fn view(&self) -> String {
        let inner_w = self.width.saturating_sub(4);

        let (status_icon, status_style, status_text) = match self.status {
            SidebarThreadStatus::Running => {
                (theme::TOOL_PENDING, theme::green_dark_style(), "Running")
            }
            SidebarThreadStatus::Completed => {
                (theme::TOOL_SUCCESS, theme::green_style(), "Completed")
            }
            SidebarThreadStatus::Failed => (theme::TOOL_ERROR, theme::red_style(), "Failed"),
        };

        let title = format!(
            "{} {} {} {}",
            status_style.render(&[status_icon]),
            theme::subtle_style().bold(true).render(&[&self.alias]),
            theme::half_muted_style().render(&["@"]),
            theme::half_muted_style().render(&[&self.model_name]),
        );

        let task_display = if self.task.len() > inner_w {
            format!("{}…", &self.task[..inner_w.saturating_sub(1)])
        } else {
            self.task.clone()
        };
        let task_line = theme::half_muted_style().render(&[&task_display]);
        let status_line = theme::half_muted_style().render(&[status_text]);
        let header_sep = theme::muted_style().render(&[&theme::SECTION_SEP.repeat(inner_w)]);

        let viewport_content = self.viewport.view();

        let mut body_parts = vec![title, task_line, status_line, header_sep];
        if viewport_content.is_empty() {
            body_parts.push(theme::half_muted_style().render(&["No messages yet"]));
        } else {
            body_parts.push(viewport_content);
        }

        let body = body_parts.join("\n");

        let border_color = match self.status {
            SidebarThreadStatus::Running => theme::GREEN_DARK,
            SidebarThreadStatus::Completed => theme::FG_HALF_MUTED,
            SidebarThreadStatus::Failed => theme::RED,
        };

        Style::new()
            .border(ROUNDED_BORDER, &[true])
            .border_foreground(Color::parse(border_color))
            .padding(&[0, 1])
            .width(self.width as u16)
            .render(&[&body])
    }

    fn focus(&mut self) -> Cmd {
        self.is_focused = true;
        None
    }

    fn blur(&mut self) {
        self.is_focused = false;
    }

    fn focused(&self) -> bool {
        self.is_focused
    }
}
