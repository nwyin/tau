use ruse::prelude::*;

use crate::tools::TodoItem;
use crate::tui::sidebar::{self, SidebarState, SidebarThread, SidebarThreadStatus};

pub enum SidebarAction {
    OpenThread(usize),
    Back,
}

pub struct SidebarThreadData {
    pub alias: String,
    pub status: SidebarThreadStatus,
}

pub struct SidebarData {
    pub model_id: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub context_window: u64,
    pub total_cost: f64,
    pub thinking_level: String,
    pub active_tools: Vec<String>,
    pub todos: Vec<TodoItem>,
    pub cwd: String,
    pub threads: Vec<SidebarThreadData>,
}

pub struct SidebarPane {
    cursor: usize,
    is_focused: bool,
    pending_action: Option<SidebarAction>,
    width: usize,
    height: usize,
    data: SidebarData,
}

impl SidebarPane {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            cursor: 0,
            is_focused: false,
            pending_action: None,
            width,
            height,
            data: SidebarData {
                model_id: String::new(),
                tokens_in: 0,
                tokens_out: 0,
                context_window: 0,
                total_cost: 0.0,
                thinking_level: "off".to_string(),
                active_tools: Vec::new(),
                todos: Vec::new(),
                cwd: String::new(),
                threads: Vec::new(),
            },
        }
    }

    pub fn set_data(&mut self, data: SidebarData) {
        self.data = data;
        // Clamp cursor if threads shrank
        if !self.data.threads.is_empty() {
            self.cursor = self.cursor.min(self.data.threads.len() - 1);
        } else {
            self.cursor = 0;
        }
    }

    pub fn set_size(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }

    #[allow(dead_code)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn take_action(&mut self) -> Option<SidebarAction> {
        self.pending_action.take()
    }

    #[allow(dead_code)]
    pub fn thread_count(&self) -> usize {
        self.data.threads.len()
    }
}

impl Pane for SidebarPane {
    fn update(&mut self, msg: &Msg) -> Cmd {
        if let Msg::KeyPress(key) = msg {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if !self.data.threads.is_empty() && self.cursor + 1 < self.data.threads.len() {
                        self.cursor += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                }
                KeyCode::Enter => {
                    if !self.data.threads.is_empty() {
                        self.pending_action = Some(SidebarAction::OpenThread(self.cursor));
                    }
                }
                KeyCode::Escape => {
                    self.pending_action = Some(SidebarAction::Back);
                }
                _ => {}
            }
        }
        None
    }

    fn view(&self) -> String {
        let threads: Vec<SidebarThread> = self
            .data
            .threads
            .iter()
            .map(|t| SidebarThread {
                alias: &t.alias,
                status: t.status,
            })
            .collect();
        let selected = if self.is_focused {
            Some(self.cursor)
        } else {
            None
        };
        sidebar::render_sidebar(&SidebarState {
            width: self.width,
            height: self.height,
            model_id: &self.data.model_id,
            tokens_in: self.data.tokens_in,
            tokens_out: self.data.tokens_out,
            context_window: self.data.context_window,
            total_cost: self.data.total_cost,
            thinking_level: &self.data.thinking_level,
            active_tools: &self.data.active_tools,
            todos: &self.data.todos,
            cwd: &self.data.cwd,
            threads: &threads,
            selected_thread: selected,
        })
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
