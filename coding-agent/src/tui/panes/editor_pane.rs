use ruse::prelude::*;

use crate::tui::anim::GradientSpinner;
use crate::tui::theme;

struct TabState {
    #[allow(dead_code)]
    prefix: String,
    candidates: Vec<String>,
    index: usize,
}

pub struct EditorPane {
    input: TextInput,
    tab_state: Option<TabState>,
    spinner: GradientSpinner,
    is_busy: bool,
    pending_submit: Option<String>,
    slash_commands: Vec<String>,
}

impl EditorPane {
    pub fn new() -> Self {
        Self {
            input: TextInput::new()
                .with_placeholder("Ready for instructions")
                .with_width(80),
            tab_state: None,
            spinner: GradientSpinner::new("Thinking"),
            is_busy: false,
            pending_submit: None,
            slash_commands: Vec::new(),
        }
    }

    pub fn take_submit(&mut self) -> Option<String> {
        self.pending_submit.take()
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.is_busy = busy;
        if busy {
            self.spinner = GradientSpinner::new("Thinking");
        }
    }

    pub fn is_busy(&self) -> bool {
        self.is_busy
    }

    pub fn set_slash_commands(&mut self, cmds: Vec<String>) {
        self.slash_commands = cmds;
    }

    pub fn set_width(&mut self, w: usize) {
        self.input.set_width(w);
    }

    #[allow(dead_code)]
    pub fn set_value(&mut self, s: &str) {
        self.input.set_value(s);
        self.tab_state = None;
    }

    pub fn value(&self) -> String {
        self.input.value()
    }

    pub fn tick_spinner(&mut self) {
        self.spinner.tick();
    }

    /// Run tab completion against slash commands. Called by the parent
    /// when Tab is pressed and the input starts with '/'.
    pub fn tab_complete(&mut self) {
        // If already cycling, advance
        if let Some(ref mut state) = self.tab_state {
            if state.candidates.is_empty() {
                return;
            }
            state.index = (state.index + 1) % state.candidates.len();
            let completed = format!("{} ", state.candidates[state.index]);
            self.input.set_value(&completed);
            return;
        }

        let value = self.input.value().to_string();
        if !value.starts_with('/') || value.contains(' ') {
            return;
        }

        let candidates: Vec<String> = self
            .slash_commands
            .iter()
            .filter(|c| c.starts_with(&value))
            .cloned()
            .collect();

        if candidates.is_empty() {
            return;
        }

        let first = candidates[0].clone();
        self.tab_state = Some(TabState {
            prefix: value,
            candidates,
            index: 0,
        });
        self.input.set_value(&format!("{} ", first));
    }
}

impl Pane for EditorPane {
    fn update(&mut self, msg: &Msg) -> Cmd {
        if self.is_busy {
            return None;
        }

        match msg {
            Msg::KeyPress(key) => {
                // Reset tab state on non-tab keys
                self.tab_state = None;
                match key.code {
                    KeyCode::Enter if key.modifiers.contains(Modifiers::SHIFT) => {
                        // Shift+Enter: insert a newline into the value
                        let mut val = self.input.value();
                        let pos = self.input.position();
                        val.insert(pos, '\n');
                        self.input.set_value(&val);
                        self.input.set_cursor(pos + 1);
                    }
                    KeyCode::Enter => {
                        let text = self.input.value().trim().to_string();
                        if !text.is_empty() {
                            self.pending_submit = Some(text);
                            self.input.set_value("");
                            self.tab_state = None;
                        }
                    }
                    // Tab is intercepted by the parent for focus cycling / completion
                    _ => {
                        self.input.update(msg);
                    }
                }
            }
            Msg::Paste(text) => {
                // Collapse newlines to spaces for single-line input
                let cleaned = text.replace('\n', " ").replace('\r', "");
                self.input.update(&Msg::Paste(cleaned));
                self.tab_state = None;
            }
            _ => {}
        }
        None
    }

    fn view(&self) -> String {
        if self.is_busy {
            format!("  {}", self.spinner.view())
        } else {
            let prompt = format!("  {} ", theme::primary_style().render(&[">"]));
            format!("{}{}", prompt, self.input.view())
        }
    }

    fn focus(&mut self) -> Cmd {
        self.input.focus()
    }

    fn blur(&mut self) {
        self.input.blur();
    }

    fn focused(&self) -> bool {
        self.input.focused()
    }
}
