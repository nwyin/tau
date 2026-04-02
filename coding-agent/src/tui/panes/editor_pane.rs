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
    width: usize,
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
            width: 80,
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
        self.width = w;
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

    /// Number of visual lines the editor content occupies (for layout).
    pub fn visual_lines(&self) -> usize {
        if self.is_busy {
            return 1;
        }
        let text = self.input.value();
        if text.is_empty() {
            return 1;
        }
        let w = self.width.max(1);
        // Split by explicit newlines, then count wrapped lines per segment
        text.split('\n')
            .map(|seg| {
                let chars = seg.chars().count();
                if chars == 0 {
                    1
                } else {
                    chars.div_ceil(w)
                }
            })
            .sum()
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

    /// Render the editor content with soft-wrapping and cursor.
    fn render_wrapped(&self) -> String {
        let text = self.input.value();
        let cursor_pos = self.input.position();
        let focused = self.input.focused();
        let w = self.width.max(1);

        let prompt = format!("  {} ", theme::primary_style().render(&[">"]));
        let continuation = "    "; // same width as prompt for alignment

        if text.is_empty() {
            // Show placeholder or cursor
            if focused {
                let cursor_char = Style::new().reverse(true).render(&[" "]);
                return format!("{}{}", prompt, cursor_char);
            } else {
                let placeholder = theme::half_muted_style().render(&["Ready for instructions"]);
                return format!("{}{}", prompt, placeholder);
            }
        }

        // Build visual lines by splitting on newlines, then wrapping each segment
        let chars: Vec<char> = text.chars().collect();
        let mut visual_lines: Vec<Vec<char>> = Vec::new();

        let mut seg_start = 0;
        for (i, &ch) in chars.iter().enumerate() {
            if ch == '\n' {
                // Push the segment before the newline (may be empty)
                let segment = &chars[seg_start..i];
                if segment.is_empty() {
                    visual_lines.push(Vec::new());
                } else {
                    for chunk in segment.chunks(w) {
                        visual_lines.push(chunk.to_vec());
                    }
                }
                seg_start = i + 1;
            }
        }
        // Push the last segment
        let segment = &chars[seg_start..];
        if segment.is_empty() {
            visual_lines.push(Vec::new());
        } else {
            for chunk in segment.chunks(w) {
                visual_lines.push(chunk.to_vec());
            }
        }

        // Find cursor position in the visual grid by walking through
        // visual_lines and mapping char index to (row, col).
        let (cursor_row, cursor_col) = {
            let mut char_idx = 0;
            let mut found = (0, 0);
            'outer: for (row_idx, line_chars) in visual_lines.iter().enumerate() {
                // Check if cursor is within this visual line
                if cursor_pos >= char_idx && cursor_pos <= char_idx + line_chars.len() {
                    // Could be at end of this line or start of next.
                    // It's on this line if cursor_pos < char_idx + len,
                    // OR if this is the last visual line of a segment (before a newline
                    // or at end of text).
                    if cursor_pos < char_idx + line_chars.len()
                        || cursor_pos == chars.len()
                        || (cursor_pos == char_idx + line_chars.len() && line_chars.len() < w)
                    {
                        found = (row_idx, cursor_pos - char_idx);
                        break 'outer;
                    }
                }
                char_idx += line_chars.len();
                // Account for the newline character between segments
                if char_idx < chars.len() && chars[char_idx] == '\n' {
                    char_idx += 1; // skip the newline char
                }
            }
            found
        };

        // Render each visual line
        let mut output = String::new();
        for (row_idx, line_chars) in visual_lines.iter().enumerate() {
            if row_idx > 0 {
                output.push('\n');
            }
            // Prefix: prompt for first line, continuation indent for rest
            if row_idx == 0 {
                output.push_str(&prompt);
            } else {
                output.push_str(continuation);
            }

            if focused && row_idx == cursor_row {
                // Render with cursor
                for (col_idx, &ch) in line_chars.iter().enumerate() {
                    if col_idx == cursor_col {
                        output.push_str(&Style::new().reverse(true).render(&[&ch.to_string()]));
                    } else {
                        output.push(ch);
                    }
                }
                // Cursor at end of this line
                if cursor_col >= line_chars.len() {
                    output.push_str(&Style::new().reverse(true).render(&[" "]));
                }
            } else {
                for &ch in line_chars {
                    output.push(ch);
                }
            }
        }

        output
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
                    KeyCode::Enter
                        if key.modifiers.contains(Modifiers::SHIFT)
                            || key.modifiers.contains(Modifiers::ALT) =>
                    {
                        // Shift+Enter or Alt+Enter: insert a newline
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
                self.input.update(&Msg::Paste(text.clone()));
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
            self.render_wrapped()
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
