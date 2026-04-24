use ruse::prelude::*;

pub enum ChatAction {
    SelectMsg,
    ToggleExpand(usize),
}

pub struct ChatPane {
    viewport: Viewport,
    selected_msg: usize,
    message_count: usize,
    scroll_follow: bool,
    is_focused: bool,
    pending_actions: Vec<ChatAction>,
}

impl ChatPane {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            viewport: Viewport::new(w, h),
            selected_msg: 0,
            message_count: 0,
            scroll_follow: true,
            is_focused: false,
            pending_actions: Vec::new(),
        }
    }

    pub fn set_content(&mut self, content: &str, msg_count: usize) {
        self.message_count = msg_count;
        self.viewport.set_content(content);
        if self.scroll_follow {
            self.viewport.goto_bottom();
        }
    }

    pub fn selected_msg(&self) -> usize {
        self.selected_msg
    }

    pub fn set_selected_msg(&mut self, idx: usize) {
        self.selected_msg = idx;
    }

    #[allow(dead_code)]
    pub fn scroll_follow(&self) -> bool {
        self.scroll_follow
    }

    pub fn set_scroll_follow(&mut self, follow: bool) {
        self.scroll_follow = follow;
    }

    pub fn goto_bottom(&mut self) {
        self.viewport.goto_bottom();
        self.scroll_follow = true;
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.viewport.set_width(w);
        self.viewport.set_height(h);
    }

    pub fn take_actions(&mut self) -> Vec<ChatAction> {
        std::mem::take(&mut self.pending_actions)
    }
}

impl Pane for ChatPane {
    fn update(&mut self, msg: &Msg) -> Cmd {
        match msg {
            Msg::KeyPress(key) => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.viewport.line_down(1);
                    self.scroll_follow = false;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.viewport.line_up(1);
                    self.scroll_follow = false;
                }
                KeyCode::Char('d') => {
                    self.viewport.half_page_down();
                    self.scroll_follow = false;
                }
                KeyCode::Char('u') => {
                    self.viewport.half_page_up();
                    self.scroll_follow = false;
                }
                KeyCode::Char('G') => {
                    self.viewport.goto_bottom();
                    self.scroll_follow = true;
                }
                KeyCode::Char('g') => {
                    self.viewport.goto_top();
                    self.scroll_follow = false;
                }
                KeyCode::Char('J') if self.selected_msg + 1 < self.message_count => {
                    self.selected_msg += 1;
                    self.pending_actions.push(ChatAction::SelectMsg);
                }
                KeyCode::Char('K') if self.selected_msg > 0 => {
                    self.selected_msg -= 1;
                    self.pending_actions.push(ChatAction::SelectMsg);
                }
                KeyCode::Char(' ') => {
                    self.pending_actions
                        .push(ChatAction::ToggleExpand(self.selected_msg));
                }
                _ => {}
            },
            Msg::MouseWheel(mouse) => match mouse.button {
                MouseButton::WheelUp => {
                    self.viewport.line_up(3);
                    self.scroll_follow = false;
                }
                MouseButton::WheelDown => {
                    self.viewport.line_down(3);
                }
                _ => {}
            },
            _ => {}
        }
        None
    }

    fn view(&self) -> String {
        self.viewport.view()
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
